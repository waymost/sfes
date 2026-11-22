// @source: anomie-regs#4202
// @source: anomie-regs#4203
// @source: anomie-regs#4204
// @source: anomie-regs#4205
// @source: anomie-regs#4206
// @source: anomie-regs#4214
// @source: anomie-regs#4215
// @source: anomie-regs#4216
// @source: anomie-regs#4217
// @source: anomie-timing#s-cpu/registers
// @source: fullsnes#snesmathsmultiplydivide
// @source: adr-0023
//! The 5A22's multiply and divide unit: `$4202–$4206` and the results in `$4214–$4217`,
//! which appear a fixed number of CPU cycles after the write that starts the operation
//! (ADR 0023).

use crate::bus::Registers;

/// The CPU cycles after the `$4203` write before the product can be read.
const MULTIPLY_CYCLES: u8 = 8;
/// The CPU cycles after the `$4206` write before the quotient and remainder can be read.
const DIVIDE_CYCLES: u8 = 16;

/// The unit's registers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MathRegister {
    /// $4202 WRMPYA: the multiplicand.
    Multiplicand,
    /// $4203 WRMPYB: the multiplier; a write starts a multiplication.
    Multiplier,
    /// $4204 WRDIVL: the dividend's low byte.
    DividendLow,
    /// $4205 WRDIVH: the dividend's high byte.
    DividendHigh,
    /// $4206 WRDIVB: the divisor; a write starts a division.
    Divisor,
    /// $4214 RDDIVL: the quotient's low byte.
    QuotientLow,
    /// $4215 RDDIVH: the quotient's high byte.
    QuotientHigh,
    /// $4216 RDMPYL: the product's or remainder's low byte.
    ProductLow,
    /// $4217 RDMPYH: the product's or remainder's high byte.
    ProductHigh,
}

/// The multiply and divide unit.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Math {
    /// $4202 WRMPYA: the multiplicand, kept by a multiplication.
    multiplicand: u8,
    /// $4204–$4205 WRDIVL, WRDIVH: the dividend, kept by a division.
    dividend: u16,
    /// $4214–$4215 RDDIVL, RDDIVH: the quotient, or the multiplier after a multiplication.
    quotient: u16,
    /// $4216–$4217 RDMPYL, RDMPYH: the product or the remainder.
    product: u16,
    /// The quotient, or the multiplier, the running operation leaves.
    next_quotient: u16,
    /// The product or remainder the running operation leaves.
    next_product: u16,
    /// CPU cycle ends until the running operation's results appear; 0 when idle.
    remaining: u8,
}

impl Math {
    /// The unit at power-on: `$4202` `$FF` and `$4204–$4205` `$FFFF` (`anomie-regs#4202`,
    /// `#4204`), the results 0 and nothing running (ADR 0023, point 5).
    // @source: anomie-regs#4202
    // @source: anomie-regs#4204
    // @source: fullsnes#snesiomap
    // @source: adr-0023
    pub(crate) const fn power_on() -> Self {
        Self {
            multiplicand: 0xFF,
            dividend: 0xFFFF,
            quotient: 0,
            product: 0,
            next_quotient: 0,
            next_product: 0,
            remaining: 0,
        }
    }

    /// $4202 WRMPYA: sets the multiplicand.
    // @source: anomie-regs#4202
    // @source: fullsnes#snesmathsmultiplydivide
    const fn write_multiplicand(&mut self, value: u8) {
        self.multiplicand = value;
    }

    /// $4203 WRMPYB: starts an unsigned 8 × 8 multiplication. The product goes to
    /// `$4216–$4217` and the multiplier to `$4214`, with `$4215` 0
    /// (`fullsnes#snesmathsmultiplydivide`; ADR 0023, points 1 and 3).
    // @source: anomie-regs#4203
    // @source: anomie-regs#4216
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: adr-0023
    const fn write_multiplier(&mut self, value: u8) {
        let product = (self.multiplicand as u16) * (value as u16);
        self.start(value as u16, product, MULTIPLY_CYCLES);
    }

