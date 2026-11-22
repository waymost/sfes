// @source: anomie-regs#420b
// @source: anomie-regs#420c
// @source: anomie-regs#43x0
// @source: anomie-regs#43x1
// @source: anomie-regs#43x2
// @source: anomie-regs#43x5
// @source: anomie-regs#43xb
// @source: anomie-regs#dma-and-hdma/dma
// @source: anomie-timing#s-cpu/dma
// @source: anomie-timing#s-cpu/hdma
// @source: fullsnes#snesdmaandhdmachannel07registers
// @source: fullsnes#snesdmaandhdmastartenableregisters
// @source: fullsnes#snesunpredictablethings
// @source: adr-0025
// @source: adr-0026
//! General DMA and HDMA: the eight channels' registers `$43x0–$43xB`, the `$420B` request,
//! `$420C`, and the pause the bus runs at the start of a CPU cycle for a transfer, an HDMA
//! step or both (ADR 0025, ADR 0026). The HDMA steps are in [`hdma`].

mod hdma;

use sfes_api::{DmaEvent, Observer};

use crate::{
    bus::{
        Bus, PORT_2_GROUNDED, Registers,
        decode::{BBus, CpuRegister, Speed, Target, decode, decode_b_bus},
        ports::{Port, PortRegister},
    },
    timing::HdmaStep,
};

/// The number of DMA channels.
const CHANNELS: usize = 8;
/// The registers each channel holds, `$43x0–$43xB`.
const REGISTERS: usize = 12;
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

/// Each mode's unit: the B-bus offsets of its bytes, in order. Mode 0 is `p`, 1 is
/// `p, p+1`, 2 and 6 are `p, p`, 3 and 7 are `p, p, p+1, p+1`, 4 is `p, p+1, p+2, p+3`
/// and 5 is `p, p+1, p, p+1` (ADR 0025, point 5).
const UNITS: [&[u8]; 8] = [
    &[0],
    &[0, 1],
    &[0, 0],
    &[0, 0, 1, 1],
    &[0, 1, 2, 3],
    &[0, 1, 0, 1],
    &[0, 0],
    &[0, 0, 1, 1],
];

/// The unit of the transfer mode in `$43x0` value `control` (ADR 0025, point 5).
// @source: anomie-regs#43x0
// @source: anomie-regs#43x5
// @source: fullsnes#snesdmaandhdmachannel07registers
// @source: adr-0025
const fn unit(control: u8) -> &'static [u8] {
    UNITS[(control & MODE) as usize]
}

/// The B-bus offset of the byte at `index` in a transfer whose `$43x0` is `control`: the
/// mode's unit, repeated (ADR 0025, point 5).
// @source: anomie-regs#43x0
// @source: anomie-regs#43x5
// @source: fullsnes#snesdmaandhdmachannel07registers
// @source: adr-0025
const fn unit_offset(control: u8, index: usize) -> u8 {
    let unit = unit(control);
    unit[index % unit.len()]
}

/// One of the eight DMA channels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DmaChannel(u8);

impl DmaChannel {
    /// Every channel, in the order a transfer and HDMA serve them.
    const ALL: [Self; CHANNELS] = [
        Self(0),
        Self(1),
        Self(2),
        Self(3),
        Self(4),
        Self(5),
        Self(6),
        Self(7),
    ];

    /// Channel `number`; only its low three bits count.
    // @source: anomie-regs#43x0
    pub(crate) const fn new(number: u8) -> Self {
        Self(number & 0x07)
    }

    /// The channel's number, 0–7.
    // @source: anomie-regs#43x0
    pub(crate) const fn number(self) -> u8 {
        self.0
    }
}

/// A set of channels, one bit each: what `$420B` and `$420C` hold, and HDMA's flags.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ChannelSet(u8);

impl ChannelSet {
    /// No channel.
    const EMPTY: Self = Self(0);

    /// The channels whose bits are set in `bits`, as `$420B` and `$420C` take them.
    // @source: anomie-regs#420b
    // @source: anomie-regs#420c
    const fn from_bits(bits: u8) -> Self {
        Self(bits)
    }

    /// Whether the set has no channel.
    // @source: adr-0025
    const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Whether `channel` is in the set.
    // @source: adr-0025
    const fn contains(self, channel: DmaChannel) -> bool {
        self.0 & (1 << channel.0) != 0
    }

    /// Adds `channel`.
    // @source: adr-0026
    const fn insert(&mut self, channel: DmaChannel) {
        self.0 |= 1 << channel.0;
    }

