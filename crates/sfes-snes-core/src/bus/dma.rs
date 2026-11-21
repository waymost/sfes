// @source: anomie-regs#420b
// @source: anomie-regs#43x0
// @source: anomie-regs#43x1
// @source: anomie-regs#43x2
// @source: anomie-regs#43x5
// @source: anomie-regs#43xb
// @source: anomie-regs#dma-and-hdma/dma
// @source: anomie-timing#s-cpu/dma
// @source: fullsnes#snesdmaandhdmastartenableregisters
// @source: fullsnes#snesdmaandhdmachannel07registers
// @source: fullsnes#snesunpredictablethings
// @source: adr-0025
//! General DMA: the eight channels' registers `$43x0–$43xB`, the `$420B` request and the
//! transfer the bus runs at the start of the CPU cycle its pause falls before (ADR 0025).

use sfes_api::{DmaEvent, Observer};

use crate::bus::{
    Bus, PORT_2_GROUNDED,
    decode::{BBus, Target, decode, decode_b_bus},
};

/// The number of DMA channels.
pub(crate) const CHANNELS: usize = 8;
/// The registers each channel holds, `$43x0–$43xB`.
const REGISTERS: usize = 12;
/// `$43xF`, the mirror of `$43xB`.
const MIRROR: u16 = 0x0F;
/// The register `$43xF` mirrors.
const UNUSED: u16 = 0x0B;
/// Every channel register at power-on (ADR 0025, point 1).
const POWER_ON: u8 = 0xFF;
/// CPU cycle ends from the `$420B` write's own to the start of the pause: the write's and
/// the one more cycle the CPU gets (ADR 0025, point 3).
const START_DELAY: u8 = 2;
/// The pause first waits to a multiple of this many master cycles since reset.
const GRID: u64 = 8;
/// Master cycles of overhead for the whole transfer.
const TRANSFER_OVERHEAD: u64 = 8;
/// Master cycles of overhead for each channel.
const CHANNEL_OVERHEAD: u64 = 8;
/// Master cycles each byte takes.
const BYTE_CYCLES: u64 = 8;

/// `$43x0` bit 7: transfer direction, set for B bus to A bus.
const DIRECTION: u8 = 0x80;
/// `$43x0` bit 3: the A-bus address stays fixed.
const FIXED: u8 = 0x08;
/// `$43x0` bit 4: the A-bus address is decremented, when not fixed.
const DECREMENT: u8 = 0x10;
/// `$43x0` bits 2–0: the transfer mode.
const MODE: u8 = 0x07;

/// Each mode's B-bus offsets, of which the first `UNIT_LENGTHS` entries are used.
const UNIT_OFFSETS: [[u8; 4]; 8] = [
    [0, 0, 0, 0],
    [0, 1, 0, 0],
    [0, 0, 0, 0],
    [0, 0, 1, 1],
    [0, 1, 2, 3],
    [0, 1, 0, 1],
    [0, 0, 0, 0],
    [0, 0, 1, 1],
];
/// Each mode's unit length in bytes.
const UNIT_LENGTHS: [usize; 8] = [1, 2, 2, 4, 4, 4, 2, 4];

/// The B-bus offset of the byte at `index` in a transfer in `mode`: mode 0 is `p`, 1 is
/// `p, p+1`, 2 and 6 are `p, p`, 3 and 7 are `p, p, p+1, p+1`, 4 is `p, p+1, p+2, p+3`
/// and 5 is `p, p+1, p, p+1` (ADR 0025, point 5).
// @source: anomie-regs#43x0
// @source: anomie-regs#43x5
// @source: fullsnes#snesdmaandhdmachannel07registers
// @source: adr-0025
pub(crate) fn unit_offset(mode: u8, index: u32) -> u8 {
    let mode = usize::from(mode & MODE);
    let offsets = UNIT_OFFSETS.get(mode).copied().unwrap_or_default();
    let length = UNIT_LENGTHS.get(mode).copied().unwrap_or(1);
    offsets
        .get(index as usize % length)
        .copied()
        .unwrap_or_default()
}

