// @source: fullsnes#snescartridgeromheader
// @source: adr-0016
//! Finding and reading the cartridge header (EMU-04, EMU-09).
//!
//! The header describing the cartridge sits at CPU address `$00:FFC0`, followed by the
//! exception vectors at `$FFE0–$FFFF`. Where that is in a ROM image depends on the board, so
//! [`Header::detect`] scores the LoROM, HiROM and ExHiROM candidate locations as ADR 0016
//! decides, after dropping any 512-byte copier header. [`Header::detect_with`] takes a board
//! and a region that override detection.

use alloc::vec::Vec;

/// The length of a copier header.
const COPIER_HEADER: usize = 512;
/// The length of the header area read at a candidate: `$FFC0–$FFFF`.
pub(crate) const AREA: usize = 0x40;
/// The longest title, `$FFC0–$FFD4`.
pub const TITLE_LENGTH: usize = 21;

/// Offsets inside the area, relative to `$FFC0`.
pub(crate) const TITLE: usize = 0x00;
/// `$FFD5`, ROM speed and map mode.
pub(crate) const MAP_MODE: usize = 0x15;
/// `$FFD6`, chipset.
const CHIPSET: usize = 0x16;
/// `$FFD7`, ROM size.
pub(crate) const ROM_SIZE: usize = 0x17;
/// `$FFD8`, RAM size.
pub(crate) const RAM_SIZE: usize = 0x18;
/// `$FFD9`, country.
pub(crate) const COUNTRY: usize = 0x19;
/// `$FFDA`, developer ID.
const DEVELOPER: usize = 0x1A;
/// `$FFDB`, ROM version.
const VERSION: usize = 0x1B;
/// `$FFDC`, checksum complement.
pub(crate) const COMPLEMENT: usize = 0x1C;
/// `$FFDE`, checksum.
pub(crate) const CHECKSUM: usize = 0x1E;
/// `$FFFC`, the 6502-mode RESET vector.
pub(crate) const RESET_VECTOR: usize = 0x3C;

/// The largest ROM size byte that scores: `1 SHL $0D` KiB is 8 MiB, ExHiROM rounded up.
pub(crate) const LARGEST_ROM_SIZE: u8 = 0x0D;

/// The board a header location belongs to.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Mapping {
    /// 32 KiB banks; header at image offset `$7FC0`.
    LoRom,
    /// 64 KiB banks; header at image offset `$FFC0`.
    HiRom,
    /// 64 KiB banks beyond 4 MiB; header at image offset `$40FFC0`.
    ExHiRom,
}

impl Mapping {
    /// Every board, in the order candidates are scored.
    pub const ALL: [Self; 3] = [Self::LoRom, Self::HiRom, Self::ExHiRom];

    /// Where the header (`$FFC0`) is in an image without copier header.
    // @source: fullsnes#snescartridgeromheader
    // @source: fullsnes#snescartridgeromimageinterleave
    #[must_use]
    pub const fn header_offset(self) -> usize {
        match self {
            Self::LoRom => 0x7FC0,
            Self::HiRom => 0xFFC0,
            Self::ExHiRom => 0x40_FFC0,
        }
    }

    /// Whether `mode`, the low four bits of `$FFD5`, names a board of this kind.
    // @source: fullsnes#snescartridgeromheader
    const fn accepts_map_mode(self, mode: u8) -> bool {
        match self {
            Self::LoRom => matches!(mode, 0x0 | 0x2 | 0x3),
            Self::HiRom => matches!(mode, 0x1 | 0xA),
            Self::ExHiRom => mode == 0x5,
        }
    }
}

/// The video standard a cartridge was made for (EMU-09).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Region {
    /// 60 Hz.
    Ntsc,
    /// 50 Hz.
    Pal,
}

impl Region {
    /// The video standard the country byte `$FFD9` implies, or `None` where the documents
    /// leave it open (ADR 0016).
    // @source: fullsnes#snescartridgeromheader
    // @source: adr-0016
    #[must_use]
    pub const fn from_country(country: u8) -> Option<Self> {
        match country {
            0x00 | 0x01 | 0x0D | 0x0F | 0x10 => Some(Self::Ntsc),
            0x02..=0x0C | 0x11 => Some(Self::Pal),
            _ => None,
        }
    }
}

