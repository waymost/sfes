// @source: adr-0017
// @source: adr-0016
//! A loaded cartridge: its header, board, ROM, RAM and memory map (EMU-04).

use alloc::{sync::Arc, vec, vec::Vec};

use sfes_api::RegionId;

use crate::{
    board::Board,
    header::{Header, HeaderError, Options, strip_copier_header},
    map::MemoryMap,
};

/// The largest RAM size byte mapped: `1 SHL 7` KiB is 128 KiB (ADR 0017).
const LARGEST_RAM_SIZE: u8 = 7;

/// Why a ROM image could not be loaded.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CartError {
    /// The header could not be found.
    #[error(transparent)]
    Header(#[from] HeaderError),
    /// The header asks for more cartridge RAM than any board maps.
    #[error("RAM size byte ${ram_size:02X} is larger than the largest mapped, $07 (128 KiB)")]
    RamSize {
        /// The header's RAM size byte.
        ram_size: u8,
    },
}

/// A cartridge ready for the bus: the ROM, the cartridge RAM and the map that reaches them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Cartridge {
    header: Header,
    board: Board,
    rom: Arc<[u8]>,
    ram: Vec<u8>,
    map: Arc<MemoryMap>,
}

impl Cartridge {
    /// Loads a ROM image: detects its header and board, drops any copier header, allocates
    /// the cartridge RAM the header asks for and builds the memory map (ADR 0016, ADR 0017).
    ///
    /// # Errors
    ///
    /// [`CartError::Header`] when no header is found and [`CartError::RamSize`] when it asks
    /// for more than 128 KiB of RAM.
    // @source: adr-0016
    // @source: adr-0017
    pub fn load(image: &[u8], options: Options) -> Result<Self, CartError> {
        let header = Header::detect_with(image, options)?;
        if header.ram_size > LARGEST_RAM_SIZE {
            return Err(CartError::RamSize {
                ram_size: header.ram_size,
            });
        }
        let ram_length = if header.ram_size == 0 {
            0
        } else {
            1024_usize << header.ram_size
        };
        let board = Board::from(header.mapping);
        let rom = Arc::<[u8]>::from(strip_copier_header(image));
        let map = Arc::new(MemoryMap::new(board, rom.len(), ram_length));
        Ok(Self {
            header,
            board,
            rom,
            ram: vec![0; ram_length],
            map,
        })
    }

    /// The header the cartridge was loaded with.
    // @source: adr-0017
    #[must_use]
    pub const fn header(&self) -> &Header {
        &self.header
    }

    /// The board.
    // @source: adr-0017
    #[must_use]
    pub const fn board(&self) -> Board {
        self.board
    }

    /// The memory map, shared with every clone of the cartridge.
    // @source: adr-0017
    #[must_use]
    pub const fn map(&self) -> &Arc<MemoryMap> {
        &self.map
    }

    /// The ROM, without its copier header.
    // @source: adr-0017
    #[must_use]
    pub fn rom(&self) -> &[u8] {
        &self.rom
    }

    /// The cartridge RAM; empty when there is none.
    // @source: adr-0017
    #[must_use]
    pub fn ram(&self) -> &[u8] {
        &self.ram
    }

    /// The cartridge RAM, writable.
    // @source: adr-0017
    pub fn ram_mut(&mut self) -> &mut [u8] {
        &mut self.ram
    }

    /// The cartridge's memory regions: `Rom` and `CartRam`; every other region is empty here
    /// (FND-06).
    // @source: adr-0017
    #[must_use]
    pub fn region(&self, id: RegionId) -> &[u8] {
        match id {
            RegionId::Rom => &self.rom,
            RegionId::CartRam => &self.ram,
            RegionId::Wram | RegionId::Vram | RegionId::Cgram | RegionId::Oam | RegionId::Aram => {
                &[]
            }
        }
    }

