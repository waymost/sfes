// @source: fullsnes#snescartridgeromheader
//! SNES cartridge header detection, boards and coprocessors.
//!
//! - [`Header`]: finds the cartridge header in a ROM image by scoring the LoROM, HiROM and
//!   ExHiROM locations, drops a copier header, and gives the region (EMU-04, EMU-09,
//!   ADR 0016).
//!
//! Boards, the memory map and coprocessors follow; see the technical design, section 5.
//! `docs/chips/cart.md` explains the cartridge and this crate.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod header;

pub use crate::header::{
    Candidate, Header, HeaderError, Mapping, Options, Region, TITLE_LENGTH, checksum,
    has_copier_header, strip_copier_header,
};
