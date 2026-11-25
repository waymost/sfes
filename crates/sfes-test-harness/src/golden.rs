//! Golden frame hashes for test ROMs and own test cartridges (QA-01 layer 3, ADR 0015).
//!
//! Each golden is a TOML file in `tests/assets/golden/`:
//!
//! ```toml
//! rom = "fetched/krom/RotZoom.sfc"   # relative to tests/assets/
//! rom_sha256 = "…"
//!
//! [[input]]                          # optional; held from `frame` until the port's next entry
//! frame = 30
//! port = 0
//! buttons = "START"                  # sfes_api::Buttons flag names, "A | B"; "" releases all
//!
//! [[checkpoint]]
//! frame = 60
//! width = 256
//! height = 224
//! blake3 = "…"                       # BLAKE3 of the pixels, each u16 little-endian
//! status = "unverified"              # or "oracle-confirmed"
//! oracle = "MesenCE <version>"       # with `blessed`, absent until a person blesses the entry
//! blessed = "2026-11-18"
//! ```
//!
//! Frame *n* is the *n*-th frame rendered after power-on with seed 0; input entries for frame
//! *n* are applied before it runs. A [`FrameMachine`] is driven through the input script by
//! [`observe`] and [`capture`]; [`mismatches`] compares the result with the checkpoints and
//! [`update`] rewrites the ones that changed, resetting them to unverified.

use std::{
    fmt, fs, io,
    path::{Path, PathBuf},
};

pub use sfes_api::Rgb555;
use sfes_api::{Buttons, Fast, Stop, VideoFrame};
use sfes_snes_core::{BuildError, Snes};
use sha2::{Digest, Sha256};

use crate::roms::{self, Rom, RomError};

/// The golden directory, relative to this crate's manifest.
pub const DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/assets/golden");

/// One golden file.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Golden {
    /// The ROM, relative to `tests/assets/`.
    pub rom: String,
    /// The ROM's SHA-256, 64 lower-case hex digits.
    pub rom_sha256: String,
    /// The input script, in frame order.
    #[serde(default, rename = "input", skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<Input>,
    /// The checkpoints, in frame order.
    #[serde(rename = "checkpoint")]
    pub checkpoints: Vec<Checkpoint>,
}

/// Buttons held on one port from one frame until the port's next entry.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    /// The first frame the buttons are held for, counted from 1.
    pub frame: u32,
    /// The controller port.
    pub port: u8,
    /// The buttons held.
    pub buttons: Buttons,
}

/// The expected frame at one point of the run.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    /// The frame number, counted from 1.
    pub frame: u32,
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// BLAKE3 of the pixels, 64 lower-case hex digits.
    pub blake3: String,
    /// How far the entry has been checked.
    pub status: Status,
    /// The oracle the frame was compared with, and its version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub oracle: Option<String>,
    /// The date the frame was blessed, `YYYY-MM-DD`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blessed: Option<String>,
}

/// How far a checkpoint has been checked (QA-01).
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Not yet blessed, or blessed by hand against an oracle's output.
    Unverified,
    /// Confirmed by the differential harness (M8).
    OracleConfirmed,
}

/// A frame's size and hash.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FrameHash {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// BLAKE3 of the pixels, 64 lower-case hex digits.
    pub blake3: String,
}

impl fmt::Display for FrameHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            width,
            height,
            blake3,
        } = self;
        write!(formatter, "{width}x{height} {blake3}")
    }
}

/// A frame copied out of the machine.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    /// Width in pixels.
    pub width: u16,
    /// Height in pixels.
    pub height: u16,
    /// The pixels, row-major.
    pub pixels: Vec<Rgb555>,
}

/// A checkpoint whose frame differs from the golden.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Mismatch {
    /// The frame number.
    pub frame: u32,
    /// The golden's size and hash.
    pub expected: FrameHash,
    /// The frame the machine rendered.
    pub found: FrameHash,
}

impl fmt::Display for Mismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self {
            frame,
            expected,
            found,
        } = self;
        write!(
            formatter,
            "frame {frame}: expected {expected}, found {found}"
        )
    }
}

/// A machine the runner can drive one frame at a time.
pub trait FrameMachine {
    /// Holds `buttons` on `port` from the next frame on.
    fn set_buttons(&mut self, port: u8, buttons: Buttons);

    /// Runs one frame and returns it.
    fn run_frame(&mut self) -> VideoFrame<'_>;
}

