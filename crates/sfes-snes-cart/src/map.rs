// @source: anomie-memmap#memory-map
// @source: anomie-memmap#hardware/address-bus-a
// @source: adr-0017
//! The memory map: what answers each 8 KiB page of the 24-bit address space (EMU-04).
//!
//! The console's own areas (WRAM, I/O) are the same for every cartridge; the board fills the
//! rest (`anomie-memmap#memory-map`). [`MemoryMap::new`] builds the whole table once at load,
//! with ROM and SRAM offsets already mirrored, so the bus finds an address with one lookup
//! (ADR 0017).

use alloc::{boxed::Box, vec};

use crate::board::{Board, mirror};

/// The size of a page.
pub const PAGE_SIZE: u32 = 0x2000;
/// The number of pages in the 24-bit address space.
pub const PAGES: usize = 0x100_0000 / PAGE_SIZE as usize;
/// The size of WRAM, banks `$7E–$7F`.
const WRAM_SIZE: u32 = 0x2_0000;
/// The largest ROM the map can reach: 16 MiB, the whole address space.
const LARGEST_ROM: u32 = 0x100_0000;

/// What answers one page, or one address once resolved.
///
/// In the table each offset is that of the page's first byte; [`MemoryMap::resolve`] returns
/// the offset of the address itself.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Page {
    /// Cartridge ROM, at this offset in the image without its copier header.
    Rom(u32),
    /// Cartridge RAM, at this offset (`RegionId::CartRam`).
    CartRam(u32),
    /// Work RAM, at this offset in its 128 KiB.
    Wram(u32),
    /// The console's registers at `$2000–$5FFF` of banks `$00–$3F` and `$80–$BF`; the bus
    /// decodes the address further.
    Io,
    /// A coprocessor's registers or memory; the id is the board's own. No board uses it yet.
    Coproc(u8),
    /// Nothing answers; the read sees open bus.
    Open,
}

/// The table of 2,048 pages, built once per cartridge and shared (technical design section 5).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryMap {
    /// One entry per 8 KiB page, indexed by `address >> 13`.
    pages: Box<[Page]>,
    /// The ROM length the offsets were mirrored to.
    rom_length: u32,
    /// The RAM length, a power of two, or 0 for none.
    sram_length: u32,
}

impl MemoryMap {
    /// The map of `board` with a ROM of `rom_length` bytes and an SRAM of `sram_length` bytes, a
    /// power of two or 0 for none. Only the first 16 MiB of a longer ROM can be reached.
    // @source: anomie-memmap#memory-map
    // @source: adr-0017
    #[must_use]
    pub fn new(board: Board, rom_length: usize, sram_length: usize) -> Self {
        let rom_length = u32::try_from(rom_length).map_or(LARGEST_ROM, |length| {
            if length > LARGEST_ROM {
                LARGEST_ROM
            } else {
                length
            }
        });
        let sram_length = u32::try_from(sram_length).unwrap_or_default();
        let mut pages = vec![Page::Open; PAGES].into_boxed_slice();
        for (index, page) in pages.iter_mut().enumerate() {
            let start = u32::try_from(index).unwrap_or_default() * PAGE_SIZE;
            *page = Self::build(board, start, rom_length, sram_length);
        }
        Self {
            pages,
            rom_length,
            sram_length,
        }
    }

    /// The entry for the page starting at `start`.
    // @source: anomie-memmap#memory-map
    // @source: adr-0017
    fn build(board: Board, start: u32, rom_length: u32, sram_length: u32) -> Page {
        let [_, bank, high, low] = start.to_be_bytes();
        let address = u16::from_be_bytes([high, low]);
        if let Some(page) = console_page(bank, address) {
            return page;
        }
        if sram_length > 0
            && let Some(offset) = board.ram_offset(bank, address)
        {
            return Page::CartRam(offset & (sram_length - 1));
        }
        match board.rom_offset(bank, address) {
            Some(offset) if rom_length > 0 => Page::Rom(mirror(offset, rom_length)),
            Some(_) | None => Page::Open,
        }
    }

    /// The table entry for the page holding `address`; bits above 23 are ignored.
    // @source: adr-0017
    #[must_use]
    pub fn page(&self, address: u32) -> Page {
        let index = usize::try_from((address & 0xFF_FFFF) / PAGE_SIZE).unwrap_or_default();
        self.pages.get(index).copied().unwrap_or(Page::Open)
    }