    /// Removes `channel`.
    // @source: adr-0026
    const fn remove(&mut self, channel: DmaChannel) {
        self.0 &= !(1 << channel.0);
    }

    /// The channels in this set and not in `other`.
    // @source: adr-0026
    const fn without(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// The channels in the set, lowest first.
    // @source: adr-0025
    fn iter(self) -> impl Iterator<Item = DmaChannel> {
        DmaChannel::ALL
            .into_iter()
            .filter(move |channel| self.contains(*channel))
    }

    /// The highest channel in the set.
    // @source: adr-0026
    fn last(self) -> Option<DmaChannel> {
        self.iter().last()
    }
}

/// A channel register, `$43x0–$43xB`, by its low nibble (`$43xF` is `$43xB`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(usize)]
pub(crate) enum ChannelRegister {
    /// $43x0 `DMAPx`: direction, HDMA addressing, address step and mode.
    Control = 0x0,
    /// $43x1 `BBADx`: the B-bus address.
    BAddress = 0x1,
    /// $43x2 `A1TxL`: the A-bus address's low byte, and the HDMA table's start.
    AAddressLow = 0x2,
    /// $43x3 `A1TxH`: the A-bus address's high byte.
    AAddressHigh = 0x3,
    /// $43x4 `A1Bx`: the A-bus bank, and the HDMA table's.
    ABank = 0x4,
    /// $43x5 `DASxL`: the byte count's low byte, or HDMA's indirect address.
    CountLow = 0x5,
    /// $43x6 `DASxH`: the byte count's high byte.
    CountHigh = 0x6,
    /// $43x7 `DASBx`: HDMA's indirect bank.
    IndirectBank = 0x7,
    /// $43x8 `A2AxL`: HDMA's current table address, low byte.
    TableLow = 0x8,
    /// $43x9 `A2AxH`: HDMA's current table address, high byte.
    TableHigh = 0x9,
    /// $43xA `NLTRx`: HDMA's repeat flag and line count.
    LineCounter = 0xA,
    /// $43xB and $43xF: a byte with no function.
    Unused = 0xB,
}

/// `$420B`, `$420C` and the channel registers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DmaRegister {
    /// $420B MDMAEN: starts a transfer on the set channels.
    Enable,
    /// $420C HDMAEN: the channels HDMA works on.
    HdmaEnable,
    /// $43x0–$43xB, and $43xF: one channel's register.
    Channel(DmaChannel, ChannelRegister),
}

/// One channel's registers, `$43x0–$43xB`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ChannelRegisters {
    registers: [u8; REGISTERS],
}

impl ChannelRegisters {
    /// The value of `register`.
    // @source: anomie-regs#43x0
    // @source: fullsnes#snesdmaandhdmachannel07registers
    const fn get(&self, register: ChannelRegister) -> u8 {
        self.registers[register as usize]
    }

    /// Sets `register`.
    // @source: anomie-regs#43x0
    // @source: fullsnes#snesdmaandhdmachannel07registers
    const fn set(&mut self, register: ChannelRegister, value: u8) {
        self.registers[register as usize] = value;
    }

    /// The 16-bit value of `low` and the register after it, which holds the high byte:
    /// `$43x2/3`, `$43x5/6` or `$43x8/9`.
    // @source: anomie-regs#43x2
    // @source: anomie-regs#43x5
    // @source: anomie-regs#43x8
    const fn word(&self, low: ChannelRegister) -> u16 {
        let index = low as usize;
        u16::from_le_bytes([self.registers[index], self.registers[index + 1]])
    }

    /// Sets `low` and the register after it to `value`.
    // @source: anomie-regs#43x2
    // @source: anomie-regs#43x5
    // @source: anomie-regs#43x8
    const fn set_word(&mut self, low: ChannelRegister, value: u16) {
        let index = low as usize;
        let [low_byte, high_byte] = value.to_le_bytes();
        self.registers[index] = low_byte;
        self.registers[index + 1] = high_byte;
    }

    /// $43x0 `DMAPx`: direction, HDMA addressing, address step and mode.
    // @source: anomie-regs#43x0
    const fn control(&self) -> u8 {
        self.get(ChannelRegister::Control)
    }

