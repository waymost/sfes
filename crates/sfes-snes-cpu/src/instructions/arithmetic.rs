// @source: wdc-w65c816s#table-5-5
// @source: 6502org-65c816-opcodes#6.1.1.1
// @source: 6502org-65c816-opcodes#6.1.1.2
// @source: adr-0007
//! Arithmetic and compares: ADC and SBC in binary and decimal mode, CMP, CPX and CPY.
//!
//! ADC and SBC work at the accumulator's width and in decimal (BCD) arithmetic when the D
//! flag is set; the compares are always binary and ignore the carry. Decimal mode adds no
//! cycles on the 65C816 (Table 7-1), so it changes only the result and the flags. How
//! decimal results are formed, for any digit values, and what V means in decimal mode is
//! ADR 0007.

use crate::{
    addressing::Mode,
    bus::Bus65816,
    registers::{Cpu65816, Flags},
    width::{Width, at_width},
};

/// What an addition or subtraction produced, before it is written to the registers.
// @source: adr-0007
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Sum {
    /// The result, at the operation's width.
    result: u16,
    /// The carry out: an unsigned carry for addition, no borrow for subtraction.
    carry: bool,
    /// Signed overflow.
    overflow: bool,
}

impl Cpu65816 {
    /// ADC: "A+M+C → A", setting N, V, Z and C.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-65816-manual#chapter-18-adc
    pub(crate) fn adc(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        at_width!(self.p, Flags::M, W => self.add_with_carry::<W>(bus, mode, false));
    }

    /// SBC: "A – M – (C) → A", subtracting one more when the carry is clear, and setting N,
    /// V, Z and C (set when no borrow was needed).
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-65816-manual#chapter-18-sbc
    pub(crate) fn sbc(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        at_width!(self.p, Flags::M, W => self.add_with_carry::<W>(bus, mode, true));
    }

    /// CMP: "A-M", setting N, Z and C from the result and discarding it.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-65816-manual#chapter-18-cmp
    pub(crate) fn cmp(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        at_width!(self.p, Flags::M, W => self.compare::<W>(bus, mode, self.a));
    }

    /// CPX: "X-M", at the width the X flag selects.
    // @source: wdc-w65c816s#table-5-5
    // @source: 6502org-65c816-opcodes#6.1.1.2
    pub(crate) fn cpx(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        self.compare_index(bus, mode, self.x);
    }

    /// CPY: "Y-M", at the width the X flag selects.
    // @source: wdc-w65c816s#table-5-5
    // @source: 6502org-65c816-opcodes#6.1.1.2
    pub(crate) fn cpy(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        self.compare_index(bus, mode, self.y);
    }

    /// ADC, or SBC when `subtract` is set, at width `W`: binary or decimal by the D flag. The
    /// result goes to the accumulator; an 8-bit result leaves B alone.
    // @source: 6502org-65c816-opcodes#6.1.1.1
    // @source: adr-0007
    fn add_with_carry<W: Width>(&mut self, bus: &mut impl Bus65816, mode: Mode, subtract: bool) {
        let data = self.read_operand::<W>(bus, mode).to_u16();
        let accumulator = self.accumulator::<W>().to_u16();
        let carry = self.p.contains(Flags::C);
        let sum = match (self.p.contains(Flags::D), subtract) {
            (false, false) => binary_add::<W>(accumulator, data, carry),
            (false, true) => binary_add::<W>(accumulator, !data, carry),
            (true, false) => decimal_add::<W>(accumulator, data, carry),
            (true, true) => decimal_subtract::<W>(accumulator, data, carry),
        };
        let Sum {
            result,
            carry,
            overflow,
        } = sum;
        let result = W::from_register(result);
        self.set_accumulator(result);
        self.p.set_negative_zero(result);
        self.p.set(Flags::C, carry);
        self.p.set(Flags::V, overflow);
    }

    /// CPX or CPY at the width the X flag selects.
    // @source: 6502org-65c816-opcodes#6.1.1.2
    fn compare_index(&mut self, bus: &mut impl Bus65816, mode: Mode, register: u16) {
        at_width!(self.p, Flags::X, W => self.compare::<W>(bus, mode, register));
    }

