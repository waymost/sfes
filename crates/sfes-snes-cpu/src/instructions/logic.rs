// @source: wdc-w65c816s#table-5-5
// @source: 6502org-65c816-opcodes#6.1.2.1
//! Logic and bit tests: ORA, AND, EOR, BIT, TSB and TRB. All work at the accumulator's
//! width, 8 bits when the M flag is set and 16 bits otherwise.

use crate::{
    addressing::{Address, Mode},
    bus::Bus65816,
    registers::{Cpu65816, Flags},
    width::Width,
};

/// A bitwise operation that combines the accumulator with memory into the accumulator.
// @source: 6502org-65c816-opcodes#6.1.2.1
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Bitwise {
    /// "A V M → A"
    Or,
    /// "A^M → A"
    And,
    /// "A xv M → A"
    ExclusiveOr,
}

impl Bitwise {
    /// The operation on two values of the same width.
    // @source: wdc-w65c816s#table-5-5
    fn apply(self, accumulator: u16, data: u16) -> u16 {
        match self {
            Self::Or => accumulator | data,
            Self::And => accumulator & data,
            Self::ExclusiveOr => accumulator ^ data,
        }
    }
}

impl Cpu65816 {
    /// ORA: "A V M → A", setting N and Z from the result.
    // @source: wdc-w65c816s#table-5-5
    // @source: 6502org-65c816-opcodes#6.1.2.1
    pub(crate) fn ora(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        self.bitwise(bus, mode, Bitwise::Or);
    }

    /// AND: "A^M → A", setting N and Z from the result.
    // @source: wdc-w65c816s#table-5-5
    // @source: 6502org-65c816-opcodes#6.1.2.1
    pub(crate) fn and(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        self.bitwise(bus, mode, Bitwise::And);
    }

    /// EOR: "A xv M → A", setting N and Z from the result.
    // @source: wdc-w65c816s#table-5-5
    // @source: 6502org-65c816-opcodes#6.1.2.1
    pub(crate) fn eor(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        self.bitwise(bus, mode, Bitwise::ExclusiveOr);
    }

    /// BIT: sets Z from "A ^ M" and leaves A alone. Except in immediate mode, N and V take
    /// the two highest bits of the data itself (Table 5-5 note 2; the manual's BIT entry).
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#table-5-5-note-2
    // @source: wdc-65816-manual#chapter-18-bit
    pub(crate) fn bit(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        if self.p.contains(Flags::M) {
            self.test_bits::<u8>(bus, mode);
        } else {
            self.test_bits::<u16>(bus, mode);
        }
    }

    /// TSB: sets the memory bits that are set in A, "AVM → M", and sets Z from A AND M as
    /// BIT does. A read-modify-write instruction.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-65816-manual#chapter-18-tsb
    // @source: 6502org-65c816-opcodes#6.1.2.3
    pub(crate) fn tsb(&mut self, bus: &mut impl Bus65816, address: Address) {
        if self.p.contains(Flags::M) {
            self.modify_operand::<u8>(bus, address, |cpu, data| cpu.test_and_change(data, true));
        } else {
            self.modify_operand::<u16>(bus, address, |cpu, data| cpu.test_and_change(data, true));
        }
    }

    /// TRB: clears the memory bits that are set in A, and sets Z from A AND M as BIT does. A
    /// read-modify-write instruction.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-65816-manual#chapter-18-trb
    // @source: 6502org-65c816-opcodes#6.1.2.3
    pub(crate) fn trb(&mut self, bus: &mut impl Bus65816, address: Address) {
        if self.p.contains(Flags::M) {
            self.modify_operand::<u8>(bus, address, |cpu, data| cpu.test_and_change(data, false));
        } else {
            self.modify_operand::<u16>(bus, address, |cpu, data| cpu.test_and_change(data, false));
        }
    }

    /// ORA, AND or EOR at the width the M flag selects.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.4
    fn bitwise(&mut self, bus: &mut impl Bus65816, mode: Mode, operation: Bitwise) {
        if self.p.contains(Flags::M) {
            self.bitwise_at::<u8>(bus, mode, operation);
        } else {
            self.bitwise_at::<u16>(bus, mode, operation);
        }
    }

    /// `operation` on the accumulator and the operand at width `W`; the result goes to the
    /// accumulator and sets N and Z. An 8-bit result leaves B alone.
    // @source: wdc-w65c816s#table-5-5
    // @source: 6502org-65c816-opcodes#6.1.2.1
    fn bitwise_at<W: Width>(&mut self, bus: &mut impl Bus65816, mode: Mode, operation: Bitwise) {
        let data = self.read_operand::<W>(bus, mode);
        let accumulator = self.accumulator::<W>();
        let result = W::from_register(operation.apply(accumulator.to_u16(), data.to_u16()));
        self.set_accumulator(result);
        self.p.set_negative_zero(result);
    }

