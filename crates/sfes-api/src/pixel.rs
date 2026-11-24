//! A pixel of a rendered frame.

/// Bit 15, which an RGB555 colour leaves clear.
const UNUSED_BIT: u16 = 0x8000;
/// One 5-bit channel.
const CHANNEL: u16 = 0x1F;

/// One RGB555 pixel: red in bits 4–0, green in bits 9–5, blue in bits 14–10, and bit 15
/// clear. It can only be made with bit 15 clear.
///
/// It is `repr(transparent)`, so a slice of pixels has the layout of a slice of `u16`.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub struct Rgb555(u16);

/// A `u16` with bit 15 set, which is not an RGB555 colour.
#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{0:#06X} has bit 15 set, so it is not an RGB555 colour")]
pub struct InvalidRgb555(pub u16);

impl Rgb555 {
    /// Black: every channel 0.
    pub const BLACK: Self = Self(0);

    /// The colour whose bits are `value`.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidRgb555`] when bit 15 of `value` is set.
    pub const fn new(value: u16) -> Result<Self, InvalidRgb555> {
        if value & UNUSED_BIT == 0 {
            Ok(Self(value))
        } else {
            Err(InvalidRgb555(value))
        }
    }

    /// The colour with these channels, each masked to its low 5 bits.
    #[must_use]
    pub const fn from_channels(red: u8, green: u8, blue: u8) -> Self {
        Self((red as u16 & CHANNEL) | (green as u16 & CHANNEL) << 5 | (blue as u16 & CHANNEL) << 10)
    }

    /// The colour of `value` with bit 15 dropped.
    #[must_use]
    pub const fn from_bits_truncate(value: u16) -> Self {
        Self(value & !UNUSED_BIT)
    }

    /// The colour of two bytes, low byte first, as CGRAM stores it, with bit 15 dropped.
    #[must_use]
    pub const fn from_le_bytes(bytes: [u8; 2]) -> Self {
        Self::from_bits_truncate(u16::from_le_bytes(bytes))
    }

    /// Each channel replaced by `function` of it, masked to 5 bits.
    #[must_use]
    pub fn map(self, function: impl Fn(u8) -> u8) -> Self {
        Self::from_channels(
            function(self.red()),
            function(self.green()),
            function(self.blue()),
        )
    }

    /// Each channel replaced by `function` of it and the same channel of `other`, masked to
    /// 5 bits.
    #[must_use]
    pub fn zip_map(self, other: Self, function: impl Fn(u8, u8) -> u8) -> Self {
        Self::from_channels(
            function(self.red(), other.red()),
            function(self.green(), other.green()),
            function(self.blue(), other.blue()),
        )
    }

    /// Its bits, with bit 15 clear.
    #[must_use]
    pub const fn bits(self) -> u16 {
        self.0
    }

    /// Its red channel, 0 to 31.
    #[must_use]
    pub const fn red(self) -> u8 {
        self.channel(0)
    }

    /// Its green channel, 0 to 31.
    #[must_use]
    pub const fn green(self) -> u8 {
        self.channel(5)
    }

    /// Its blue channel, 0 to 31.
    #[must_use]
    pub const fn blue(self) -> u8 {
        self.channel(10)
    }

    /// Its bits as two bytes, low byte first, as CGRAM and goldens store them.
    #[must_use]
    pub const fn to_le_bytes(self) -> [u8; 2] {
        self.0.to_le_bytes()
    }

    /// The 5-bit channel starting at bit `shift`.
    const fn channel(self, shift: u32) -> u8 {
        ((self.0 >> shift) & CHANNEL) as u8
    }
}

impl TryFrom<u16> for Rgb555 {
    type Error = InvalidRgb555;

    fn try_from(value: u16) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Rgb555> for u16 {
    fn from(pixel: Rgb555) -> Self {
        pixel.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pixel_needs_bit_15_clear() {
        assert_eq!(Rgb555::new(0x7FFF).map(Rgb555::bits), Ok(0x7FFF));
        assert_eq!(Rgb555::new(0x8000), Err(InvalidRgb555(0x8000)));
        assert_eq!(Rgb555::try_from(0xFFFF), Err(InvalidRgb555(0xFFFF)));
        assert_eq!(Rgb555::default(), Rgb555::BLACK);
    }

    #[test]
    fn colours_are_made_from_channels_and_truncated_bits() {
        let pixel = Rgb555::from_channels(3, 10, 31);
        assert_eq!(pixel.bits(), 0x7D43);
        assert_eq!(Rgb555::from_channels(0x23, 0, 0).red(), 3);
        assert_eq!(Rgb555::from_bits_truncate(0xFFFF).bits(), 0x7FFF);
        assert_eq!(Rgb555::from_le_bytes([0x43, 0xFD]), pixel);
        assert_eq!(
            pixel.map(|c| c / 2).bits(),
            Rgb555::from_channels(1, 5, 15).bits()
        );
        assert_eq!(pixel.zip_map(pixel, u8::min), pixel);
    }

    #[test]
    fn the_channels_are_red_low() {
        let pixel = Rgb555::new(0x7C00 | 0x0140 | 0x0003).unwrap();
        assert_eq!((pixel.red(), pixel.green(), pixel.blue()), (3, 10, 31));
        assert_eq!(pixel.to_le_bytes(), [0x43, 0x7D]);
        assert_eq!(u16::from(pixel), 0x7D43);
    }
}