    /// $43x2–$43x4 `A1TxL`, `A1TxH`, `A1Bx`: the 24-bit A-bus address.
    // @source: anomie-regs#43x2
    const fn a_address(&self) -> u32 {
        let [low, high] = self.word(ChannelRegister::AAddressLow).to_le_bytes();
        u32::from_le_bytes([low, high, self.get(ChannelRegister::ABank), 0])
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
        let address = self.word(ChannelRegister::AAddressLow);
        let address = if control & FIXED != 0 {
            address
        } else if control & DECREMENT != 0 {
            address.wrapping_sub(1)
        } else {
            address.wrapping_add(1)
        };
        self.set_word(ChannelRegister::AAddressLow, address);
        let count = self.word(ChannelRegister::CountLow).wrapping_sub(1);
        self.set_word(ChannelRegister::CountLow, count);
        count
    }
}

/// The DMA channels, the `$420B` request not yet started and HDMA's state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Dma {
    channels: [ChannelRegisters; CHANNELS],
    /// $420B MDMAEN as last written, until its transfer starts.
    requested: ChannelSet,
    /// CPU cycle ends left before the requested transfer's pause begins.
    start_in: u8,
    /// $420C HDMAEN: the channels HDMA works on (ADR 0026, point 1).
    hdma_enabled: ChannelSet,
    /// Each channel's `DoTransfer` flag (ADR 0026, point 1).
    do_transfer: ChannelSet,
    /// The channels terminated for this frame (ADR 0026, point 1).
    terminated: ChannelSet,
    /// The HDMA step that fell due and has not yet run (ADR 0026, point 3).
    hdma_due: Option<HdmaStep>,
}

impl Dma {
    /// The channels at power-on: every register `$FF`, nothing requested (ADR 0025,
    /// points 1 and 2), and no HDMA channel enabled, flagged or due (ADR 0026, point 1).
    // @source: anomie-regs#420b
    // @source: anomie-regs#420c
    // @source: anomie-regs#43x0
    // @source: anomie-regs#43x1
    // @source: anomie-regs#43x2
    // @source: anomie-regs#43x5
    // @source: anomie-regs#43xb
    // @source: fullsnes#snesiomap
    // @source: adr-0025
    // @source: adr-0026
    pub(crate) const fn power_on() -> Self {
        Self {
            channels: [ChannelRegisters {
                registers: [POWER_ON; REGISTERS],
            }; CHANNELS],
            requested: ChannelSet::EMPTY,
            start_in: 0,
            hdma_enabled: ChannelSet::EMPTY,
            do_transfer: ChannelSet::EMPTY,
            terminated: ChannelSet::EMPTY,
            hdma_due: None,
        }
    }

    /// Reset: `$420B` is 0 and a transfer not yet started is dropped; the channel
    /// registers are kept (ADR 0025, points 1 and 2). `$420C` is 0, HDMA's flags clear and
    /// a step not yet run is dropped (ADR 0026, point 1).
    // @source: anomie-regs#420b
    // @source: anomie-regs#420c
    // @source: anomie-regs#43x0
    // @source: adr-0025
    // @source: adr-0026
    pub(crate) const fn reset(&mut self) {
        self.requested = ChannelSet::EMPTY;
        self.start_in = 0;
        self.hdma_enabled = ChannelSet::EMPTY;
        self.do_transfer = ChannelSet::EMPTY;
        self.terminated = ChannelSet::EMPTY;
        self.hdma_due = None;
    }

    /// The channel and register of `$43xN` at `offset`, or `None` for `$43xC–$43xE` and
    /// `$4380–$43FF`. `$43xF` is `$43xB` (ADR 0025, point 1).
    // @source: anomie-regs#43xb
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0025
    pub(crate) const fn slot(offset: u16) -> Option<(DmaChannel, ChannelRegister)> {
        if offset < 0x4300 || offset > 0x437F {
            return None;
        }
        let [low, _] = offset.to_le_bytes();
        let register = match low & 0x0F {
            0x0 => ChannelRegister::Control,
            0x1 => ChannelRegister::BAddress,
            0x2 => ChannelRegister::AAddressLow,
            0x3 => ChannelRegister::AAddressHigh,
            0x4 => ChannelRegister::ABank,
            0x5 => ChannelRegister::CountLow,
            0x6 => ChannelRegister::CountHigh,
            0x7 => ChannelRegister::IndirectBank,
            0x8 => ChannelRegister::TableLow,
            0x9 => ChannelRegister::TableHigh,
            0xA => ChannelRegister::LineCounter,
            0xB | 0xF => ChannelRegister::Unused,
            _ => return None,
        };
        Some((DmaChannel::new(low >> 4), register))
    }

    /// `channel`'s registers.
    // @source: anomie-regs#43x0
    const fn channel(&self, channel: DmaChannel) -> &ChannelRegisters {
        &self.channels[channel.0 as usize]
    }

