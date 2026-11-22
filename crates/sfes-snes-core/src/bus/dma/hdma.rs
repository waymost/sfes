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
            BYTE_CYCLES, Byte, CHANNEL_OVERHEAD, ChannelRegister, ChannelRegisters, ChannelSet,
            DIRECTION, DmaChannel, dma_a_target, unit,
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

impl ChannelRegisters {
    /// The A-bus address of the next table byte, `$43x8/9` in bank `$43x4`, and moves it on
    /// by one, wrapping at 16 bits with the bank kept (ADR 0026, point 6).
    // @source: anomie-regs#43x2
    // @source: anomie-regs#43x8
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0026
    const fn next_table_address(&mut self) -> u32 {
        let address = self.word(ChannelRegister::TableLow);
        self.set_word(ChannelRegister::TableLow, address.wrapping_add(1));
        let [low, high] = address.to_le_bytes();
        u32::from_le_bytes([low, high, self.get(ChannelRegister::ABank), 0])
    }

    /// The A-bus address of the next data byte: the next table byte (direct) or `$43x5/6`
    /// in bank `$43x7` (indirect), moved on by one, wrapping at 16 bits with the bank kept
    /// (ADR 0026, point 6).
    // @source: anomie-regs#43x0
    // @source: anomie-regs#43x5
    // @source: anomie-regs#43x8
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0026
    const fn next_data_address(&mut self) -> u32 {
        if self.control() & INDIRECT == 0 {
            return self.next_table_address();
        }
        let address = self.word(ChannelRegister::CountLow);
        self.set_word(ChannelRegister::CountLow, address.wrapping_add(1));
        let [low, high] = address.to_le_bytes();
        u32::from_le_bytes([low, high, self.get(ChannelRegister::IndirectBank), 0])
    }
}

impl Bus {
    /// Runs the HDMA step that fell due, if any, and returns the channels it worked on
    /// (ADR 0026, points 3 and 9).
    // @source: anomie-timing#s-cpu/hdma
    // @source: adr-0026
    pub(crate) fn run_due_hdma<O: Observer>(&mut self, observer: &mut O) -> ChannelSet {
        match self.dma.hdma_due.take() {
            Some(HdmaStep::Setup) => self.hdma_setup(observer),
            Some(HdmaStep::Line) => self.hdma_line(observer),
            None => ChannelSet::EMPTY,
        }
    }

    /// The frame's setup: clears every channel's terminated flag, then for each enabled
    /// channel copies `$43x2/3` into `$43x8/9` and reloads from the table without the
    /// last-channel oddity (ADR 0026, points 4 and 5).
    // @source: anomie-regs#43x8
    // @source: anomie-regs#43xa
    // @source: anomie-regs#dma-and-hdma/hdma
    // @source: anomie-timing#s-cpu/hdma
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0026
    fn hdma_setup<O: Observer>(&mut self, observer: &mut O) -> ChannelSet {
        self.dma.terminated = ChannelSet::EMPTY;
        let channels = self.dma.hdma_enabled;
        if channels.is_empty() {
            return ChannelSet::EMPTY;
        }
        self.advance(STEP_OVERHEAD, observer);
        for channel in channels.iter() {
            self.advance(CHANNEL_OVERHEAD, observer);
            let registers = self.dma.channel_mut(channel);
            let start = registers.word(ChannelRegister::AAddressLow);
            registers.set_word(ChannelRegister::TableLow, start);
            self.reload(channel, false, observer);
        }
        channels
    }

    /// A line's transfer: for each channel enabled and not terminated, moves one unit if
    /// `DoTransfer` is set, decrements `$43xA` and reloads when its line count reaches 0
    /// (ADR 0026, points 4 and 6).
    // @source: anomie-regs#420c
    // @source: anomie-regs#43xa
    // @source: anomie-regs#dma-and-hdma/hdma
    // @source: anomie-timing#s-cpu/hdma
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0026
    fn hdma_line<O: Observer>(&mut self, observer: &mut O) -> ChannelSet {
        let active = self.dma.hdma_enabled.without(self.dma.terminated);
        let Some(last) = active.last() else {
            return ChannelSet::EMPTY;
        };
        self.advance(STEP_OVERHEAD, observer);
        for channel in active.iter() {
            self.advance(CHANNEL_OVERHEAD, observer);
            if self.dma.do_transfer.contains(channel) {
                self.move_unit(channel, observer);
            }
            let registers = self.dma.channel_mut(channel);
            let counter = registers.get(ChannelRegister::LineCounter).wrapping_sub(1);
            registers.set(ChannelRegister::LineCounter, counter);
            if counter & REPEAT != 0 {
                self.dma.do_transfer.insert(channel);
            } else {
                self.dma.do_transfer.remove(channel);
            }
            if counter & LINE_COUNT == 0 {
                self.reload(channel, channel == last, observer);
            }
        }
        active
    }

    /// Moves one unit of `channel`'s mode, one 8-cycle slot per byte (ADR 0026, point 6).
    // @source: anomie-regs#43x0
    // @source: anomie-regs#dma-and-hdma/hdma
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0026
    fn move_unit<O: Observer>(&mut self, channel: DmaChannel, observer: &mut O) {
        let control = self.dma.channel(channel).control();
        for offset in unit(control) {
            let registers = self.dma.channel_mut(channel);
            let byte = Byte {
                a_address: registers.next_data_address(),
                b_address: registers
                    .get(ChannelRegister::BAddress)
                    .wrapping_add(*offset),
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
    // @source: anomie-regs#43x5
    // @source: anomie-regs#43xa
    // @source: anomie-regs#dma-and-hdma/hdma
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0026
    fn reload<O: Observer>(&mut self, channel: DmaChannel, last: bool, observer: &mut O) {
        let counter = self.read_table(channel);
        let registers = self.dma.channel_mut(channel);
        registers.set(ChannelRegister::LineCounter, counter);
        if registers.control() & INDIRECT != 0 {
            let address = if last && counter == 0 {
                let high = self.read_table(channel);
                self.advance(BYTE_CYCLES, observer);
                u16::from_le_bytes([0, high])
            } else {
                let low = self.read_table(channel);
                self.advance(BYTE_CYCLES, observer);
                let high = self.read_table(channel);
                self.advance(BYTE_CYCLES, observer);
                u16::from_le_bytes([low, high])
            };
            self.dma
                .channel_mut(channel)
                .set_word(ChannelRegister::CountLow, address);
        }
        if counter == 0 {
            self.dma.terminated.insert(channel);
        } else {
            self.dma.do_transfer.insert(channel);
        }
    }

    /// Reads `channel`'s next table byte through the A bus as a DMA does, placing it on
    /// the data bus (ADR 0026, point 8).
    // @source: anomie-memmap#hardware/data-bus
    // @source: anomie-regs#43x8
    // @source: anomie-regs#dma-and-hdma/dma
    // @source: adr-0025
    // @source: adr-0026
    fn read_table(&mut self, channel: DmaChannel) -> u8 {
        let address = self.dma.channel_mut(channel).next_table_address();
        let target = dma_a_target(decode(self.cartridge.map(), address));
        let value = self.read_dma_a(target).unwrap_or(self.open_bus_value);
        self.open_bus_value = value;
        value
    }
}