/// One channel's registers, `$43x0–$43xB` by their low nibble.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Channel {
    registers: [u8; REGISTERS],
}

impl Channel {
    /// $43x0 `DMAPx`: direction, HDMA addressing, address step and mode.
    // @source: anomie-regs#43x0
    const fn control(&self) -> u8 {
        self.registers[0x0]
    }

    /// $43x1 `BBADx`: the B-bus address.
    // @source: anomie-regs#43x1
    const fn b_address(&self) -> u8 {
        self.registers[0x1]
    }

    /// $43x2–$43x4 `A1TxL`, `A1TxH`, `A1Bx`: the 24-bit A-bus address.
    // @source: anomie-regs#43x2
    const fn a_address(&self) -> u32 {
        u32::from_le_bytes([
            self.registers[0x2],
            self.registers[0x3],
            self.registers[0x4],
            0,
        ])
    }

    /// $43x5–$43x6 `DASxL`, `DASxH`: the byte count.
    // @source: anomie-regs#43x5
    const fn count(&self) -> u16 {
        u16::from_le_bytes([self.registers[0x5], self.registers[0x6]])
    }

    /// After a byte: steps `$43x2/3` by `$43x0` bits 4–3, wrapping at 16 bits and leaving
    /// `$43x4`, and decrements the count. Returns the count left (ADR 0025, point 5).
    // @source: anomie-regs#43x0
    // @source: anomie-regs#43x2
    // @source: anomie-regs#43x5
    // @source: anomie-regs#dma-and-hdma/dma
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0025
    const fn step(&mut self) -> u16 {
        let control = self.control();
        let address = u16::from_le_bytes([self.registers[0x2], self.registers[0x3]]);
        let address = if control & FIXED != 0 {
            address
        } else if control & DECREMENT != 0 {
            address.wrapping_sub(1)
        } else {
            address.wrapping_add(1)
        };
        let [low, high] = address.to_le_bytes();
        self.registers[0x2] = low;
        self.registers[0x3] = high;
        let count = self.count().wrapping_sub(1);
        let [low, high] = count.to_le_bytes();
        self.registers[0x5] = low;
        self.registers[0x6] = high;
        count
    }
}

/// The DMA channels and the `$420B` request not yet started.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Dma {
    channels: [Channel; CHANNELS],
    /// $420B MDMAEN as last written, until its transfer starts.
    requested: u8,
    /// CPU cycle ends left before the requested transfer's pause begins.
    start_in: u8,
}

impl Dma {
    /// The channels at power-on: every register `$FF`, nothing requested (ADR 0025,
    /// points 1 and 2).
    // @source: anomie-regs#420b
    // @source: anomie-regs#43x0
    // @source: anomie-regs#43x1
    // @source: anomie-regs#43x2
    // @source: anomie-regs#43x5
    // @source: anomie-regs#43xb
    // @source: fullsnes#snesiomap
    // @source: adr-0025
    pub(crate) const fn power_on() -> Self {
        Self {
            channels: [Channel {
                registers: [POWER_ON; REGISTERS],
            }; CHANNELS],
            requested: 0,
            start_in: 0,
        }
    }

    /// Reset: `$420B` is 0 and a transfer not yet started is dropped; the channel
    /// registers are kept (ADR 0025, points 1 and 2).
    // @source: anomie-regs#420b
    // @source: anomie-regs#43x0
    // @source: adr-0025
    pub(crate) const fn reset(&mut self) {
        self.requested = 0;
        self.start_in = 0;
    }

    /// The channel and register of `$43xN` at `offset`, or `None` for `$43xC–$43xE` and
    /// `$4380–$43FF`. `$43xF` is `$43xB` (ADR 0025, point 1).
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: anomie-regs#43xb
    // @source: adr-0025
    fn slot(offset: u16) -> Option<(usize, usize)> {
        if !(0x4300..=0x437F).contains(&offset) {
            return None;
        }
        let channel = usize::from((offset >> 4) & 0x07);
        let register = match offset & 0x0F {
            MIRROR => UNUSED,
            register @ 0x0..=UNUSED => register,
            _ => return None,
        };
        Some((channel, usize::from(register)))
    }

