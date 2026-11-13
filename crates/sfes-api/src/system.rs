//! The console-neutral machine contract (FND-07, FND-08).

use alloc::vec::Vec;

use crate::state::{StateError, StateHash};

/// A memory region that can be read as a slice and written between steps (FND-06).
#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub enum RegionId {
    /// Work RAM.
    Wram,
    /// Video RAM.
    Vram,
    /// Palette RAM.
    Cgram,
    /// Sprite attribute RAM.
    Oam,
    /// Audio RAM.
    Aram,
    /// Cartridge RAM (SRAM or BW-RAM).
    CartRam,
    /// Read-only cartridge ROM.
    Rom,
}

/// Static description of one memory region.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RegionDescriptor {
    /// Which region.
    pub id: RegionId,
    /// Human-readable name.
    pub name: &'static str,
    /// Size in bytes, or 0 when it depends on the cartridge.
    pub size: usize,
    /// Whether it can be written through the API.
    pub writable: bool,
}

/// Static description of a console: name, ports, regions, video and audio.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SystemDescriptor {
    /// System id recorded in states, movies and specs (MS-04), e.g. `"snes"`.
    pub id: &'static str,
    /// Display name.
    pub name: &'static str,
    /// Number of controller ports.
    pub ports: usize,
    /// Memory regions the system exposes.
    pub regions: &'static [RegionDescriptor],
    /// Native audio sample rate in Hz.
    pub audio_rate: u32,
}

/// Where a call to [`System::run`] should stop. Every stop takes effect at the first instruction
/// boundary at or after its target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Stop {
    /// Run this many frames.
    Frames(u32),
    /// Run this many scanlines.
    Scanlines(u32),
    /// Run this many instructions.
    Instructions(u32),
    /// Run this many master-clock cycles.
    Cycles(u64),
    /// Run until a watch fires, or at most `max_frames` frames.
    Watch {
        /// Frame limit.
        max_frames: u32,
    },
}

/// Why [`System::run`] returned.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StopReason {
    /// The requested amount of emulation completed.
    Completed,
    /// A watch fired.
    Watch,
    /// The frame limit of a [`Stop::Watch`] was reached first.
    FrameLimit,
}

/// The result of a side-effect-free bus read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Peek {
    /// The value.
    pub value: u8,
    /// `true` when reading the address has side effects, so `value` is the last latched value.
    pub latched: bool,
}

/// One rendered video frame.
#[derive(Clone, Copy, Debug)]
pub struct VideoFrame<'frame> {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// RGB555 pixels, row-major.
    pub pixels: &'frame [u16],
}

/// Audio produced since the last call to [`System::take_audio`].
#[derive(Clone, Copy, Debug)]
pub struct AudioChunk<'chunk> {
    /// Sample rate in Hz.
    pub rate: u32,
    /// Interleaved stereo `i16` samples.
    pub samples: &'chunk [i16],
}

/// What any emulated console offers. Machines are owned values: `Clone` is the snapshot.
pub trait System: Clone + Send {
    /// One port's input for one frame.
    type Input: Copy + Default;

    /// Static description of this system.
    fn descriptor(&self) -> &'static SystemDescriptor;

    /// Presses the reset button.
    fn reset(&mut self);

    /// Powers off and on, filling undefined power-on state from `seed`.
    fn power_cycle(&mut self, seed: u64);

    /// Sets the input held on `port` for the next frame.
    fn set_input(&mut self, port: usize, input: Self::Input);

    /// Runs until `until` is reached or a watch fires.
    fn run(&mut self, until: Stop) -> StopReason;

    /// Reads a memory region.
    fn region(&self, id: RegionId) -> &[u8];

    /// Writes a memory region between steps.
    fn region_mut(&mut self, id: RegionId) -> &mut [u8];

    /// Reads the CPU bus without side effects.
    fn peek_bus(&self, address: u32) -> Peek;

    /// The most recently rendered frame.
    fn video(&self) -> VideoFrame<'_>;

    /// Audio produced since the last call.
    fn take_audio(&mut self) -> AudioChunk<'_>;

    /// Serializes the complete machine state (FND-03).
    fn save_state(&self) -> Vec<u8>;

    /// Loads a state produced by [`System::save_state`] onto this machine.
    ///
    /// # Errors
    ///
    /// Returns a [`StateError`] when the data is malformed, from an unsupported version, or
    /// saved from a different ROM or accuracy profile.
    fn load_state(&mut self, bytes: &[u8]) -> Result<(), StateError>;

    /// BLAKE3 hash of the canonical state.
    fn state_hash(&self) -> StateHash;

    /// Runs one frame.
    fn run_frame(&mut self) -> StopReason {
        self.run(Stop::Frames(1))
    }
}
