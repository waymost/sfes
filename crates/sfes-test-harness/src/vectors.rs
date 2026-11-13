//! Where the fetched vector sets are, and how much of them a run covers.
//!
//! Three environment variables control a run:
//!
//! - `SFES_VECTORS_DIR`: the directory `cargo xtask vectors fetch` filled. Default:
//!   `tests/assets/vectors/` in the workspace.
//! - `SFES_REQUIRE_VECTORS=1`: a missing set is an error. Otherwise it is skipped with a
//!   message. CI sets it.
//! - `SFES_VECTOR_SAMPLE=<n>`: run only the first `n` tests of each file.

use std::{env, ffi::OsString, num::NonZeroUsize, path::PathBuf};

/// The variable naming the directory that holds the fetched sets.
pub const DIRECTORY_VARIABLE: &str = "SFES_VECTORS_DIR";
/// The variable that turns a missing set into an error.
pub const REQUIRE_VARIABLE: &str = "SFES_REQUIRE_VECTORS";
/// The variable that limits each file to its first tests.
pub const SAMPLE_VARIABLE: &str = "SFES_VECTOR_SAMPLE";

/// The default directory, relative to this crate's manifest.
const DEFAULT_DIRECTORY: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/assets/vectors");

/// The settings of one vector run.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Config {
    /// The directory holding one subdirectory per vector set.
    pub directory: PathBuf,
    /// Whether a missing set is an error and not a skip.
    pub required: bool,
    /// How many tests of each file to run; `None` runs them all.
    pub sample: Option<NonZeroUsize>,
}

/// A vector-run variable holds a value that cannot be used.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ConfigError {
    /// `SFES_REQUIRE_VECTORS` is neither `0` nor `1`.
    #[error("{REQUIRE_VARIABLE} must be 0 or 1, not {0:?}")]
    Require(String),
    /// `SFES_VECTOR_SAMPLE` is not a positive number.
    #[error("{SAMPLE_VARIABLE} must be a positive number of tests, not {0:?}")]
    Sample(String),
}

impl Config {
    /// Reads the settings from the environment.
    ///
    /// # Errors
    ///
    /// Returns a [`ConfigError`] when a variable is set to a value that cannot be used.
    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_values(
            env::var_os(DIRECTORY_VARIABLE),
            env::var_os(REQUIRE_VARIABLE),
            env::var_os(SAMPLE_VARIABLE),
        )
    }

    /// Builds the settings from the three variables' values. An unset or empty variable
    /// takes its default.
    fn from_values(
        directory: Option<OsString>,
        require: Option<OsString>,
        sample: Option<OsString>,
    ) -> Result<Self, ConfigError> {
        let directory = directory
            .filter(|value| !value.is_empty())
            .map_or_else(|| PathBuf::from(DEFAULT_DIRECTORY), PathBuf::from);
        let required = match text(require) {
            None => false,
            Some(value) => match value.as_str() {
                "0" => false,
                "1" => true,
                _ => return Err(ConfigError::Require(value)),
            },
        };
        let sample = match text(sample) {
            None => None,
            Some(value) => match value.parse::<NonZeroUsize>() {
                Ok(count) => Some(count),
                Err(_) => return Err(ConfigError::Sample(value)),
            },
        };
        Ok(Self {
            directory,
            required,
            sample,
        })
    }
}

/// A variable's value as text, or `None` when it is unset or empty.
fn text(value: Option<OsString>) -> Option<String> {
    value
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The settings for the given `SFES_REQUIRE_VECTORS` and `SFES_VECTOR_SAMPLE` values.
    // @source: adr-0004
    fn config(require: Option<&str>, sample: Option<&str>) -> Result<Config, ConfigError> {
        Config::from_values(
            None,
            require.map(OsString::from),
            sample.map(OsString::from),
        )
    }

    // @source: adr-0004
    #[test]
    fn unset_variables_take_the_defaults() {
        let config = config(None, None).unwrap();
        assert!(config.directory.ends_with("tests/assets/vectors"));
        assert!(!config.required);
        assert_eq!(config.sample, None);
    }

    // @source: adr-0004
    #[test]
    fn empty_variables_take_the_defaults() {
        let empty = Some(OsString::new());
        let config = Config::from_values(empty.clone(), empty.clone(), empty).unwrap();
        assert_eq!(config, config_defaults());
    }

    /// The settings with every variable unset.
    // @source: adr-0004
    fn config_defaults() -> Config {
        config(None, None).unwrap()
    }

    // @source: adr-0004
    #[test]
    fn the_directory_variable_replaces_the_default() {
        let config = Config::from_values(Some(OsString::from("/somewhere")), None, None).unwrap();
        assert_eq!(config.directory, PathBuf::from("/somewhere"));
    }

    // @source: adr-0004
    #[test]
    fn require_accepts_only_zero_and_one() {
        assert!(config(Some("1"), None).unwrap().required);
        assert!(!config(Some("0"), None).unwrap().required);
        assert_eq!(
            config(Some("yes"), None),
            Err(ConfigError::Require("yes".to_owned()))
        );
    }

    // @source: adr-0004
    #[test]
    fn sample_accepts_only_a_positive_count() {
        assert_eq!(
            config(None, Some("500")).unwrap().sample,
            NonZeroUsize::new(500)
        );
        assert_eq!(
            config(None, Some("0")),
            Err(ConfigError::Sample("0".to_owned()))
        );
        assert_eq!(
            config(None, Some("many")),
            Err(ConfigError::Sample("many".to_owned()))
        );
    }
}
