// @source: wdc-w65c816s#table-5-5
// @source: wdc-w65c816s#7.1
// @source: wdc-65816-manual#chapter-18
// @source: 6502org-65c816-opcodes#6.8.1
// @source: 6502org-65c816-opcodes#6.8.2
// @source: 6502org-65c816-opcodes#6.8.3
// @source: adr-0008
// @source: adr-0009
//! Pushes and pulls of the registers (PHA, PHX, PHY, PHP, PHB, PHD, PHK, PLA, PLX, PLY, PLP,
//! PLB, PLD) and the pushes of an effective address (PEA, PEI, PER).
//!
//! A push writes the high byte first, at 0,S, and the low byte below it; a pull reads the
//! low byte first. In emulation mode the stack stays in page 1, except in PEA, PEI, PER,
//! PHD, PLD (datasheet 7.1) and PLB (ADR 0009), whose stack cycles may leave it; S is back in
//! page 1 when they end (ADR 0008).

use crate::{
    addressing::StackWrap,
    bus::Bus65816,
    registers::{Cpu65816, Flags},
    width::{Width, at_width},
};

impl Cpu65816 {
    /// PHA: pushes the accumulator, A or C by the M flag (rows 22c and note 1).
    // @source: wdc-w65c816s#table-5-7-22c
    // @source: wdc-w65c816s#table-5-7-note-1
    // @source: wdc-65816-manual#chapter-18-pha
    // @source: 6502org-65c816-opcodes#6.8.2
    pub(crate) fn pha(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        at_width!(self.p, Flags::M, W => {
            let value = self.accumulator::<W>();
            self.push_value(bus, value, StackWrap::PageOne);
        });
    }

    /// PHX: pushes X, at the width the X flag selects.
    // @source: wdc-w65c816s#table-5-7-22c
    // @source: wdc-w65c816s#table-5-7-note-1
    // @source: wdc-65816-manual#chapter-18-phx
    // @source: 6502org-65c816-opcodes#6.8.2
    pub(crate) fn phx(&mut self, bus: &mut impl Bus65816) {
        self.push_index(bus, self.x);
    }

    /// PHY: pushes Y, at the width the X flag selects.
    // @source: wdc-w65c816s#table-5-7-22c
    // @source: wdc-w65c816s#table-5-7-note-1
    // @source: wdc-65816-manual#chapter-18-phy
    // @source: 6502org-65c816-opcodes#6.8.2
    pub(crate) fn phy(&mut self, bus: &mut impl Bus65816) {
        self.push_index(bus, self.y);
    }

    /// PHP: pushes the status register, always one byte (note 12, datasheet 7.20). In
    /// emulation mode bit 4 is the X flag, which is set there, so the pushed B bit is 1.
    // @source: wdc-w65c816s#table-5-7-22c
    // @source: wdc-w65c816s#table-5-7-note-12
    // @source: wdc-w65c816s#7.20
    // @source: wdc-65816-manual#chapter-18-php
    // @source: 6502org-65c816-opcodes#6.8.3
    pub(crate) fn php(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.push(bus, self.p.bits(), StackWrap::PageOne);
    }

    /// PHB: "DBR → Ms, S-1 → S", one byte.
    // @source: wdc-w65c816s#table-5-7-22c
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-65816-manual#chapter-18-phb
    // @source: 6502org-65c816-opcodes#6.8.3
    pub(crate) fn phb(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.push(bus, self.dbr, StackWrap::PageOne);
    }

    /// PHK: "PBR → Ms, S-1 → S", one byte.
    // @source: wdc-w65c816s#table-5-7-22c
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-65816-manual#chapter-18-phk
    // @source: 6502org-65c816-opcodes#6.8.3
    pub(crate) fn phk(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.push(bus, self.pbr, StackWrap::PageOne);
    }

    /// PHD: "D → Ms, Ms-1, S-2 → S", always two bytes. One of the instructions of 7.1 whose
    /// stack cycles may leave page 1 in emulation mode.
    // @source: wdc-w65c816s#table-5-7-22c
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#7.1
    // @source: adr-0008
    // @source: wdc-65816-manual#chapter-18-phd
    // @source: 6502org-65c816-opcodes#6.8.3
    pub(crate) fn phd(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.push_value(bus, self.d, StackWrap::BankZero);
        self.restore_stack_page();
    }

    /// PLA: pulls the accumulator, A or C by the M flag, and sets N and Z from it.
    // @source: wdc-w65c816s#table-5-7-22b
    // @source: wdc-w65c816s#table-5-7-note-1
    // @source: wdc-65816-manual#chapter-18-pla
    // @source: 6502org-65c816-opcodes#6.8.2
    pub(crate) fn pla(&mut self, bus: &mut impl Bus65816) {
        self.pull_internal_cycles(bus);
        at_width!(self.p, Flags::M, W => {
            let value = self.pull_value::<W>(bus, StackWrap::PageOne);
            self.p.set_negative_zero(value);
            self.set_accumulator(value);
        });
    }

