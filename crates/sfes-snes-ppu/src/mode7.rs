// @source: anomie-regs#211b
// @source: anomie-regs#211c
// @source: anomie-regs#2134
// @source: fullsnes#snesmathsmultiplydivide
// @source: adr-0029
//! The Mode 7 registers' shared write-twice latch, and the multiplier read at
//! `$2134–$2136` (ADR 0029, point 8). Until #93 only M7A and M7B keep their values.

/// The latch and both M7A and M7B at power-on: fullsnes's "(FFh)(w2)".
const POWER_ON: u8 = 0xFF;

/// M7A, M7B and the latch they share with `$210D`, `$210E` and `$211D–$2120`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Mode7 {
    /// The last value written to any of the registers sharing it.
    latch: u8,
    /// $211B M7A: matrix parameter A, the multiplicand.
    matrix_a: u16,
    /// $211C M7B: matrix parameter B; its high byte is the multiplier.
    matrix_b: u16,
}

impl Mode7 {
    /// The registers at power-on: all `$FF` (ADR 0029, point 8). Reset keeps them.
    // @source: fullsnes#snesiomap
    // @source: adr-0029
    pub(crate) const fn power_on() -> Self {
        let word = u16::from_le_bytes([POWER_ON, POWER_ON]);
        Self {
            latch: POWER_ON,
            matrix_a: word,
            matrix_b: word,
        }
    }

    /// A write of `value` to a register that shares the latch: the register's new value is
    /// `value` over the latch, and the latch becomes `value`.
    // @source: anomie-regs#211b
    // @source: adr-0029
    pub(crate) const fn write(&mut self, value: u8) -> u16 {
        let word = u16::from_le_bytes([self.latch, value]);
        self.latch = value;
        word
    }

    /// $211B M7A.
    // @source: anomie-regs#211b
    // @source: fullsnes#snesmathsmultiplydivide
    pub(crate) const fn write_matrix_a(&mut self, value: u8) {
        self.matrix_a = self.write(value);
    }

    /// $211C M7B.
    // @source: anomie-regs#211c
    // @source: fullsnes#snesmathsmultiplydivide
    pub(crate) const fn write_matrix_b(&mut self, value: u8) {
        self.matrix_b = self.write(value);
    }

    /// $2134–$2136 MPYL/MPYM/MPYH: the 24-bit signed product of M7A and M7B's high byte,
    /// low byte first.
    // @source: anomie-regs#2134
    // @source: anomie-regs#2135
    // @source: anomie-regs#2136
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: adr-0029
    pub(crate) const fn product(self) -> [u8; 3] {
        let [_, multiplier] = self.matrix_b.to_le_bytes();
        let multiplicand = i16::from_le_bytes(self.matrix_a.to_le_bytes()) as i32;
        let product = multiplicand * i8::from_le_bytes([multiplier]) as i32;
        let [low, middle, high, _] = product.to_le_bytes();
        [low, middle, high]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: anomie-regs#211b
    // @source: anomie-regs#2134
    // @source: fullsnes#snesiomap
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: adr-0029
    #[test]
    fn the_product_is_signed_16_by_8_bits() {
        let mut mode7 = Mode7::power_on();
        // $FFFF times $FF is -1 times -1.
        assert_eq!(mode7.product(), [0x01, 0x00, 0x00]);
        mode7.write_matrix_a(0x00);
        mode7.write_matrix_a(0x80);
        mode7.write_matrix_b(0x7F);
        // -32768 times 127 is -$3F8000.
        assert_eq!(mode7.product(), [0x00, 0x80, 0xC0]);
        mode7.write_matrix_a(0x34);
        mode7.write_matrix_a(0x12);
        mode7.write_matrix_b(0x02);
        assert_eq!(mode7.product(), [0x68, 0x24, 0x00]);
    }

    // @source: anomie-regs#211b
    // @source: adr-0029
    #[test]
    fn the_latch_is_shared() {
        let mut mode7 = Mode7::power_on();
        mode7.write_matrix_a(0x34);
        // A write to another register that shares the latch, such as M7C.
        mode7.write(0x56);
        mode7.write_matrix_a(0x12);
        assert_eq!(mode7.matrix_a, 0x1256);
        assert_eq!(mode7.write(0x78), 0x7812);
    }
}
