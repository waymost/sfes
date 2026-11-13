//! State hashing and state-loading errors (FND-03).

/// BLAKE3 hash of a machine's canonical state serialization.
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    Eq,
    Hash,
    Ord,
    PartialEq,
    PartialOrd,
    serde::Deserialize,
    serde::Serialize,
)]
pub struct StateHash(pub [u8; 32]);

impl StateHash {
    /// Hashes `bytes` with BLAKE3.
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        Self(*blake3::hash(bytes).as_bytes())
    }
}

/// Why a serialized state could not be loaded. Loading never panics (FND-09).
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum StateError {
    /// The data does not start with the state magic.
    #[error("not an sfes state: bad magic")]
    BadMagic,
    /// The data ended before a complete header or section.
    #[error("state data is truncated")]
    Truncated,
    /// A section's version is not one this build can load.
    #[error("section {section:?} has version {found}; this build supports up to {supported}")]
    Unsupported {
        /// Section tag.
        section: [u8; 4],
        /// Version found in the data.
        found: u16,
        /// Newest version this build supports.
        supported: u16,
    },
    /// A section tag this build does not know.
    #[error("unknown section {0:?}")]
    UnknownSection([u8; 4]),
    /// A required section is absent.
    #[error("missing section {0:?}")]
    MissingSection([u8; 4]),
    /// Bytes follow the last section.
    #[error("trailing bytes after the last section")]
    TrailingBytes,
    /// The state was saved from a different ROM.
    #[error("state was saved from a different ROM")]
    RomMismatch,
    /// The state was saved with a different accuracy profile.
    #[error("state was saved with accuracy profile {found}, not {expected}")]
    AccuracyMismatch {
        /// Profile of this machine.
        expected: &'static str,
        /// Profile name recorded in the data.
        found: alloc::string::String,
    },
    /// A section's contents could not be decoded.
    #[error("section {0:?} could not be decoded")]
    Decode([u8; 4]),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_matches_blake3_test_vector() {
        // Expected value: the BLAKE3 reference test vector for the empty input.
        let expected = [
            0xAF, 0x13, 0x49, 0xB9, 0xF5, 0xF9, 0xA1, 0xA6, 0xA0, 0x40, 0x4D, 0xEA, 0x36, 0xDC,
            0xC9, 0x49, 0x9B, 0xCB, 0x25, 0xC9, 0xAD, 0xC1, 0x12, 0xB7, 0xCC, 0x9A, 0x93, 0xCA,
            0xE4, 0x1F, 0x32, 0x62,
        ];
        assert_eq!(StateHash::of(&[]), StateHash(expected));
    }
}
