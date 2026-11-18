// @source: wdc-w65c816s#table-5-5
// @source: 6502org-65c816-opcodes#6.1.3
// @source: 6502org-65c816-opcodes#6.1.1.3
//! Shifts, rotates, increments and decrements: ASL, LSR, ROL, ROR, INC and DEC on the
//! accumulator or memory, and INX, INY, DEX and DEY.
//!
//! The memory forms are read-modify-write instructions and use `modify_operand` (ADR 0006).
//! The accumulator and index forms are one opcode fetch and one internal cycle (rows 8 and
//! 19a of Table 5-7).

use crate::{
    addressing::Address,
    bus::Bus65816,
    registers::{Cpu65816, Flags},
    width::{Width, at_width},
};

/// The operation of a shift, rotate, increment or decrement.
// @source: wdc-w65c816s#table-5-5
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Modify {
    /// ASL: "C←15/7 … 1 0←0".
    ShiftLeft,
    /// LSR: "0 → 15/7 … 1 0 → C".
    ShiftRight,
    /// ROL: the carry enters bit 0 and the top bit goes to C.
    RotateLeft,
    /// ROR: the carry enters the top bit and bit 0 goes to C.
    RotateRight,
    /// INC: adds one.
    Increment,
    /// DEC: subtracts one.
    Decrement,
}

impl Modify {
    /// The operation on `value` at width `W`. Sets N and Z from the result, and C from the bit
    /// shifted out for the shifts and rotates; INC and DEC leave C alone.
    // @source: 6502org-65c816-opcodes#6.1.3
    // @source: 6502org-65c816-opcodes#6.1.1.3
    fn apply<W: Width>(self, flags: &mut Flags, value: W) -> W {
        let value = value.to_u16();
        let carry_in = flags.contains(Flags::C);
        let (result, carry_out) = match self {
            Self::ShiftLeft => (value << 1, Some(value & W::SIGN != 0)),
            Self::ShiftRight => (value >> 1, Some(value & 1 != 0)),
            Self::RotateLeft => (value << 1 | u16::from(carry_in), Some(value & W::SIGN != 0)),
            Self::RotateRight => {
                let top = if carry_in { W::SIGN } else { 0 };
                (value >> 1 | top, Some(value & 1 != 0))
            }
            Self::Increment => (value.wrapping_add(1), None),
            Self::Decrement => (value.wrapping_sub(1), None),
        };
        let result = W::from_register(result);
        flags.set_negative_zero(result);
        if let Some(carry_out) = carry_out {
            flags.set(Flags::C, carry_out);
        }
        result
    }
}

impl Cpu65816 {
    /// The accumulator form (row 8): the operation on A or C, by the M flag, after one
    /// internal cycle at PBR,PC+1. An 8-bit result leaves B alone.
    // @source: wdc-w65c816s#table-5-7-8
    // @source: 6502org-65c816-opcodes#6.1.3
    pub(crate) fn modify_accumulator(&mut self, bus: &mut impl Bus65816, operation: Modify) {
        self.internal_at_program_counter(bus);
        at_width!(self.p, Flags::M, W => {
            let value = self.accumulator::<W>();
            let result = operation.apply(&mut self.p, value);
            self.set_accumulator(result);
        });
    }

    /// The memory form: the operation on the operand at `address`, by the M flag, as a
    /// read-modify-write instruction (rows 1d, 6b, 10b and 16b).
    // @source: wdc-w65c816s#table-5-7-1d
    // @source: wdc-w65c816s#table-5-7-6b
    // @source: wdc-w65c816s#table-5-7-10b
    // @source: wdc-w65c816s#table-5-7-16b
    pub(crate) fn modify_memory(
        &mut self,
        bus: &mut impl Bus65816,
        address: Address,
        operation: Modify,
    ) {
        at_width!(self.p, Flags::M, W => {
            self.modify_operand::<W>(bus, address, |cpu, value| {
                operation.apply(&mut cpu.p, value)
            });
        });
    }

    /// INX: "X+1 → X", at the width the X flag selects.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#table-5-7-19a
    pub(crate) fn inx(&mut self, bus: &mut impl Bus65816) {
        self.x = self.modify_index(bus, self.x, Modify::Increment);
    }

    /// INY: "Y+1 → Y", at the width the X flag selects.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#table-5-7-19a
    pub(crate) fn iny(&mut self, bus: &mut impl Bus65816) {
        self.y = self.modify_index(bus, self.y, Modify::Increment);
    }

    /// DEX: "X-1 → X", at the width the X flag selects. Table 5-5 prints the destination as
    /// A, a misprint (ADR 0012).
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-65816-manual#chapter-18-dex
    // @source: 6502org-65c816-opcodes#6.1.1.3
    // @source: adr-0012
    pub(crate) fn dex(&mut self, bus: &mut impl Bus65816) {
        self.x = self.modify_index(bus, self.x, Modify::Decrement);
    }

