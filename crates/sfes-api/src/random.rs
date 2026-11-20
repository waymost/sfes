// @source: vigna-splitmix
//! The seeded generator for power-on state that hardware leaves undefined (FND-02).

/// `SplitMix64`, written from Sebastiano Vigna's public-domain description in `splitmix.c`
/// (<https://prng.di.unimi.it/splitmix.c>, archived in
/// `provenance/archive/vigna-splitmix/splitmix.c`). It is the fixed-increment version of
/// Java 8's `SplittableRandom` (Steele, Lea and Flood, OOPSLA 2014).
///
/// Each output adds the odd constant `0x9E3779B97F4A7C15` to the 64-bit state, then mixes a
/// copy of the state with two xor-shift-multiply rounds and a final xor-shift. Any seed is
/// valid; seed 0 is the project's documented default (FND-02).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// The constant added to the state for each output.
    const INCREMENT: u64 = 0x9E37_79B9_7F4A_7C15;

    /// A generator whose state is `seed`.
    // @source: vigna-splitmix
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// The next 64-bit output.
    // @source: vigna-splitmix
    pub const fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(Self::INCREMENT);
        let mut mixed = self.state;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        mixed ^ (mixed >> 31)
    }

    /// Fills `bytes` with successive outputs, each written little-endian; the last output is
    /// cut short when `bytes` is not a multiple of 8 long.
    // @source: vigna-splitmix
    pub fn fill(&mut self, bytes: &mut [u8]) {
        for chunk in bytes.chunks_mut(8) {
            let output = self.next_u64().to_le_bytes();
            chunk.copy_from_slice(&output[..chunk.len()]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // The expected values come from a separate script that transcribes the archived
    // description's `next()`, independent of this implementation.
    // @source: vigna-splitmix
    #[test]
    fn outputs_follow_the_description() {
        let mut generator = SplitMix64::new(0);
        assert_eq!(generator.next_u64(), 0xE220_A839_7B1D_CDAF);
        assert_eq!(generator.next_u64(), 0x6E78_9E6A_A1B9_65F4);
        assert_eq!(generator.next_u64(), 0x06C4_5D18_8009_454F);
        let mut generator = SplitMix64::new(0x0123_4567_89AB_CDEF);
        assert_eq!(generator.next_u64(), 0x157A_3807_A48F_AA9D);
        assert_eq!(generator.next_u64(), 0xD573_529B_34A1_D093);
    }

    // @source: vigna-splitmix
    #[test]
    fn fill_writes_outputs_little_endian() {
        let mut bytes = [0; 11];
        SplitMix64::new(0).fill(&mut bytes);
        assert_eq!(
            bytes,
            [
                0xAF, 0xCD, 0x1D, 0x7B, 0x39, 0xA8, 0x20, 0xE2, 0xF4, 0x65, 0xB9
            ]
        );
    }
}
