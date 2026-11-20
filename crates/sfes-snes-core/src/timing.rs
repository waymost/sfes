// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: fullsnes#snestiminghvcounters
// @source: adr-0018
//! Line and frame boundaries, provisional until the scheduler (#81): every line is 1364
//! master cycles and a frame is 262 lines (NTSC) or 312 (PAL), with no short or long lines
//! (ADR 0018, point 6).

use sfes_api::Cycles;

use crate::Region;

/// Master cycles per scanline.
pub(crate) const LINE_LENGTH: u64 = 1364;

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
        }
    }

    /// Restarts the frame at `start`, as reset does: the SNES "returns to a known timing
    /// position on reset" (ADR 0018). The count of lines run is kept.
    // @source: anomie-timing
    // @source: adr-0018
    pub(crate) const fn restart(&mut self, start: Cycles) {
        self.line = 0;
        self.next_line = Cycles(start.0 + LINE_LENGTH);
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

#[cfg(test)]
mod tests {
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
}
