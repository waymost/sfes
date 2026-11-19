//! Loading third-party test ROMs (ADR 0014).
//!
//! Committed ROMs live under `tests/assets/third-party/`; ROMs with no license that allows
//! committing them are downloaded by `cargo xtask roms fetch` into `tests/assets/fetched/`.
//! A test asks for a ROM by its path relative to `tests/assets/`, as the manifest names it.
//! When the file is absent the test is skipped with a message, unless
//! `SFES_REQUIRE_VECTORS=1` (which CI sets) makes that an error, as for the vector sets.

use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::vectors::{self, ConfigError};

/// The asset directory, relative to this crate's manifest.
const ASSETS: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/assets");

/// The outcome of asking for a ROM.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Rom {
    /// The ROM's bytes.
    Present(Vec<u8>),
    /// The ROM is absent and not required; the message says how to fetch it.
    Skipped(String),
}

/// A ROM could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum RomError {
    /// An environment variable holds a value that cannot be used.
    #[error(transparent)]
    Config(#[from] ConfigError),
    /// The ROM is absent and `SFES_REQUIRE_VECTORS=1`.
    #[error(
        "{path} is missing and {} is 1; run `cargo xtask roms fetch`",
        vectors::REQUIRE_VARIABLE
    )]
    Missing {
        /// The path asked for, relative to `tests/assets/`.
        path: String,
    },
    /// The file exists but could not be read.
    #[error("reading {path}: {source}")]
    Read {
        /// The path asked for, relative to `tests/assets/`.
        path: String,
        /// The underlying error.
        source: io::Error,
    },
}

/// Loads the ROM at `path`, relative to `tests/assets/` (`fetched/krom/RotZoom.sfc`).
///
/// # Errors
///
/// Returns a [`RomError`] when the file cannot be read, when it is absent and
/// `SFES_REQUIRE_VECTORS=1`, or when that variable holds an unusable value.
pub fn load(path: &str) -> Result<Rom, RomError> {
    let required = vectors::Config::from_env()?.required;
    load_from(&PathBuf::from(ASSETS), path, required)
}

/// Loads `path` from `assets`, treating an absent file as an error when `required`.
// @source: adr-0014
fn load_from(assets: &Path, path: &str, required: bool) -> Result<Rom, RomError> {
    match fs::read(assets.join(path)) {
        Ok(bytes) => Ok(Rom::Present(bytes)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            if required {
                Err(RomError::Missing {
                    path: String::from(path),
                })
            } else {
                Ok(Rom::Skipped(format!(
                    "skipped: {path} is not present; run `cargo xtask roms fetch`"
                )))
            }
        }
        Err(source) => Err(RomError::Read {
            path: String::from(path),
            source,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: adr-0014
    #[test]
    fn a_committed_rom_loads() {
        let assets = PathBuf::from(ASSETS);
        let rom = load_from(&assets, "third-party/gilyon/cputest-basic.sfc", true).unwrap();
        let Rom::Present(bytes) = rom else {
            panic!("expected the committed ROM, got {rom:?}");
        };
        assert_eq!(bytes.len(), 256 * 1024);
    }

    // @source: adr-0014
    #[test]
    fn an_absent_rom_is_skipped_unless_required() {
        let assets = PathBuf::from(ASSETS);
        let path = "fetched/absent/none.sfc";
        let skipped = load_from(&assets, path, false).unwrap();
        assert!(
            matches!(&skipped, Rom::Skipped(message) if message.contains("roms fetch")),
            "{skipped:?}"
        );
        let missing = load_from(&assets, path, true).unwrap_err();
        assert!(matches!(missing, RomError::Missing { .. }), "{missing}");
    }
}