/// Overrides for [`Header::detect_with`] (API-02).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Options {
    /// Read the header at this board's location without scoring.
    pub mapping: Option<Mapping>,
    /// Use this region instead of the one the header implies.
    pub region: Option<Region>,
}

/// A candidate's score, as reported in [`HeaderError::Ambiguous`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Candidate {
    /// The board whose location was read.
    pub mapping: Mapping,
    /// Its score (ADR 0016), out of 12.
    pub score: u8,
}

/// The header could not be found or read.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum HeaderError {
    /// The image is too short to hold any header location.
    #[error("a {length}-byte ROM image is too short to hold a cartridge header")]
    TooShort {
        /// The image length, without copier header.
        length: usize,
    },
    /// No candidate location holds a RESET vector in `$8000–$FFFE`.
    #[error(
        "no header location has a RESET vector in $8000-$FFFE (read: {tried:?}); pass a board override"
    )]
    NoHeader {
        /// The boards whose locations fit in the image and were read.
        tried: Vec<Mapping>,
    },
    /// Two or more candidates share the highest score.
    #[error("header locations score equally ({candidates:?}); pass a board override")]
    Ambiguous {
        /// The tied candidates.
        candidates: Vec<Candidate>,
    },
    /// The overriding board's header location is beyond the image.
    #[error("a {length}-byte ROM image has no {mapping:?} header location")]
    OutOfRange {
        /// The board asked for.
        mapping: Mapping,
        /// The image length, without copier header.
        length: usize,
    },
}

/// A cartridge header, as found in a ROM image.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Header {
    /// The board whose location held the header.
    pub mapping: Mapping,
    /// Whether the image started with a 512-byte copier header, which was dropped.
    pub copier_header: bool,
    /// The score that chose this header, or `None` when a board override chose it.
    pub score: Option<u8>,
    /// `$FFC0–$FFD4`: the title, as stored.
    pub title: [u8; TITLE_LENGTH],
    /// `$FFD5`: ROM speed (bit 4) and map mode (bits 3–0).
    pub map_mode: u8,
    /// `$FFD6`: chipset.
    pub chipset: u8,
    /// `$FFD7`: ROM size, `1 SHL n` KiB.
    pub rom_size: u8,
    /// `$FFD8`: RAM size, `1 SHL n` KiB, 0 for none.
    pub ram_size: u8,
    /// `$FFD9`: country.
    pub country: u8,
    /// `$FFDA`: developer ID.
    pub developer: u8,
    /// `$FFDB`: ROM version.
    pub version: u8,
    /// `$FFDC`: checksum complement.
    pub checksum_complement: u16,
    /// `$FFDE`: checksum.
    pub checksum: u16,
    /// `$FFFC`: the RESET vector.
    pub reset_vector: u16,
    /// The region: the override, or the one the country byte implies.
    pub region: Option<Region>,
}

impl Header {
    /// Finds the header in `image` by scoring the candidate locations (ADR 0016).
    ///
    /// # Errors
    ///
    /// Returns a [`HeaderError`] when no location holds a plausible header or two share the
    /// highest score.
    // @source: adr-0016
    pub fn detect(image: &[u8]) -> Result<Self, HeaderError> {
        Self::detect_with(image, Options::default())
    }

    /// Finds the header in `image`, taking the board and region from `options` when given.
    ///
    /// # Errors
    ///
    /// Returns a [`HeaderError`] as [`Header::detect`] does, or
    /// [`HeaderError::OutOfRange`] when the overriding board's location is beyond the image.
    // @source: adr-0016
    pub fn detect_with(image: &[u8], options: Options) -> Result<Self, HeaderError> {
        let Options { mapping, region } = options;
        let copier_header = has_copier_header(image);
        let rom = strip_copier_header(image);
        let (mapping, score) = if let Some(mapping) = mapping {
            (mapping, None)
        } else {
            let best = best_candidate(rom)?;
            (best.mapping, Some(best.score))
        };
        let Some(bytes) = area(rom, mapping) else {
            return Err(HeaderError::OutOfRange {
                mapping,
                length: rom.len(),
            });
        };
        let mut header = Self::read(mapping, bytes);
        header.copier_header = copier_header;
        header.score = score;
        header.region = region.or(header.region);
        Ok(header)
    }

