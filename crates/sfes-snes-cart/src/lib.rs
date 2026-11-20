// @source: fullsnes#snescartridgeromheader
// @source: adr-0017
//! SNES cartridge header detection, boards and coprocessors.
//!
//! - [`Header`]: finds the cartridge header in a ROM image by scoring the LoROM, HiROM and
//!   ExHiROM locations, drops a copier header, and gives the region (EMU-04, EMU-09,
//!   ADR 0016).
//!
//! - [`Cartridge`]: loads an image, keeps its ROM and cartridge RAM as `RegionId::Rom` and
//!   `RegionId::CartRam`, and builds its [`MemoryMap`] (ADR 0017).
//! - [`Board`]: how LoROM, HiROM and ExHiROM boards answer Address Bus A.
//! - [`MemoryMap`]: what answers each 8 KiB page, with ROM and SRAM mirrored.
//!
//! Coprocessors follow; see the technical design, section 5.
//! `docs/chips/cart.md` explains the cartridge and this crate.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod board;
mod cartridge;
mod header;
mod map;

pub use crate::{
    board::{Board, mirror},
    cartridge::{CartError, Cartridge},
    header::{
        Candidate, Header, HeaderError, Mapping, Options, Region, TITLE_LENGTH, checksum,
        has_copier_header, strip_copier_header,
    },
    map::{MemoryMap, PAGE_SIZE, PAGES, Page, console_page},
};
