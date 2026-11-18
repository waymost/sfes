// @source: wdc-w65c816s#table-5-5
// @source: wdc-w65c816s#7.20
// @source: wdc-w65c816s#7.21
// @source: wdc-65816-manual#chapter-18
// @source: 6502org-65c816-opcodes#6.10.1
// @source: 6502org-65c816-opcodes#6.10.2
// @source: 6502org-65c816-opcodes#6.10.3
//! Register transfers (TAX, TAY, TXA, TYA, TXY, TYX, TXS, TSX, TCD, TDC, TCS, TSC) and XBA.
//!
//! Every transfer copies the whole 16-bit source and the destination's width decides how
//! much of it is kept (datasheet 7.20): A by the M flag, X and Y by the X flag. D, S and the
//! C forms are always 16 bits wide, except that S stays in page 1 in emulation mode (7.21).
//! All but TXS and TCS set N and Z from the value stored.

use crate::{
    bus::Bus65816,
    registers::{Cpu65816, Flags},
    width::{Width, at_width},
};

impl Cpu65816 {
    /// TAX: "A → X", 16 bits (B and A) into a 16-bit X.
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-65816-manual#chapter-18-tax
    // @source: 6502org-65c816-opcodes#6.10.1
    pub(crate) fn tax(&mut self, bus: &mut impl Bus65816) {
        self.x = self.transfer_to_index(bus, self.a);
    }

    /// TAY: "A → Y", 16 bits (B and A) into a 16-bit Y. Its opcode is `A8` (Table 5-4); the
    /// `AB` of Table 5-5 is a misprint (ADR 0012).
    // @source: wdc-w65c816s#table-5-4
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-65816-manual#chapter-18-tay
    // @source: 6502org-65c816-opcodes#6.10.1
    // @source: adr-0012
    pub(crate) fn tay(&mut self, bus: &mut impl Bus65816) {
        self.y = self.transfer_to_index(bus, self.a);
    }

    /// TSX: "S → X"; an 8-bit X takes the low byte of S.
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-65816-manual#chapter-18-tsx
    // @source: 6502org-65c816-opcodes#6.10.1
    pub(crate) fn tsx(&mut self, bus: &mut impl Bus65816) {
        self.x = self.transfer_to_index(bus, self.s);
    }

    /// TXY: "X → Y".
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-65816-manual#chapter-18-txy
    // @source: 6502org-65c816-opcodes#6.10.1
    pub(crate) fn txy(&mut self, bus: &mut impl Bus65816) {
        self.y = self.transfer_to_index(bus, self.x);
    }

    /// TYX: "Y → X".
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-65816-manual#chapter-18-tyx
    // @source: 6502org-65c816-opcodes#6.10.1
    pub(crate) fn tyx(&mut self, bus: &mut impl Bus65816) {
        self.x = self.transfer_to_index(bus, self.y);
    }

    /// TXA: "X → A"; an 8-bit A leaves B alone.
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-65816-manual#chapter-18-txa
    // @source: 6502org-65c816-opcodes#6.10.1
    pub(crate) fn txa(&mut self, bus: &mut impl Bus65816) {
        self.transfer_to_accumulator(bus, self.x);
    }

    /// TYA: "Y → A"; an 8-bit A leaves B alone.
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-65816-manual#chapter-18-tya
    // @source: 6502org-65c816-opcodes#6.10.1
    pub(crate) fn tya(&mut self, bus: &mut impl Bus65816) {
        self.transfer_to_accumulator(bus, self.y);
    }

    /// TXS: "X → S", no flags. In emulation mode only the low byte is kept and S stays in
    /// page 1; in native mode an 8-bit X puts S in page 0.
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-w65c816s#7.21
    // @source: wdc-65816-manual#chapter-18-txs
    // @source: 6502org-65c816-opcodes#6.10.1
    pub(crate) fn txs(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.set_stack_pointer(self.x);
    }

    /// TCS: "C → S", 16 bits whatever the M flag, no flags. In emulation mode "a 01 is
    /// forced into SH" (7.21).
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-w65c816s#7.20
    // @source: wdc-w65c816s#7.21
    // @source: wdc-65816-manual#chapter-18-tcs
    // @source: 6502org-65c816-opcodes#6.10.2
    pub(crate) fn tcs(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.set_stack_pointer(self.a);
    }

    /// TCD: "C → D", 16 bits whatever the M flag; N and Z from the 16-bit value.
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-w65c816s#7.20
    // @source: wdc-65816-manual#chapter-18-tcd
    // @source: 6502org-65c816-opcodes#6.10.2
    pub(crate) fn tcd(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.d = self.a;
        self.p.set_negative_zero(self.d);
    }

    /// TDC: "D → C", 16 bits whatever the M flag; N and Z from the 16-bit value.
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-w65c816s#7.20
    // @source: wdc-65816-manual#chapter-18-tdc
    // @source: 6502org-65c816-opcodes#6.10.2
    pub(crate) fn tdc(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.a = self.d;
        self.p.set_negative_zero(self.a);
    }

    /// TSC: "S → C", 16 bits whatever the M flag; N and Z from the 16-bit value.
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-w65c816s#7.21
    // @source: wdc-65816-manual#chapter-18-tsc
    // @source: 6502org-65c816-opcodes#6.10.2
    pub(crate) fn tsc(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.a = self.s;
        self.p.set_negative_zero(self.a);
    }

