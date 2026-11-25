//! The command line: `sfes-player [options] <rom>`.

use std::path::PathBuf;

use clap::{Parser, ValueEnum};
use sfes_snes_core::Region;

use crate::layout::Aspect;

/// The keys, shown after the options in the help text.
const KEYS: &str = "\
Keys: arrows D-pad; Z B, X A, A Y, S X, Q L, W R; Enter Start; Backspace or right Shift Select
      Space or P pause; N or . advance one frame while paused; R reset; Esc quit";

/// How the picture's pixels are shaped, as the command line names them.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum AspectArgument {
    /// Square pixels.
    Square,
    /// The picture stretched 8/7 wider, for a TV-like view.
    #[value(name = "8:7")]
    Tv,
}

impl From<AspectArgument> for Aspect {
    fn from(argument: AspectArgument) -> Self {
        match argument {
            AspectArgument::Square => Self::Square,
            AspectArgument::Tv => Self::Tv,
        }
    }
}

/// A video standard, as the command line names it.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum RegionArgument {
    /// 60 Hz.
    Ntsc,
    /// 50 Hz.
    Pal,
}

impl From<RegionArgument> for Region {
    fn from(argument: RegionArgument) -> Self {
        match argument {
            RegionArgument::Ntsc => Self::Ntsc,
            RegionArgument::Pal => Self::Pal,
        }
    }
}

/// The development viewer for sfes (PLY-01).
#[derive(Debug, Parser)]
#[command(version, about, after_help = KEYS)]
struct Arguments {
    /// The ROM image to load.
    rom: PathBuf,
    /// Square pixels, or the picture stretched 8/7 for a TV-like view.
    #[arg(long, value_enum, default_value = "square")]
    aspect: AspectArgument,
    /// Run this video standard whatever the ROM header says.
    #[arg(long, value_enum)]
    region: Option<RegionArgument>,
    /// Run this many frames without a window, print the last frame's size, and exit.
    #[arg(long, value_name = "COUNT")]
    frames: Option<u32>,
}

/// The options of a run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Options {
    /// The ROM image to load.
    pub(crate) rom: PathBuf,
    /// A video standard that overrides the header's.
    pub(crate) region: Option<Region>,
    /// How the picture's pixels are shaped.
    pub(crate) aspect: Aspect,
    /// Run this many frames without a window.
    pub(crate) frames: Option<u32>,
}

impl From<Arguments> for Options {
    fn from(arguments: Arguments) -> Self {
        let Arguments {
            rom,
            aspect,
            region,
            frames,
        } = arguments;
        Self {
            rom,
            region: region.map(Region::from),
            aspect: Aspect::from(aspect),
            frames,
        }
    }
}

/// Reads the process's arguments. On `--help`, `--version` or a mistake, prints and exits.
pub(crate) fn parse() -> Options {
    Options::from(Arguments::parse())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_all(arguments: &[&str]) -> Result<Options, clap::Error> {
        Arguments::try_parse_from(["sfes-player"].into_iter().chain(arguments.iter().copied()))
            .map(Options::from)
    }

    #[test]
    fn a_rom_alone_runs_with_square_pixels_and_the_header_region() {
        assert_eq!(
            parse_all(&["game.sfc"]).ok(),
            Some(Options {
                rom: PathBuf::from("game.sfc"),
                region: None,
                aspect: Aspect::Square,
                frames: None,
            })
        );
    }

    #[test]
    fn every_option_is_read_in_any_order() {
        assert_eq!(
            parse_all(&[
                "--frames", "3", "game.sfc", "--aspect", "8:7", "--region", "pal"
            ])
            .ok(),
            Some(Options {
                rom: PathBuf::from("game.sfc"),
                region: Some(Region::Pal),
                aspect: Aspect::Tv,
                frames: Some(3),
            })
        );
    }

    #[test]
    fn mistakes_are_errors() {
        assert!(parse_all(&[]).is_err());
        assert!(parse_all(&["a.sfc", "b.sfc"]).is_err());
        assert!(parse_all(&["--bogus", "a.sfc"]).is_err());
        assert!(parse_all(&["a.sfc", "--aspect"]).is_err());
        assert!(parse_all(&["a.sfc", "--aspect", "wide"]).is_err());
        assert!(parse_all(&["a.sfc", "--region", "secam"]).is_err());
        assert!(parse_all(&["a.sfc", "--frames", "-1"]).is_err());
    }
}