    /// `register` minus the operand at width `W`, always binary and without the carry: N and
    /// Z from the difference, C set when the register is greater than or equal to the data.
    /// V and the register are unchanged.
    // @source: 6502org-65c816-opcodes#6.1.1.2
    // @source: wdc-65816-manual#chapter-18-cmp
    fn compare<W: Width>(&mut self, bus: &mut impl Bus65816, mode: Mode, register: u16) {
        let data = self.read_operand::<W>(bus, mode).to_u16();
        let register = W::from_register(register).to_u16();
        let difference = W::from_register(register.wrapping_sub(data));
        self.p.set_negative_zero(difference);
        self.p.set(Flags::C, register >= data);
    }
}

/// The number of bits in width `W`.
// @source: wdc-w65c816s#2.4
fn bits<W: Width>() -> u32 {
    W::SIGN.trailing_zeros() + 1
}

/// The mask of all bits of width `W`.
// @source: wdc-w65c816s#2.4
fn mask<W: Width>() -> u16 {
    W::SIGN | (W::SIGN - 1)
}

/// Signed overflow of `left + right`, given the sum: the operands have the same sign and the
/// sum's sign differs from it.
// @source: 6502org-65c816-opcodes#6.1.1.1
fn signed_overflow<W: Width>(left: u16, right: u16, sum: u16) -> bool {
    !(left ^ right) & (left ^ sum) & W::SIGN != 0
}

/// Binary `accumulator + data + carry` at width `W`. SBC passes the complement of the data,
/// which makes it "accumulator - data - 1 + carry".
// @source: 6502org-65c816-opcodes#6.1.1.1
fn binary_add<W: Width>(accumulator: u16, data: u16, carry: bool) -> Sum {
    let data = data & mask::<W>();
    let total = u32::from(accumulator) + u32::from(data) + u32::from(carry);
    let [low, high, ..] = total.to_le_bytes();
    let result = u16::from_le_bytes([low, high]) & mask::<W>();
    Sum {
        result,
        carry: total >> bits::<W>() != 0,
        overflow: signed_overflow::<W>(accumulator, data, result),
    }
}

/// Decimal `accumulator + data + carry` at width `W`, one 4-bit digit at a time from the
/// lowest: a digit sum above 9 is adjusted by 6 and carries into the next digit. V is the
/// signed overflow of the sum as it stands before the top digit is adjusted (ADR 0007).
// @source: wdc-65816-manual#chapter-18-adc
// @source: adr-0007
fn decimal_add<W: Width>(accumulator: u16, data: u16, carry: bool) -> Sum {
    let digits = bits::<W>() / 4;
    let mut result = 0_u16;
    let mut carry = u16::from(carry);
    let mut unadjusted = 0_u16;
    for digit in 0..digits {
        let shift = 4 * digit;
        let mut sum = (accumulator >> shift & 0xF) + (data >> shift & 0xF) + carry;
        if digit + 1 == digits {
            unadjusted = result | (sum & 0xF) << shift;
        }
        if sum > 9 {
            sum += 6;
        }
        carry = u16::from(sum > 0xF);
        result |= (sum & 0xF) << shift;
    }
    Sum {
        result,
        carry: carry != 0,
        overflow: signed_overflow::<W>(accumulator, data, unadjusted),
    }
}

