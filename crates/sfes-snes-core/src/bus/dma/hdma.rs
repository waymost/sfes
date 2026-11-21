// @source: anomie-regs#420c
// @source: anomie-regs#43x0
// @source: anomie-regs#43x5
// @source: anomie-regs#43x8
// @source: anomie-regs#43xa
// @source: anomie-regs#dma-and-hdma/hdma
// @source: anomie-timing#s-cpu/hdma
// @source: fullsnes#snesdmaandhdmachannel07registers
// @source: adr-0026
//! HDMA's two steps: the frame's setup at H=6 of line 0 and a line's transfer at H=278,
//! each run inside a pause (ADR 0026, points 4 to 6).

use sfes_api::Observer;

use crate::{
    bus::{
        Bus,
        decode::decode,
        dma::{
            BYTE_CYCLES, Byte, CHANNEL_OVERHEAD, CHANNELS, Channel, DIRECTION, MODE, UNIT_LENGTHS,
            dma_a_target, unit_offset,
        },
    },
    timing::HdmaStep,
};

/// Master cycles of overhead for a step that works on any channel (ADR 0026, point 4).
const STEP_OVERHEAD: u64 = 18;
/// `$43x0` bit 6: indirect addressing.
const INDIRECT: u8 = 0x40;
/// `$43xA` bit 7: repeat.
const REPEAT: u8 = 0x80;
/// `$43xA` bits 6–0: the line count.
const LINE_COUNT: u8 = 0x7F;

impl Channel {
    /// $43x4 `A1Bx`: the table's bank.
    // @source: anomie-regs#43x2
    // @source: fullsnes#snesdmaandhdmachannel07registers
    const fn table_bank(&self) -> u8 {
        self.registers[0x4]
    }

    /// $43x8–$43x9 `A2AxL`, `A2AxH`: the current table address.
    // @source: anomie-regs#43x8
    const fn table_address(&self) -> u16 {
        u16::from_le_bytes([self.registers[0x8], self.registers[0x9]])
    }

    /// Sets `$43x8–$43x9`.
    // @source: anomie-regs#43x8
    const fn set_table_address(&mut self, address: u16) {
        let [low, high] = address.to_le_bytes();
        self.registers[0x8] = low;
        self.registers[0x9] = high;
    }

    /// $43x5–$43x6 `DASxL`, `DASxH`: the current indirect address.
    // @source: anomie-regs#43x5
    const fn indirect_address(&self) -> u16 {
        u16::from_le_bytes([self.registers[0x5], self.registers[0x6]])
    }

    /// Sets `$43x5–$43x6`.
    // @source: anomie-regs#43x5
    const fn set_indirect_address(&mut self, address: u16) {
        let [low, high] = address.to_le_bytes();
        self.registers[0x5] = low;
        self.registers[0x6] = high;
    }

    /// $43x7 `DASBx`: the indirect address's bank.
    // @source: anomie-regs#43x5
    const fn indirect_bank(&self) -> u8 {
        self.registers[0x7]
    }

    /// $43xA `NLTRx`: repeat and the line count.
    // @source: anomie-regs#43xa
    const fn line_counter(&self) -> u8 {
        self.registers[0xA]
    }

    /// Sets `$43xA`.
    // @source: anomie-regs#43xa
    const fn set_line_counter(&mut self, value: u8) {
        self.registers[0xA] = value;
    }

    /// The A-bus address of the next data byte, `$43x8/9` in bank `$43x4` (direct) or
    /// `$43x5/6` in bank `$43x7` (indirect), and moves it on by one, wrapping at 16 bits
    /// with the bank kept (ADR 0026, point 6).
    // @source: anomie-regs#43x0
    // @source: anomie-regs#43x5
    // @source: anomie-regs#43x8
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0026
    const fn next_data_address(&mut self) -> u32 {
        let (bank, address) = if self.control() & INDIRECT != 0 {
            let address = self.indirect_address();
            self.set_indirect_address(address.wrapping_add(1));
            (self.indirect_bank(), address)
        } else {
            let address = self.table_address();
            self.set_table_address(address.wrapping_add(1));
            (self.table_bank(), address)
        };
        let [low, high] = address.to_le_bytes();
        u32::from_le_bytes([low, high, bank, 0])
    }

    /// The A-bus address of the next table byte, `$43x8/9` in bank `$43x4`, and moves it on
    /// by one (ADR 0026, point 6).
    // @source: anomie-regs#43x8
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0026
    const fn next_table_address(&mut self) -> u32 {
        let address = self.table_address();
        self.set_table_address(address.wrapping_add(1));
        let [low, high] = address.to_le_bytes();
        u32::from_le_bytes([low, high, self.table_bank(), 0])
    }
}

impl Bus {
    /// Runs the HDMA step that fell due, if any, and returns the channels it worked on
    /// (ADR 0026, points 3 and 9).
    // @source: anomie-timing#s-cpu/hdma
    // @source: adr-0026
    pub(crate) fn run_due_hdma<O: Observer>(&mut self, observer: &mut O) -> u8 {
        match self.dma.hdma_due.take() {
            Some(HdmaStep::Setup) => self.hdma_setup(observer),
            Some(HdmaStep::Line) => self.hdma_line(observer),
            None => 0,
        }
    }

