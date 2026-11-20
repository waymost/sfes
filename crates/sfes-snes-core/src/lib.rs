// @source: adr-0018
//! The Snes machine: bus, scheduler, DMA/HDMA, 5A22 I/O and controller ports.
//!
//! - [`Snes`]: one SNES as an owned, cloneable value, built by [`SnesBuilder`], with an
//!   inherent API shaped like `sfes_api::System` until `impl System` arrives in M4 (FND-01,
//!   FND-06, FND-08; ADR 0018).
//! - The bus, the line timing and the CPU's bus view are provisional until #80 and #81
//!   (ADR 0018, point 6).
//!
//! `docs/chips/core.md` explains the machine and this crate.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod bus;
mod machine;
mod timing;

pub use sfes_snes_cart::Region;

pub use crate::machine::{BuildError, Snes, SnesBuilder};