    /// `channel`'s registers, writable.
    // @source: anomie-regs#43x0
    const fn channel_mut(&mut self, channel: DmaChannel) -> &mut ChannelRegisters {
        &mut self.channels[channel.0 as usize]
    }

    /// $420B MDMAEN: requests a transfer on the set channels, replacing one not yet
    /// started. Its pause begins two CPU cycle ends later (ADR 0025, points 2 and 3).
    // @source: anomie-regs#420b
    // @source: anomie-timing#s-cpu/dma
    // @source: fullsnes#snesdmaandhdmastartenableregisters
    // @source: adr-0025
    const fn write_enable(&mut self, value: u8) {
        self.requested = ChannelSet::from_bits(value);
        self.start_in = START_DELAY;
    }

    /// $420C HDMAEN: the channels HDMA works on from its next step (ADR 0026, points 1
    /// and 7).
    // @source: anomie-regs#420c
    // @source: fullsnes#snesdmaandhdmastartenableregisters
    // @source: adr-0026
    const fn write_hdma_enable(&mut self, value: u8) {
        self.hdma_enabled = ChannelSet::from_bits(value);
    }

    /// $420C HDMAEN as last written.
    // @source: anomie-regs#420c
    #[cfg(test)]
    pub(crate) const fn hdma_enabled(&self) -> u8 {
        self.hdma_enabled.0
    }

    /// Writes the register at `offset` in `$420B–$420C` or `$4300–$437F` without the
    /// CPU, for tests.
    // @source: anomie-regs#420c
    // @source: anomie-regs#43x0
    #[cfg(test)]
    pub(crate) fn write_at(&mut self, offset: u16, value: u8) {
        let register = match offset {
            0x420B => Some(DmaRegister::Enable),
            0x420C => Some(DmaRegister::HdmaEnable),
            _ => Self::slot(offset)
                .map(|(channel, register)| DmaRegister::Channel(channel, register)),
        };
        if let Some(register) = register {
            self.write(register, value);
        }
    }

    /// Reads the channel register at `offset`, for tests; `None` where open bus answers.
    // @source: anomie-regs#43x0
    #[cfg(test)]
    pub(crate) fn peek_at(&self, offset: u16) -> Option<u8> {
        let (channel, register) = Self::slot(offset)?;
        self.peek(DmaRegister::Channel(channel, register))
    }

    /// Marks the HDMA step `step` due, to run at the next point the bus can pause
    /// (ADR 0026, point 3).
    // @source: anomie-timing#s-cpu/hdma
    // @source: adr-0026
    pub(crate) const fn hdma_fell_due(&mut self, step: HdmaStep) {
        self.hdma_due = Some(step);
    }

    /// Whether an HDMA step is due with a channel to work on: an enabled channel for setup,
    /// one enabled and not terminated for a line (ADR 0026, points 3, 5 and 6).
    // @source: anomie-regs#dma-and-hdma/hdma
    // @source: anomie-timing#s-cpu/hdma
    // @source: adr-0026
    const fn hdma_has_work(&self) -> bool {
        match self.hdma_due {
            Some(HdmaStep::Setup) => !self.hdma_enabled.is_empty(),
            Some(HdmaStep::Line) => !self.hdma_enabled.without(self.terminated).is_empty(),
            None => false,
        }
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
    const fn take_due(&mut self) -> Option<ChannelSet> {
        if !self.requested.is_empty() && self.start_in == 0 {
            let channels = self.requested;
            self.requested = ChannelSet::EMPTY;
            Some(channels)
        } else {
            None
        }
    }
}

impl Registers for Dma {
    type Register = DmaRegister;

    /// A channel register reads back as written, with no side effects; `$420B` and `$420C`
    /// are write-only (ADR 0025, point 1).
    // @source: anomie-regs#420b
    // @source: anomie-regs#420c
    // @source: anomie-regs#43x0
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0025
    fn peek(&self, register: DmaRegister) -> Option<u8> {
        match register {
            DmaRegister::Channel(channel, register) => Some(self.channel(channel).get(register)),
            DmaRegister::Enable | DmaRegister::HdmaEnable => None,
        }
    }

