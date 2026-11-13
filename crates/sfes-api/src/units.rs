//! Newtypes for time and addresses, so units cannot be mixed by accident.

/// Master-clock cycles since power-on (21.477 MHz NTSC, 21.281 MHz PAL).
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    serde::Deserialize,
    serde::Serialize,
)]
pub struct Cycles(pub u64);

/// A 24-bit CPU address. The constructor masks to 24 bits, so every value is in range.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Addr24(u32);

impl Addr24 {
    /// The mask applied by [`Addr24::new`].
    pub const MASK: u32 = 0x00FF_FFFF;

    /// Creates an address from the low 24 bits of `raw`.
    #[must_use]
    pub const fn new(raw: u32) -> Self {
        Self(raw & Self::MASK)
    }

    /// Creates an address from a bank byte and a 16-bit offset within the bank.
    #[must_use]
    pub const fn from_bank_offset(bank: u8, offset: u16) -> Self {
        Self(((bank as u32) << 16) | offset as u32)
    }

    /// The address as a 24-bit value.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// The bank byte (bits 16–23).
    #[must_use]
    pub const fn bank(self) -> u8 {
        self.0.to_le_bytes()[2]
    }

    /// The 16-bit offset within the bank.
    #[must_use]
    pub const fn offset(self) -> u16 {
        let [low, high, _, _] = self.0.to_le_bytes();
        u16::from_le_bytes([low, high])
    }
}

impl From<Addr24> for u32 {
    fn from(address: Addr24) -> Self {
        let Addr24(raw) = address;
        raw
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_masks_to_24_bits() {
        let address = Addr24::new(0xFF12_3456);
        assert_eq!(address.get(), 0x0012_3456);
        assert_eq!(address.bank(), 0x12);
        assert_eq!(address.offset(), 0x3456);
    }

    #[test]
    fn bank_and_offset_round_trip() {
        let address = Addr24::from_bank_offset(0x7E, 0x1234);
        assert_eq!(u32::from(address), 0x007E_1234);
        assert_eq!(
            Addr24::from_bank_offset(address.bank(), address.offset()),
            address
        );
    }
}
