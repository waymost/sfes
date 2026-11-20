// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: anomie-timing#s-ppu/blanking-periods
// @source: anomie-timing#s-ppu/registers
// @source: anomie-regs#2133
// @source: fullsnes#snestiminghvcounters
// @source: fullsnes#snestiminghvevents
// @source: adr-0001
// @source: adr-0018
// @source: adr-0019
// @source: adr-0020
//! The scheduler: line and frame position, the interlace field, vertical blank, the refresh
//! schedule and the deadline table (technical design section 4; ADR 0019, point 6;
//! ADR 0020).

mod deadlines;
mod position;

use sfes_api::Cycles;

use crate::{
    Region,
    timing::{
        deadlines::{Deadline, Deadlines},
        position::LineKind,
    },
};

/// Master cycles the CPU is paused for each refresh.
pub(crate) const REFRESH_LENGTH: u64 = 40;
/// Where the first refresh after power-on or reset falls in its line.
const FIRST_REFRESH: u64 = 538;
/// The offset in the line each later refresh comes closest to.
const REFRESH_TARGET: u64 = 536;
/// Where H=1, when the field toggles, falls in line 0.
const FIELD_TOGGLE: u64 = 4;
/// The first line that can start vertical blank: 225 ($E1).
const FIRST_VBLANK_LINE: u16 = 225;
/// The line that starts vertical blank with overscan: 240 ($F0).
const OVERSCAN_VBLANK_LINE: u16 = 240;
/// `$2133` SETINI bit 0: screen interlace.
const SCREEN_INTERLACE: u8 = 0x01;
/// `$2133` SETINI bit 2: overscan.
const OVERSCAN: u8 = 0x04;

/// What servicing a deadline did.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Event {
    /// A refresh fell due: the caller pauses the CPU for [`REFRESH_LENGTH`] cycles.
    Refresh,
    /// A line started.
    Line {
        /// The line's number, its V position.
        line: u16,
        /// When it started.
        at: Cycles,
        /// Whether it started vertical blank, ending a frame.
        frame_ended: bool,
    },
}

/// The machine's place in its lines and frames, and the next time each scheduled event is
/// due.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Timing {
    /// The video standard, which sets the lines per frame.
    region: Region,
    /// `$2133` SETINI's interlace and overscan bits as last written (ADR 0020, point 6).
    screen_mode: u8,
    /// `$213F` bit 7 from H=1 of the current frame's line 0.
    field: bool,
    /// The line now running, its V position.
    line: u16,
    /// When the line now running started.
    line_start: Cycles,
    /// The length of the line now running.
    line_kind: LineKind,
    /// Whether vertical blank has begun in this frame.
    vblank: bool,
    /// Lines started since power-on; the target of `Stop::Scanlines`.
    lines: u64,
    /// Frames ended since power-on; the target of `Stop::Frames`.
    frames: u64,
    /// How far into its line the next refresh is due.
    refresh_offset: u64,
    /// When the next refresh and the next line are due.
    deadlines: Deadlines,
}

impl Timing {
    /// Line 0 of a frame starting at `start`, as at power-on.
    // @source: fullsnes#snestiminghvevents
    // @source: fullsnes#snesiomap
    // @source: adr-0018
    // @source: adr-0020
    pub(crate) const fn new(region: Region, start: Cycles) -> Self {
        let mut timing = Self {
            region,
            screen_mode: 0,
            field: false,
            line: 0,
            line_start: start,
            line_kind: LineKind::Normal,
            vblank: false,
            lines: 0,
            frames: 0,
            refresh_offset: FIRST_REFRESH,
            deadlines: Deadlines::new(start),
        };
        timing.restart(start);
        timing
    }

    /// Restarts the frame at `start`, as reset does: the SNES "returns to a known timing
    /// position on reset" (ADR 0018), H=0, V=0 with field 0 (ADR 0020, point 4), `$2133` is
    /// 0 and the refresh schedule starts over (ADR 0019). The counts of lines and frames
    /// are kept.
    // @source: anomie-timing
    // @source: fullsnes#snestiminghvevents
    // @source: fullsnes#snesiomap
    // @source: adr-0018
    // @source: adr-0019
    // @source: adr-0020
    pub(crate) const fn restart(&mut self, start: Cycles) {
        self.screen_mode = 0;
        // Field 0 at H=0; the toggle at H=1 of this line makes the frame field 1.
        self.field = true;
        self.line = 0;
        self.line_start = start;
        self.line_kind = LineKind::Normal;
        self.vblank = false;
        self.refresh_offset = FIRST_REFRESH;
        self.deadlines
            .set(Deadline::Refresh, Cycles(start.0 + FIRST_REFRESH));
        self.deadlines
            .set(Deadline::Line, Cycles(start.0 + LineKind::Normal.length()));
    }

    /// Takes bits 0 and 2 of a CPU write to `$2133` SETINI (ADR 0020, point 6).
    // @source: anomie-regs#2133
    // @source: adr-0020
    pub(crate) const fn write_screen_mode(&mut self, value: u8) {
        self.screen_mode = value & (SCREEN_INTERLACE | OVERSCAN);
    }

    /// Whether `$2133` has interlace on.
    // @source: anomie-regs#2133
    const fn interlace(&self) -> bool {
        self.screen_mode & SCREEN_INTERLACE != 0
    }

    /// Whether `$2133` has overscan on.
    // @source: anomie-regs#2133
    const fn overscan(&self) -> bool {
        self.screen_mode & OVERSCAN != 0
    }

