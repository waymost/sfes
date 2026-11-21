// @source: anomie-regs#2140
// @source: adr-0019
// @source: adr-0027
//! The APU unit as the bus sees it: the four ports at `$2140–$2143`. Until M3 this is a
//! placeholder, not hardware behaviour (ADR 0027): reads return `$00`, writes are kept for
//! the SPC700 that is not there yet, and catching up does nothing. M3 replaces the inside
//! of [`Apu`]; the bus's calls into it stay as they are (ADR 0027, point 7).

use sfes_api::Cycles;

/// The number of APU ports.
pub(crate) const APU_PORTS: usize = 4;

/// The APU unit: the CPU's side of the four ports (ADR 0027, point 1).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Apu {
    /// $2140–$2143 APUIO0–APUIO3 as written: the values the CPU last wrote, which the
    /// SPC700 reads as `$F4–$F7`.
    to_apu: [u8; APU_PORTS],
    /// $2140–$2143 APUIO0–APUIO3 as read: the values the SPC700 last wrote.
    to_cpu: [u8; APU_PORTS],
}

impl Apu {
    /// The ports at power-on: all eight latches `$00` (ADR 0027, point 6).
    // @source: adr-0027
    pub(crate) const fn power_on() -> Self {
        Self {
            to_apu: [0; APU_PORTS],
            to_cpu: [0; APU_PORTS],
        }
    }

    /// The reset button: all eight latches `$00`, as at power-on (ADR 0027, point 6).
    // @source: adr-0027
    pub(crate) const fn reset(&mut self) {
        *self = Self::power_on();
    }

    /// Runs the APU up to `now`, before an access to a port (technical design section 4,
    /// catch-up rule 1). The placeholder has nothing to run (ADR 0027, point 3).
    // @source: adr-0019
    // @source: adr-0027
    #[expect(
        clippy::unused_self,
        reason = "the catch-up M3 fills in (ADR 0027, point 7)"
    )]
    pub(crate) const fn run_until(&mut self, now: Cycles) {
        let _ = now;
    }

    /// $2140–$2143 APUIO0–APUIO3: a CPU read of `port`, the value the APU side last wrote
    /// (anomie-regs#2140; ADR 0027, point 4).
    // @source: anomie-regs#2140
    // @source: adr-0027
    pub(crate) fn read_port(&mut self, port: u8) -> u8 {
        self.peek_port(port)
    }

    /// What a read of `port` would return, without its side effects (ADR 0027, point 4).
    // @source: anomie-regs#2140
    // @source: adr-0027
    pub(crate) fn peek_port(self, port: u8) -> u8 {
        self.to_cpu
            .get(usize::from(port))
            .copied()
            .unwrap_or_default()
    }

    /// $2140–$2143 APUIO0–APUIO3: a CPU write of `value` to `port`, kept for the APU side
    /// and never read back by the CPU (anomie-regs#2140; ADR 0027, point 5).
    // @source: anomie-regs#2140
    // @source: adr-0027
    pub(crate) fn write_port(&mut self, port: u8, value: u8) {
        if let Some(latch) = self.to_apu.get_mut(usize::from(port)) {
            *latch = value;
        }
    }

    /// The value the CPU last wrote to `port`, as the SPC700 would read it.
    // @source: adr-0027
    #[cfg(test)]
    pub(crate) fn written(self, port: u8) -> u8 {
        self.to_apu
            .get(usize::from(port))
            .copied()
            .unwrap_or_default()
    }
}