    /// $4204 WRDIVL: sets the dividend's low byte.
    // @source: anomie-regs#4204
    const fn write_dividend_low(&mut self, value: u8) {
        let [_, high] = self.dividend.to_le_bytes();
        self.dividend = u16::from_le_bytes([value, high]);
    }

    /// $4205 WRDIVH: sets the dividend's high byte.
    // @source: anomie-regs#4205
    const fn write_dividend_high(&mut self, value: u8) {
        let [low, _] = self.dividend.to_le_bytes();
        self.dividend = u16::from_le_bytes([low, value]);
    }

    /// $4206 WRDIVB: starts an unsigned 16 / 8 division. The quotient goes to `$4214–$4215`
    /// and the remainder to `$4216–$4217`; a divisor of 0 gives `$FFFF` and the dividend
    /// (`anomie-regs#4206`, `fullsnes#snesmathsmultiplydivide`; ADR 0023, point 1).
    // @source: anomie-regs#4206
    // @source: anomie-regs#4214
    // @source: anomie-regs#4216
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: adr-0023
    const fn write_divisor(&mut self, value: u8) {
        let (quotient, remainder) = match value {
            0 => (0xFFFF, self.dividend),
            divisor => (
                self.dividend / (divisor as u16),
                self.dividend % (divisor as u16),
            ),
        };
        self.start(quotient, remainder, DIVIDE_CYCLES);
    }

    /// Starts an operation whose results appear after `cycles` CPU cycles. One already
    /// running is abandoned (ADR 0023, point 4). The count includes the starting write's
    /// own cycle, whose end [`Math::clock`] sees first.
    // @source: anomie-timing#s-cpu/registers
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: adr-0023
    const fn start(&mut self, quotient: u16, product: u16, cycles: u8) {
        self.next_quotient = quotient;
        self.next_product = product;
        self.remaining = cycles + 1;
    }

    /// The end of a CPU cycle: counts down a running operation and moves its results into
    /// `$4214–$4217` at the end of its last cycle (ADR 0023, point 2).
    // @source: anomie-timing#s-cpu/registers
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: adr-0023
    pub(crate) const fn clock(&mut self) {
        if self.remaining == 0 {
            return;
        }
        self.remaining -= 1;
        if self.remaining == 0 {
            self.quotient = self.next_quotient;
            self.product = self.next_product;
        }
    }
}

impl Registers for Math {
    type Register = MathRegister;

    /// $4214–$4217 RDDIVL–RDMPYH: the results, none of which a read changes. The operand
    /// registers are write-only.
    // @source: anomie-regs#4214
    // @source: anomie-regs#4215
    // @source: anomie-regs#4216
    // @source: anomie-regs#4217
    // @source: adr-0023
    fn peek(&self, register: MathRegister) -> Option<u8> {
        let [quotient_low, quotient_high] = self.quotient.to_le_bytes();
        let [product_low, product_high] = self.product.to_le_bytes();
        match register {
            MathRegister::QuotientLow => Some(quotient_low),
            MathRegister::QuotientHigh => Some(quotient_high),
            MathRegister::ProductLow => Some(product_low),
            MathRegister::ProductHigh => Some(product_high),
            MathRegister::Multiplicand
            | MathRegister::Multiplier
            | MathRegister::DividendLow
            | MathRegister::DividendHigh
            | MathRegister::Divisor => None,
        }
    }

