//! sfes: a headless-first, library-first SNES emulator core.
//!
//! This facade re-exports the workspace crates behind features: `snes` (default), `movie`,
//! `pool` and `env`. It builds `no_std` + `alloc` with default features off.

#![no_std]
#![forbid(unsafe_code)]

pub use sfes_api as api;
#[cfg(feature = "env")]
pub use sfes_env as env;
#[cfg(feature = "movie")]
pub use sfes_movie as movie;
#[cfg(feature = "pool")]
pub use sfes_pool as pool;
#[cfg(feature = "snes")]
pub use sfes_snes_core as snes;
