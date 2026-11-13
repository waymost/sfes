//! Console-neutral traits and shared types for sfes.
//!
//! Four traits carry the design: [`System`] (what any console offers), [`Accuracy`] (a sealed
//! marker chosen at compile time), [`Observer`] (zero-cost hooks) and, in the CPU crate, the bus
//! trait a CPU core uses to talk to its machine. This crate builds `no_std` + `alloc`; the
//! default `std` feature only adds `std` support to its dependencies.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod accuracy;
mod input;
mod observer;
mod state;
mod system;
mod units;

pub use crate::{
    accuracy::{Accuracy, Fast},
    input::Buttons,
    observer::{BusId, DmaEvent, InstrEvent, Interrupt, NoopObserver, Observer},
    state::{StateError, StateHash},
    system::{
        AudioChunk, Peek, RegionDescriptor, RegionId, Stop, StopReason, System, SystemDescriptor,
        VideoFrame,
    },
    units::{Addr24, Cycles},
};
