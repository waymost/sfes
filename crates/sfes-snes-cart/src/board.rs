// @source: anomie-memmap#memory-map
// @source: fullsnes#snesmemorymap
// @source: adr-0017
//! The boards: how a cartridge answers the addresses on Address Bus A (EMU-04).
//!
//! "LoROM", "HiROM" and the rest are just different ways the cartridge responds to the
//! addresses the CPU drives (`anomie-memmap#memory-map`). A [`Board`] gives, for each address,
//! the ROM or SRAM offset that answers it before mirroring; [`crate::MemoryMap`] mirrors those
//! offsets and fixes them in a table at load (ADR 0017).

use crate::header::Mapping;

/// The size of a LoROM bank's ROM window, `$8000–$FFFF`.
const LOROM_BANK: u32 = 0x8000;
/// The size of a HiROM bank.
const HIROM_BANK: u32 = 0x1_0000;
/// Where the slow banks start in an ExHiROM image: after the 4 MiB of fast banks.
const EXHIROM_SLOW: u32 = 0x40_0000;
/// The size of a HiROM SRAM window, `$6000–$7FFF`.
const HIROM_SRAM_WINDOW: u32 = 0x2000;
/// The size of a LoROM SRAM window, `$0000–$7FFF`.
const LOROM_SRAM_WINDOW: u32 = 0x8000;

/// A cartridge board: the wiring between Address Bus A and the cartridge's ROM and RAM.
///
/// Coprocessor boards (SA-1, GSU, NEC DSP) join as variants in later milestones.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Board {
    /// ROM in 32 KiB banks at `$8000–$FFFF`; SRAM at `$70–$7D,$F0–$FF:0000–7FFF`.
    LoRom,
    /// ROM in 64 KiB banks at `$40–$7D,$C0–$FF`; SRAM at `$20–$3F,$A0–$BF:6000–7FFF`.
    HiRom,
    /// HiROM with up to 4 MiB of further ROM in banks `$00–$7D`; SRAM at `$80–$BF:6000–7FFF`.
    ExHiRom,
}

impl From<Mapping> for Board {
    // @source: adr-0017
    fn from(mapping: Mapping) -> Self {
        match mapping {
            Mapping::LoRom => Self::LoRom,
            Mapping::HiRom => Self::HiRom,
            Mapping::ExHiRom => Self::ExHiRom,
        }
    }
}

impl Board {
    /// The ROM offset that answers `bank:address`, before mirroring, or `None` where the board
    /// maps no ROM. Addresses the console itself answers (`is_console_area`) are never asked.
    // @source: fullsnes#snescartlorommappingromdividedinto32kbanksaround1500games
    // @source: fullsnes#snescarthirommappingromdividedinto64kbanksaround500games
    // @source: fullsnes#snescartridgeromimageinterleave
    // @source: adr-0017
    #[must_use]
    pub const fn rom_offset(self, bank: u8, address: u16) -> Option<u32> {
        let bank = bank as u32;
        let address = address as u32;
        let upper_half = address >= 0x8000;
        let full_bank = (bank & 0x7F) >= 0x40;
        match self {
            // ROM at $00-$7D,$80-$FF:8000-FFFF, mirrored at $40-$7D,$C0-$FF:0000-7FFF.
            Self::LoRom if upper_half || full_bank => {
                Some((bank & 0x7F) * LOROM_BANK + (address & 0x7FFF))
            }
            // ROM at $40-$7D,$C0-$FF:0000-FFFF, mirrored at $00-$3F,$80-$BF:8000-FFFF.
            Self::HiRom if upper_half || full_bank => Some((bank & 0x3F) * HIROM_BANK + address),
            // As HiROM, with the slow banks below $80 placed after the 4 MiB of fast banks.
            Self::ExHiRom if upper_half || full_bank => {
                let slow = if bank < 0x80 { EXHIROM_SLOW } else { 0 };
                Some(slow + (bank & 0x3F) * HIROM_BANK + address)
            }
            Self::LoRom | Self::HiRom | Self::ExHiRom => None,
        }
    }