    /// DEY: "Y-1 → Y", at the width the X flag selects.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#table-5-7-19a
    pub(crate) fn dey(&mut self, bus: &mut impl Bus65816) {
        self.y = self.modify_index(bus, self.y, Modify::Decrement);
    }

    /// An implied index-register operation (row 19a): one internal cycle at PBR,PC+1, then
    /// the operation at the width the X flag selects. Returns the new register value; the
    /// high byte of an 8-bit index register stays zero.
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: 6502org-65c816-opcodes#6.1.1.3
    fn modify_index(&mut self, bus: &mut impl Bus65816, register: u16, operation: Modify) -> u16 {
        self.internal_at_program_counter(bus);
        at_width!(self.p, Flags::X, W => {
            operation.apply(&mut self.p, W::from_register(register)).to_u16()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Cycle, emulation, native, run};

    // @source: 6502org-65c816-opcodes#6.1.3
    #[test]
    fn asl_follows_the_tutorial_example() {
        // DBR $12, m = 1, $12ABCD holds $8F: ASL $ABCD writes $1E, n = 0, z = 0, c = 1.
        let mut cpu = native(Flags::M | Flags::N | Flags::Z);
        cpu.dbr = 0x12;
        let bus = run(&mut cpu, &[0x0E, 0xCD, 0xAB], &[(0x12_ABCD, &[0x8F])]);
        assert_eq!(bus.cycles.last(), Some(&Cycle::Write(0x12_ABCD, 0x1E)));
        assert_eq!(cpu.p, Flags::M | Flags::C);
    }

    // @source: 6502org-65c816-opcodes#6.1.1.3
    #[test]
    fn inx_follows_the_tutorial_example() {
        // X $7FFF, x = 0: INX gives $8000, n = 1, z = 0.
        let mut cpu = native(Flags::Z);
        cpu.x = 0x7FFF;
        let bus = run(&mut cpu, &[0xE8], &[]);
        assert_eq!(cpu.x, 0x8000);
        assert_eq!(cpu.p, Flags::N);
        assert_eq!(
            bus.cycles,
            [Cycle::Opcode(0x12_8000), Cycle::Internal(0x12_8001)]
        );
    }

    // @source: 6502org-65c816-opcodes#6.1.3
    #[test]
    fn rotates_move_the_carry_through_the_top_bit_of_the_width() {
        // ROR A with m = 1 and C set: $01 becomes $80 with C set, and B is kept.
        let mut cpu = native(Flags::M | Flags::C);
        cpu.a = 0x3401;
        run(&mut cpu, &[0x6A], &[]);
        assert_eq!(cpu.a, 0x3480);
        assert_eq!(cpu.p, Flags::M | Flags::N | Flags::C);

        // ROL A with m = 0 and C clear: $8000 becomes $0000 with C and Z set.
        let mut cpu = native(Flags::empty());
        cpu.a = 0x8000;
        run(&mut cpu, &[0x2A], &[]);
        assert_eq!(cpu.a, 0x0000);
        assert_eq!(cpu.p, Flags::Z | Flags::C);
    }

    // @source: 6502org-65c816-opcodes#6.1.3
    #[test]
    fn lsr_clears_n() {
        let mut cpu = emulation();
        cpu.a = 0x00FF;
        cpu.p |= Flags::N;
        run(&mut cpu, &[0x4A], &[]);
        assert_eq!(cpu.a, 0x007F);
        assert!(cpu.p.contains(Flags::C));
        assert!(!cpu.p.intersects(Flags::N | Flags::Z));
    }

    // @source: 6502org-65c816-opcodes#6.1.1.3
    #[test]
    fn eight_bit_dey_wraps_in_the_low_byte_and_inc_leaves_carry_alone() {
        let mut cpu = native(Flags::X | Flags::C);
        cpu.y = 0x0000;
        run(&mut cpu, &[0x88], &[]);
        assert_eq!(cpu.y, 0x00FF);
        assert_eq!(cpu.p, Flags::X | Flags::C | Flags::N);
    }

    // @source: wdc-w65c816s#table-5-7-6b
    // @source: adr-0006
    #[test]
    fn absolute_x_memory_form_always_spends_the_indexing_cycle() {
        let mut cpu = native(Flags::M | Flags::X);
        cpu.dbr = 0x34;
        cpu.x = 0x01;
        let bus = run(&mut cpu, &[0xFE, 0x00, 0x56], &[(0x34_5601, &[0x41])]);
        assert_eq!(
            bus.cycles[3..],
            [
                Cycle::Internal(0x34_5601),
                Cycle::Data(0x34_5601),
                Cycle::Internal(0x34_5601),
                Cycle::Write(0x34_5601, 0x42),
            ]
        );
    }
}
