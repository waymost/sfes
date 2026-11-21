// @source: anomie-regs#2137
// @source: anomie-regs#213c
// @source: anomie-regs#213d
// @source: anomie-regs#4201
// @source: fullsnes#snespputimersandstatus
// @source: adr-0021
//! The H/V counter latch read at `$213C` OPHCT and `$213D` OPVCT (ADR 0021, point 7). It is
//! PPU2 state, kept in the bus until the PPU exists.

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

/// The latched counters, their flip-flops, the latch flag and PPU2 open bus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CounterLatch {
    /// $213C OPHCT: the latched H.
    horizontal: Counter,
    /// $213D OPVCT: the latched V.
    vertical: Counter,
    /// `$213F` bit 6: the counters were latched since `$213F` was last read.
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

    /// What a `$213C` read would return, without its side effects.
    // @source: anomie-regs#213c
    pub(crate) const fn peek_horizontal(&self) -> u8 {
        self.horizontal.byte(self.open_bus)
    }

    /// What a `$213D` read would return, without its side effects.
    // @source: anomie-regs#213d
    pub(crate) const fn peek_vertical(&self) -> u8 {
        self.vertical.byte(self.open_bus)
    }

    /// A `$213C` read: the next byte of OPHCT.
    // @source: anomie-regs#213c
    // @source: fullsnes#snespputimersandstatus
    pub(crate) const fn read_horizontal(&mut self) -> u8 {
        let value = self.peek_horizontal();
        self.horizontal.high_next = !self.horizontal.high_next;
        self.open_bus = value;
        value
    }

    /// A `$213D` read: the next byte of OPVCT.
    // @source: anomie-regs#213d
    // @source: fullsnes#snespputimersandstatus
    pub(crate) const fn read_vertical(&mut self) -> u8 {
        let value = self.peek_vertical();
        self.vertical.high_next = !self.vertical.high_next;
        self.open_bus = value;
        value
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: anomie-regs#213c
    // @source: anomie-regs#213d
    // @source: fullsnes#snespputimersandstatus
    // @source: fullsnes#snesiomap
    #[test]
    fn each_counter_reads_low_then_high_with_open_bus() {
        let mut latch = CounterLatch::power_on();
        assert_eq!(latch.read_horizontal(), 0xFF);
        // Bit 0 is bit 8 of $1FF; bits 7-1 are the last PPU2 value read, $FF.
        assert_eq!(latch.read_horizontal(), 0xFF);
        latch.latch(0x105, 0x0A3);
        assert!(latch.latched);
        assert_eq!(latch.read_vertical(), 0x05);
        // The last value read was $05: bits 7-1 are $04, bit 0 is bit 8 of $105.
        assert_eq!(latch.read_vertical(), 0x05);
        // OPHCT's flip-flop was left on the low byte; a latch does not reset it.
        assert_eq!(latch.read_horizontal(), 0xA3);
        assert_eq!(latch.peek_horizontal(), 0xA2);
        assert_eq!(latch.read_horizontal(), 0xA2);
    }
}
