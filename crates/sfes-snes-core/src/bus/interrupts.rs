// @source: anomie-timing#s-cpu/interrupts
// @source: anomie-timing#s-cpu/registers
// @source: anomie-regs#4200
// @source: anomie-regs#4210
// @source: anomie-regs#4211
// @source: fullsnes#snesppuinterrupts
// @source: wdc-w65c816s#2.18
// @source: wdc-w65c816s#2.21
// @source: adr-0004
// @source: adr-0021
//! The 5A22's interrupt outputs and the CPU's view of them: `$4200` bit 7, the NMI and IRQ
//! flags of `$4210` and `$4211`, the NMI edge latch and the lines recorded at the start of
//! each CPU cycle (ADR 0004; ADR 0021, points 1, 4 and 5).

use sfes_snes_cpu::Pending;

/// `$4200` NMITIMEN bit 7: NMI enable.
const NMI_ENABLE: u8 = 0x80;
/// `$4200` bits 4 and 5: the IRQ mode; both clear disables and acknowledges the IRQ.
const IRQ_MODE: u8 = 0x30;

/// $4200 NMITIMEN bit 7: the NMI flag reaches /NMI.
const STATE_NMI_ENABLE: u8 = 0x01;
/// $4210 RDNMI bit 7: the timer's NMI output is low.
const STATE_NMI_FLAG: u8 = 0x02;
/// $4211 TIMEUP bit 7: the timer's IRQ output is low, and with it /IRQ.
const STATE_IRQ_FLAG: u8 = 0x04;
/// The CPU's /NMI input is low: the flag and the enable are both set.
const STATE_NMI_LINE: u8 = 0x08;
/// A high-to-low change of /NMI not yet taken (ADR 0004).
const STATE_NMI_LATCHED: u8 = 0x10;

/// The interrupt state of the main CPU's bus.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Interrupts {
    /// The `STATE_*` bits.
    state: u8,
    /// The latch and the IRQ line at the start of the last CPU cycle (ADR 0021, point 1).
    recorded: Pending,
}

impl Interrupts {
    /// The state at power-on and after reset: `$4200` 0, both flags clear, no latched NMI
    /// (ADR 0021, point 8).
    // @source: anomie-regs#4200
    // @source: anomie-regs#4210
    // @source: anomie-regs#4211
    // @source: fullsnes#snesiomap
    // @source: adr-0021
    pub(crate) fn power_on() -> Self {
        Self::default()
    }

    /// Records the latch and the IRQ line at the start of a CPU cycle (ADR 0021, point 1).
    // @source: anomie-timing#s-cpu/interrupts
    // @source: adr-0021
    pub(crate) const fn record(&mut self) {
        self.recorded = Pending {
            nmi: self.has(STATE_NMI_LATCHED),
            irq: self.has(STATE_IRQ_FLAG),
        };
    }

    /// Whether `bit` of the state is set.
    // @source: adr-0021
    const fn has(self, bit: u8) -> bool {
        self.state & bit != 0
    }

    /// Sets or clears `bit` of the state.
    // @source: adr-0021
    const fn put(&mut self, bit: u8, value: bool) {
        if value {
            self.state |= bit;
        } else {
            self.state &= !bit;
        }
    }

    /// The requests the CPU sees at an instruction boundary: those recorded at the start of
    /// the step's last cycle (ADR 0004; ADR 0021, point 1).
    // @source: anomie-timing#s-cpu/interrupts
    // @source: adr-0004
    // @source: adr-0021
    pub(crate) const fn recorded(self) -> Pending {
        self.recorded
    }

    /// The vector read of an NMI entry clears the edge latch (ADR 0004).
    // @source: wdc-w65c816s#2.21
    // @source: adr-0004
    pub(crate) const fn acknowledge_nmi(&mut self) {
        self.put(STATE_NMI_LATCHED, false);
    }

    /// Latches an NMI and records it, as if a cycle had just started.
    // @source: adr-0004
    #[cfg(test)]
    pub(crate) const fn latch_nmi(&mut self) {
        self.put(STATE_NMI_LATCHED, true);
        self.record();
    }

    /// Whether an NMI is latched and not yet taken.
    // @source: adr-0004
    #[cfg(test)]
    pub(crate) const fn nmi_latched(self) -> bool {
        self.has(STATE_NMI_LATCHED)
    }

    /// A `$4200` write: bit 7 gates the NMI flag onto /NMI, and clearing bits 4 and 5
    /// acknowledges the IRQ (ADR 0021, points 4 and 5). The NMI flag is kept.
    // @source: anomie-regs#4200
    // @source: anomie-timing#s-cpu/interrupts
    // @source: fullsnes#snesppuinterrupts
    // @source: adr-0021
    pub(crate) const fn write_enable(&mut self, value: u8) {
        self.put(STATE_NMI_ENABLE, value & NMI_ENABLE != 0);
        if value & IRQ_MODE == 0 {
            self.put(STATE_IRQ_FLAG, false);
        }
        self.update_nmi_line();
    }

    /// The NMI flag is set at H=0.5 of the line that starts vertical blank.
    // @source: anomie-timing#s-cpu/interrupts
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0021
    pub(crate) const fn set_nmi_flag(&mut self) {
        self.put(STATE_NMI_FLAG, true);
        self.update_nmi_line();
    }

