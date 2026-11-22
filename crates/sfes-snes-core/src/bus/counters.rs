// @source: anomie-regs#2137
// @source: anomie-regs#213c
// @source: anomie-regs#213d
// @source: anomie-regs#4201
// @source: fullsnes#snespputimersandstatus
// @source: adr-0021
//! The H/V counter latch read at `$213C` OPHCT and `$213D` OPVCT (ADR 0021, point 7). It is
//! PPU2 state, kept in the bus until the PPU exists.

use crate::bus::Registers;

/// OPHCT and OPVCT at power-on (fullsnes#snesiomap).
const COUNTER_POWER_ON: u16 = 0x1FF;
/// The bit of the high byte that holds bit 8 of a counter; the rest is PPU2 open bus.
const COUNTER_HIGH: u8 = 0x01;

/// One of the two latched counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CounterRegister {
    /// $213C OPHCT: the latched H.
    Horizontal,
    /// $213D OPVCT: the latched V.
    Vertical,
}

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

/// The latched counters, their flip-flops, the latch flag and PPU2 open bus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CounterLatch {
    /// $213C OPHCT: the latched H.
    horizontal: Counter,
    /// $213D OPVCT: the latched V.
    vertical: Counter,
    /// $213F STAT78 bit 6: the counters were latched since `$213F` was last read.
    latched: bool,
    /// The last value read from `$213C` or `$213D`, until the PPU's PPU2 open bus replaces
    /// it (ADR 0021, point 7).
    open_bus: u8,
}

impl CounterLatch {
    /// The latch at power-on: both counters `$1FF`, both flip-flops on the low byte and
    /// PPU2 open bus 0 (ADR 0021, point 8). Reset keeps all of it.
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
            open_bus: 0,
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

    /// The counter `register` names.
    // @source: anomie-regs#213c
    // @source: anomie-regs#213d
    const fn counter(&self, register: CounterRegister) -> &Counter {
        match register {
            CounterRegister::Horizontal => &self.horizontal,
            CounterRegister::Vertical => &self.vertical,
        }
    }
}

impl Registers for CounterLatch {
    type Register = CounterRegister;

    /// $213C OPHCT or $213D OPVCT: the counter's next byte, low then high.
    // @source: anomie-regs#213c
    // @source: anomie-regs#213d
    // @source: fullsnes#snespputimersandstatus
    fn peek(&self, register: CounterRegister) -> Option<u8> {
        Some(self.counter(register).byte(self.open_bus))
    }

    /// A read flips the counter's flip-flop, and its value becomes PPU2 open bus.
    // @source: anomie-regs#213c
    // @source: anomie-regs#213d
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0021
    fn after_read(&mut self, register: CounterRegister) {
        let value = self.counter(register).byte(self.open_bus);
        let counter = match register {
            CounterRegister::Horizontal => &mut self.horizontal,
            CounterRegister::Vertical => &mut self.vertical,
        };
        counter.high_next = !counter.high_next;
        self.open_bus = value;
    }

    /// The counters are read-only; a write is lost.
    // @source: anomie-regs#213c
    fn write(&mut self, _register: CounterRegister, _value: u8) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: anomie-regs#213c
    // @source: anomie-regs#213d
    // @source: fullsnes#snesiomap
    // @source: fullsnes#snespputimersandstatus
    #[test]
    fn each_counter_reads_low_then_high_with_open_bus() {
        let mut latch = CounterLatch::power_on();
        let horizontal = CounterRegister::Horizontal;
        let vertical = CounterRegister::Vertical;
        assert_eq!(latch.read(horizontal), Some(0xFF));
        // Bit 0 is bit 8 of $1FF; bits 7-1 are the last PPU2 value read, $FF.
        assert_eq!(latch.read(horizontal), Some(0xFF));
        latch.latch(0x105, 0x0A3);
        assert!(latch.latched);
        assert_eq!(latch.read(vertical), Some(0x05));
        // The last value read was $05: bits 7-1 are $04, bit 0 is bit 8 of $105.
        assert_eq!(latch.read(vertical), Some(0x05));
        // OPHCT's flip-flop was left on the low byte; a latch does not reset it.
        assert_eq!(latch.read(horizontal), Some(0xA3));
        assert_eq!(latch.peek(horizontal), Some(0xA2));
        assert_eq!(latch.read(horizontal), Some(0xA2));
    }
}