    /// $4202–$4206 WRMPYA–WRDIVB: the operands, two of which start an operation. The results
    /// are read-only.
    // @source: anomie-regs#4202
    // @source: anomie-regs#4203
    // @source: anomie-regs#4204
    // @source: anomie-regs#4205
    // @source: anomie-regs#4206
    // @source: adr-0023
    fn write(&mut self, register: MathRegister, value: u8) {
        match register {
            MathRegister::Multiplicand => self.write_multiplicand(value),
            MathRegister::Multiplier => self.write_multiplier(value),
            MathRegister::DividendLow => self.write_dividend_low(value),
            MathRegister::DividendHigh => self.write_dividend_high(value),
            MathRegister::Divisor => self.write_divisor(value),
            MathRegister::QuotientLow
            | MathRegister::QuotientHigh
            | MathRegister::ProductLow
            | MathRegister::ProductHigh => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Clocks `math` through `cycles` CPU cycle ends.
    // @source: adr-0023
    fn run(math: &mut Math, cycles: u8) {
        for _ in 0..cycles {
            math.clock();
        }
    }

    /// The results as `[$4214, $4215, $4216, $4217]`.
    // @source: anomie-regs#4214
    // @source: anomie-regs#4216
    fn results(math: Math) -> [u8; 4] {
        [
            MathRegister::QuotientLow,
            MathRegister::QuotientHigh,
            MathRegister::ProductLow,
            MathRegister::ProductHigh,
        ]
        .map(|register| math.peek(register).unwrap_or_default())
    }

    // @source: anomie-regs#4202
    // @source: anomie-regs#4203
    // @source: anomie-regs#4216
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: adr-0023
    #[test]
    fn the_product_appears_eight_cycles_after_the_write() {
        let mut math = Math::power_on();
        math.write_multiplicand(200);
        math.write_multiplier(250);
        // The write's own cycle, then seven: one short.
        run(&mut math, MULTIPLY_CYCLES);
        assert_eq!(results(math), [0, 0, 0, 0]);
        run(&mut math, 1);
        // 50000 = $C350; $4214 takes the multiplier, $4215 is 0.
        assert_eq!(results(math), [250, 0, 0x50, 0xC3]);
        // $4202 is kept: a new multiplier alone starts another multiplication.
        math.write_multiplier(3);
        run(&mut math, MULTIPLY_CYCLES + 1);
        assert_eq!(results(math), [3, 0, 0x58, 0x02]);
    }

    // @source: anomie-regs#4204
    // @source: anomie-regs#4206
    // @source: anomie-regs#4214
    // @source: anomie-regs#4216
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: adr-0023
    #[test]
    fn the_quotient_and_remainder_appear_sixteen_cycles_after_the_write() {
        let mut math = Math::power_on();
        math.write_dividend_low(0x39);
        math.write_dividend_high(0x30);
        math.write_divisor(100);
        run(&mut math, DIVIDE_CYCLES);
        assert_eq!(results(math), [0, 0, 0, 0]);
        run(&mut math, 1);
        // 12345 = 123 × 100 + 45.
        assert_eq!(results(math), [123, 0, 45, 0]);
        // The dividend is kept: 12345 = 1763 × 7 + 4, and 1763 = $06E3.
        math.write_divisor(7);
        run(&mut math, DIVIDE_CYCLES + 1);
        assert_eq!(results(math), [0xE3, 0x06, 4, 0]);
    }

    // @source: anomie-regs#4206
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: adr-0023
    #[test]
    fn dividing_by_zero_gives_ffff_and_the_dividend() {
        let mut math = Math::power_on();
        math.write_dividend_low(0x34);
        math.write_dividend_high(0x12);
        math.write_divisor(0);
        run(&mut math, DIVIDE_CYCLES + 1);
        assert_eq!(results(math), [0xFF, 0xFF, 0x34, 0x12]);
    }

    // @source: anomie-regs#4202
    // @source: anomie-regs#4204
    // @source: adr-0023
    #[test]
    fn power_on_operands_are_all_ones() {
        // $FF × $FF = $FE01, and $FFFF / $FF = $0101 remainder 0.
        let mut math = Math::power_on();
        math.write_multiplier(0xFF);
        run(&mut math, MULTIPLY_CYCLES + 1);
        assert_eq!(results(math), [0xFF, 0, 0x01, 0xFE]);
        math.write_divisor(0xFF);
        run(&mut math, DIVIDE_CYCLES + 1);
        assert_eq!(results(math), [0x01, 0x01, 0, 0]);
    }

    // @source: anomie-timing#s-cpu/registers
    // @source: adr-0023
    #[test]
    fn a_new_operation_abandons_the_running_one() {
        let mut math = Math::power_on();
        math.write_multiplicand(2);
        math.write_multiplier(3);
        run(&mut math, 4);
        math.write_multiplier(5);
        // The first multiplication's time passes with no result.
        run(&mut math, 5);
        assert_eq!(results(math), [0, 0, 0, 0]);
        run(&mut math, 4);
        assert_eq!(results(math), [5, 0, 10, 0]);
    }
}