/// Decimal `accumulator - data - 1 + carry` at width `W`, one 4-bit digit at a time from the
/// lowest: a digit difference below 0 is adjusted by 6 and borrows from the next digit. C is
/// set when no borrow is left. V is the signed overflow of the binary subtraction as it
/// stands before the top digit is adjusted (ADR 0007).
// @source: wdc-65816-manual#chapter-18-sbc
// @source: adr-0007
fn decimal_subtract<W: Width>(accumulator: u16, data: u16, carry: bool) -> Sum {
    let digits = bits::<W>() / 4;
    let mut result = 0_u16;
    let mut borrow = u16::from(!carry);
    let mut unadjusted = 0_u16;
    for digit in 0..digits {
        let shift = 4 * digit;
        let mut difference = (accumulator >> shift & 0xF)
            .wrapping_sub(data >> shift & 0xF)
            .wrapping_sub(borrow);
        if digit + 1 == digits {
            unadjusted = result | (difference & 0xF) << shift;
        }
        borrow = u16::from(difference > 0xF);
        if borrow != 0 {
            difference = difference.wrapping_sub(6);
        }
        result |= (difference & 0xF) << shift;
    }
    Sum {
        result,
        carry: borrow == 0,
        overflow: signed_overflow::<W>(accumulator, !data & mask::<W>(), unadjusted),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{native, run};

    // @source: 6502org-65c816-opcodes#6.1.1.1
    #[test]
    fn binary_sbc_follows_the_tutorial_example() {
        // Example 1: accumulator $0001, m = 0, d = 0, c = 1, SBC #$2003 gives $DFFE with n
        // set and v, z and c clear.
        let mut cpu = native(Flags::C);
        cpu.a = 0x0001;
        run(&mut cpu, &[0xE9, 0x03, 0x20], &[]);
        assert_eq!(cpu.a, 0xDFFE);
        assert_eq!(cpu.p, Flags::N);
    }

    // @source: 6502org-65c816-opcodes#6.1.1.1
    #[test]
    fn decimal_sbc_follows_the_tutorial_example() {
        // Example 2: as example 1 with d = 1: $7998 with n, z and c clear.
        let mut cpu = native(Flags::C | Flags::D);
        cpu.a = 0x0001;
        run(&mut cpu, &[0xE9, 0x03, 0x20], &[]);
        assert_eq!(cpu.a, 0x7998);
        assert!(!cpu.p.intersects(Flags::N | Flags::Z | Flags::C));
    }

    // @source: wdc-65816-manual#chapter-18-adc
    #[test]
    fn decimal_adc_carries_between_digits_and_out_of_the_top() {
        // 09 + 03 = 12 in decimal; 99 + 01 = 00 with a carry out.
        let mut cpu = native(Flags::M | Flags::D);
        cpu.a = 0x09;
        run(&mut cpu, &[0x69, 0x03], &[]);
        assert_eq!(cpu.a, 0x12);
        assert!(!cpu.p.contains(Flags::C));
        run(&mut cpu, &[0x69, 0x88], &[]);
        assert_eq!(cpu.a, 0x00);
        assert!(cpu.p.contains(Flags::C | Flags::Z));
    }

    // @source: 6502org-65c816-opcodes#6.1.1.1
    #[test]
    fn binary_adc_sets_carry_and_signed_overflow() {
        // $7F + $01 overflows into the sign bit; $FF + $01 carries out.
        let mut cpu = native(Flags::M);
        cpu.a = 0x7F;
        run(&mut cpu, &[0x69, 0x01], &[]);
        assert_eq!(cpu.a, 0x80);
        assert_eq!(cpu.p, Flags::M | Flags::N | Flags::V);
        cpu.a = 0xFF;
        cpu.p = Flags::M;
        run(&mut cpu, &[0x69, 0x01], &[]);
        assert_eq!(cpu.a, 0x00);
        assert_eq!(cpu.p, Flags::M | Flags::Z | Flags::C);
    }

    // @source: 6502org-65c816-opcodes#6.1.1.2
    #[test]
    fn cmp_follows_the_tutorial_example_and_ignores_decimal_mode() {
        // Accumulator $1234, m = 0, CMP #$1234: n = 0, z = 1, c = 1.
        let mut cpu = native(Flags::D | Flags::V | Flags::N);
        cpu.a = 0x1234;
        run(&mut cpu, &[0xC9, 0x34, 0x12], &[]);
        assert_eq!(cpu.p, Flags::D | Flags::V | Flags::Z | Flags::C);
        assert_eq!(cpu.a, 0x1234);
    }

    // @source: 6502org-65c816-opcodes#6.1.1.2
    #[test]
    fn cpx_and_cpy_take_their_width_from_the_x_flag() {
        // 8-bit: only the low byte of X takes part.
        let mut cpu = native(Flags::X);
        cpu.x = 0x0010;
        run(&mut cpu, &[0xE0, 0x20], &[]);
        assert_eq!(cpu.p, Flags::X | Flags::N);

        // 16-bit: Y $8000 against $7FFF is greater or equal.
        let mut cpu = native(Flags::empty());
        cpu.y = 0x8000;
        run(&mut cpu, &[0xC0, 0xFF, 0x7F], &[]);
        assert_eq!(cpu.p, Flags::C);
    }
}