    /// Reads a channel register at `offset`, with no side effects, or `None` where open bus
    /// answers (ADR 0025, point 1).
    // @source: anomie-regs#43x0
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0025
    pub(crate) fn read(&self, offset: u16) -> Option<u8> {
        let (channel, register) = Self::slot(offset)?;
        self.channels
            .get(channel)
            .and_then(|channel| channel.registers.get(register))
            .copied()
    }

    /// Writes a channel register at `offset`; a write where open bus answers is lost
    /// (ADR 0025, point 1).
    // @source: anomie-regs#43x0
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0025
    pub(crate) fn write(&mut self, offset: u16, value: u8) {
        if let Some((channel, register)) = Self::slot(offset)
            && let Some(byte) = self
                .channels
                .get_mut(channel)
                .and_then(|channel| channel.registers.get_mut(register))
        {
            *byte = value;
        }
    }

    /// $420B MDMAEN: requests a transfer on the set channels, replacing one not yet
    /// started. Its pause begins two CPU cycle ends later (ADR 0025, points 2 and 3).
    // @source: anomie-regs#420b
    // @source: anomie-timing#s-cpu/dma
    // @source: fullsnes#snesdmaandhdmastartenableregisters
    // @source: adr-0025
    pub(crate) const fn write_enable(&mut self, value: u8) {
        self.requested = value;
        self.start_in = START_DELAY;
    }

    /// Counts the end of a CPU cycle towards the requested transfer's pause.
    // @source: anomie-timing#s-cpu/dma
    // @source: adr-0025
    pub(crate) const fn cycle_ended(&mut self) {
        self.start_in = self.start_in.saturating_sub(1);
    }

    /// The channels whose transfer's pause begins now, if any, clearing the request.
    // @source: anomie-timing#s-cpu/dma
    // @source: fullsnes#snesdmaandhdmastartenableregisters
    // @source: adr-0025
    const fn take_due(&mut self) -> Option<u8> {
        if self.requested != 0 && self.start_in == 0 {
            let channels = self.requested;
            self.requested = 0;
            Some(channels)
        } else {
            None
        }
    }
}

/// One byte's addresses: the A-bus address and the B-bus register.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Byte {
    a_address: u32,
    b_address: u8,
    from_b_bus: bool,
}

impl Bus {
    /// Runs the requested transfer if its pause begins at the start of this CPU cycle,
    /// which takes `clock` master cycles: aligns to 8 cycles since reset, runs the
    /// overheads and each set channel in order, then waits to a whole number of `clock`s
    /// since the pause began (ADR 0025, points 3 and 4).
    // @source: anomie-timing#s-cpu/dma
    // @source: anomie-regs#420b
    // @source: anomie-regs#dma-and-hdma/dma
    // @source: fullsnes#snesdmaandhdmastartenableregisters
    // @source: adr-0025
    pub(crate) fn start_due_dma<O: Observer>(&mut self, clock: u64, observer: &mut O) {
        let Some(channels) = self.dma.take_due() else {
            return;
        };
        let pause = self.now;
        let since_origin = pause.0 - self.timing.origin().0;
        self.advance(GRID - since_origin % GRID, observer);
        self.advance(TRANSFER_OVERHEAD, observer);
        for channel in 0..CHANNELS {
            if channels & (1 << channel) != 0 {
                self.advance(CHANNEL_OVERHEAD, observer);
                self.run_channel(channel, observer);
            }
        }
        let elapsed = self.now.0 - pause.0;
        self.advance(clock - elapsed % clock, observer);
    }

    /// Moves `channel`'s bytes until its count reaches 0, one 8-cycle slot each
    /// (ADR 0025, point 5).
    // @source: anomie-regs#dma-and-hdma/dma
    // @source: anomie-regs#43x5
    // @source: anomie-timing#s-cpu/dma
    // @source: adr-0025
    fn run_channel<O: Observer>(&mut self, channel: usize, observer: &mut O) {
        let mut index = 0;
        loop {
            let Some(registers) = self.dma.channels.get(channel).copied() else {
                return;
            };
            let control = registers.control();
            let byte = Byte {
                a_address: registers.a_address(),
                b_address: registers
                    .b_address()
                    .wrapping_add(unit_offset(control, index)),
                from_b_bus: control & DIRECTION != 0,
            };
            self.move_byte(channel, byte, observer);
            index = index.wrapping_add(1);
            let left = match self.dma.channels.get_mut(channel) {
                Some(registers) => registers.step(),
                None => 0,
            };
            self.advance(BYTE_CYCLES, observer);
            if left == 0 {
                return;
            }
        }
    }

