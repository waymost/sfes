// @source: wdc-w65c816s
//! 65C816 CPU core, generic over its bus (main CPU and SA-1).
//!
//! The core is written from the W65C816S datasheet and checked against the `SingleStepTests`
//! vectors, bus cycle by bus cycle (EMU-02). ADR 0004 defines the bus contract and what one
//! [`Cpu65816::step`] covers. `docs/chips/cpu.md` explains the chip and this crate.

#![no_std]
#![forbid(unsafe_code)]

#[cfg(test)]
extern crate std;

mod addressing;
mod bus;
mod instructions;
mod registers;
mod step;
mod width;

pub use crate::{
    bus::{Bus65816, Pending, Pins},
    registers::{Cpu65816, Flags, RunState},
};