    /// `$420B`, `$420C` or a channel register (ADR 0025, points 1 to 3; ADR 0026, point 1).
    // @source: anomie-regs#420b
    // @source: anomie-regs#420c
    // @source: anomie-regs#43x0
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: adr-0025
    // @source: adr-0026
    fn write(&mut self, register: DmaRegister, value: u8) {
        match register {
            DmaRegister::Enable => self.write_enable(value),
            DmaRegister::HdmaEnable => self.write_hdma_enable(value),
            DmaRegister::Channel(channel, register) => {
                self.channel_mut(channel).set(register, value);
            }
        }
    }
}

/// One byte's addresses: the A-bus address and the B-bus register, its direction and
/// whether HDMA moves it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Byte {
    a_address: u32,
    b_address: u8,
    from_b_bus: bool,
    hdma: bool,
}

impl Bus {
    /// Runs the pause that begins at the start of this CPU cycle, which runs at `speed`, if
    /// a requested transfer or an HDMA step is due: aligns to 8 cycles since reset, runs the
    /// HDMA step, then the transfer's overhead and each set channel in order, then waits to
    /// a whole number of CPU cycles at `speed` since the pause began (ADR 0025, points 3
    /// and 4; ADR 0026, point 3).
    // @source: anomie-regs#420b
    // @source: anomie-regs#dma-and-hdma
    // @source: anomie-regs#dma-and-hdma/dma
    // @source: anomie-timing#s-cpu/dma
    // @source: anomie-timing#s-cpu/hdma
    // @source: fullsnes#snesdmaandhdmastartenableregisters
    // @source: adr-0025
    // @source: adr-0026
    pub(crate) fn start_due_pause<O: Observer>(&mut self, speed: Speed, observer: &mut O) {
        let channels = self.dma.take_due();
        if channels.is_none() && !self.dma.hdma_has_work() {
            // A step with no channel to work on takes no time (ADR 0026, point 3).
            self.run_due_hdma(observer);
            return;
        }
        let pause = self.now;
        let since_origin = pause - self.timing.origin();
        self.advance_paused(GRID - since_origin % GRID, observer);
        if let Some(channels) = channels {
            self.advance_paused(TRANSFER_OVERHEAD, observer);
            for channel in channels.iter() {
                let worked = self.advance_paused(CHANNEL_OVERHEAD, observer);
                if !worked.contains(channel) {
                    self.run_channel(channel, observer);
                }
            }
        }
        // An HDMA step that falls due in the closing wait pauses the CPU on its own.
        let clock = speed.cycles();
        let elapsed = self.now - pause;
        self.advance(clock - elapsed % clock, observer);
    }

    /// Advances the clock by `cycles` within a pause, then runs an HDMA step that fell due,
    /// with no wait of its own. Returns the channels the step worked on (ADR 0026, points 3
    /// and 9).
    // @source: anomie-regs#420b
    // @source: anomie-regs#dma-and-hdma
    // @source: adr-0025
    // @source: adr-0026
    fn advance_paused<O: Observer>(&mut self, cycles: u64, observer: &mut O) -> ChannelSet {
        self.advance(cycles, observer);
        self.run_due_hdma(observer)
    }

    /// Moves `channel`'s bytes until its count reaches 0, one 8-cycle slot each
    /// (ADR 0025, point 5), or until an HDMA step works on the same channel (ADR 0026,
    /// point 9).
    // @source: anomie-regs#420b
    // @source: anomie-regs#43x5
    // @source: anomie-regs#dma-and-hdma
    // @source: anomie-regs#dma-and-hdma/dma
    // @source: anomie-timing#s-cpu/dma
    // @source: adr-0025
    // @source: adr-0026
    fn run_channel<O: Observer>(&mut self, channel: DmaChannel, observer: &mut O) {
        let mut index = 0;
        loop {
            let registers = *self.dma.channel(channel);
            let control = registers.control();
            let byte = Byte {
                a_address: registers.a_address(),
                b_address: registers
                    .get(ChannelRegister::BAddress)
                    .wrapping_add(unit_offset(control, index)),
                from_b_bus: control & DIRECTION != 0,
                hdma: false,
            };
            self.move_byte(channel, byte, observer);
            index = index.wrapping_add(1);
            let left = self.dma.channel_mut(channel).step();
            let worked = self.advance_paused(BYTE_CYCLES, observer);
            if left == 0 || worked.contains(channel) {
                return;
            }
        }
    }

