//! Test-vector runners, golden-hash checks and differential harness.
//!
//! - [`vectors`]: where the fetched vector sets are and how much of them to run.
//! - [`sst65816`]: runs the `SingleStepTests` 65816 vectors against
//!   [`sfes_snes_cpu::Cpu65816`], comparing every cycle's address, value and signals
//!   (ADR 0004).

#![forbid(unsafe_code)]

pub mod sst65816;
pub mod vectors;