    /// The SRAM offset that answers `bank:address`, before mirroring, or `None` outside the
    /// board's SRAM area. Only asked when the cartridge has RAM.
    // @source: fullsnes#snesmemorymap
    // @source: fullsnes#snescartlorommappingromdividedinto32kbanksaround1500games
    // @source: fullsnes#snescarthirommappingromdividedinto64kbanksaround500games
    // @source: adr-0017
    #[must_use]
    pub const fn ram_offset(self, bank: u8, address: u16) -> Option<u32> {
        let address = address as u32;
        let in_window = address >= 0x6000 && address < 0x8000;
        match self {
            // $70-$7D,$F0-$FF:0000-7FFF, one 32 KiB chunk per bank from $70.
            Self::LoRom if ((bank >= 0x70 && bank <= 0x7D) || bank >= 0xF0) && address < 0x8000 => {
                Some((bank as u32 & 0x0F) * LOROM_SRAM_WINDOW + address)
            }
            // $20-$3F,$A0-$BF:6000-7FFF, one 8 KiB chunk per bank from $20.
            Self::HiRom if (bank & 0x7F) >= 0x20 && (bank & 0x7F) <= 0x3F && in_window => {
                Some((bank as u32 & 0x1F) * HIROM_SRAM_WINDOW + (address - 0x6000))
            }
            // $80-$BF:6000-7FFF, one 8 KiB chunk per bank from $80.
            Self::ExHiRom if bank >= 0x80 && bank <= 0xBF && in_window => {
                Some((bank as u32 & 0x3F) * HIROM_SRAM_WINDOW + (address - 0x6000))
            }
            Self::LoRom | Self::HiRom | Self::ExHiRom => None,
        }
    }
}

/// The offset in a ROM of `length` bytes that answers the unmirrored `offset`, mirrored the
/// way the checksum sums it: the largest power-of-two part first, then the remainder repeated
/// (ADR 0017). `length` is at most 16 MiB; 0 gives 0.
// @source: fullsnes#snescartridgeromheader
// @source: adr-0017
#[must_use]
pub const fn mirror(offset: u32, length: u32) -> u32 {
    let mut base = 0;
    let mut offset = offset;
    let mut length = length;
    while length > 0 {
        offset &= length.next_power_of_two() - 1;
        if offset < length {
            return base + offset;
        }
        // `offset` is past the largest power-of-two part, so it falls in the remainder.
        let first = 1 << length.ilog2();
        base += first;
        offset -= first;
        length -= first;
    }
    base
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: fullsnes#snescartridgeromheader
    // @source: adr-0017
    #[test]
    fn a_power_of_two_rom_repeats_whole() {
        assert_eq!(mirror(0x1234, 0x8000), 0x1234);
        assert_eq!(mirror(0x9234, 0x8000), 0x1234);
        assert_eq!(mirror(0x7F_FFFF, 0x10_0000), 0xF_FFFF);
    }

    // @source: fullsnes#snescartridgeromheader
    // @source: adr-0017
    #[test]
    fn the_remainder_repeats_after_the_larger_part() {
        // 3 MiB: 2 MiB, then the last 1 MiB twice.
        assert_eq!(mirror(0x1F_FFFF, 0x30_0000), 0x1F_FFFF);
        assert_eq!(mirror(0x30_0000, 0x30_0000), 0x20_0000);
        assert_eq!(mirror(0x3F_FFFF, 0x30_0000), 0x2F_FFFF);
        // 5 MiB ExHiROM: "4MB + 4 x Last 1MB".
        for copy in 0..4 {
            assert_eq!(
                mirror(0x40_0000 + copy * 0x10_0000 + 0x10, 0x50_0000),
                0x40_0010
            );
        }
    }

    // @source: adr-0017
    #[test]
    fn a_remainder_that_is_not_a_power_of_two_is_mirrored_again() {
        // 7 KiB = 4 KiB + 2 KiB + 1 KiB; the 3 KiB remainder mirrors within a 4 KiB window.
        assert_eq!(mirror(0x1C00, 0x1C00), 0x1800);
        assert_eq!(mirror(0x1E00, 0x1C00), 0x1A00);
        assert_eq!(mirror(0x1FFF, 0x1C00), 0x1BFF);
        assert_eq!(mirror(0x2000, 0x1C00), 0);
        for length in 1..=0x80 {
            for offset in 0..0x200 {
                assert!(mirror(offset, length) < length, "{offset} {length}");
            }
        }
    }

    // @source: fullsnes#snescartridgeromheader
    // @source: fullsnes#snescartridgeromimageinterleave
    // @source: adr-0017
    #[test]
    fn the_vectors_come_from_each_boards_header_area() {
        assert_eq!(Board::LoRom.rom_offset(0x00, 0xFFFC), Some(0x7FFC));
        assert_eq!(Board::HiRom.rom_offset(0x00, 0xFFFC), Some(0xFFFC));
        assert_eq!(Board::ExHiRom.rom_offset(0x00, 0xFFFC), Some(0x40_FFFC));
    }
}
