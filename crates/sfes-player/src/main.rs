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
mod rom;
mod video;

use std::sync::Arc;

use anyhow::{Context, Result};
use sfes_api::{Fast, Stop};
use sfes_snes_core::Snes;

use crate::cli::Options;

fn main() -> Result<()> {
    let options = cli::parse();
    let rom = rom::load(&options.rom, options.entry.as_deref())?;
    let machine = build(&options, rom.bytes)?;
    if let Some(count) = options.frames {
        headless(machine, count);
        return Ok(());
    }
    app::run(machine, rom.name, options.aspect)
}

/// Loads the ROM image and powers the machine on.
fn build(options: &Options, image: Vec<u8>) -> Result<Snes<Fast>> {
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