    /// Whether the ROM asks for FastROM timing (`$FFD5` bit 4).
    // @source: fullsnes#snescartridgeromheader
    #[must_use]
    pub const fn fast_rom(&self) -> bool {
        self.map_mode & 0x10 != 0
    }

    /// Reads the fields of the header area `bytes` found at `mapping`'s location.
    // @source: fullsnes#snescartridgeromheader
    // @source: snesdev-wiki#ROM_header
    fn read(mapping: Mapping, bytes: &[u8; AREA]) -> Self {
        let mut title = [0; TITLE_LENGTH];
        title.copy_from_slice(&bytes[TITLE..TITLE + TITLE_LENGTH]);
        let country = bytes[COUNTRY];
        Self {
            mapping,
            copier_header: false,
            score: None,
            title,
            map_mode: bytes[MAP_MODE],
            chipset: bytes[CHIPSET],
            rom_size: bytes[ROM_SIZE],
            ram_size: bytes[RAM_SIZE],
            country,
            developer: bytes[DEVELOPER],
            version: bytes[VERSION],
            checksum_complement: word(bytes, COMPLEMENT),
            checksum: word(bytes, CHECKSUM),
            reset_vector: word(bytes, RESET_VECTOR),
            region: Region::from_country(country),
        }
    }
}

/// Whether `image` starts with a copier header: its length is 512 modulo 1024.
// @source: fullsnes#snescartridgeromimageheadersandfileextensions
#[must_use]
pub fn has_copier_header(image: &[u8]) -> bool {
    image.len() % 1024 == COPIER_HEADER
}

/// `image` without its copier header, if it has one.
// @source: fullsnes#snescartridgeromimageheadersandfileextensions
#[must_use]
pub fn strip_copier_header(image: &[u8]) -> &[u8] {
    if has_copier_header(image) {
        image.get(COPIER_HEADER..).unwrap_or_default()
    } else {
        image
    }
}

/// The 16-bit checksum of `rom` without copier header: every byte added, an image whose length
/// is not a power of two being summed as its largest power-of-two part followed by the rest,
/// itself summed this way, repeated up to twice that part. This is the layout [`crate::mirror`]
/// maps (ADR 0017), so the sum is that of every byte the cartridge answers in a window of the
/// length rounded up to a power of two.
// @source: fullsnes#snescartridgeromheader
// @source: snesdev-wiki#ROM_header
// @source: adr-0016
// @source: adr-0017
#[must_use]
pub fn checksum(rom: &[u8]) -> u16 {
    let sum = |bytes: &[u8]| {
        bytes
            .iter()
            .fold(0_u16, |total, byte| total.wrapping_add(u16::from(*byte)))
    };
    if rom.is_empty() || rom.len().is_power_of_two() {
        return sum(rom);
    }
    let first_length = 1_usize << rom.len().ilog2();
    let (first, rest) = rom.split_at(first_length);
    // `rest` fills a window of its own length rounded up to a power of two, mirrored the same
    // way, and that window repeats until it is as long as `first`. Only the copies modulo
    // $10000 change a 16-bit sum.
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the sum is modulo $10000, so only the low 16 bits of the count matter"
    )]
    let copies = (first_length / rest.len().next_power_of_two()) as u16;
    sum(first).wrapping_add(checksum(rest).wrapping_mul(copies))
}

/// The candidate with the highest score, or the error that stops detection.
// @source: adr-0016
fn best_candidate(rom: &[u8]) -> Result<Candidate, HeaderError> {
    let present = Mapping::ALL
        .into_iter()
        .filter(|mapping| area(rom, *mapping).is_some())
        .collect::<Vec<_>>();
    if present.is_empty() {
        return Err(HeaderError::TooShort { length: rom.len() });
    }
    let image_checksum = checksum(rom);
    let candidates = present
        .iter()
        .filter_map(|mapping| {
            let bytes = area(rom, *mapping)?;
            let score = score(*mapping, bytes, rom.len(), image_checksum)?;
            Some(Candidate {
                mapping: *mapping,
                score,
            })
        })
        .collect::<Vec<_>>();
    let Some(best) = candidates.iter().map(|candidate| candidate.score).max() else {
        return Err(HeaderError::NoHeader { tried: present });
    };
    let tied = candidates
        .into_iter()
        .filter(|candidate| candidate.score == best)
        .collect::<Vec<_>>();
    match tied.as_slice() {
        [winner] => Ok(*winner),
        _ => Err(HeaderError::Ambiguous { candidates: tied }),
    }
}