    /// The writable cartridge region: `CartRam`. The ROM and every other region are empty
    /// here.
    // @source: adr-0017
    pub fn region_mut(&mut self, id: RegionId) -> &mut [u8] {
        match id {
            RegionId::CartRam => &mut self.ram,
            RegionId::Rom
            | RegionId::Wram
            | RegionId::Vram
            | RegionId::Cgram
            | RegionId::Oam
            | RegionId::Aram => &mut [],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{header::Mapping, map::Page};

    /// A blank image of `length` bytes with a minimal `mapping` header asking for RAM size
    /// `ram_size`.
    // @source: fullsnes#snescartridgeromheader
    fn image(mapping: Mapping, length: usize, ram_size: u8) -> Vec<u8> {
        let mut image = vec![0; length];
        let header = mapping.header_offset();
        let map_mode = match mapping {
            Mapping::LoRom => 0x20,
            Mapping::HiRom => 0x21,
            Mapping::ExHiRom => 0x25,
        };
        image[header..header + 0x15].fill(b' ');
        image[header + 0x15] = map_mode;
        image[header + 0x17] = 0x0C;
        image[header + 0x18] = ram_size;
        image[header + 0x3D] = 0x80;
        image
    }

    // @source: fullsnes#snescartridgeromheader
    // @source: adr-0017
    #[test]
    fn the_header_sets_the_board_and_ram() {
        let cartridge = Cartridge::load(&image(Mapping::HiRom, 0x2_0000, 3), Options::default())
            .expect("loads");
        assert_eq!(cartridge.board(), Board::HiRom);
        assert_eq!(cartridge.ram().len(), 8 * 1024);
        assert_eq!(cartridge.region(RegionId::CartRam).len(), 8 * 1024);
        assert_eq!(cartridge.map().resolve(0x20_6000), Page::CartRam(0));
        assert_eq!(cartridge.map().resolve(0x00_FFFD), Page::Rom(0xFFFD));
    }

    // @source: fullsnes#snescartridgeromimageheadersandfileextensions
    // @source: adr-0017
    #[test]
    fn the_rom_region_has_no_copier_header() {
        let rom = image(Mapping::LoRom, 0x8000, 0);
        let mut copied = vec![0xAA; 512];
        copied.extend_from_slice(&rom);
        let cartridge = Cartridge::load(&copied, Options::default()).expect("loads");
        assert_eq!(cartridge.region(RegionId::Rom), rom.as_slice());
        assert_eq!(cartridge.region(RegionId::CartRam), &[] as &[u8]);
        assert_eq!(cartridge.region(RegionId::Wram), &[] as &[u8]);
    }

    // @source: adr-0017
    #[test]
    fn only_the_cartridge_ram_is_writable() {
        let mut cartridge =
            Cartridge::load(&image(Mapping::LoRom, 0x8000, 1), Options::default()).expect("loads");
        cartridge.region_mut(RegionId::CartRam)[0x7FF] = 0x5A;
        assert_eq!(cartridge.ram()[0x7FF], 0x5A);
        assert_eq!(cartridge.region_mut(RegionId::Rom).len(), 0);
        assert_eq!(cartridge.map().resolve(0x70_0FFF), Page::CartRam(0x7FF));
    }

    // @source: fullsnes#snescartridgeromheader
    // @source: adr-0017
    #[test]
    fn ram_over_128_kib_is_refused() {
        assert!(Cartridge::load(&image(Mapping::LoRom, 0x8000, 7), Options::default()).is_ok());
        assert_eq!(
            Cartridge::load(&image(Mapping::LoRom, 0x8000, 8), Options::default()),
            Err(CartError::RamSize { ram_size: 8 })
        );
    }

    // @source: adr-0013
    // @source: anomie-memmap#memory-map
    #[test]
    fn the_own_halt_cartridge_starts_in_its_rom() {
        let rom = include_bytes!("../../../tests/assets/carts/halt.sfc");
        let cartridge = Cartridge::load(rom, Options::default()).expect("loads");
        assert_eq!(cartridge.board(), Board::LoRom);
        let Page::Rom(offset) = cartridge.map().resolve(0x00_FFFC) else {
            panic!("the reset vector is not in ROM");
        };
        let offset = offset as usize;
        let vector = u16::from_le_bytes([rom[offset], rom[offset + 1]]);
        assert_eq!(vector, cartridge.header().reset_vector);
    }
}
