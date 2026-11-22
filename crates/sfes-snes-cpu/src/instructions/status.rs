// @source: wdc-w65c816s#table-5-5
// @source: wdc-w65c816s#7.10
// @source: wdc-w65c816s#7.16
// @source: wdc-65816-manual#chapter-18
// @source: 6502org-65c816-opcodes#6.4.1
// @source: 6502org-65c816-opcodes#6.4.2
// @source: 6502org-65c816-opcodes#6.7
// @source: 6502org-65c816-opcodes#6.10.4
// @source: adr-0010
//! The status register and the mode: CLC, CLD, CLI, CLV, SEC, SED, SEI, REP, SEP and XCE;
//! and the instructions that do nothing, NOP and WDM.
//!
//! A change of the M, X or E flag takes effect after the instruction's last cycle: every
//! cycle of the instruction itself still reports the old E, M and X signals. Changes of P go
//! through [`Cpu65816::set_status`] and of E through [`Cpu65816::set_emulation`], which apply
//! what the datasheet says follows from them (sections 2.7, 2.8 and 7.10).

use crate::{
    bus::Bus65816,
    registers::{Cpu65816, Flags},
};

impl Cpu65816 {
    /// CLC, CLD, CLI, CLV, SEC, SED and SEI: one internal cycle (row 19a), then `flag` set to
    /// `value`. None of them is M or X, so no width changes. P changes in the final cycle, so
    /// the IRQ check at the next boundary still uses the old I flag (ADR 0022).
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-65816-manual#chapter-18-clc
    // @source: wdc-65816-manual#chapter-18-cld
    // @source: wdc-65816-manual#chapter-18-cli
    // @source: wdc-65816-manual#chapter-18-clv
    // @source: wdc-65816-manual#chapter-18-sec
    // @source: wdc-65816-manual#chapter-18-sed
    // @source: wdc-65816-manual#chapter-18-sei
    // @source: 6502org-65c816-opcodes#6.4.1
    // @source: anomie-timing#s-cpu/interrupts
    // @source: adr-0022
    pub(crate) fn set_flag(&mut self, bus: &mut impl Bus65816, flag: Flags, value: bool) {
        self.internal_at_program_counter(bus);
        self.delay_irq_disable();
        self.p.set(flag, value);
    }

    /// REP: clears each status bit that is set in the immediate operand. Note 1: always 3
    /// cycles, the third an internal cycle at PBR,PC+1, the operand's address. In emulation
    /// mode M and X stay set. P changes in the final cycle, so the IRQ check at the next
    /// boundary still uses the old I flag (ADR 0022).
    // @source: wdc-w65c816s#table-5-7-18
    // @source: wdc-w65c816s#table-5-7-note-1
    // @source: wdc-w65c816s#7.8.3.1
    // @source: wdc-65816-manual#chapter-18-rep
    // @source: 6502org-65c816-opcodes#6.4.2
    // @source: anomie-timing#s-cpu/interrupts
    // @source: adr-0022
    pub(crate) fn rep(&mut self, bus: &mut impl Bus65816) {
        let operand = Flags::from_bits_retain(self.fetch(bus));
        self.internal_at_last_fetch(bus);
        self.delay_irq_disable();
        self.set_status(self.p.difference(operand));
    }

    /// SEP: sets each status bit that is set in the immediate operand; cycles as REP. A set
    /// X flag clears the high bytes of X and Y. As for REP, the next IRQ check uses the old I
    /// flag (ADR 0022).
    // @source: wdc-w65c816s#table-5-7-18
    // @source: wdc-w65c816s#table-5-7-note-1
    // @source: wdc-w65c816s#2.7
    // @source: wdc-65816-manual#chapter-18-sep
    // @source: 6502org-65c816-opcodes#6.4.2
    // @source: anomie-timing#s-cpu/interrupts
    // @source: adr-0022
    pub(crate) fn sep(&mut self, bus: &mut impl Bus65816) {
        let operand = Flags::from_bits_retain(self.fetch(bus));
        self.internal_at_last_fetch(bus);
        self.delay_irq_disable();
        self.set_status(self.p.union(operand));
    }

    /// XCE: exchanges C and E after one internal cycle (row 19a). Entering emulation mode
    /// sets M and X, puts S in page 1 and clears the high bytes of X and Y; B is kept
    /// (7.10). Entering native mode leaves M and X set, as emulation mode had them.
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-w65c816s#7.10
    // @source: wdc-65816-manual#chapter-18-xce
    // @source: 6502org-65c816-opcodes#6.10.4
    pub(crate) fn xce(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        let carry = self.p.contains(Flags::C);
        self.p.set(Flags::C, self.e);
        self.set_emulation(carry);
    }

