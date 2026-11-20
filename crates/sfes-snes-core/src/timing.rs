// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: fullsnes#snestiminghvcounters
// @source: adr-0018
// @source: adr-0019
//! Line and frame boundaries and the refresh schedule. Lines and frames are provisional
//! until the scheduler (#81): every line is 1364 master cycles and a frame is 262 lines
//! (NTSC) or 312 (PAL), with no short or long lines (ADR 0018, point 6). Refresh follows
//! ADR 0019, point 6.

use sfes_api::Cycles;

use crate::Region;

/// Master cycles per scanline.
pub(crate) const LINE_LENGTH: u64 = 1364;
/// Master cycles the CPU is paused for each refresh.
pub(crate) const REFRESH_LENGTH: u64 = 40;
/// Where the first refresh after power-on or reset falls in its line.
const FIRST_REFRESH: u64 = 538;
/// The offset in the line each later refresh comes closest to.
const REFRESH_TARGET: u64 = 536;

/// Where the machine is in its frame.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Timing {
    /// Lines per frame for the region.
    lines_per_frame: u16,
    /// The line now running, 0 at the start of a frame.
    pub(crate) line: u16,
    /// When the next line starts.
    pub(crate) next_line: Cycles,
    /// Lines started since power-on; the target of `Stop::Scanlines`.
    pub(crate) lines: u64,
    /// The start of the line in which the next refresh is due.
    refresh_line: Cycles,
    /// How far into that line the next refresh is due.
    refresh_offset: u64,
}

/// What a line boundary was.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Boundary {
    /// A line inside the frame started.
    Line(u16),
    /// The frame ended and line 0 of the next one started.
    Frame,
}

impl Timing {
    /// Line 0 of a frame starting at `start`.
    // @source: fullsnes#snestimingoscillators
    // @source: anomie-timing
    // @source: adr-0018
    pub(crate) const fn new(region: Region, start: Cycles) -> Self {
        let lines_per_frame = match region {
            Region::Ntsc => 262,
            Region::Pal => 312,
        };
        Self {
            lines_per_frame,
            line: 0,
            next_line: Cycles(start.0 + LINE_LENGTH),
            lines: 0,
            refresh_line: start,
            refresh_offset: FIRST_REFRESH,
        }
    }

    /// Restarts the frame at `start`, as reset does: the SNES "returns to a known timing
    /// position on reset" (ADR 0018), and the refresh schedule starts over with it
    /// (ADR 0019). The count of lines run is kept.
    // @source: anomie-timing
    // @source: adr-0018
    // @source: adr-0019
    pub(crate) const fn restart(&mut self, start: Cycles) {
        self.line = 0;
        self.next_line = Cycles(start.0 + LINE_LENGTH);
        self.refresh_line = start;
        self.refresh_offset = FIRST_REFRESH;
    }

    /// When the next refresh is due.
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: adr-0019
    pub(crate) const fn next_refresh(&self) -> Cycles {
        Cycles(self.refresh_line.0 + self.refresh_offset)
    }

    /// Whether a refresh is due at or before `now`; if so, schedules the next one, in the
    /// following line (ADR 0019, point 6). The caller pauses the CPU.
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: adr-0019
    pub(crate) const fn take_refresh(&mut self, now: Cycles) -> bool {
        if now.0 < self.next_refresh().0 {
            return false;
        }
        self.refresh_offset = following_refresh(self.refresh_offset, LINE_LENGTH);
        self.refresh_line = Cycles(self.refresh_line.0 + LINE_LENGTH);
        true
    }

    /// The next boundary at or before `now`, if one is due, after moving past it.
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: adr-0018
    pub(crate) const fn next_due(&mut self, now: Cycles) -> Option<(Boundary, Cycles)> {
        if now.0 < self.next_line.0 {
            return None;
        }
        let at = self.next_line;
        self.next_line = Cycles(at.0 + LINE_LENGTH);
        self.lines += 1;
        self.line += 1;
        if self.line == self.lines_per_frame {
            self.line = 0;
            Some((Boundary::Frame, at))
        } else {
            Some((Boundary::Line(self.line), at))
        }
    }
}

