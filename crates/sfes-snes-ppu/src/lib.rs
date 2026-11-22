// @source: anomie-regs#registers
// @source: fullsnes#snespictureprocessingunitppu
// @source: adr-0028
// @source: adr-0029
//! SNES PPU: scanline renderer, layers and sprite evaluation.
//!
//! - [`Ppu`]: PPU1 and PPU2 as one unit: VRAM, CGRAM and OAM and their ports, the
//!   registers at `$2100–$213F` as [`Register`], the multiplier, the status registers,
//!   PPU1 and PPU2 open bus, the H/V counter latch, and [`Ppu::render_line`] (ADR 0028;
//!   ADR 0029).
//! - [`Signals`]: what the machine passes the PPU before each access to its registers.
//! - [`LineOut`]: one rendered line, 256 or 512 pixels.
//!
//! The machine decodes addresses, syncs the PPU before each access to its registers and
//! presents the rendered lines as frames (ADR 0028). `docs/chips/ppu.md` explains the chip
//! and this crate.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;
#[cfg(test)]
extern crate std;

mod counters;
mod line;
mod memory;
mod mode7;
mod ppu;
mod register;
mod signals;

pub use crate::{
    line::{LineOut, LineWidth},
    ppu::{CGRAM_SIZE, OAM_SIZE, Ppu, VRAM_SIZE},
    register::{NotAPpuRegister, ReadKind, Register},
    signals::{Period, Signals},
};