    /// The frame's setup: clears every channel's terminated flag, then for each enabled
    /// channel copies `$43x2/3` into `$43x8/9` and reloads from the table without the
    /// last-channel oddity (ADR 0026, points 4 and 5).
    // @source: anomie-regs#dma-and-hdma/hdma
    // @source: anomie-regs#43x8
    // @source: anomie-regs#43xa
    // @source: anomie-timing#s-cpu/hdma
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0026
    fn hdma_setup<O: Observer>(&mut self, observer: &mut O) -> u8 {
        self.dma.terminated = 0;
        let channels = self.dma.hdma_enabled;
        if channels == 0 {
            return 0;
        }
        self.advance(STEP_OVERHEAD, observer);
        for channel in 0..CHANNELS {
            if channels & (1 << channel) != 0 {
                self.advance(CHANNEL_OVERHEAD, observer);
                if let Some(registers) = self.dma.channels.get_mut(channel) {
                    let [low, high, ..] = registers.a_address().to_le_bytes();
                    registers.set_table_address(u16::from_le_bytes([low, high]));
                }
                self.reload(channel, false, observer);
            }
        }
        channels
    }

    /// A line's transfer: for each channel enabled and not terminated, moves one unit if
    /// `DoTransfer` is set, decrements `$43xA` and reloads when its line count reaches 0
    /// (ADR 0026, points 4 and 6).
    // @source: anomie-regs#dma-and-hdma/hdma
    // @source: anomie-regs#43xa
    // @source: anomie-regs#420c
    // @source: anomie-timing#s-cpu/hdma
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0026
    fn hdma_line<O: Observer>(&mut self, observer: &mut O) -> u8 {
        let active = self.dma.hdma_enabled & !self.dma.terminated;
        if active == 0 {
            return 0;
        }
        let last = usize::try_from(active.ilog2()).unwrap_or_default();
        self.advance(STEP_OVERHEAD, observer);
        for channel in 0..CHANNELS {
            let bit = 1 << channel;
            if active & bit == 0 {
                continue;
            }
            self.advance(CHANNEL_OVERHEAD, observer);
            if self.dma.do_transfer & bit != 0 {
                self.move_unit(channel, observer);
            }
            let Some(registers) = self.dma.channels.get_mut(channel) else {
                continue;
            };
            let counter = registers.line_counter().wrapping_sub(1);
            registers.set_line_counter(counter);
            if counter & REPEAT != 0 {
                self.dma.do_transfer |= bit;
            } else {
                self.dma.do_transfer &= !bit;
            }
            if counter & LINE_COUNT == 0 {
                self.reload(channel, channel == last, observer);
            }
        }
        active
    }

    /// Moves one unit of `channel`'s mode, one 8-cycle slot per byte (ADR 0026, point 6).
    // @source: anomie-regs#dma-and-hdma/hdma
    // @source: anomie-regs#43x0
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0026
    fn move_unit<O: Observer>(&mut self, channel: usize, observer: &mut O) {
        let Some(control) = self.dma.channels.get(channel).map(Channel::control) else {
            return;
        };
        let length = UNIT_LENGTHS
            .get(usize::from(control & MODE))
            .copied()
            .unwrap_or(1);
        for index in 0..length {
            let Some(registers) = self.dma.channels.get_mut(channel) else {
                return;
            };
            let offset = unit_offset(control, u32::try_from(index).unwrap_or_default());
            let byte = Byte {
                a_address: registers.next_data_address(),
                b_address: registers.b_address().wrapping_add(offset),
                from_b_bus: control & DIRECTION != 0,
                hdma: true,
            };
            self.move_byte(channel, byte, observer);
            self.advance(BYTE_CYCLES, observer);
        }
    }

    /// Reloads `channel` from its table: `$43xA`, then in indirect mode the indirect
    /// address, two bytes in two slots, or with the last-channel oddity one byte into
    /// `$43x6` with `$43x5` set to 0. A line count of `$00` terminates the channel; any
    /// other sets `DoTransfer` (ADR 0026, points 4 to 6).
    // @source: anomie-regs#dma-and-hdma/hdma
    // @source: anomie-regs#43x5
    // @source: anomie-regs#43xa
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0026
    fn reload<O: Observer>(&mut self, channel: usize, last: bool, observer: &mut O) {
        let bit = 1 << channel;
        let counter = self.read_table(channel);
        let Some(registers) = self.dma.channels.get_mut(channel) else {
            return;
        };
        registers.set_line_counter(counter);
        if registers.control() & INDIRECT != 0 {
            if last && counter == 0 {
                let high = self.read_table(channel);
                self.advance(BYTE_CYCLES, observer);
                if let Some(registers) = self.dma.channels.get_mut(channel) {
                    registers.set_indirect_address(u16::from_le_bytes([0, high]));
                }
            } else {
                let low = self.read_table(channel);
                self.advance(BYTE_CYCLES, observer);
                let high = self.read_table(channel);
                self.advance(BYTE_CYCLES, observer);
                if let Some(registers) = self.dma.channels.get_mut(channel) {
                    registers.set_indirect_address(u16::from_le_bytes([low, high]));
                }
            }
        }
        if counter == 0 {
            self.dma.terminated |= bit;
        } else {
            self.dma.do_transfer |= bit;
        }
    }

    /// Reads `channel`'s next table byte through the A bus as a DMA does, placing it on
    /// the data bus (ADR 0026, point 8).
    // @source: anomie-regs#43x8
    // @source: anomie-regs#dma-and-hdma/dma
    // @source: anomie-memmap#hardware/data-bus
    // @source: adr-0025
    // @source: adr-0026
    fn read_table(&mut self, channel: usize) -> u8 {
        let Some(address) = self
            .dma
            .channels
            .get_mut(channel)
            .map(Channel::next_table_address)
        else {
            return self.open_bus_value;
        };
        let target = dma_a_target(decode(self.cartridge.map(), address));
        let value = self.read_dma_a(target).unwrap_or(self.open_bus_value);
        self.open_bus_value = value;
        value
    }
}