/// Where the refresh after one due `offset` cycles into a line of `line_length` cycles
/// falls in the next line: "some multiple of 8 cycles after the previous pause that comes
/// closest to 536" (anomie-timing). Every candidate is congruent to `offset - line_length`
/// modulo 8; of the two nearest 536, the closer wins, and the earlier on a tie, which
/// 1364- and 1360-cycle lines never produce.
// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: adr-0019
const fn following_refresh(offset: u64, line_length: u64) -> u64 {
    let residue = (offset % 8 + 8 - line_length % 8) % 8;
    let above = REFRESH_TARGET - REFRESH_TARGET % 8 + residue;
    let below = above - 8;
    if above - REFRESH_TARGET < REFRESH_TARGET - below {
        above
    } else {
        below
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    // @source: fullsnes#snestimingoscillators
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    #[test]
    fn a_frame_is_262_or_312_lines_of_1364_cycles() {
        for (region, lines, frame_length) in
            [(Region::Ntsc, 262, 357_368), (Region::Pal, 312, 425_568)]
        {
            let mut timing = Timing::new(region, Cycles(0));
            let mut boundaries = 0;
            let mut now = 0;
            let frame_at = loop {
                now += 6;
                if let Some((boundary, at)) = timing.next_due(Cycles(now)) {
                    boundaries += 1;
                    if boundary == Boundary::Frame {
                        break at;
                    }
                }
            };
            assert_eq!(boundaries, lines);
            assert_eq!(frame_at, Cycles(frame_length));
            assert_eq!(timing.line, 0);
        }
    }

    // @source: anomie-timing
    // @source: adr-0018
    #[test]
    fn restart_returns_to_line_0() {
        let mut timing = Timing::new(Region::Ntsc, Cycles(0));
        assert!(timing.next_due(Cycles(1364)).is_some());
        timing.restart(Cycles(2000));
        assert_eq!(timing.line, 0);
        assert_eq!(timing.next_due(Cycles(3363)), None);
        assert_eq!(
            timing.next_due(Cycles(3364)),
            Some((Boundary::Line(1), Cycles(3364)))
        );
        assert_eq!(timing.lines, 2);
    }

    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: adr-0019
    #[test]
    fn refreshes_alternate_between_538_and_534_into_the_line() {
        let mut timing = Timing::new(Region::Ntsc, Cycles(0));
        let mut due = Vec::new();
        for _ in 0..4 {
            let at = timing.next_refresh();
            assert!(!timing.take_refresh(Cycles(at.0 - 1)));
            assert!(timing.take_refresh(at));
            due.push(at.0);
        }
        assert_eq!(due, [538, 1364 + 534, 2 * 1364 + 538, 3 * 1364 + 534]);
        // Each is a multiple of 8 cycles after the one before.
        for pair in due.windows(2) {
            assert_eq!((pair[1] - pair[0]) % 8, 0);
        }
    }

    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: adr-0019
    #[test]
    fn a_1360_cycle_line_keeps_the_refresh_offset() {
        assert_eq!(following_refresh(538, 1360), 538);
        assert_eq!(following_refresh(534, 1360), 534);
        assert_eq!(following_refresh(538, 1364), 534);
        assert_eq!(following_refresh(534, 1364), 538);
    }

    // @source: anomie-timing
    // @source: adr-0019
    #[test]
    fn restart_restarts_the_refresh_schedule() {
        let mut timing = Timing::new(Region::Ntsc, Cycles(0));
        assert!(timing.take_refresh(Cycles(600)));
        timing.restart(Cycles(1000));
        assert_eq!(timing.next_refresh(), Cycles(1538));
    }
}