    /// Moves one byte between the A bus and the B bus at the start of its slot, places it
    /// on the data bus and reports it (ADR 0025, points 5 to 8 and 11; ADR 0026, points 8
    /// and 10).
    // @source: anomie-memmap#hardware/data-bus
    // @source: anomie-regs#2180
    // @source: anomie-regs#2181
    // @source: anomie-regs#dma-and-hdma/dma
    // @source: fullsnes#snesmemoryworkramaccess
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0025
    // @source: adr-0026
    fn move_byte<O: Observer>(&mut self, channel: DmaChannel, byte: Byte, observer: &mut O) {
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
                channel: channel.number(),
                hdma: byte.hdma,
                source,
                destination,
                value,
                at,
            });
        }
    }

    /// A DMA read through the A bus: `$4000–$41FF` is open bus, with `$4017` bits 4–2
    /// set and no pad clocked; every other target reads as for the CPU (ADR 0025,
    /// point 6). `$4016` and `$4017` are the only registers there.
    // @source: anomie-timing#s-cpu/dma
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0025
    fn read_dma_a(&mut self, target: Target) -> Option<u8> {
        match target {
            Target::Cpu(CpuRegister::Ports(PortRegister::Serial(Port::Two))) => {
                Some(self.open_bus_value | PORT_2_GROUNDED)
            }
            Target::Cpu(CpuRegister::Ports(PortRegister::Serial(Port::One))) => None,
            target => self.read_target(target),
        }
    }

    /// A DMA write through the A bus: lost in `$4000–$41FF`, where `$4016` is the only
    /// register; every other target is written as by the CPU (ADR 0025, point 6).
    // @source: anomie-timing#s-cpu/dma
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0025
    fn write_dma_a(&mut self, target: Target, value: u8) {
        match target {
            Target::Cpu(CpuRegister::Ports(PortRegister::Serial(_))) => {}
            target => self.write_target(target, value),
        }
    }
}

/// What a DMA's A-bus address reaches: the B-bus registers, `$420B`, `$420C` and
/// `$4300–$437F` are out of its reach and answer open bus (ADR 0025, point 6).
// @source: anomie-regs#43x2
// @source: anomie-regs#dma-and-hdma/dma
// @source: fullsnes#snesunpredictablethings
// @source: adr-0025
const fn dma_a_target(target: Target) -> Target {
    match target {
        Target::BBus(_) | Target::Cpu(CpuRegister::Dma(_)) => Target::Unanswered,
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
            let offsets = core::array::from_fn::<u8, 8, _>(|index| unit_offset(mode, index));
            assert_eq!(offsets, pattern, "mode {mode}");
        }
        // Only bits 2-0 of $43x0 select the mode.
        assert_eq!(unit_offset(0xFC, 3), 3);
        assert_eq!(
            UNITS.map(<[u8]>::len),
            [1, 2, 2, 4, 4, 4, 2, 4],
            "unit lengths"
        );
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
            let mut channel = ChannelRegisters {
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
        let mut channel = ChannelRegisters {
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
                assert_eq!(dma.peek_at(0x4300 | (channel << 4) | register), Some(0xFF));
            }
        }
        dma.write_at(0x4375, 0x12);
        dma.write_at(0x431F, 0x34);
        assert_eq!(dma.peek_at(0x4375), Some(0x12));
        assert_eq!(dma.peek_at(0x431B), Some(0x34));
        assert_eq!(dma.peek_at(0x431F), Some(0x34));
        for open in [0x430C, 0x430D, 0x437E, 0x4380, 0x43FF] {
            dma.write_at(open, 0x56);
            assert_eq!(dma.peek_at(open), None, "{open:#06x}");
        }
        // Reset keeps the registers and drops a request.
        dma.write_enable(0x01);
        dma.reset();
        assert_eq!(dma.peek_at(0x4375), Some(0x12));
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
        assert_eq!(dma.take_due(), Some(ChannelSet::from_bits(0x05)));
        assert_eq!(dma.take_due(), None);
        // A write of 0 requests nothing.
        dma.write_enable(0);
        dma.cycle_ended();
        dma.cycle_ended();
        assert_eq!(dma.take_due(), None);
    }

    // @source: anomie-regs#420b
    // @source: adr-0025
    #[test]
    fn a_channel_set_lists_its_channels_lowest_first() {
        let set = ChannelSet::from_bits(0b1010_0100);
        let numbers = set
            .iter()
            .map(DmaChannel::number)
            .collect::<alloc::vec::Vec<_>>();
        assert_eq!(numbers, [2, 5, 7]);
        assert_eq!(set.last(), Some(DmaChannel::new(7)));
        assert_eq!(ChannelSet::EMPTY.last(), None);
        assert_eq!(
            set.without(ChannelSet::from_bits(0x80)),
            ChannelSet::from_bits(0x24)
        );
    }
}