    /// PLX: pulls X at the width the X flag selects, and sets N and Z from it.
    // @source: wdc-w65c816s#table-5-7-22b
    // @source: wdc-w65c816s#table-5-7-note-1
    // @source: wdc-65816-manual#chapter-18-plx
    // @source: 6502org-65c816-opcodes#6.8.2
    pub(crate) fn plx(&mut self, bus: &mut impl Bus65816) {
        self.x = self.pull_index(bus);
    }

    /// PLY: pulls Y at the width the X flag selects, and sets N and Z from it.
    // @source: wdc-w65c816s#table-5-7-22b
    // @source: wdc-w65c816s#table-5-7-note-1
    // @source: wdc-65816-manual#chapter-18-ply
    // @source: 6502org-65c816-opcodes#6.8.2
    pub(crate) fn ply(&mut self, bus: &mut impl Bus65816) {
        self.y = self.pull_index(bus);
    }

    /// PLP: pulls the status register, always one byte. In emulation mode M and X stay set,
    /// and a set X flag clears the high bytes of X and Y, as for any change of P. P is pulled
    /// in the final cycle, so the IRQ check at the next boundary still uses the old I flag
    /// (ADR 0022).
    // @source: wdc-w65c816s#table-5-7-22b
    // @source: wdc-w65c816s#table-5-7-note-12
    // @source: wdc-w65c816s#7.20
    // @source: wdc-65816-manual#chapter-18-plp
    // @source: 6502org-65c816-opcodes#6.8.3
    // @source: anomie-timing#s-cpu/interrupts
    // @source: adr-0022
    pub(crate) fn plp(&mut self, bus: &mut impl Bus65816) {
        self.pull_internal_cycles(bus);
        self.delay_irq_disable();
        let value = self.pull(bus, StackWrap::PageOne);
        self.set_status(Flags::from_bits_retain(value));
    }

    /// PLB: "S + 1 → S, Ms → DBR", one byte, setting N and Z. Its stack cycle may leave page
    /// 1 in emulation mode, although 7.1 does not list it (ADR 0009).
    // @source: wdc-w65c816s#table-5-7-22b
    // @source: wdc-w65c816s#table-5-5
    // @source: adr-0009
    // @source: wdc-65816-manual#chapter-18-plb
    // @source: 6502org-65c816-opcodes#6.8.3
    pub(crate) fn plb(&mut self, bus: &mut impl Bus65816) {
        self.pull_internal_cycles(bus);
        let value = self.pull(bus, StackWrap::BankZero);
        self.restore_stack_page();
        self.p.set_negative_zero(value);
        self.dbr = value;
    }

    /// PLD: "S + 2 → S, Ms − 1, Ms → D", always two bytes, setting N and Z from the 16-bit
    /// value. One of the instructions of 7.1.
    // @source: wdc-w65c816s#table-5-7-22b
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#7.1
    // @source: adr-0008
    // @source: wdc-65816-manual#chapter-18-pld
    // @source: 6502org-65c816-opcodes#6.8.3
    pub(crate) fn pld(&mut self, bus: &mut impl Bus65816) {
        self.pull_internal_cycles(bus);
        let value = self.pull_value::<u16>(bus, StackWrap::BankZero);
        self.restore_stack_page();
        self.p.set_negative_zero(value);
        self.d = value;
    }

    /// PEA: fetches the 16-bit operand "AAL", "AAH" and pushes it, high byte first (row
    /// 22d). No memory is read at that address.
    // @source: wdc-w65c816s#table-5-7-22d
    // @source: wdc-w65c816s#7.1
    // @source: adr-0008
    // @source: wdc-65816-manual#chapter-18-pea
    // @source: 6502org-65c816-opcodes#6.8.1
    pub(crate) fn pea(&mut self, bus: &mut impl Bus65816) {
        let value = self.fetch_address(bus);
        self.push_value(bus, value, StackWrap::BankZero);
        self.restore_stack_page();
    }

    /// PEI: pushes the 16-bit value at 0,D+DO (row 22e), whatever the M and X flags.
    // @source: wdc-w65c816s#table-5-7-22e
    // @source: wdc-w65c816s#7.1
    // @source: wdc-w65c816s#7.2
    // @source: adr-0008
    // @source: wdc-65816-manual#chapter-18-pei
    // @source: 6502org-65c816-opcodes#6.8.1
    pub(crate) fn pei(&mut self, bus: &mut impl Bus65816) {
        let value = self.effective_indirect(bus);
        self.push_value(bus, value, StackWrap::BankZero);
        self.restore_stack_page();
    }