    /// The NMI flag is cleared at H=0 of line 0.
    // @source: anomie-timing#s-cpu/interrupts
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0021
    pub(crate) const fn end_vertical_blank(&mut self) {
        self.put(STATE_NMI_FLAG, false);
        self.update_nmi_line();
    }

    /// The IRQ flag is set at a trigger point.
    // @source: anomie-timing#s-cpu/interrupts
    // @source: adr-0021
    pub(crate) const fn set_irq_flag(&mut self) {
        self.put(STATE_IRQ_FLAG, true);
    }

    /// The NMI flag, `$4210` bit 7, without the read's side effect.
    // @source: anomie-regs#4210
    pub(crate) const fn nmi_flag(self) -> bool {
        self.has(STATE_NMI_FLAG)
    }

    /// The IRQ flag, `$4211` bit 7, without the read's side effect.
    // @source: anomie-regs#4211
    pub(crate) const fn irq_flag(self) -> bool {
        self.has(STATE_IRQ_FLAG)
    }

    /// A `$4210` read: returns the NMI flag and clears it (ADR 0021, point 4).
    // @source: anomie-timing#s-cpu/registers
    // @source: anomie-regs#4210
    // @source: adr-0021
    pub(crate) const fn read_nmi_flag(&mut self) -> bool {
        let flag = self.has(STATE_NMI_FLAG);
        self.put(STATE_NMI_FLAG, false);
        self.update_nmi_line();
        flag
    }

    /// A `$4211` read: returns the IRQ flag and clears it (ADR 0021, point 5).
    // @source: anomie-timing#s-cpu/registers
    // @source: anomie-regs#4211
    // @source: adr-0021
    pub(crate) const fn read_irq_flag(&mut self) -> bool {
        let flag = self.has(STATE_IRQ_FLAG);
        self.put(STATE_IRQ_FLAG, false);
        flag
    }

    /// Works out /NMI from the flag and the enable, and latches a high-to-low change
    /// (`wdc-w65c816s#2.21`; fullsnes: the internal flag is set when "[4200h].7 AND
    /// [4210h].7" changes from 0 to 1).
    // @source: wdc-w65c816s#2.21
    // @source: fullsnes#snesppuinterrupts
    // @source: adr-0021
    const fn update_nmi_line(&mut self) {
        let line = self.has(STATE_NMI_FLAG) && self.has(STATE_NMI_ENABLE);
        if line && !self.has(STATE_NMI_LINE) {
            self.put(STATE_NMI_LATCHED, true);
        }
        self.put(STATE_NMI_LINE, line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: wdc-w65c816s#2.21
    // @source: fullsnes#snesppuinterrupts
    // @source: adr-0021
    #[test]
    fn an_nmi_is_latched_when_flag_and_enable_are_both_set() {
        let mut interrupts = Interrupts::power_on();
        interrupts.set_nmi_flag();
        interrupts.record();
        assert!(
            !interrupts.recorded().nmi,
            "disabled NMIs set only the flag"
        );
        assert!(interrupts.nmi_flag());
        // Enabling with the flag set is a fresh edge.
        interrupts.write_enable(0x80);
        interrupts.record();
        assert!(interrupts.recorded().nmi);
        interrupts.acknowledge_nmi();
        interrupts.record();
        assert!(!interrupts.recorded().nmi);
        // /NMI stays low: no second edge until it goes high and low again.
        interrupts.write_enable(0x80);
        interrupts.record();
        assert!(!interrupts.recorded().nmi);
        interrupts.write_enable(0x00);
        interrupts.write_enable(0x80);
        interrupts.record();
        assert!(interrupts.recorded().nmi);
    }

    // @source: anomie-timing#s-cpu/interrupts
    // @source: anomie-regs#4210
    // @source: adr-0021
    #[test]
    fn reading_4210_clears_the_flag_but_not_the_latch() {
        let mut interrupts = Interrupts::power_on();
        interrupts.write_enable(0x80);
        interrupts.set_nmi_flag();
        assert!(interrupts.read_nmi_flag());
        assert!(!interrupts.read_nmi_flag());
        interrupts.record();
        assert!(interrupts.recorded().nmi);
        // Writing $4200 does not clear the flag.
        interrupts.acknowledge_nmi();
        interrupts.set_nmi_flag();
        interrupts.write_enable(0x00);
        assert!(interrupts.nmi_flag());
        interrupts.end_vertical_blank();
        assert!(!interrupts.nmi_flag());
    }

    // @source: anomie-timing#s-cpu/interrupts
    // @source: fullsnes#snesppuinterrupts
    // @source: adr-0021
    #[test]
    fn the_irq_is_cleared_by_4211_and_by_disabling_it() {
        let mut interrupts = Interrupts::power_on();
        interrupts.set_irq_flag();
        interrupts.record();
        assert!(interrupts.recorded().irq);
        assert!(interrupts.read_irq_flag());
        assert!(!interrupts.irq_flag());
        // The record keeps the line as it was when the cycle started.
        assert!(interrupts.recorded().irq);
        interrupts.set_irq_flag();
        interrupts.write_enable(0x10);
        assert!(interrupts.irq_flag(), "a mode other than 00 keeps the flag");
        interrupts.write_enable(0x80);
        assert!(!interrupts.irq_flag());
    }
}