    /// Moves one byte between the A bus and the B bus at the start of its slot, places it
    /// on the data bus and reports it (ADR 0025, points 5 to 8 and 11).
    // @source: anomie-regs#dma-and-hdma/dma
    // @source: anomie-regs#2180
    // @source: anomie-regs#2181
    // @source: fullsnes#snesmemoryworkramaccess
    // @source: fullsnes#snesunpredictablethings
    // @source: anomie-memmap#hardware/data-bus
    // @source: adr-0025
    fn move_byte<O: Observer>(&mut self, channel: usize, byte: Byte, observer: &mut O) {
        let at = self.now;
        let a_target = dma_a_target(decode(self.cartridge.map(), byte.a_address));
        let b_device = decode_b_bus(byte.b_address);
        // WRAM through both buses: the B-bus side is not answered (ADR 0025, point 7).
        let wram_twice = matches!(a_target, Target::Wram(_))
            && matches!(
                b_device,
                BBus::WramData
                    | BBus::WramAddressLow
                    | BBus::WramAddressMiddle
                    | BBus::WramAddressHigh
            );
        let value = if byte.from_b_bus {
            let read = if wram_twice {
                None
            } else {
                self.read_target(Target::BBus(b_device))
            };
            let value = read.unwrap_or(self.open_bus_value);
            self.open_bus_value = value;
            self.write_dma_a(a_target, value);
            value
        } else {
            let value = self.read_dma_a(a_target).unwrap_or(self.open_bus_value);
            self.open_bus_value = value;
            if !wram_twice {
                self.write_b_bus(b_device, value);
            }
            value
        };
        if O::ACTIVE {
            let b_address = 0x2100 | u32::from(byte.b_address);
            let (source, destination) = if byte.from_b_bus {
                (b_address, byte.a_address)
            } else {
                (byte.a_address, b_address)
            };
            observer.on_dma(&DmaEvent {
                channel: u8::try_from(channel).unwrap_or_default(),
                hdma: false,
                source,
                destination,
                value,
                at,
            });
        }
    }

    /// A DMA read through the A bus: `$4000–$41FF` is open bus, with `$4017` bits 4–2
    /// set and no pad clocked; every other target reads as for the CPU (ADR 0025,
    /// point 6).
    // @source: fullsnes#snesunpredictablethings
    // @source: anomie-timing#s-cpu/dma
    // @source: adr-0025
    fn read_dma_a(&mut self, target: Target) -> Option<u8> {
        match target {
            Target::Cpu(0x4017) => Some(self.open_bus_value | PORT_2_GROUNDED),
            Target::Cpu(0x4000..=0x41FF) => None,
            target => self.read_target(target),
        }
    }

    /// A DMA write through the A bus: lost in `$4000–$41FF`; every other target is
    /// written as by the CPU (ADR 0025, point 6).
    // @source: fullsnes#snesunpredictablethings
    // @source: anomie-timing#s-cpu/dma
    // @source: adr-0025
    fn write_dma_a(&mut self, target: Target, value: u8) {
        match target {
            Target::Cpu(0x4000..=0x41FF) => {}
            target => self.write_target(target, value),
        }
    }
}

