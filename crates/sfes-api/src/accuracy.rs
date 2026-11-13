//! Accuracy profiles: a sealed marker trait chosen at compile time.
//!
//! Branches such as `if A::DOT_PPU { … }` are resolved at compile time, so each profile
//! compiles to its own specialized code. Sealing stops downstream crates from adding profiles
//! the core does not support.

mod sealed {
    pub trait Sealed {}
}

/// An accuracy profile, used as a type parameter of a machine.
pub trait Accuracy: sealed::Sealed + Copy + Default + Send + Sync + 'static {
    /// Profile name, recorded in save states and movies.
    const NAME: &'static str;
    /// Whether the PPU renders dot by dot (`false`: scanline rendering).
    const DOT_PPU: bool;
}

/// The fast profile: scanline rendering and catch-up scheduling.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Fast;

impl sealed::Sealed for Fast {}

impl Accuracy for Fast {
    const NAME: &'static str = "fast";
    const DOT_PPU: bool = false;
}