    /// BIT at width `W`.
    // @source: wdc-65816-manual#chapter-18-bit
    // @source: wdc-w65c816s#table-5-5-note-2
    fn test_bits<W: Width>(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        let data = self.read_operand::<W>(bus, mode).to_u16();
        self.set_zero_from_and::<W>(data);
        if mode != Mode::Immediate {
            self.p.set(Flags::N, data & W::SIGN != 0);
            self.p.set(Flags::V, data & (W::SIGN >> 1) != 0);
        }
    }

    /// The modification of TSB (`set`) or TRB: Z from A AND the data, then the data with the
    /// accumulator's bits set or cleared.
    // @source: wdc-65816-manual#chapter-18-tsb
    // @source: wdc-65816-manual#chapter-18-trb
    fn test_and_change<W: Width>(&mut self, data: W, set: bool) -> W {
        let data = data.to_u16();
        self.set_zero_from_and::<W>(data);
        let accumulator = self.accumulator::<W>().to_u16();
        let result = if set {
            data | accumulator
        } else {
            data & !accumulator
        };
        W::from_register(result)
    }

    /// Sets Z when the accumulator AND `data` is zero at width `W`, and clears it otherwise.
    // @source: wdc-65816-manual#chapter-18-bit
    fn set_zero_from_and<W: Width>(&mut self, data: u16) {
        let accumulator = self.accumulator::<W>().to_u16();
        self.p.set(Flags::Z, accumulator & data == 0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Cycle, native, run};

    // @source: 6502org-65c816-opcodes#6.1.2.1
    #[test]
    fn eor_follows_the_tutorial_example() {
        // Accumulator $0F06, m = 0, EOR #$F103: $FE05, n = 1, z = 0.
        let mut cpu = native(Flags::Z);
        cpu.a = 0x0F06;
        run(&mut cpu, &[0x49, 0x03, 0xF1], &[]);
        assert_eq!(cpu.a, 0xFE05);
        assert_eq!(cpu.p, Flags::N);
    }

    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.4
    #[test]
    fn eight_bit_ora_and_and_keep_b() {
        let mut cpu = native(Flags::M);
        cpu.a = 0xAB0F;
        run(&mut cpu, &[0x09, 0xF0], &[]);
        assert_eq!(cpu.a, 0xABFF);
        assert_eq!(cpu.p, Flags::M | Flags::N);
        run(&mut cpu, &[0x29, 0x00], &[]);
        assert_eq!(cpu.a, 0xAB00);
        assert_eq!(cpu.p, Flags::M | Flags::Z);
    }

    // @source: 6502org-65c816-opcodes#6.1.2.2
    #[test]
    fn bit_follows_the_tutorial_example() {
        // Accumulator $43, DBR $12, m = 1, $12ABCD holds $9C: n = 1, v = 0, z = 1.
        let mut cpu = native(Flags::M | Flags::V);
        cpu.a = 0x43;
        cpu.dbr = 0x12;
        run(&mut cpu, &[0x2C, 0xCD, 0xAB], &[(0x12_ABCD, &[0x9C])]);
        assert_eq!(cpu.a, 0x43);
        assert_eq!(cpu.p, Flags::M | Flags::N | Flags::Z);
    }

    // @source: wdc-w65c816s#table-5-5-note-2
    #[test]
    fn sixteen_bit_bit_takes_n_and_v_from_bits_15_and_14_except_when_immediate() {
        let mut cpu = native(Flags::empty());
        cpu.a = 0x0001;
        run(&mut cpu, &[0x24, 0x10], &[(0x00_0010, &[0x00, 0x40])]);
        assert_eq!(cpu.p, Flags::V | Flags::Z);

        let mut cpu = native(Flags::N);
        cpu.a = 0x0001;
        run(&mut cpu, &[0x89, 0x01, 0x40], &[]);
        assert_eq!(cpu.p, Flags::N);
    }

    // @source: 6502org-65c816-opcodes#6.1.2.3
    #[test]
    fn tsb_and_trb_follow_the_tutorial_example() {
        // Accumulator $43, DBR $12, m = 1, $12ABCD holds $9C: TSB writes $DF, z = 1.
        let mut cpu = native(Flags::M);
        cpu.a = 0x43;
        cpu.dbr = 0x12;
        let bus = run(&mut cpu, &[0x0C, 0xCD, 0xAB], &[(0x12_ABCD, &[0x9C])]);
        assert_eq!(bus.cycles.last(), Some(&Cycle::Write(0x12_ABCD, 0xDF)));
        assert_eq!(cpu.p, Flags::M | Flags::Z);

        // TRB clears bits 0, 1 and 6 of $DF.
        let mut cpu = native(Flags::M);
        cpu.a = 0x43;
        cpu.dbr = 0x12;
        let bus = run(&mut cpu, &[0x1C, 0xCD, 0xAB], &[(0x12_ABCD, &[0xDF])]);
        assert_eq!(bus.cycles.last(), Some(&Cycle::Write(0x12_ABCD, 0x9C)));
        assert_eq!(cpu.p, Flags::M);
    }
}