    /// NOP: one internal cycle (row 19a).
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-65816-manual#chapter-18-nop
    // @source: 6502org-65c816-opcodes#6.7
    pub(crate) fn nop(&self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
    }

    /// WDM: "performs no operation" (7.16) as a two-byte instruction. Its second cycle is an
    /// internal cycle at the second byte's address, which is skipped (ADR 0010).
    // @source: wdc-w65c816s#7.16
    // @source: wdc-65816-manual#chapter-18-wdm
    // @source: 6502org-65c816-opcodes#6.7
    // @source: adr-0010
    pub(crate) fn wdm(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.pc = self.pc.wrapping_add(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bus::Pins,
        testing::{Cycle, emulation, native, run},
    };

    // @source: 6502org-65c816-opcodes#6.4.2
    #[test]
    fn sep_follows_the_tutorial_example() {
        // e = 0: SEP #$21 sets m and c and leaves the other flags.
        let mut cpu = native(Flags::V | Flags::D);
        let bus = run(&mut cpu, &[0xE2, 0x21], &[]);
        assert_eq!(cpu.p, Flags::V | Flags::D | Flags::M | Flags::C);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Internal(0x12_8001),
            ]
        );
        assert_eq!(cpu.pc, 0x8002);
    }

    // @source: wdc-w65c816s#table-5-7-note-1
    // @source: 6502org-65c816-opcodes#6.4.2
    #[test]
    fn rep_cannot_clear_m_and_x_in_emulation_mode_and_changes_the_pins_after_its_cycles() {
        let mut cpu = emulation();
        run(&mut cpu, &[0xC2, 0xFF], &[]);
        assert_eq!(cpu.p, Flags::M | Flags::X);

        let mut cpu = native(Flags::M | Flags::X);
        let bus = run(&mut cpu, &[0xC2, 0x30], &[]);
        assert_eq!(cpu.p, Flags::empty());
        assert!(bus.pins.iter().all(|pins| pins.contains(Pins::M | Pins::X)));
    }

    // @source: wdc-w65c816s#2.7
    #[test]
    fn sep_x_clears_the_index_high_bytes() {
        let mut cpu = native(Flags::empty());
        cpu.x = 0x1234;
        cpu.y = 0x5678;
        run(&mut cpu, &[0xE2, 0x10], &[]);
        assert_eq!((cpu.x, cpu.y), (0x0034, 0x0078));
    }

    // @source: 6502org-65c816-opcodes#6.4.1
    #[test]
    fn clc_follows_the_tutorial_example() {
        let mut cpu = native(Flags::C | Flags::Z);
        run(&mut cpu, &[0x18], &[]);
        assert_eq!(cpu.p, Flags::Z);
    }

    // @source: 6502org-65c816-opcodes#6.10.4
    #[test]
    fn xce_follows_the_tutorial_example() {
        // c = 0, e = 1: XCE gives c = 1, e = 0, and M and X stay set.
        let mut cpu = emulation();
        let bus = run(&mut cpu, &[0xFB], &[]);
        assert!(!cpu.e);
        assert_eq!(cpu.p, Flags::M | Flags::X | Flags::I | Flags::C);
        assert!(bus.pins.iter().all(|pins| pins.contains(Pins::E)));
    }

    // @source: wdc-w65c816s#7.10
    #[test]
    fn xce_into_emulation_mode_narrows_the_registers_and_keeps_b() {
        let mut cpu = native(Flags::C);
        cpu.a = 0x1234;
        cpu.x = 0x5678;
        cpu.y = 0x9ABC;
        cpu.s = 0x1FF0;
        run(&mut cpu, &[0xFB], &[]);
        assert!(cpu.e);
        assert_eq!(
            (cpu.a, cpu.x, cpu.y, cpu.s),
            (0x1234, 0x0078, 0x00BC, 0x01F0)
        );
        assert_eq!(cpu.p, Flags::M | Flags::X);
    }

    // @source: 6502org-65c816-opcodes#6.7
    // @source: adr-0010
    #[test]
    fn wdm_skips_its_second_byte_in_an_internal_cycle() {
        let mut cpu = native(Flags::empty());
        let bus = run(&mut cpu, &[0x42, 0xCA], &[]);
        assert_eq!(
            bus.cycles,
            [Cycle::Opcode(0x12_8000), Cycle::Internal(0x12_8001)]
        );
        assert_eq!(cpu.pc, 0x8002);
    }
}
