// @source: wdc-w65c816s#2.4
// @source: wdc-w65c816s#2.7
//! Operand width: the accumulator and the index registers are 8 or 16 bits wide, selected by
//! the M and X flags. An instruction is written once, generic over [`Width`], and the opcode
//! `match` picks `u8` or `u16`.

/// An operand width, `u8` or `u16`. The trait is private to the crate, so nothing else can
/// implement it.
// @source: wdc-w65c816s#2.4
pub(crate) trait Width: Copy {
    /// The sign bit of this width, as a 16-bit mask.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "bring-up: first used by the load instructions (#33)"
        )
    )]
    const SIGN: u16;

    /// The low part of a 16-bit register at this width.
    // @source: wdc-w65c816s#2.4
    fn from_register(register: u16) -> Self;

    /// `register` with its low part replaced by this value. An 8-bit value leaves the high
    /// byte alone, which is how the B accumulator survives 8-bit operations.
    // @source: wdc-w65c816s#2.4
    fn store_in(self, register: u16) -> u16;

    /// The value zero-extended to 16 bits.
    // @source: wdc-w65c816s#2.4
    fn to_u16(self) -> u16;
}

impl Width for u8 {
    const SIGN: u16 = 0x0080;

    // @source: wdc-w65c816s#2.4
    fn from_register(register: u16) -> Self {
        register.to_le_bytes()[0]
    }

    // @source: wdc-w65c816s#2.4
    fn store_in(self, register: u16) -> u16 {
        u16::from_le_bytes([self, register.to_le_bytes()[1]])
    }

    // @source: wdc-w65c816s#2.4
    fn to_u16(self) -> u16 {
        u16::from(self)
    }
}

impl Width for u16 {
    const SIGN: u16 = 0x8000;

    // @source: wdc-w65c816s#2.4
    fn from_register(register: u16) -> Self {
        register
    }

    // @source: wdc-w65c816s#2.4
    fn store_in(self, _register: u16) -> u16 {
        self
    }

    // @source: wdc-w65c816s#2.4
    fn to_u16(self) -> u16 {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: wdc-w65c816s#2.4
    #[test]
    fn eight_bit_values_use_the_low_byte_and_keep_the_high_byte() {
        assert_eq!(<u8 as Width>::from_register(0x12AB), 0xAB);
        assert_eq!(0xCD_u8.store_in(0x12AB), 0x12CD);
        assert_eq!(0xCD_u8.to_u16(), 0x00CD);
        assert_eq!(<u8 as Width>::SIGN, 0x0080);
    }

    // @source: wdc-w65c816s#2.4
    #[test]
    fn sixteen_bit_values_use_the_whole_register() {
        assert_eq!(<u16 as Width>::from_register(0x12AB), 0x12AB);
        assert_eq!(0xBEEF_u16.store_in(0x12AB), 0xBEEF);
        assert_eq!(0xBEEF_u16.to_u16(), 0xBEEF);
        assert_eq!(<u16 as Width>::SIGN, 0x8000);
    }
}