    /// What answers `address` itself, with the offset inside the page added and mirrored.
    // @source: adr-0017
    #[must_use]
    pub fn resolve(&self, address: u32) -> Page {
        let inside = address & (PAGE_SIZE - 1);
        match self.page(address) {
            Page::Rom(start) => {
                let offset = start + inside;
                // Only a ROM whose length is not a multiple of 8 KiB ends inside a page.
                if offset < self.rom_length {
                    Page::Rom(offset)
                } else {
                    Page::Rom(mirror(offset, self.rom_length))
                }
            }
            Page::CartRam(start) => Page::CartRam((start + inside) & (self.sram_length - 1)),
            Page::Wram(start) => Page::Wram(start + inside),
            page @ (Page::Io | Page::Coproc(_) | Page::Open) => page,
        }
    }

    /// Every page in address order.
    // @source: adr-0017
    #[must_use]
    pub fn pages(&self) -> &[Page] {
        &self.pages
    }
}

/// The page the console itself answers at `bank:address`, which no board changes, or `None`
/// for the cartridge's addresses.
// @source: anomie-memmap#memory-map
// @source: fullsnes#snesmemorymap
#[must_use]
pub const fn console_page(bank: u8, address: u16) -> Option<Page> {
    match bank {
        // WRAM, 128 KiB.
        0x7E | 0x7F => Some(Page::Wram((bank as u32 - 0x7E) * 0x1_0000 + address as u32)),
        // The system area of banks $00-$3F and $80-$BF.
        0x00..=0x3F | 0x80..=0xBF => match address {
            // A mirror of $7E:0000-1FFF.
            0x0000..=0x1FFF => Some(Page::Wram(address as u32 % WRAM_SIZE)),
            // Address Bus B at $2100-$21FF and the CPU's registers at $4000-$43FF; the rest
            // of $2000-$5FFF answers nothing unless the bus finds otherwise.
            0x2000..=0x5FFF => Some(Page::Io),
            // $6000-$7FFF is the cartridge's (or open), $8000-$FFFF is /CART.
            _ => None,
        },
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const KIB: usize = 1024;
    const MIB: usize = 1024 * KIB;

    // @source: anomie-memmap#memory-map
    #[test]
    fn the_console_areas_are_the_same_on_every_board() {
        for board in [Board::LoRom, Board::HiRom, Board::ExHiRom] {
            let map = MemoryMap::new(board, 4 * MIB, 8 * KIB);
            for bank in [0x00_u8, 0x3F, 0x80, 0xBF] {
                let base = u32::from(bank) << 16;
                assert_eq!(map.resolve(base), Page::Wram(0));
                assert_eq!(map.resolve(base | 0x1FFF), Page::Wram(0x1FFF));
                assert_eq!(map.resolve(base | 0x2100), Page::Io);
                assert_eq!(map.resolve(base | 0x4218), Page::Io);
                assert_eq!(map.resolve(base | 0x5FFF), Page::Io);
            }
            assert_eq!(map.resolve(0x7E_0000), Page::Wram(0));
            assert_eq!(map.resolve(0x7E_FFFF), Page::Wram(0xFFFF));
            assert_eq!(map.resolve(0x7F_0000), Page::Wram(0x1_0000));
            assert_eq!(map.resolve(0x7F_FFFF), Page::Wram(0x1_FFFF));
        }
    }

    // @source: fullsnes#snescartlorommappingromdividedinto32kbanksaround1500games
    // @source: adr-0017
    #[test]
    fn lorom_maps_32_kib_banks_and_their_mirrors() {
        let map = MemoryMap::new(Board::LoRom, 2 * MIB, 0);
        assert_eq!(map.resolve(0x00_8000), Page::Rom(0));
        assert_eq!(map.resolve(0x00_FFFF), Page::Rom(0x7FFF));
        assert_eq!(map.resolve(0x01_8000), Page::Rom(0x8000));
        assert_eq!(map.resolve(0x3F_FFFF), Page::Rom(0x1F_FFFF));
        // WS2 mirrors WS1.
        assert_eq!(map.resolve(0x81_8123), Page::Rom(0x8123));
        // The lower halves of $40-$7D,$C0-$FF mirror the upper halves.
        assert_eq!(map.resolve(0x41_0123), map.resolve(0x41_8123));
        assert_eq!(map.resolve(0xC1_0123), map.resolve(0x41_8123));
        // Banks $40 and up are past 2 MiB and mirror the ROM from its start.
        assert_eq!(map.resolve(0x40_8000), Page::Rom(0));
        assert_eq!(map.resolve(0xFF_FFFF), Page::Rom(0x1F_FFFF));
        // Without SRAM, $70-$7D lower halves are ROM mirrors and $6000-$7FFF of the system
        // banks is open.
        assert_eq!(map.resolve(0x70_0000), map.resolve(0x70_8000));
        assert_eq!(map.resolve(0x00_6000), Page::Open);
    }

    // @source: fullsnes#snesmemorymap
    // @source: fullsnes#snescartlorommappingromdividedinto32kbanksaround1500games
    // @source: adr-0017
    #[test]
    fn lorom_sram_fills_the_lower_halves_of_banks_70_to_7d() {
        let map = MemoryMap::new(Board::LoRom, MIB, 128 * KIB);
        assert_eq!(map.resolve(0x70_0000), Page::CartRam(0));
        assert_eq!(map.resolve(0x70_7FFF), Page::CartRam(0x7FFF));
        // "64K or 128K SRAM, which is divided into 32K chunks, mapped to bank 70h, 71h".
        assert_eq!(map.resolve(0x71_0000), Page::CartRam(0x8000));
        assert_eq!(map.resolve(0x73_7FFF), Page::CartRam(0x1_FFFF));
        assert_eq!(map.resolve(0x74_0000), Page::CartRam(0));
        assert_eq!(map.resolve(0xF1_0000), Page::CartRam(0x8000));
        assert_eq!(map.resolve(0xFF_0000), Page::CartRam(0x1_8000));
        // The upper halves stay ROM.
        assert_eq!(map.resolve(0x70_8000), Page::Rom(0x8_0000));
        // Banks $6F and $7E are not SRAM.
        assert_eq!(map.resolve(0x6F_0000), map.resolve(0x6F_8000));
        assert_eq!(map.resolve(0x7E_0000), Page::Wram(0));
    }

    // @source: fullsnes#snescartlorommappingromdividedinto32kbanksaround1500games
    // @source: adr-0017
    #[test]
    fn a_small_sram_repeats_through_its_area() {
        let map = MemoryMap::new(Board::LoRom, MIB, 2 * KIB);
        assert_eq!(map.resolve(0x70_0000), Page::CartRam(0));
        assert_eq!(map.resolve(0x70_0800), Page::CartRam(0));
        assert_eq!(map.resolve(0x70_27FF), Page::CartRam(0x7FF));
        assert_eq!(map.resolve(0x7D_7FFF), Page::CartRam(0x7FF));
    }

    // @source: fullsnes#snescarthirommappingromdividedinto64kbanksaround500games
    // @source: adr-0017
    #[test]
    fn hirom_maps_64_kib_banks_with_their_upper_halves_mirrored_low() {
        let map = MemoryMap::new(Board::HiRom, 4 * MIB, 0);
        assert_eq!(map.resolve(0xC0_0000), Page::Rom(0));
        assert_eq!(map.resolve(0xC0_FFFF), Page::Rom(0xFFFF));
        assert_eq!(map.resolve(0xFF_FFFF), Page::Rom(0x3F_FFFF));
        assert_eq!(map.resolve(0x40_1234), Page::Rom(0x1234));
        assert_eq!(map.resolve(0x7D_0000), Page::Rom(0x3D_0000));
        // $00-$3F,$80-$BF:8000-FFFF mirror the upper halves.
        assert_eq!(map.resolve(0x00_8000), Page::Rom(0x8000));
        assert_eq!(map.resolve(0x3F_FFFF), Page::Rom(0x3F_FFFF));
        assert_eq!(map.resolve(0x81_9ABC), Page::Rom(0x1_9ABC));
        // No SRAM: $6000-$7FFF is open.
        assert_eq!(map.resolve(0x20_6000), Page::Open);
    }

    // @source: fullsnes#snesmemorymap
    // @source: fullsnes#snescarthirommappingromdividedinto64kbanksaround500games
    // @source: adr-0017
    #[test]
    fn hirom_sram_is_at_6000_in_banks_20_to_3f() {
        let map = MemoryMap::new(Board::HiRom, MIB, 32 * KIB);
        assert_eq!(map.resolve(0x20_6000), Page::CartRam(0));
        assert_eq!(map.resolve(0x20_7FFF), Page::CartRam(0x1FFF));
        assert_eq!(map.resolve(0x21_6000), Page::CartRam(0x2000));
        assert_eq!(map.resolve(0x23_7FFF), Page::CartRam(0x7FFF));
        assert_eq!(map.resolve(0x24_6000), Page::CartRam(0));
        assert_eq!(map.resolve(0xA1_6000), Page::CartRam(0x2000));
        assert_eq!(map.resolve(0x30_6000), Page::CartRam(0x0000));
        assert_eq!(map.resolve(0x1F_6000), Page::Open);
        assert_eq!(map.resolve(0x9F_6000), Page::Open);
        // A 1 MiB ROM repeats through 4 MiB of banks.
        assert_eq!(map.resolve(0x50_0000), Page::Rom(0));
    }

    // @source: fullsnes#snescartridgeromimageinterleave
    // @source: fullsnes#snescarthirommappingromdividedinto64kbanksaround500games
    // @source: adr-0017
    #[test]
    fn exhirom_puts_the_slow_banks_after_the_fast_ones() {
        // Tales of Phantasia's size: 4 MiB fast, then 2 MiB slow.
        let map = MemoryMap::new(Board::ExHiRom, 6 * MIB, 8 * KIB);
        assert_eq!(map.resolve(0xC0_0000), Page::Rom(0));
        assert_eq!(map.resolve(0xFF_FFFF), Page::Rom(0x3F_FFFF));
        assert_eq!(map.resolve(0x80_8000), Page::Rom(0x8000));
        assert_eq!(map.resolve(0x40_0000), Page::Rom(0x40_0000));
        assert_eq!(map.resolve(0x5F_FFFF), Page::Rom(0x5F_FFFF));
        // Banks $60-$7D are past 6 MiB and repeat the 2 MiB of slow banks.
        assert_eq!(map.resolve(0x60_0000), Page::Rom(0x40_0000));
        // The header and vectors: $00:FFC0 is image offset $40FFC0.
        assert_eq!(map.resolve(0x00_FFC0), Page::Rom(0x40_FFC0));
        assert_eq!(map.resolve(0x00_FFFC), Page::Rom(0x40_FFFC));
        // SRAM is at $80-$BF:6000-7FFF only.
        assert_eq!(map.resolve(0x80_6000), Page::CartRam(0));
        assert_eq!(map.resolve(0xBF_7FFF), Page::CartRam(0x1FFF));
        assert_eq!(map.resolve(0x20_6000), Page::Open);
    }

    // @source: fullsnes#snescartridgeromheader
    // @source: adr-0017
    #[test]
    fn exhirom_5_mib_mirrors_its_last_mib() {
        // Dai Kaiju Monogatari 2: "4MB + 4 x Last 1MB".
        let map = MemoryMap::new(Board::ExHiRom, 5 * MIB, 0);
        for bank in [0x40_u32, 0x50, 0x60, 0x70] {
            assert_eq!(map.resolve(bank << 16 | 0x1234), Page::Rom(0x40_1234));
        }
    }

    // @source: adr-0017
    #[test]
    fn every_offset_is_inside_the_rom_and_ram() {
        let rom_lengths = [32 * KIB, 33 * KIB, 40 * KIB + 5, 3 * MIB, 5 * MIB, 6 * MIB];
        for board in [Board::LoRom, Board::HiRom, Board::ExHiRom] {
            for rom_length in rom_lengths {
                for sram_length in [0, 2 * KIB, 128 * KIB] {
                    let map = MemoryMap::new(board, rom_length, sram_length);
                    for address in (0..0x100_0000).step_by(0x3FF) {
                        match map.resolve(address) {
                            Page::Rom(offset) => {
                                assert!((offset as usize) < rom_length, "{address:06X}");
                            }
                            Page::CartRam(offset) => {
                                assert!((offset as usize) < sram_length, "{address:06X}");
                            }
                            Page::Wram(offset) => assert!(offset < WRAM_SIZE),
                            Page::Io | Page::Coproc(_) | Page::Open => {}
                        }
                    }
                }
            }
        }
    }

    // @source: fullsnes#snescartridgeromheader
    // @source: adr-0017
    #[test]
    fn resolving_matches_mirroring_each_address() {
        // A ROM whose length is not a multiple of 8 KiB ends inside a page.
        let rom_length = 33 * KIB + 512;
        let map = MemoryMap::new(Board::HiRom, rom_length, 0);
        for address in 0xC0_0000..0xC2_0000 {
            let expected = mirror(address & 0xFFFF, u32::try_from(rom_length).unwrap());
            assert_eq!(map.resolve(address), Page::Rom(expected), "{address:06X}");
        }
    }

    // @source: anomie-memmap#memory-map
    #[test]
    fn the_map_has_one_page_per_8_kib() {
        let map = MemoryMap::new(Board::LoRom, 32 * KIB, 0);
        assert_eq!(map.pages().len(), 2048);
        assert_eq!(map.page(0xFF_FFFF), map.pages()[2047]);
        assert_eq!(map.page(0x100_8000), map.page(0x00_8000));
    }
}