    /// The earliest time a scheduled event is due: `next_deadline` in the technical
    /// design.
    // @source: adr-0001
    // @source: adr-0020
    pub(crate) const fn next_deadline(&self) -> Cycles {
        self.deadlines.next()
    }

    /// When the next refresh is due.
    // @source: adr-0019
    #[cfg(test)]
    pub(crate) const fn next_refresh(&self) -> Cycles {
        self.deadlines.due(Deadline::Refresh)
    }

    /// Lines started since power-on.
    // @source: adr-0020
    pub(crate) const fn lines(&self) -> u64 {
        self.lines
    }

    /// Frames ended since power-on.
    // @source: adr-0020
    pub(crate) const fn frames(&self) -> u64 {
        self.frames
    }

    /// The line and dot (V and H) at `now`, which is no earlier than the start of the line
    /// now running and before the next deadline (ADR 0020, point 3).
    // @source: anomie-regs#213c
    // @source: anomie-timing#s-ppu/rendering
    // @source: adr-0020
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "read by $2137 and $213C-$213D in #82 and by the PPU"
        )
    )]
    pub(crate) const fn position(&self, now: Cycles) -> (u16, u16) {
        let dot = self
            .line_kind
            .dot_at(now.0.saturating_sub(self.line_start.0));
        (self.line, dot)
    }

    /// `$213F` bit 7, the interlace field, at `now`: it toggles at H=1 of line 0
    /// (ADR 0020, point 4).
    // @source: anomie-timing#s-ppu/registers
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0020
    #[cfg_attr(not(test), expect(dead_code, reason = "read by $213F with the PPU"))]
    pub(crate) const fn field(&self, now: Cycles) -> bool {
        if self.line == 0 && now.0 < self.line_start.0 + FIELD_TOGGLE {
            !self.field
        } else {
            self.field
        }
    }

    /// Services the first deadline due at or before `now`, if any, and schedules its next
    /// occurrence (ADR 0020, point 9).
    // @source: adr-0001
    // @source: adr-0019
    // @source: adr-0020
    pub(crate) fn service(&mut self, now: Cycles) -> Option<Event> {
        match self.deadlines.first_due(now)? {
            Deadline::Refresh => {
                self.schedule_refresh();
                Some(Event::Refresh)
            }
            Deadline::Line => Some(self.start_line()),
        }
    }

    /// Schedules the refresh after the one now due, in the following line (ADR 0019,
    /// point 6). The line now running is the one the due refresh fell in.
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: adr-0019
    // @source: adr-0020
    const fn schedule_refresh(&mut self) {
        let length = self.line_kind.length();
        self.refresh_offset = following_refresh(self.refresh_offset, length);
        let at = Cycles(self.line_start.0 + length + self.refresh_offset);
        self.deadlines.set(Deadline::Refresh, at);
    }

    /// Starts the line that is due: the next line, or line 0 after the frame's last line,
    /// with its length, the field toggle and the start of vertical blank (ADR 0020,
    /// points 1, 2, 4 and 5).
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: anomie-timing#s-ppu/blanking-periods
    // @source: anomie-regs#2133
    // @source: fullsnes#snestiminghvcounters
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0020
    fn start_line(&mut self) -> Event {
        let at = self.deadlines.due(Deadline::Line);
        self.line = if self.line >= self.last_line() {
            0
        } else {
            self.line + 1
        };
        self.line_start = at;
        if self.line == 0 {
            self.field = !self.field;
            self.vblank = false;
        }
        self.line_kind = self.kind_of(self.line);
        let frame_ended = !self.vblank
            && (FIRST_VBLANK_LINE..=OVERSCAN_VBLANK_LINE).contains(&self.line)
            && (self.line == OVERSCAN_VBLANK_LINE || !self.overscan());
        if frame_ended {
            self.vblank = true;
            self.frames += 1;
        }
        self.lines += 1;
        self.deadlines
            .set(Deadline::Line, Cycles(at.0 + self.line_kind.length()));
        Event::Line {
            line: self.line,
            at,
            frame_ended,
        }
    }

    /// The frame's last line: 261 on NTSC and 311 on PAL, one more with interlace on in
    /// field 0 (ADR 0020, point 2).
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: anomie-regs#213c
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0020
    const fn last_line(&self) -> u16 {
        let last = match self.region {
            Region::Ntsc => 261,
            Region::Pal => 311,
        };
        if self.interlace() && !self.field {
            last + 1
        } else {
            last
        }
    }

    /// The length of `line` when it starts now (ADR 0020, point 1).
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: fullsnes#snestiminghvcounters
    // @source: adr-0020
    const fn kind_of(&self, line: u16) -> LineKind {
        match self.region {
            Region::Ntsc if line == 240 && !self.interlace() && self.field => LineKind::Short,
            Region::Pal if line == 311 && self.interlace() && self.field => LineKind::Long,
            Region::Ntsc | Region::Pal => LineKind::Normal,
        }
    }
}

/// Where the refresh after one due `offset` cycles into a line of `line_length` cycles
/// falls in the next line: "some multiple of 8 cycles after the previous pause that comes
/// closest to 536" (anomie-timing). Every candidate is congruent to `offset - line_length`
/// modulo 8; of the two nearest 536, the closer wins, and the earlier on a tie, which
/// 1360-, 1364- and 1368-cycle lines never produce.
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
mod tests;
