//! Reference player for sfes: the development viewer (PLY-01).
//!
//! `sfes-player <rom>` opens a window that runs the ROM and draws each frame at integer scale
//! (ADR 0039). It uses only the public API, so it sees what any other consumer sees
//! (technical design section 11).

mod app;
mod cli;
mod input;
mod layout;
mod pacing;
mod video;

use std::{ffi::OsStr, fs, sync::Arc};

use anyhow::{Context, Result};
use sfes_api::{Fast, Stop};
use sfes_snes_core::Snes;

use crate::cli::Options;

fn main() -> Result<()> {
    let options = cli::parse();
    let machine = build(&options)?;
    if let Some(count) = options.frames {
        headless(machine, count);
        return Ok(());
    }
    let name = options
        .rom
        .file_name()
        .and_then(OsStr::to_str)
        .unwrap_or("rom")
        .to_owned();
    app::run(machine, name, options.aspect)
}

/// Loads the ROM and powers the machine on.
fn build(options: &Options) -> Result<Snes<Fast>> {
    let image =
        fs::read(&options.rom).with_context(|| format!("reading {}", options.rom.display()))?;
    let mut builder = Snes::<Fast>::builder(Arc::from(image));
    if let Some(region) = options.region {
        builder = builder.region(region);
    }
    builder
        .build()
        .with_context(|| format!("loading {}", options.rom.display()))
}

/// Runs `count` frames with no window and prints the size of the last one.
fn headless(mut machine: Snes<Fast>, count: u32) {
    machine.run(Stop::Frames(count));
    let frame = machine.video();
    println!(
        "{count} frames, last frame {} x {}",
        frame.width, frame.height
    );
}