    /// PER: fetches the 16-bit offset, spends an internal cycle at the offset's high byte,
    /// and pushes the address of the next instruction plus the offset (row 22f, 3.5.20). The
    /// sum wraps in 16 bits.
    // @source: wdc-w65c816s#table-5-7-22f
    // @source: wdc-w65c816s#3.5.20
    // @source: wdc-w65c816s#7.1
    // @source: adr-0008
    // @source: wdc-65816-manual#chapter-18-per
    // @source: 6502org-65c816-opcodes#6.8.1
    pub(crate) fn per(&mut self, bus: &mut impl Bus65816) {
        let offset = self.fetch_address(bus);
        self.internal_at_last_fetch(bus);
        let value = self.pc.wrapping_add(offset);
        self.push_value(bus, value, StackWrap::BankZero);
        self.restore_stack_page();
    }

    /// PHX and PHY: the internal cycle, then the register at the width the X flag selects.
    // @source: wdc-w65c816s#table-5-7-22c
    // @source: wdc-w65c816s#table-5-7-note-1
    fn push_index(&mut self, bus: &mut impl Bus65816, register: u16) {
        self.internal_at_program_counter(bus);
        at_width!(self.p, Flags::X, W => {
            self.push_value(bus, W::from_register(register), StackWrap::PageOne);
        });
    }

    /// PLX and PLY: the register at the width the X flag selects, setting N and Z. The high
    /// byte of an 8-bit index register stays zero.
    // @source: wdc-w65c816s#table-5-7-22b
    // @source: wdc-w65c816s#table-5-7-note-1
    fn pull_index(&mut self, bus: &mut impl Bus65816) -> u16 {
        self.pull_internal_cycles(bus);
        at_width!(self.p, Flags::X, W => {
            let value = self.pull_value::<W>(bus, StackWrap::PageOne);
            self.p.set_negative_zero(value);
            value.to_u16()
        })
    }

    /// Cycles 2 and 3 of row 22b: two internal cycles at PBR,PC+1.
    // @source: wdc-w65c816s#table-5-7-22b
    fn pull_internal_cycles(&self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.internal_at_program_counter(bus);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Cycle, emulation, native, run};