/// The score of the header area `bytes` at `mapping`'s location in an image of
/// `image_length` bytes whose checksum is `image_checksum`, or `None` when its RESET vector
/// rules it out (ADR 0016).
// @source: fullsnes#snescartridgeromheader
// @source: snesdev-wiki#ROM_header
// @source: adr-0016
fn score(
    mapping: Mapping,
    bytes: &[u8; AREA],
    image_length: usize,
    image_checksum: u16,
) -> Option<u8> {
    let reset_vector = word(bytes, RESET_VECTOR);
    if !(0x8000..=0xFFFE).contains(&reset_vector) {
        return None;
    }
    let stored = word(bytes, CHECKSUM);
    let complement = word(bytes, COMPLEMENT);
    let map_mode = bytes[MAP_MODE];
    let rom_size = bytes[ROM_SIZE];
    let title = &bytes[TITLE..TITLE + TITLE_LENGTH];
    let checks = [
        (stored == image_checksum, 4),
        (stored ^ complement == 0xFFFF, 4),
        (
            map_mode & 0xE0 == 0x20 && mapping.accepts_map_mode(map_mode & 0x0F),
            2,
        ),
        (title.iter().all(|byte| is_title_byte(*byte)), 1),
        (rom_size_fits(rom_size, image_length), 1),
    ];
    Some(
        checks
            .into_iter()
            .filter(|(passed, _)| *passed)
            .map(|(_, points)| points)
            .sum(),
    )
}

/// Whether `byte` may appear in a title: ASCII `$20–$7E` or JIS X 0201 katakana `$A1–$DF`.
// @source: fullsnes#snescartridgeromheader
// @source: snesdev-wiki#ROM_header
const fn is_title_byte(byte: u8) -> bool {
    matches!(byte, 0x20..=0x7E | 0xA1..=0xDF)
}

/// Whether the ROM size byte `rom_size` is at most `$0D` and `1 SHL n` KiB holds
/// `image_length` bytes.
// @source: fullsnes#snescartridgeromheader
// @source: snesdev-wiki#ROM_header
fn rom_size_fits(rom_size: u8, image_length: usize) -> bool {
    rom_size <= LARGEST_ROM_SIZE && (1024_usize << rom_size) >= image_length
}

/// The 64-byte header area at `mapping`'s location, if `rom` holds all of it.
// @source: fullsnes#snescartridgeromheader
fn area(rom: &[u8], mapping: Mapping) -> Option<&[u8; AREA]> {
    let start = mapping.header_offset();
    rom.get(start..start.checked_add(AREA)?)?.try_into().ok()
}