/// A golden file or its ROM could not be used.
#[derive(Debug, thiserror::Error)]
pub enum GoldenError {
    /// The file could not be read.
    #[error("reading {path}: {source}")]
    Read {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        source: io::Error,
    },
    /// The file is not valid TOML in the golden format.
    #[error("parsing {path}: {source}")]
    Parse {
        /// The file.
        path: PathBuf,
        /// The underlying error.
        source: toml::de::Error,
    },
    /// The file parses but breaks a rule of the format.
    #[error("{path}: {problem}")]
    Invalid {
        /// The file.
        path: PathBuf,
        /// What is wrong.
        problem: String,
    },
    /// The ROM could not be loaded.
    #[error(transparent)]
    Rom(#[from] RomError),
    /// The ROM is not the one the golden was made from.
    #[error("{rom} has SHA-256 {found}, the golden says {expected}")]
    RomHash {
        /// The ROM, relative to `tests/assets/`.
        rom: String,
        /// The golden's `rom_sha256`.
        expected: String,
        /// The ROM's hash.
        found: String,
    },
    /// The machine could not be built from the ROM.
    #[error(transparent)]
    Build(#[from] BuildError),
}

impl Golden {
    /// Parses and validates a golden from TOML `text`; `path` names it in errors.
    ///
    /// # Errors
    ///
    /// Returns [`GoldenError::Parse`] for text that is not in the golden format and
    /// [`GoldenError::Invalid`] for the first rule it breaks.
    pub fn from_toml(path: &Path, text: &str) -> Result<Self, GoldenError> {
        let golden = toml::from_str::<Self>(text).map_err(|source| GoldenError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
        match golden.problems().into_iter().next() {
            Some(problem) => Err(GoldenError::Invalid {
                path: path.to_path_buf(),
                problem,
            }),
            None => Ok(golden),
        }
    }

    /// The golden as TOML, in the order [`Golden::from_toml`] reads it.
    ///
    /// # Errors
    ///
    /// Returns the serializer's error; with these types it does not fail in practice.
    pub fn to_toml(&self) -> Result<String, toml::ser::Error> {
        toml::to_string(self)
    }

    /// Every rule of the format this golden breaks.
    // @source: adr-0015
    #[must_use]
    pub fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if !is_lower_hex(&self.rom_sha256) {
            problems.push(String::from("rom_sha256 must be 64 lower-case hex digits"));
        }
        if self.checkpoints.is_empty() {
            problems.push(String::from("a golden needs at least one checkpoint"));
        }
        if self.inputs.iter().any(|input| input.frame == 0)
            || self
                .checkpoints
                .iter()
                .any(|checkpoint| checkpoint.frame == 0)
        {
            problems.push(String::from("frames are counted from 1"));
        }
        let inputs_ordered = self
            .inputs
            .windows(2)
            .all(|pair| (pair[0].frame, pair[0].port) < (pair[1].frame, pair[1].port));
        if !inputs_ordered {
            problems.push(String::from(
                "inputs must be in frame order, at most one per port and frame",
            ));
        }
        let checkpoints_ordered = self
            .checkpoints
            .windows(2)
            .all(|pair| pair[0].frame < pair[1].frame);
        if !checkpoints_ordered {
            problems.push(String::from(
                "checkpoints must be in increasing frame order",
            ));
        }
        for checkpoint in &self.checkpoints {
            problems.extend(
                checkpoint
                    .problems()
                    .into_iter()
                    .map(|problem| format!("checkpoint at frame {}: {problem}", checkpoint.frame)),
            );
        }
        problems
    }

    /// The last checkpoint's frame, or 0 when there are none.
    #[must_use]
    pub fn last_frame(&self) -> u32 {
        self.checkpoints
            .last()
            .map_or(0, |checkpoint| checkpoint.frame)
    }
}

impl Checkpoint {
    /// Every rule of the format this checkpoint breaks.
    // @source: adr-0015
    fn problems(&self) -> Vec<String> {
        let mut problems = Vec::new();
        if !is_lower_hex(&self.blake3) {
            problems.push(String::from("blake3 must be 64 lower-case hex digits"));
        }
        if self.oracle.is_some() != self.blessed.is_some() {
            problems.push(String::from(
                "oracle and blessed come together or not at all",
            ));
        }
        if self.status == Status::OracleConfirmed && self.oracle.is_none() {
            problems.push(String::from("an oracle-confirmed entry names its oracle"));
        }
        if self
            .oracle
            .as_deref()
            .is_some_and(|oracle| oracle.trim().is_empty())
        {
            problems.push(String::from("oracle names the oracle and its version"));
        }
        if self.blessed.as_deref().is_some_and(|date| !is_date(date)) {
            problems.push(String::from("blessed is a date, YYYY-MM-DD"));
        }
        problems
    }

    /// This checkpoint's size and hash.
    #[must_use]
    pub fn frame_hash(&self) -> FrameHash {
        FrameHash {
            width: self.width,
            height: self.height,
            blake3: self.blake3.clone(),
        }
    }
}

/// Reads and validates the golden at `path`.
///
/// # Errors
///
/// Returns a [`GoldenError`] when the file cannot be read, parsed or validated.
pub fn load(path: &Path) -> Result<Golden, GoldenError> {
    let text = fs::read_to_string(path).map_err(|source| GoldenError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    Golden::from_toml(path, &text)
}

/// The golden files in `directory`, sorted by path. A missing directory holds none.
///
/// # Errors
///
/// Returns the I/O error when the directory exists but cannot be listed.
pub fn files(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "toml")
        {
            paths.push(path);
        }
    }
    paths.sort();
    Ok(paths)
}

/// Loads the golden's ROM and checks its SHA-256. An absent ROM is [`Rom::Skipped`] unless
/// `SFES_REQUIRE_VECTORS=1` (ADR 0014).
///
/// # Errors
///
/// Returns a [`GoldenError`] when the ROM cannot be loaded or has another hash.
pub fn rom(golden: &Golden) -> Result<Rom, GoldenError> {
    let rom = roms::load(&golden.rom)?;
    if let Rom::Present(bytes) = &rom {
        let found = sha256_hex(bytes);
        if found != golden.rom_sha256 {
            return Err(GoldenError::RomHash {
                rom: golden.rom.clone(),
                expected: golden.rom_sha256.clone(),
                found,
            });
        }
    }
    Ok(rom)
}

/// Powers on a machine with `rom` and seed 0.
///
/// # Errors
///
/// Returns [`GoldenError::Build`] when the machine cannot be built from `rom`.
// @source: adr-0015
// @source: adr-0018
pub fn boot(rom: &[u8]) -> Result<Box<dyn FrameMachine>, GoldenError> {
    let machine = Snes::<Fast>::builder(std::sync::Arc::from(rom)).build()?;
    Ok(Box::new(machine))
}

impl FrameMachine for Snes {
    // @source: adr-0015
    fn set_buttons(&mut self, port: u8, buttons: Buttons) {
        self.set_input(usize::from(port), buttons);
    }

