// @source: fullsnes#snescartridgeromheader
// @source: adr-0016
//! ROM images for tests, here and in the crates that load cartridges: a plausible header for
//! a board, a valid checksum and, optionally, code at the RESET vector. Built only for this
//! crate's tests and with the `test-support` feature.

use alloc::{vec, vec::Vec};

use crate::header::{
    CHECKSUM, COMPLEMENT, COUNTRY, LARGEST_ROM_SIZE, MAP_MODE, Mapping, RAM_SIZE, RESET_VECTOR,
    ROM_SIZE, TITLE, TITLE_LENGTH, checksum,
};

/// The title every test image carries, padded with spaces.
pub const TEST_TITLE: &[u8; TITLE_LENGTH] = b"SFES TEST            ";
/// The RESET vector every test image carries.
pub const TEST_RESET_VECTOR: u16 = 0x8000;

/// The header fields a test image chooses; every field not named here is set by
/// [`write_header`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TestHeader {
    /// `$FFD9`: the country, `$01` (NTSC) by default.
    pub country: u8,
    /// `$FFD8`: the RAM size, `1 SHL n` KiB, 0 (none) by default.
    pub ram_size: u8,
}

impl Default for TestHeader {
    fn default() -> Self {
        Self {
            country: 0x01,
            ram_size: 0,
        }
    }
}

/// A zeroed image of `length` bytes with `header` at `mapping`'s location.
// @source: fullsnes#snescartridgeromheader
// @source: adr-0016
#[must_use]
pub fn image(mapping: Mapping, length: usize, header: TestHeader) -> Vec<u8> {
    let mut image = vec![0; length];
    write_header(&mut image, mapping, header);
    image
}

/// A zeroed image of `length` bytes with `code` at the RESET vector, `$00:8000`, and `header`
/// at `mapping`'s location.
// @source: fullsnes#snescartridgeromheader
// @source: adr-0017
#[must_use]
pub fn image_with_code(
    mapping: Mapping,
    length: usize,
    code: &[u8],
    header: TestHeader,
) -> Vec<u8> {
    let mut image = vec![0; length];
    // `$00:8000` is the start of the 32 KiB that holds the header.
    let start = mapping.header_offset() & !0x7FFF;
    image[start..start + code.len()].copy_from_slice(code);
    write_header(&mut image, mapping, header);
    image
}

/// Writes the 64-byte header area at `mapping`'s location in `image`: [`TEST_TITLE`], the
/// board's map mode with Slow ROM, the smallest ROM size byte covering the image, `header`'s
/// country and RAM size, every other field 0, the RESET vector [`TEST_RESET_VECTOR`], the other
/// vectors 0, and then the checksum and its complement.
// @source: fullsnes#snescartridgeromheader
// @source: adr-0016
pub fn write_header(image: &mut [u8], mapping: Mapping, header: TestHeader) {
    let TestHeader { country, ram_size } = header;
    let base = mapping.header_offset();
    image[base..base + 0x40].fill(0);
    image[base + TITLE..base + TITLE + TITLE_LENGTH].copy_from_slice(TEST_TITLE);
    image[base + MAP_MODE] = match mapping {
        Mapping::LoRom => 0x20,
        Mapping::HiRom => 0x21,
        Mapping::ExHiRom => 0x25,
    };
    image[base + ROM_SIZE] = (0..=LARGEST_ROM_SIZE)
        .find(|size| (1024_usize << size) >= image.len())
        .unwrap_or(LARGEST_ROM_SIZE);
    image[base + RAM_SIZE] = ram_size;
    image[base + COUNTRY] = country;
    image[base + RESET_VECTOR..base + RESET_VECTOR + 2]
        .copy_from_slice(&TEST_RESET_VECTOR.to_le_bytes());
    fill_checksum(image, mapping);
}

/// Writes the checksum and its complement into the header at `mapping`'s location, as
/// fullsnes describes: the sum taken with `$FFDC–$FFDF` as `FF FF 00 00`.
// @source: fullsnes#snescartridgeromheader
pub fn fill_checksum(image: &mut [u8], mapping: Mapping) {
    let base = mapping.header_offset();
    image[base + COMPLEMENT..base + CHECKSUM + 2].copy_from_slice(&[0xFF, 0xFF, 0, 0]);
    let sum = checksum(image);
    image[base + COMPLEMENT..base + COMPLEMENT + 2].copy_from_slice(&(sum ^ 0xFFFF).to_le_bytes());
    image[base + CHECKSUM..base + CHECKSUM + 2].copy_from_slice(&sum.to_le_bytes());
}
