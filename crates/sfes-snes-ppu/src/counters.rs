// @source: anomie-regs#2137
// @source: anomie-regs#213c
// @source: anomie-regs#213d
// @source: fullsnes#snespputimersandstatus
// @source: adr-0021
// @source: adr-0028
// @source: adr-0029
//! The H/V counter latch read at `$213C` OPHCT and `$213D` OPVCT (ADR 0021, point 7), with
//! PPU2 open bus in the high byte's bits 7–1 (ADR 0028, point 4), and the latch flag of
//! `$213F` bit 6 (ADR 0029, point 10).

/// OPHCT and OPVCT at power-on (fullsnes#snesiomap).
const COUNTER_POWER_ON: u16 = 0x1FF;
/// The bit of the high byte that holds bit 8 of a counter; the rest is PPU2 open bus.
const COUNTER_HIGH: u8 = 0x01;

/// One latched counter and its low/high flip-flop.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Counter {
    /// The latched value, 9 bits.
    value: u16,
    /// The next read returns the high byte.
    high_next: bool,
}

impl Counter {
    /// The byte a read returns, with `open_bus` in bits 7–1 of the high byte.
    // @source: anomie-regs#213c
    // @source: fullsnes#snespputimersandstatus
    const fn byte(self, open_bus: u8) -> u8 {
        let [low, high] = self.value.to_le_bytes();
        if self.high_next {
            (high & COUNTER_HIGH) | (open_bus & !COUNTER_HIGH)
        } else {
            low
        }
    }
}

/// Which of the two latched counters a read names.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CounterRegister {
    /// $213C OPHCT: the latched H.
    Horizontal,
    /// $213D OPVCT: the latched V.
    Vertical,
}

/// The latched counters, their flip-flops and the latch flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CounterLatch {
    /// $213C OPHCT: the latched H.
    horizontal: Counter,
    /// $213D OPVCT: the latched V.
    vertical: Counter,
    /// $213F STAT78 bit 6: the counters were latched since `$213F` was last read.
    latched: bool,
}

impl CounterLatch {
    /// The latch at power-on: both counters `$1FF` and both flip-flops on the low byte
    /// (ADR 0021, point 8). Reset keeps all of it.
    // @source: fullsnes#snesiomap
    // @source: adr-0021
    pub(crate) const fn power_on() -> Self {
        let counter = Counter {
            value: COUNTER_POWER_ON,
            high_next: false,
        };
        Self {
            horizontal: counter,
            vertical: counter,
            latched: false,
        }
    }

    /// Latches `line` and `dot` and sets the latch flag. The flip-flops are kept.
    // @source: anomie-regs#213c
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0021
    pub(crate) const fn latch(&mut self, line: u16, dot: u16) {
        self.horizontal.value = dot;
        self.vertical.value = line;
        self.latched = true;
    }

    /// Whether the counters were latched since the latch flag was last cleared: `$213F`
    /// bit 6.
    // @source: anomie-regs#213f
    // @source: fullsnes#snespputimersandstatus
    pub(crate) const fn latched(&self) -> bool {
        self.latched
    }

    /// A `$213F` read: both flip-flops return to the low byte, and the latch flag clears
    /// if `external_latch`, `$4201` bit 7, is set (ADR 0029, point 10).
    // @source: anomie-regs#213c
    // @source: anomie-regs#213f
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0029
    pub(crate) const fn read_status(&mut self, external_latch: bool) {
        self.horizontal.high_next = false;
        self.vertical.high_next = false;
        if external_latch {
            self.latched = false;
        }
    }

    /// The byte a read of `register` returns, with `open_bus` as PPU2 open bus.
    // @source: anomie-regs#213c
    // @source: anomie-regs#213d
    // @source: fullsnes#snespputimersandstatus
    pub(crate) const fn peek(&self, register: CounterRegister, open_bus: u8) -> u8 {
        match register {
            CounterRegister::Horizontal => self.horizontal.byte(open_bus),
            CounterRegister::Vertical => self.vertical.byte(open_bus),
        }
    }

    /// A read of `register` flips its flip-flop.
    // @source: anomie-regs#213c
    // @source: anomie-regs#213d
    // @source: fullsnes#snespputimersandstatus
    pub(crate) const fn flip(&mut self, register: CounterRegister) {
        let counter = match register {
            CounterRegister::Horizontal => &mut self.horizontal,
            CounterRegister::Vertical => &mut self.vertical,
        };
        counter.high_next = !counter.high_next;
    }
}