    // @source: adr-0015
    // @source: adr-0018
    fn run_frame(&mut self) -> VideoFrame<'_> {
        self.run(Stop::Frames(1));
        self.video()
    }
}

/// The size and BLAKE3 hash of `frame`: its pixels, each `u16` little-endian, row-major.
// @source: adr-0015
#[must_use]
pub fn frame_hash(frame: &VideoFrame<'_>) -> FrameHash {
    let mut hasher = blake3::Hasher::new();
    for pixel in frame.pixels {
        hasher.update(&pixel.to_le_bytes());
    }
    FrameHash {
        width: frame.width,
        height: frame.height,
        blake3: hasher.finalize().to_hex().to_string(),
    }
}

/// Runs frames 1 to `last`, applying `inputs` before each frame, and calls `visit` with each
/// frame's number and image.
// @source: adr-0015
pub fn play(
    machine: &mut dyn FrameMachine,
    inputs: &[Input],
    last: u32,
    mut visit: impl FnMut(u32, &VideoFrame<'_>),
) {
    let mut pending = inputs.iter().peekable();
    for frame in 1..=last {
        while let Some(input) = pending.next_if(|input| input.frame <= frame) {
            machine.set_buttons(input.port, input.buttons);
        }
        let image = machine.run_frame();
        visit(frame, &image);
    }
}

/// Runs the golden's input script on `machine` and returns the frame at each checkpoint, in
/// checkpoint order.
// @source: adr-0015
pub fn observe(golden: &Golden, machine: &mut dyn FrameMachine) -> Vec<FrameHash> {
    let mut wanted = golden
        .checkpoints
        .iter()
        .map(|checkpoint| checkpoint.frame)
        .peekable();
    let mut observed = Vec::with_capacity(golden.checkpoints.len());
    play(
        machine,
        &golden.inputs,
        golden.last_frame(),
        |frame, image| {
            if wanted.next_if_eq(&frame).is_some() {
                observed.push(frame_hash(image));
            }
        },
    );
    observed
}

/// Runs the golden's input script on `machine` to `frame` and copies that frame out.
// @source: adr-0015
pub fn capture(golden: &Golden, machine: &mut dyn FrameMachine, frame: u32) -> Option<Frame> {
    let mut captured = None;
    play(machine, &golden.inputs, frame, |number, image| {
        if number == frame {
            captured = Some(Frame {
                width: image.width,
                height: image.height,
                pixels: image.pixels.to_vec(),
            });
        }
    });
    captured
}

/// The checkpoints whose frame differs from `observed`, which [`observe`] returned.
// @source: adr-0015
#[must_use]
pub fn mismatches(golden: &Golden, observed: &[FrameHash]) -> Vec<Mismatch> {
    golden
        .checkpoints
        .iter()
        .zip(observed)
        .filter_map(|(checkpoint, found)| {
            let expected = checkpoint.frame_hash();
            (expected != *found).then(|| Mismatch {
                frame: checkpoint.frame,
                expected,
                found: found.clone(),
            })
        })
        .collect::<Vec<_>>()
}

/// Writes `observed` into the checkpoints that differ, resetting each to unverified with no
/// oracle or blessing date, and returns how many changed.
// @source: adr-0015
pub fn update(golden: &mut Golden, observed: &[FrameHash]) -> usize {
    let mut changed = 0;
    for (checkpoint, found) in golden.checkpoints.iter_mut().zip(observed) {
        if checkpoint.frame_hash() == *found {
            continue;
        }
        let FrameHash {
            width,
            height,
            blake3,
        } = found.clone();
        checkpoint.width = width;
        checkpoint.height = height;
        checkpoint.blake3 = blake3;
        checkpoint.status = Status::Unverified;
        checkpoint.oracle = None;
        checkpoint.blessed = None;
        changed += 1;
    }
    changed
}

/// A new golden for the ROM `bytes` (named `rom`, relative to `tests/assets/`) with a checkpoint
/// at each of `frames`, taken from a run of `machine` through `inputs`. Every checkpoint is
/// unverified: a person blesses it after comparing the frame with the oracle's.
///
/// # Errors
///
/// Returns [`GoldenError::Invalid`] when `frames` or `inputs` break a rule of the format.
// @source: adr-0015
pub fn create(
    rom: &str,
    bytes: &[u8],
    inputs: Vec<Input>,
    frames: &[u32],
    machine: &mut dyn FrameMachine,
) -> Result<Golden, GoldenError> {
    let checkpoints = frames
        .iter()
        .map(|frame| Checkpoint {
            frame: *frame,
            width: 0,
            height: 0,
            blake3: "0".repeat(64),
            status: Status::Unverified,
            oracle: None,
            blessed: None,
        })
        .collect::<Vec<_>>();
    let mut golden = Golden {
        rom: String::from(rom),
        rom_sha256: sha256_hex(bytes),
        inputs,
        checkpoints,
    };
    if let Some(problem) = golden.problems().into_iter().next() {
        return Err(GoldenError::Invalid {
            path: PathBuf::from(rom),
            problem,
        });
    }
    let observed = observe(&golden, machine);
    update(&mut golden, &observed);
    Ok(golden)
}

/// The SHA-256 of `bytes` as lower-case hex.
fn sha256_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| {
            [
                DIGITS[usize::from(byte >> 4)],
                DIGITS[usize::from(byte & 0x0F)],
            ]
        })
        .map(char::from)
        .collect::<String>()
}