    // @source: wdc-w65c816s#table-5-7-22c
    // @source: 6502org-65c816-opcodes#6.8.2
    #[test]
    fn a_sixteen_bit_push_writes_the_high_byte_first() {
        let mut cpu = native(Flags::empty());
        cpu.a = 0xABCD;
        cpu.s = 0x1FFF;
        let bus = run(&mut cpu, &[0x48], &[]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Internal(0x12_8001),
                Cycle::Write(0x00_1FFF, 0xAB),
                Cycle::Write(0x00_1FFE, 0xCD),
            ]
        );
        assert_eq!(cpu.s, 0x1FFD);
    }

    // @source: wdc-w65c816s#table-5-7-22b
    // @source: 6502org-65c816-opcodes#6.8.2
    #[test]
    fn pla_follows_the_tutorial_example() {
        // S $01FD, m = 0, $0001FE holds $AB and $0001FF holds $CD: PLA gives $CDAB, n = 1,
        // z = 0, S $01FF.
        let mut cpu = native(Flags::Z);
        cpu.s = 0x01FD;
        let bus = run(&mut cpu, &[0x68], &[(0x00_01FE, &[0xAB, 0xCD])]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Internal(0x12_8001),
                Cycle::Internal(0x12_8001),
                Cycle::Data(0x00_01FE),
                Cycle::Data(0x00_01FF),
            ]
        );
        assert_eq!((cpu.a, cpu.s, cpu.p), (0xCDAB, 0x01FF, Flags::N));
    }

    // @source: wdc-w65c816s#table-5-7-22b
    // @source: 6502org-65c816-opcodes#6.8.3
    #[test]
    fn pld_follows_the_tutorial_example() {
        // S $01FD, $0001FE holds $AB and $0001FF holds $CD: PLD gives D $CDAB, n = 1, z = 0,
        // S $01FF. PLD is two bytes whatever the M flag.
        let mut cpu = emulation();
        cpu.s = 0x01FD;
        run(&mut cpu, &[0x2B], &[(0x00_01FE, &[0xAB, 0xCD])]);
        assert_eq!((cpu.d, cpu.s), (0xCDAB, 0x01FF));
        assert!(cpu.p.contains(Flags::N) && !cpu.p.contains(Flags::Z));
    }

    // @source: wdc-w65c816s#table-5-7-22d
    // @source: 6502org-65c816-opcodes#6.8.1
    #[test]
    fn pea_follows_the_tutorial_example() {
        // S $01FF: PEA #$1234 leaves $12 at $0001FF, $34 at $0001FE and S $01FD.
        let mut cpu = emulation();
        cpu.s = 0x01FF;
        let bus = run(&mut cpu, &[0xF4, 0x34, 0x12], &[]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Program(0x12_8002),
                Cycle::Write(0x00_01FF, 0x12),
                Cycle::Write(0x00_01FE, 0x34),
            ]
        );
        assert_eq!(cpu.s, 0x01FD);
    }

    // @source: wdc-w65c816s#7.1
    // @source: adr-0008
    #[test]
    fn emulation_pushes_of_two_bytes_leave_page_one_only_in_the_listed_instructions() {
        // PHD from S $0100 writes $000100 and $0000FF; S ends at $01FE.
        let mut cpu = emulation();
        cpu.s = 0x0100;
        cpu.d = 0x1234;
        let bus = run(&mut cpu, &[0x0B], &[]);
        assert_eq!(
            bus.cycles[2..],
            [Cycle::Write(0x00_0100, 0x12), Cycle::Write(0x00_00FF, 0x34)]
        );
        assert_eq!(cpu.s, 0x01FE);

        // PHA from S $0100 wraps to $01FF.
        let mut cpu = emulation();
        cpu.s = 0x0100;
        cpu.a = 0x0056;
        let bus = run(&mut cpu, &[0x48], &[]);
        assert_eq!(bus.cycles[2..], [Cycle::Write(0x00_0100, 0x56)]);
        assert_eq!(cpu.s, 0x01FF);
    }

    // @source: adr-0009
    #[test]
    fn emulation_plb_reads_past_page_one() {
        // PLB from S $01FF reads $000200 and leaves S at $0100.
        let mut cpu = emulation();
        cpu.s = 0x01FF;
        let bus = run(&mut cpu, &[0xAB], &[(0x00_0200, &[0x80])]);
        assert_eq!(bus.cycles[3..], [Cycle::Data(0x00_0200)]);
        assert_eq!((cpu.dbr, cpu.s), (0x80, 0x0100));
        assert!(cpu.p.contains(Flags::N));
    }

    // @source: wdc-w65c816s#table-5-7-22e
    // @source: wdc-w65c816s#7.2
    #[test]
    fn pei_reads_its_value_into_the_next_page_in_emulation_mode() {
        // D $0C00, offset $FF: the value is at $000CFF and $000D00.
        let mut cpu = emulation();
        cpu.d = 0x0C00;
        cpu.s = 0x01FF;
        let bus = run(&mut cpu, &[0xD4, 0xFF], &[(0x00_0CFF, &[0x78, 0x56])]);
        assert_eq!(
            bus.cycles[2..],
            [
                Cycle::Data(0x00_0CFF),
                Cycle::Data(0x00_0D00),
                Cycle::Write(0x00_01FF, 0x56),
                Cycle::Write(0x00_01FE, 0x78),
            ]
        );
    }

    // @source: wdc-w65c816s#table-5-7-22f
    // @source: wdc-w65c816s#3.5.20
    #[test]
    fn per_pushes_the_next_instruction_address_plus_the_offset() {
        // At $12:8000, PER +$0010 pushes $8013; the internal cycle is at PBR,PC+2.
        let mut cpu = native(Flags::empty());
        cpu.s = 0x01FF;
        let bus = run(&mut cpu, &[0x62, 0x10, 0x00], &[]);
        assert_eq!(
            bus.cycles[3..],
            [
                Cycle::Internal(0x12_8002),
                Cycle::Write(0x00_01FF, 0x80),
                Cycle::Write(0x00_01FE, 0x13),
            ]
        );
    }

    // @source: wdc-w65c816s#table-5-7-note-12
    // @source: 6502org-65c816-opcodes#6.8.3
    #[test]
    fn plp_keeps_m_and_x_set_in_emulation_mode() {
        let mut cpu = emulation();
        cpu.s = 0x01FE;
        run(&mut cpu, &[0x28], &[(0x00_01FF, &[0x00])]);
        assert_eq!(cpu.p, Flags::M | Flags::X);

        // In native mode, setting X clears the high bytes of X and Y.
        let mut cpu = native(Flags::empty());
        cpu.x = 0x1234;
        cpu.s = 0x01FE;
        run(&mut cpu, &[0x28], &[(0x00_01FF, &[0x10])]);
        assert_eq!((cpu.p, cpu.x), (Flags::X, 0x0034));
    }
}