/// The little-endian word at `offset` in the header area.
// @source: fullsnes#snescartridgeromheader
fn word(bytes: &[u8; AREA], offset: usize) -> u16 {
    u16::from_le_bytes([bytes[offset], bytes[offset + 1]])
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::*;
    use crate::test_support::{TestHeader, fill_checksum};

    /// A plausible header for `mapping` in a zeroed image of `length` bytes, with country
    /// `country` and the checksum filled in.
    // @source: fullsnes#snescartridgeromheader
    fn image(mapping: Mapping, length: usize, country: u8) -> Vec<u8> {
        let header = TestHeader {
            country,
            ..TestHeader::default()
        };
        crate::test_support::image(mapping, length, header)
    }

    // @source: fullsnes#snescartridgeromheader
    #[test]
    fn each_board_is_found_at_its_location() {
        for (mapping, length) in [
            (Mapping::LoRom, 0x8000),
            (Mapping::HiRom, 0x1_0000),
            (Mapping::ExHiRom, 0x50_0000),
        ] {
            let header = Header::detect(&image(mapping, length, 0x01)).unwrap();
            assert_eq!(header.mapping, mapping);
            assert_eq!(header.score, Some(12));
            assert_eq!(&header.title[..9], b"SFES TEST");
            assert_eq!(header.reset_vector, 0x8000);
            assert_eq!(header.checksum ^ header.checksum_complement, 0xFFFF);
        }
    }

    // @source: fullsnes#snescartridgeromheader
    #[test]
    fn a_hirom_image_is_not_taken_for_lorom() {
        let mut rom = image(Mapping::HiRom, 0x2_0000, 0x00);
        // Program bytes at the LoROM location that happen to look like a RESET vector.
        rom[0x7FC0 + RESET_VECTOR..0x7FC0 + RESET_VECTOR + 2].copy_from_slice(&[0x34, 0x92]);
        fill_checksum(&mut rom, Mapping::HiRom);
        let header = Header::detect(&rom).unwrap();
        assert_eq!(header.mapping, Mapping::HiRom);
    }

    // @source: fullsnes#snescartridgeromimageheadersandfileextensions
    #[test]
    fn a_copier_header_is_detected_and_dropped() {
        let rom = image(Mapping::LoRom, 0x8000, 0x01);
        let mut copied = vec![0xAA; COPIER_HEADER];
        copied.extend_from_slice(&rom);
        assert!(has_copier_header(&copied));
        assert!(!has_copier_header(&rom));
        assert_eq!(strip_copier_header(&copied), rom.as_slice());
        let header = Header::detect(&copied).unwrap();
        assert!(header.copier_header);
        assert_eq!(header.mapping, Mapping::LoRom);
        assert_eq!(header.score, Some(12));
    }

    // @source: fullsnes#snescartridgeromheader
    // @source: snesdev-wiki#ROM_header
    #[test]
    fn checksum_mirrors_the_smaller_part() {
        // 3 KiB: 2 KiB of 1s, then 1 KiB of 2s counted twice.
        let mut rom = vec![1_u8; 0x800];
        rom.extend(vec![2_u8; 0x400]);
        let expected = 0x800 + 2 * 0x400 * 2;
        assert_eq!(checksum(&rom), u16::try_from(expected).unwrap());
        // 5 KiB as 4 KiB plus four copies of the last 1 KiB, as fullsnes lists for 5 MiB.
        let mut rom = vec![1_u8; 0x1000];
        rom.extend(vec![3_u8; 0x400]);
        assert_eq!(
            checksum(&rom),
            u16::try_from(0x1000 + 4 * 3 * 0x400).unwrap()
        );
        // A remainder that is not a power of two is split again: 4 KiB + 768 bytes is counted
        // as 4 KiB + 4 × (512 bytes + 2 × the last 256).
        let mut rom = vec![0_u8; 0x1000];
        rom.extend(vec![1_u8; 0x300]);
        assert_eq!(checksum(&rom), 4 * (0x200 + 2 * 0x100));
        // The sum wraps at 16 bits.
        assert_eq!(checksum(&[0xFF; 0x200]), 0xFE00);
    }

    // @source: fullsnes#snescartridgeromheader
    // @source: adr-0016
    // @source: adr-0017
    #[test]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "lengths and offsets here are under 64 KiB, and the fill keeps its low byte"
    )]
    fn checksum_sums_the_bytes_the_mirrored_map_reads() {
        for length in [1, 3, 0x300, 0x1C00, 0x1C01, 0x2A00, 0x3001, 0x8000] {
            let rom = (0..length)
                .map(|index: usize| (index * 7 + index / 251) as u8)
                .collect::<Vec<_>>();
            let window = length.next_power_of_two();
            let mirrored = (0..window)
                .map(|offset| rom[crate::mirror(offset as u32, length as u32) as usize])
                .fold(0_u16, |total, byte| total.wrapping_add(u16::from(byte)));
            assert_eq!(checksum(&rom), mirrored, "length {length:#x}");
        }
    }

    // @source: fullsnes#snescartridgeromheader
    // @source: adr-0016
    #[test]
    fn country_codes_give_regions() {
        let cases = [
            (0x00, Some(Region::Ntsc)),
            (0x01, Some(Region::Ntsc)),
            (0x02, Some(Region::Pal)),
            (0x06, Some(Region::Pal)),
            (0x0C, Some(Region::Pal)),
            (0x0D, Some(Region::Ntsc)),
            (0x0E, None),
            (0x0F, Some(Region::Ntsc)),
            (0x10, Some(Region::Ntsc)),
            (0x11, Some(Region::Pal)),
            (0x12, None),
            (0x14, None),
            (0xFF, None),
        ];
        for (country, region) in cases {
            assert_eq!(
                Region::from_country(country),
                region,
                "country {country:#04x}"
            );
        }
        let header = Header::detect(&image(Mapping::LoRom, 0x8000, 0x02)).unwrap();
        assert_eq!(header.region, Some(Region::Pal));
    }

    // @source: adr-0016
    #[test]
    fn overrides_replace_board_and_region() {
        let rom = image(Mapping::HiRom, 0x1_0000, 0x01);
        let options = Options {
            mapping: Some(Mapping::LoRom),
            region: Some(Region::Pal),
        };
        let header = Header::detect_with(&rom, options).unwrap();
        assert_eq!(header.mapping, Mapping::LoRom);
        assert_eq!(header.score, None);
        assert_eq!(header.region, Some(Region::Pal));
        let region_only = Options {
            mapping: None,
            region: Some(Region::Pal),
        };
        let header = Header::detect_with(&rom, region_only).unwrap();
        assert_eq!(
            (header.mapping, header.region),
            (Mapping::HiRom, Some(Region::Pal))
        );
        let beyond = Options {
            mapping: Some(Mapping::ExHiRom),
            region: None,
        };
        assert_eq!(
            Header::detect_with(&rom, beyond),
            Err(HeaderError::OutOfRange {
                mapping: Mapping::ExHiRom,
                length: 0x1_0000
            })
        );
    }

    // @source: adr-0016
    #[test]
    fn detection_errors_carry_what_to_act_on() {
        assert_eq!(
            Header::detect(&[0; 0x4000]),
            Err(HeaderError::TooShort { length: 0x4000 })
        );
        assert_eq!(
            Header::detect(&vec![0; 0x1_0000]),
            Err(HeaderError::NoHeader {
                tried: vec![Mapping::LoRom, Mapping::HiRom]
            })
        );
        // Two locations with the same bytes, no title and a wrong checksum score equally.
        let mut rom = vec![0_u8; 0x1_0000];
        for base in [0x7FC0, 0xFFC0] {
            rom[base + RESET_VECTOR + 1] = 0x80;
        }
        assert_eq!(
            Header::detect(&rom),
            Err(HeaderError::Ambiguous {
                candidates: vec![
                    Candidate {
                        mapping: Mapping::LoRom,
                        score: 0
                    },
                    Candidate {
                        mapping: Mapping::HiRom,
                        score: 0
                    },
                ]
            })
        );
    }

    // @source: snesdev-wiki#ROM_header
    // @source: adr-0016
    #[test]
    fn homebrew_without_checksum_is_found_by_the_rest() {
        let mut rom = image(Mapping::LoRom, 0x8000, 0x01);
        let base = Mapping::LoRom.header_offset();
        rom[base + COMPLEMENT..base + CHECKSUM + 2].fill(0);
        let header = Header::detect(&rom).unwrap();
        assert_eq!((header.mapping, header.score), (Mapping::LoRom, Some(4)));
        rom[base + RESET_VECTOR + 1] = 0x7F;
        assert!(matches!(
            Header::detect(&rom),
            Err(HeaderError::NoHeader { .. })
        ));
    }

    // @source: adr-0013
    // @source: adr-0016
    #[test]
    fn the_own_halt_cartridge_is_lorom() {
        let rom = include_bytes!("../../../tests/assets/carts/halt.sfc");
        let header = Header::detect(rom).unwrap();
        assert_eq!(header.mapping, Mapping::LoRom);
        assert_eq!(header.score, Some(12));
        assert!(!header.copier_header);
    }

    // @source: adr-0016
    #[test]
    fn no_input_panics() {
        // A fixed pseudo-random fill (xorshift), at lengths around every boundary detection
        // looks at.
        let mut state = 0x2545_F491_u32;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state.to_le_bytes()[0]
        };
        let lengths = [
            0, 1, 0x1FF, 0x200, 0x201, 0x7FFF, 0x8000, 0x8200, 0xFFFF, 0x1_0000, 0x1_0200,
            0x1_8000, 0x40_FFFF, 0x41_0000, 0x41_0200,
        ];
        for length in lengths {
            let image = (0..length).map(|_| next()).collect::<Vec<_>>();
            let _ = Header::detect(&image);
            for mapping in Mapping::ALL {
                let options = Options {
                    mapping: Some(mapping),
                    region: None,
                };
                let _ = Header::detect_with(&image, options);
            }
            let _ = checksum(&image);
        }
    }
}