/// Whether `text` is 64 lower-case hex digits.
fn is_lower_hex(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}

/// Whether `text` has the form `YYYY-MM-DD`.
fn is_date(text: &str) -> bool {
    text.len() == 10
        && text.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A machine whose frame `n` is 4×2 pixels, each `n` plus the buttons held on port 0.
    struct Counter {
        frame: u16,
        buttons: Buttons,
        pixels: Vec<Rgb555>,
    }

    impl Counter {
        fn new() -> Self {
            Self {
                frame: 0,
                buttons: Buttons::empty(),
                pixels: vec![Rgb555::BLACK; 8],
            }
        }
    }

    impl FrameMachine for Counter {
        fn set_buttons(&mut self, port: u8, buttons: Buttons) {
            if port == 0 {
                self.buttons = buttons;
            }
        }

        fn run_frame(&mut self) -> VideoFrame<'_> {
            self.frame += 1;
            let value = self.frame.wrapping_add(self.buttons.bits());
            self.pixels.fill(pixel(value));
            VideoFrame {
                width: 4,
                height: 2,
                pixels: &self.pixels,
                field: None,
            }
        }
    }

    fn hex(digit: char) -> String {
        String::from(digit).repeat(64)
    }

    /// `bits` as a pixel.
    fn pixel(bits: u16) -> Rgb555 {
        Rgb555::new(bits).unwrap()
    }

    fn counter_hash(value: u16) -> FrameHash {
        let pixels = [pixel(value); 8];
        frame_hash(&VideoFrame {
            width: 4,
            height: 2,
            pixels: &pixels,
            field: None,
        })
    }

    fn text() -> String {
        format!(
            "rom = \"carts/halt.sfc\"\nrom_sha256 = \"{}\"\n\n\
             [[input]]\nframe = 2\nport = 0\nbuttons = \"START\"\n\n\
             [[input]]\nframe = 3\nport = 0\nbuttons = \"\"\n\n\
             [[checkpoint]]\nframe = 2\nwidth = 4\nheight = 2\nblake3 = \"{}\"\n\
             status = \"unverified\"\noracle = \"MesenCE 2.0.0\"\nblessed = \"2026-11-18\"\n\n\
             [[checkpoint]]\nframe = 4\nwidth = 4\nheight = 2\nblake3 = \"{}\"\n\
             status = \"unverified\"\n",
            hex('a'),
            counter_hash(2 + Buttons::START.bits()).blake3,
            counter_hash(4).blake3,
        )
    }

    fn golden() -> Golden {
        Golden::from_toml(Path::new("test.toml"), &text()).unwrap()
    }

    // @source: adr-0015
    #[test]
    fn a_golden_parses_and_writes_back_unchanged() {
        let golden = golden();
        assert_eq!(golden.inputs[0].buttons, Buttons::START);
        assert_eq!(golden.inputs[1].buttons, Buttons::empty());
        assert_eq!(
            golden.checkpoints[0].oracle.as_deref(),
            Some("MesenCE 2.0.0")
        );
        let written = golden.to_toml().unwrap();
        assert_eq!(
            Golden::from_toml(Path::new("again.toml"), &written).unwrap(),
            golden
        );
        assert!(written.contains("[[checkpoint]]"), "{written}");
        assert!(written.contains("buttons = \"START\""), "{written}");
    }

    // @source: adr-0015
    #[test]
    fn the_frame_hash_is_blake3_of_little_endian_pixels() {
        let pixels = [pixel(0x7FFF), pixel(0x0001)];
        let found = frame_hash(&VideoFrame {
            width: 2,
            height: 1,
            pixels: &pixels,
            field: None,
        });
        let expected = blake3::hash(&[0xFF, 0x7F, 0x01, 0x00]).to_hex().to_string();
        assert_eq!(
            found,
            FrameHash {
                width: 2,
                height: 1,
                blake3: expected
            }
        );
    }

    // @source: adr-0015
    #[test]
    fn the_runner_applies_inputs_and_matches_checkpoints() {
        let golden = golden();
        let observed = observe(&golden, &mut Counter::new());
        assert_eq!(observed.len(), 2);
        assert_eq!(mismatches(&golden, &observed), Vec::new());
    }

    // @source: adr-0015
    #[test]
    fn a_changed_frame_is_reported() {
        let golden = golden();
        let mut observed = observe(&golden, &mut Counter::new());
        observed[1] = counter_hash(99);
        let found = mismatches(&golden, &observed);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].frame, 4);
        assert!(
            found[0].to_string().starts_with("frame 4: expected 4x2 "),
            "{}",
            found[0]
        );
    }

    // @source: adr-0015
    #[test]
    fn update_rewrites_changed_checkpoints_as_unverified() {
        let mut golden = golden();
        golden.checkpoints[1].oracle = Some(String::from("MesenCE 2.0.0"));
        golden.checkpoints[1].blessed = Some(String::from("2026-11-18"));
        let mut observed = observe(&golden, &mut Counter::new());
        observed[1] = counter_hash(99);
        assert_eq!(update(&mut golden, &observed), 1);
        assert_eq!(
            golden.checkpoints[0].oracle.as_deref(),
            Some("MesenCE 2.0.0")
        );
        let changed = &golden.checkpoints[1];
        assert_eq!(changed.frame_hash(), counter_hash(99));
        assert_eq!(changed.status, Status::Unverified);
        assert_eq!(
            (changed.oracle.as_ref(), changed.blessed.as_ref()),
            (None, None)
        );
        assert_eq!(update(&mut golden, &observed), 0);
    }

    // @source: adr-0015
    #[test]
    fn create_takes_each_checkpoint_from_the_run() {
        let input = Input {
            frame: 2,
            port: 0,
            buttons: Buttons::START,
        };
        let made = create(
            "carts/halt.sfc",
            b"rom",
            vec![input],
            &[2, 4],
            &mut Counter::new(),
        )
        .unwrap();
        assert_eq!(made.rom_sha256, sha256_hex(b"rom"));
        assert_eq!(
            made.checkpoints[0].frame_hash(),
            counter_hash(2 + Buttons::START.bits())
        );
        // START is still held at frame 4: it is held until the port's next entry.
        assert_eq!(
            made.checkpoints[1].frame_hash(),
            counter_hash(4 + Buttons::START.bits())
        );
        assert!(made.checkpoints.iter().all(
            |checkpoint| checkpoint.status == Status::Unverified && checkpoint.oracle.is_none()
        ));
        assert_eq!(made.problems(), Vec::<String>::new());
        let unordered = create("a.sfc", b"", Vec::new(), &[4, 2], &mut Counter::new());
        assert!(matches!(unordered, Err(GoldenError::Invalid { .. })));
    }

    // @source: adr-0015
    #[test]
    fn capture_copies_the_requested_frame() {
        let golden = golden();
        let frame = capture(&golden, &mut Counter::new(), 2).unwrap();
        assert_eq!((frame.width, frame.height), (4, 2));
        assert_eq!(frame.pixels, vec![pixel(2 + Buttons::START.bits()); 8]);
    }

    // @source: adr-0015
    #[test]
    fn malformed_goldens_are_rejected() {
        let cases = [
            ("blake3 = \"", "blake3 = \"0"),
            ("frame = 4\n", "frame = 0\n"),
            ("frame = 3\nport", "frame = 1\nport"),
            ("2026-11-18", "18-11-2026"),
            ("blessed = \"2026-11-18\"\n", ""),
            ("rom_sha256 = \"a", "rom_sha256 = \"A"),
            (
                "status = \"unverified\"\noracle",
                "status = \"checked\"\noracle",
            ),
        ];
        for (from, to) in cases {
            let changed = text().replacen(from, to, 1);
            assert_ne!(changed, text(), "{from:?} not found");
            let error = Golden::from_toml(Path::new("bad.toml"), &changed).unwrap_err();
            assert!(
                matches!(
                    error,
                    GoldenError::Invalid { .. } | GoldenError::Parse { .. }
                ),
                "{error}"
            );
        }
    }

    // @source: adr-0015
    #[test]
    fn an_oracle_confirmed_entry_names_its_oracle() {
        let unverified = "status = \"unverified\"\n";
        let confirmed = "status = \"oracle-confirmed\"\n";
        let named = text().replacen(unverified, confirmed, 1);
        assert!(Golden::from_toml(Path::new("named.toml"), &named).is_ok());
        let last = text();
        let unnamed = format!("{}{confirmed}", last.strip_suffix(unverified).unwrap());
        let error = Golden::from_toml(Path::new("unnamed.toml"), &unnamed).unwrap_err();
        assert!(error.to_string().contains("names its oracle"), "{error}");
    }

    // @source: adr-0015
    // @source: adr-0016
    #[test]
    fn a_rom_without_a_header_boots_no_machine() {
        assert!(matches!(boot(&[]), Err(GoldenError::Build(_))));
    }

    // @source: adr-0015
    // @source: adr-0018
    // @source: adr-0028
    #[test]
    fn the_own_halt_cartridge_boots_and_runs_frames() {
        let rom = include_bytes!("../../../tests/assets/carts/halt.sfc");
        let mut machine = boot(rom).unwrap();
        machine.set_buttons(0, Buttons::START);
        let frame = machine.run_frame();
        // The first frame is a plain 256 x 224 picture (ADR 0028, point 8).
        assert_eq!(
            (frame.width, frame.height, frame.pixels.len()),
            (256, 224, 256 * 224)
        );
    }

    // @source: adr-0015
    #[test]
    fn committed_goldens_are_well_formed() {
        for path in files(Path::new(DIRECTORY)).unwrap() {
            load(&path).unwrap();
        }
    }
}