/// What a DMA's A-bus address reaches: the B-bus registers, `$420B`, `$420C` and
/// `$4300–$437F` are out of its reach and answer open bus (ADR 0025, point 6).
// @source: anomie-regs#dma-and-hdma/dma
// @source: anomie-regs#43x2
// @source: fullsnes#snesunpredictablethings
// @source: adr-0025
const fn dma_a_target(target: Target) -> Target {
    match target {
        Target::BBus(_) | Target::Cpu(0x420B | 0x420C | 0x4300..=0x437F) => Target::Unanswered,
        target => target,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: anomie-regs#43x0
    // @source: anomie-regs#43x5
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0025
    #[test]
    fn every_mode_repeats_its_b_bus_pattern() {
        let expected: [[u8; 8]; 8] = [
            [0, 0, 0, 0, 0, 0, 0, 0],
            [0, 1, 0, 1, 0, 1, 0, 1],
            [0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 1, 1, 0, 0, 1, 1],
            [0, 1, 2, 3, 0, 1, 2, 3],
            [0, 1, 0, 1, 0, 1, 0, 1],
            [0, 0, 0, 0, 0, 0, 0, 0],
            [0, 0, 1, 1, 0, 0, 1, 1],
        ];
        for (mode, pattern) in (0..8).zip(expected) {
            let offsets = core::array::from_fn::<u8, 8, _>(|index| {
                unit_offset(mode, u32::try_from(index).unwrap_or_default())
            });
            assert_eq!(offsets, pattern, "mode {mode}");
        }
        // Only bits 2-0 of $43x0 select the mode.
        assert_eq!(unit_offset(0xFC, 3), 3);
    }

    // @source: anomie-regs#43x0
    // @source: anomie-regs#43x2
    // @source: anomie-regs#43x5
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0025
    #[test]
    fn a_step_moves_the_address_by_bits_4_and_3_and_counts_down() {
        // (control, address after one step from $FFFF)
        for (control, address) in [
            (0x00, 0x0000),
            (0x10, 0xFFFE),
            (0x08, 0xFFFF),
            (0x18, 0xFFFF),
        ] {
            let mut channel = Channel {
                registers: [control, 0x18, 0xFF, 0xFF, 0x7E, 0x01, 0x00, 0, 0, 0, 0, 0],
            };
            assert_eq!(channel.step(), 0);
            assert_eq!(
                channel.a_address(),
                0x7E_0000 | address,
                "control {control:#04x}"
            );
        }
        // A count of 0 wraps to $FFFF on the first byte: $10000 bytes in all.
        let mut channel = Channel {
            registers: [0; REGISTERS],
        };
        assert_eq!(channel.step(), 0xFFFF);
    }

    // @source: anomie-regs#420b
    // @source: anomie-regs#43x0
    // @source: anomie-regs#43xb
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: fullsnes#snesiomap
    // @source: adr-0025
    #[test]
    fn the_channel_registers_read_back_with_43xf_mirroring_43xb() {
        let mut dma = Dma::power_on();
        for channel in 0..8 {
            for register in 0..=0xB {
                assert_eq!(dma.read(0x4300 | (channel << 4) | register), Some(0xFF));
            }
        }
        dma.write(0x4375, 0x12);
        dma.write(0x431F, 0x34);
        assert_eq!(dma.read(0x4375), Some(0x12));
        assert_eq!(dma.read(0x431B), Some(0x34));
        assert_eq!(dma.read(0x431F), Some(0x34));
        for open in [0x430C, 0x430D, 0x437E, 0x4380, 0x43FF] {
            dma.write(open, 0x56);
            assert_eq!(dma.read(open), None, "{open:#06x}");
        }
        // Reset keeps the registers and drops a request.
        dma.write_enable(0x01);
        dma.reset();
        assert_eq!(dma.read(0x4375), Some(0x12));
        assert_eq!(dma.take_due(), None);
    }

    // @source: anomie-timing#s-cpu/dma
    // @source: adr-0025
    #[test]
    fn a_request_is_due_after_two_cycle_ends() {
        let mut dma = Dma::power_on();
        dma.write_enable(0x05);
        dma.cycle_ended();
        assert_eq!(dma.take_due(), None);
        dma.cycle_ended();
        assert_eq!(dma.take_due(), Some(0x05));
        assert_eq!(dma.take_due(), None);
        // A write of 0 requests nothing.
        dma.write_enable(0);
        dma.cycle_ended();
        dma.cycle_ended();
        assert_eq!(dma.take_due(), None);
    }
}
