// @source: anomie-timing#s-ppu/rendering
// @source: fullsnes#snestiminghvcounters
// @source: fullsnes#snestiminghvevents
// @source: adr-0020
//! Line lengths and the dot (H position) at a point in a line (ADR 0020, points 1 and 3).

/// The kinds of line, by length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LineKind {
    /// 1364 master cycles: 340 dots, of which 323 and 327 are 6 cycles.
    Normal,
    /// 1360 master cycles: 340 dots of 4 cycles. NTSC line 240, field 1, interlace off.
    Short,
    /// 1368 master cycles: 341 dots, of which 323 and 327 are 6 cycles. PAL line 311,
    /// field 1, interlace on.
    Long,
}

/// Master cycles in a dot.
const DOT_LENGTH: u64 = 4;
/// The first of the two long dots.
const FIRST_LONG_DOT: u64 = 323;
/// The second of the two long dots.
const SECOND_LONG_DOT: u64 = 327;
/// The extra master cycles each long dot takes.
const LONG_DOT_EXTRA: u64 = 2;

impl LineKind {
    /// The line's length in master cycles.
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: fullsnes#snestiminghvcounters
    // @source: adr-0020
    pub(crate) const fn length(self) -> u64 {
        match self {
            Self::Normal => 1364,
            Self::Short => 1360,
            Self::Long => 1368,
        }
    }

    /// The number of dots in the line: 341 on a long line, 340 otherwise.
    // @source: fullsnes#snestiminghvcounters
    // @source: fullsnes#snesppuinterrupts
    // @source: adr-0020
    // @source: adr-0021
    pub(crate) const fn dots(self) -> u16 {
        match self {
            Self::Normal | Self::Short => 340,
            Self::Long => 341,
        }
    }

    /// The dot running `offset` master cycles into the line. An offset past the line's end
    /// gives the line's last dot.
    // @source: anomie-timing#s-ppu/rendering
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0020
    pub(crate) const fn dot_at(self, offset: u64) -> u16 {
        let last_offset = self.length() - 1;
        let offset = if offset > last_offset {
            last_offset
        } else {
            offset
        };
        let dot = match self {
            Self::Short => offset / DOT_LENGTH,
            Self::Normal | Self::Long => {
                let first_long = FIRST_LONG_DOT * DOT_LENGTH;
                let second_long = SECOND_LONG_DOT * DOT_LENGTH + LONG_DOT_EXTRA;
                if offset < first_long {
                    offset / DOT_LENGTH
                } else if offset < first_long + DOT_LENGTH + LONG_DOT_EXTRA {
                    FIRST_LONG_DOT
                } else if offset < second_long {
                    (offset - LONG_DOT_EXTRA) / DOT_LENGTH
                } else if offset < second_long + DOT_LENGTH + LONG_DOT_EXTRA {
                    SECOND_LONG_DOT
                } else {
                    (offset - 2 * LONG_DOT_EXTRA) / DOT_LENGTH
                }
            }
        };
        // The longest line has 341 dots, so the dot fits in u16.
        let [low, high, ..] = dot.to_le_bytes();
        u16::from_le_bytes([low, high])
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    /// How many master cycles of the line each dot covers.
    // @source: adr-0020
    fn dot_lengths(kind: LineKind) -> Vec<u64> {
        let mut lengths = Vec::new();
        for offset in 0..kind.length() {
            let dot = usize::from(kind.dot_at(offset));
            if dot == lengths.len() {
                lengths.push(0);
            }
            assert_eq!(dot + 1, lengths.len(), "dots run in order at {offset}");
            lengths[dot] += 1;
        }
        lengths
    }

    // @source: anomie-timing#s-ppu/rendering
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0020
    #[test]
    fn a_normal_line_has_340_dots_with_323_and_327_long() {
        let lengths = dot_lengths(LineKind::Normal);
        assert_eq!(lengths.len(), 340);
        for (dot, length) in lengths.iter().enumerate() {
            let expected = if dot == 323 || dot == 327 { 6 } else { 4 };
            assert_eq!(*length, expected, "dot {dot}");
        }
        assert_eq!(LineKind::Normal.dot_at(1291), 322);
        assert_eq!(LineKind::Normal.dot_at(1292), 323);
        assert_eq!(LineKind::Normal.dot_at(1298), 324);
        assert_eq!(LineKind::Normal.dot_at(1310), 327);
        assert_eq!(LineKind::Normal.dot_at(1316), 328);
        assert_eq!(LineKind::Normal.dot_at(1363), 339);
    }

    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: fullsnes#snestiminghvcounters
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0020
    #[test]
    fn a_short_line_has_340_dots_of_4_cycles() {
        let lengths = dot_lengths(LineKind::Short);
        assert_eq!(lengths.len(), 340);
        assert!(lengths.iter().all(|length| *length == 4));
        assert_eq!(LineKind::Short.dot_at(1292), 323);
        assert_eq!(LineKind::Short.dot_at(1359), 339);
    }

    // @source: fullsnes#snestiminghvcounters
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0020
    #[test]
    fn a_long_line_adds_dot_340() {
        let lengths = dot_lengths(LineKind::Long);
        assert_eq!(lengths.len(), 341);
        assert_eq!(lengths[..340], dot_lengths(LineKind::Normal)[..]);
        assert_eq!(lengths[340], 4);
        assert_eq!(LineKind::Long.dot_at(1364), 340);
        assert_eq!(LineKind::Long.dot_at(1367), 340);
    }

    // @source: anomie-regs#213c
    // @source: fullsnes#snestiminghvcounters
    // @source: fullsnes#snesppuinterrupts
    // @source: adr-0021
    #[test]
    fn a_long_line_has_one_more_dot() {
        assert_eq!(LineKind::Normal.dots(), 340);
        assert_eq!(LineKind::Short.dots(), 340);
        assert_eq!(LineKind::Long.dots(), 341);
    }

    // @source: adr-0020
    #[test]
    fn an_offset_past_the_end_is_the_last_dot() {
        assert_eq!(LineKind::Normal.dot_at(5000), 339);
        assert_eq!(LineKind::Short.dot_at(1360), 339);
        assert_eq!(LineKind::Long.dot_at(1368), 340);
    }
}