    /// XBA: "B → A", an exchange of the two accumulator bytes after two internal cycles
    /// (row 19b). N and Z come from the new A, 8 bits wide, whatever the M flag.
    // @source: wdc-w65c816s#table-5-7-19b
    // @source: wdc-w65c816s#2.4
    // @source: wdc-65816-manual#chapter-18-xba
    // @source: 6502org-65c816-opcodes#6.10.3
    pub(crate) fn xba(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.internal_at_program_counter(bus);
        self.a = self.a.swap_bytes();
        self.p
            .set_negative_zero(<u8 as Width>::from_register(self.a));
    }

    /// The internal cycle of row 19a, then `value` into X or Y at the width the X flag
    /// selects, setting N and Z. An 8-bit index register keeps the low byte, with its high
    /// byte zero.
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-w65c816s#7.20
    // @source: wdc-w65c816s#2.7
    fn transfer_to_index(&mut self, bus: &mut impl Bus65816, value: u16) -> u16 {
        self.internal_at_program_counter(bus);
        at_width!(self.p, Flags::X, W => {
            let value = W::from_register(value);
            self.p.set_negative_zero(value);
            value.to_u16()
        })
    }

    /// The internal cycle of row 19a, then `value` into the accumulator at the width the M
    /// flag selects, setting N and Z.
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-w65c816s#7.20
    // @source: wdc-w65c816s#2.4
    fn transfer_to_accumulator(&mut self, bus: &mut impl Bus65816, value: u16) {
        self.internal_at_program_counter(bus);
        at_width!(self.p, Flags::M, W => {
            let value = W::from_register(value);
            self.p.set_negative_zero(value);
            self.set_accumulator(value);
        });
    }

    /// Stores `value` in S, with SH forced to `01` in emulation mode.
    // @source: wdc-w65c816s#7.21
    // @source: wdc-w65c816s#2.11
    fn set_stack_pointer(&mut self, value: u16) {
        self.s = value;
        self.restore_stack_page();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Cycle, emulation, native, run};

    // @source: 6502org-65c816-opcodes#6.10.1
    #[test]
    fn txa_follows_the_tutorial_example() {
        // A $1234, X $ABCD, m = 1: TXA gives $12CD, n = 1, z = 0.
        let mut cpu = native(Flags::M);
        cpu.a = 0x1234;
        cpu.x = 0xABCD;
        let bus = run(&mut cpu, &[0x8A], &[]);
        assert_eq!(cpu.a, 0x12CD);
        assert_eq!(cpu.p, Flags::M | Flags::N);
        assert_eq!(
            bus.cycles,
            [Cycle::Opcode(0x12_8000), Cycle::Internal(0x12_8001)]
        );
    }

    // @source: wdc-65816-manual#chapter-18-tax
    #[test]
    fn tax_with_an_eight_bit_accumulator_copies_b_into_a_sixteen_bit_x() {
        let mut cpu = native(Flags::M);
        cpu.a = 0x8001;
        run(&mut cpu, &[0xAA], &[]);
        assert_eq!(cpu.x, 0x8001);
        assert!(cpu.p.contains(Flags::N));
    }

    // @source: 6502org-65c816-opcodes#6.10.2
    #[test]
    fn tdc_follows_the_tutorial_example() {
        // D $1234: TDC gives C $1234, n = 0, z = 0, even with m = 1.
        let mut cpu = native(Flags::M | Flags::Z | Flags::N);
        cpu.d = 0x1234;
        run(&mut cpu, &[0x7B], &[]);
        assert_eq!(cpu.a, 0x1234);
        assert_eq!(cpu.p, Flags::M);
    }

    // @source: 6502org-65c816-opcodes#6.10.3
    #[test]
    fn xba_follows_the_tutorial_example() {
        // A $6789: XBA gives $8967, n = 0, z = 0, in two internal cycles.
        let mut cpu = native(Flags::N | Flags::Z);
        cpu.a = 0x6789;
        let bus = run(&mut cpu, &[0xEB], &[]);
        assert_eq!(cpu.a, 0x8967);
        assert_eq!(cpu.p, Flags::empty());
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Internal(0x12_8001),
                Cycle::Internal(0x12_8001),
            ]
        );
    }

    // @source: wdc-w65c816s#7.21
    // @source: 6502org-65c816-opcodes#6.10.1
    #[test]
    fn txs_keeps_the_stack_in_page_one_only_in_emulation_mode() {
        let mut cpu = emulation();
        cpu.x = 0x0042;
        run(&mut cpu, &[0x9A], &[]);
        assert_eq!(cpu.s, 0x0142);

        // In 8-bit native mode the stack moves to page 0.
        let mut cpu = native(Flags::X);
        cpu.x = 0x0042;
        run(&mut cpu, &[0x9A], &[]);
        assert_eq!(cpu.s, 0x0042);
    }

    // @source: wdc-w65c816s#7.21
    // @source: wdc-65816-manual#chapter-18-tcs
    #[test]
    fn tcs_transfers_sixteen_bits_in_native_mode_and_the_low_byte_in_emulation_mode() {
        let mut cpu = native(Flags::M);
        cpu.a = 0x1F80;
        run(&mut cpu, &[0x1B], &[]);
        assert_eq!(cpu.s, 0x1F80);

        let mut cpu = emulation();
        cpu.a = 0x1F80;
        run(&mut cpu, &[0x1B], &[]);
        assert_eq!(cpu.s, 0x0180);
        assert_eq!(cpu.p, Flags::M | Flags::X | Flags::I);
    }
}
