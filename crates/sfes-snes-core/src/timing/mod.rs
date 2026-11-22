// @source: anomie-regs#2133
// @source: anomie-regs#dma-and-hdma/hdma
// @source: anomie-timing#s-cpu/auto-joypad-read
// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: anomie-timing#s-cpu/hdma
// @source: anomie-timing#s-cpu/interrupts
// @source: anomie-timing#s-cpu/registers
// @source: anomie-timing#s-ppu/blanking-periods
// @source: anomie-timing#s-ppu/registers
// @source: fullsnes#snestiminghvcounters
// @source: fullsnes#snestiminghvevents
// @source: adr-0001
// @source: adr-0018
// @source: adr-0019
// @source: adr-0020
// @source: adr-0021
// @source: adr-0024
// @source: adr-0026
//! The scheduler: line and frame position, the interlace field, vertical blank, the refresh
//! schedule, the NMI and H/V timer trigger points, the counter latch of `$4201`, the
//! auto-joypad read, HDMA's steps and the deadline table (technical design section 4;
//! ADR 0019, point 6; ADR 0020; ADR 0021; ADR 0024; ADR 0026, point 2).

mod deadlines;
mod irq;
mod position;

use sfes_api::Cycles;

pub(crate) use crate::timing::irq::TimerRegister;
use crate::{
    Region,
    timing::{
        deadlines::{Deadline, Deadlines, NEVER},
        irq::{TimedLine, Timer},
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
/// $2133 SETINI bit 0: screen interlace.
const SCREEN_INTERLACE: u8 = 0x01;
/// $2133 SETINI bit 2: overscan.
const OVERSCAN: u8 = 0x04;
/// Where H=0.5, when the NMI flag is set, falls in the line that starts vertical blank.
const NMI_FLAG: u64 = 2;
/// The dot at which horizontal blank begins.
const HBLANK_START: u16 = 274;
/// The dot at which horizontal blank ends.
const HBLANK_END: u16 = 1;
/// How long after the start of a `$4201` write its counter latch falls: one dot.
pub(crate) const WRIO_LATCH_DELAY: u64 = 4;
/// The earliest an auto-joypad read starts in its line: dot 32.5.
const AUTO_READ_EARLIEST: u64 = 130;
/// When the first frame's auto-joypad read starts after power-on or reset: dot 74.5 of
/// line 225, after 225 normal lines.
const AUTO_READ_FIRST: u64 = FIRST_VBLANK_LINE as u64 * LineKind::Normal.length() + 298;
/// Auto-joypad reads start a multiple of this many master cycles apart.
const AUTO_READ_GRID: u64 = 256;
/// Where H=6, HDMA's setup, falls in line 0.
const HDMA_SETUP: u64 = 6 * 4;
/// Where H=278, a line's HDMA transfer, falls in its line: before the long dots.
const HDMA_LINE: u64 = 278 * 4;

/// The two kinds of HDMA step (ADR 0026, point 2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HdmaStep {
    /// The frame's setup, at H=6 of line 0.
    Setup,
    /// A line's transfer, at H=278.
    Line,
}

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
    /// The NMI flag is set (ADR 0021, point 4).
    Nmi,
    /// An H/V timer trigger point passed (ADR 0021, point 5).
    Irq,
    /// The counters are latched by a `$4201` write (ADR 0021, point 7).
    Latch {
        /// The line, the latched V.
        line: u16,
        /// The dot, the latched H.
        dot: u16,
    },
    /// The auto-joypad read's next event is due (ADR 0024, point 9).
    Joypad {
        /// When it was due.
        at: Cycles,
    },
    /// An HDMA step is due (ADR 0026, point 2).
    Hdma(HdmaStep),
}

/// The machine's place in its lines and frames, and the next time each scheduled event is
/// due.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Timing {
    /// The video standard, which sets the lines per frame.
    region: Region,
    /// $2133 SETINI's interlace and overscan bits as last written (ADR 0020, point 6).
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
    /// `$4200` bits 4 and 5, HTIME and VTIME.
    timer: Timer,
    /// The line before the one now running, whose late trigger point may fall in this one.
    previous: Option<TimedLine>,
    /// Trigger points before this time have passed.
    irq_after: Cycles,
    /// When the frame started at power-on or the last reset: the auto-read's grid starts
    /// [`AUTO_READ_FIRST`] cycles later (ADR 0024, point 5).
    origin: Cycles,
    /// The HDMA step the HDMA deadline is for.
    hdma_step: HdmaStep,
    /// When the next refresh, line, NMI flag, trigger point, `$4201` latch, auto-read
    /// event and HDMA step are due.
    deadlines: Deadlines,
}

impl Timing {
    /// Line 0 of a frame starting at `start`, as at power-on.
    // @source: fullsnes#snesiomap
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0018
    // @source: adr-0020
    // @source: adr-0021
    pub(crate) fn new(region: Region, start: Cycles) -> Self {
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
            timer: Timer::power_on(),
            previous: None,
            irq_after: start,
            origin: start,
            hdma_step: HdmaStep::Setup,
            deadlines: Deadlines::new(),
        };
        timing.restart(start);
        timing
    }

    /// Restarts the frame at `start`, as reset does: the SNES "returns to a known timing
    /// position on reset" (ADR 0018), H=0, V=0 with field 0 (ADR 0020, point 4), `$2133` is
    /// 0 and the refresh schedule starts over (ADR 0019). `$4200` is 0, HTIME and VTIME are
    /// kept, and no NMI flag or latch is pending (ADR 0021, point 8). No auto-read is pending
    /// and its grid starts over (ADR 0024, points 5 and 7). HDMA's setup is next, at H=6
    /// (ADR 0026, point 2). The counts of lines and frames are kept.
    // @source: anomie-timing
    // @source: anomie-timing#s-cpu/auto-joypad-read
    // @source: anomie-timing#s-cpu/hdma
    // @source: fullsnes#snesiomap
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0018
    // @source: adr-0019
    // @source: adr-0020
    // @source: adr-0021
    // @source: adr-0024
    // @source: adr-0026
    pub(crate) fn restart(&mut self, start: Cycles) {
        self.screen_mode = 0;
        // Field 0 at H=0; the toggle at H=1 of this line makes the frame field 1.
        self.field = true;
        self.line = 0;
        self.line_start = start;
        self.line_kind = LineKind::Normal;
        self.vblank = false;
        self.refresh_offset = FIRST_REFRESH;
        self.deadlines
            .set(Deadline::Refresh, start.after(FIRST_REFRESH));
        self.deadlines
            .set(Deadline::Line, start.after(LineKind::Normal.length()));
        self.deadlines.set(Deadline::Nmi, NEVER);
        self.deadlines.set(Deadline::Latch, NEVER);
        self.deadlines.set(Deadline::Joypad, NEVER);
        self.schedule_hdma(HdmaStep::Setup, start.after(HDMA_SETUP));
        self.origin = start;
        self.timer.reset();
        self.previous = None;
        self.irq_after = start;
        self.schedule_irq(start);
    }

    /// Takes bits 0 and 2 of a CPU write to `$2133` SETINI at `now` (ADR 0020, point 6).
    /// Interlace decides the frame's last line, so the trigger point is worked out again
    /// (ADR 0021, point 9).
    // @source: anomie-regs#2133
    // @source: adr-0020
    // @source: adr-0021
    pub(crate) fn write_screen_mode(&mut self, value: u8, now: Cycles) {
        self.screen_mode = value & (SCREEN_INTERLACE | OVERSCAN);
        self.schedule_irq(now);
    }

    /// Takes the IRQ bits of a `$4200` NMITIMEN write at `now` (ADR 0021, points 3 and 5).
    // @source: anomie-regs#4200
    // @source: anomie-timing#s-cpu/interrupts
    // @source: adr-0021
    pub(crate) fn write_interrupt_enable(&mut self, value: u8, now: Cycles) {
        self.timer.write_enable(value);
        self.schedule_irq(now);
    }

    /// $4207–$420A HTIMEL–VTIMEH, written at `now` (ADR 0021, points 3 and 5).
    // @source: anomie-regs#4207
    // @source: anomie-regs#4208
    // @source: anomie-regs#4209
    // @source: anomie-regs#420a
    // @source: adr-0021
    pub(crate) fn write_timer(&mut self, register: TimerRegister, value: u8, now: Cycles) {
        self.timer.write(register, value);
        self.schedule_irq(now);
    }

    /// Latches the counters at `at`, for a `$4201` write (ADR 0021, points 3 and 7).
    // @source: anomie-regs#4201
    // @source: anomie-timing
    // @source: adr-0021
    pub(crate) const fn schedule_latch(&mut self, at: Cycles) {
        self.deadlines.set(Deadline::Latch, at);
    }

    /// Schedules the auto-read's next event at `at`, or none at `None` (ADR 0024, point 9).
    // @source: adr-0024
    pub(crate) const fn schedule_joypad(&mut self, at: Option<Cycles>) {
        let at = match at {
            Some(at) => at,
            None => NEVER,
        };
        self.deadlines.set(Deadline::Joypad, at);
    }

    /// Schedules the HDMA step `step` at `at`, or none at [`NEVER`] (ADR 0026, point 2).
    // @source: adr-0026
    const fn schedule_hdma(&mut self, step: HdmaStep, at: Cycles) {
        self.hdma_step = step;
        self.deadlines.set(Deadline::Hdma, at);
    }

    /// When the auto-read starts in the line that starts vertical blank at `line_start`:
    /// the earliest time at or after dot 32.5 on the 256-cycle grid that starts at dot 74.5
    /// of the first frame's line 225 (ADR 0024, point 5).
    // @source: anomie-timing#s-cpu/auto-joypad-read
    // @source: adr-0024
    const fn auto_read_start(&self, line_start: Cycles) -> Cycles {
        let earliest = line_start.after(AUTO_READ_EARLIEST);
        let grid = self.origin.after(AUTO_READ_FIRST).0 % AUTO_READ_GRID;
        let wait = (grid + AUTO_READ_GRID - earliest.0 % AUTO_READ_GRID) % AUTO_READ_GRID;
        earliest.after(wait)
    }

    /// The video standard the machine runs.
    // @source: adr-0018
    // @source: adr-0020
    pub(crate) const fn region(&self) -> Region {
        self.region
    }

    /// When the frame started at power-on or the last reset: DMA's 8-cycle grid counts from
    /// it (ADR 0025, point 4).
    // @source: anomie-timing#s-cpu/dma
    // @source: adr-0025
    pub(crate) const fn origin(&self) -> Cycles {
        self.origin
    }

    /// Whether vertical blank has begun and not yet ended: `$4212` bit 7 (ADR 0021,
    /// point 6).
    // @source: anomie-timing#s-cpu/registers
    // @source: adr-0020
    // @source: adr-0021
    pub(crate) const fn in_vertical_blank(&self) -> bool {
        self.vblank
    }

    /// Whether the dot at `now` is in horizontal blank, H=274 to H=1: `$4212` bit 6
    /// (ADR 0021, point 6).
    // @source: anomie-timing#s-cpu/registers
    // @source: anomie-timing#s-ppu/blanking-periods
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0021
    pub(crate) const fn in_horizontal_blank(&self, now: Cycles) -> bool {
        let (_, dot) = self.position(now);
        dot >= HBLANK_START || dot < HBLANK_END
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
        if self.line == 0 && now.0 < self.line_start.after(FIELD_TOGGLE).0 {
            !self.field
        } else {
            self.field
        }
    }

    /// Services the first deadline due at or before `now`, if any, and schedules its next
    /// occurrence (ADR 0020, point 9; ADR 0021, point 9). The auto-read's next event is
    /// scheduled by the bus (ADR 0024, point 9). HDMA's setup is followed by the same
    /// line's transfer (ADR 0026, point 2).
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0001
    // @source: adr-0019
    // @source: adr-0020
    // @source: adr-0021
    // @source: adr-0024
    // @source: adr-0026
    pub(crate) fn service(&mut self, now: Cycles) -> Option<Event> {
        let deadline = self.deadlines.first_due(now)?;
        let at = self.deadlines.due(deadline);
        match deadline {
            Deadline::Refresh => {
                self.schedule_refresh();
                Some(Event::Refresh)
            }
            Deadline::Line => Some(self.start_line()),
            Deadline::Nmi => {
                self.deadlines.set(Deadline::Nmi, NEVER);
                Some(Event::Nmi)
            }
            Deadline::Irq => {
                self.irq_after = at + 1;
                self.schedule_irq(at);
                Some(Event::Irq)
            }
            Deadline::Latch => {
                self.deadlines.set(Deadline::Latch, NEVER);
                let (line, dot) = self.position(at);
                Some(Event::Latch { line, dot })
            }
            Deadline::Joypad => {
                self.deadlines.set(Deadline::Joypad, NEVER);
                Some(Event::Joypad { at })
            }
            Deadline::Hdma => {
                let step = self.hdma_step;
                match step {
                    HdmaStep::Setup => {
                        self.schedule_hdma(HdmaStep::Line, self.line_start + HDMA_LINE);
                    }
                    HdmaStep::Line => self.schedule_hdma(HdmaStep::Line, NEVER),
                }
                Some(Event::Hdma(step))
            }
        }
    }

    /// The line now running, as the timer sees it.
    // @source: anomie-timing#s-cpu/interrupts
    // @source: adr-0021
    const fn timed_line(&self) -> TimedLine {
        let previous_length = match self.previous {
            Some(previous) => previous.kind.length(),
            None => LineKind::Normal.length(),
        };
        TimedLine {
            number: self.line,
            start: self.line_start,
            kind: self.line_kind,
            last: self.line == self.last_line(),
            previous_length,
        }
    }

    /// Schedules the earliest trigger point at or after `now` that has not passed, of the
    /// previous line and of the line now running (ADR 0021, point 9).
    // @source: anomie-timing#s-cpu/interrupts
    // @source: adr-0021
    fn schedule_irq(&mut self, now: Cycles) {
        let from = now.max(self.irq_after);
        let previous = self
            .previous
            .and_then(|previous| self.timer.trigger(&previous));
        let current = self.timer.trigger(&self.timed_line());
        let next = [previous, current]
            .into_iter()
            .flatten()
            .filter(|at| *at >= from)
            .min()
            .unwrap_or(NEVER);
        self.deadlines.set(Deadline::Irq, next);
    }

    /// Schedules the refresh after the one now due, in the following line (ADR 0019,
    /// point 6). The line now running is the one the due refresh fell in.
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: adr-0019
    // @source: adr-0020
    const fn schedule_refresh(&mut self) {
        let length = self.line_kind.length();
        self.refresh_offset = following_refresh(self.refresh_offset, length);
        let at = self.line_start.after(length + self.refresh_offset);
        self.deadlines.set(Deadline::Refresh, at);
    }

    /// Starts the line that is due: the next line, or line 0 after the frame's last line,
    /// with its length, the field toggle and the start of vertical blank (ADR 0020,
    /// points 1, 2, 4 and 5), the NMI flag and auto-read it brings and its trigger point
    /// (ADR 0021, points 4 and 9; ADR 0024, point 5). Line 0 schedules HDMA's setup, and
    /// every other line that starts before vertical blank its transfer (ADR 0026, point 2).
    // @source: anomie-regs#2133
    // @source: anomie-regs#dma-and-hdma/hdma
    // @source: anomie-timing#s-cpu/auto-joypad-read
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: anomie-timing#s-cpu/hdma
    // @source: anomie-timing#s-cpu/interrupts
    // @source: anomie-timing#s-ppu/blanking-periods
    // @source: fullsnes#snestiminghvcounters
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0020
    // @source: adr-0021
    // @source: adr-0024
    // @source: adr-0026
    fn start_line(&mut self) -> Event {
        let at = self.deadlines.due(Deadline::Line);
        self.previous = Some(self.timed_line());
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
            self.deadlines.set(Deadline::Nmi, at + NMI_FLAG);
            self.deadlines
                .set(Deadline::Joypad, self.auto_read_start(at));
        }
        if self.line == 0 {
            self.schedule_hdma(HdmaStep::Setup, at + HDMA_SETUP);
        } else if self.vblank {
            self.schedule_hdma(HdmaStep::Line, NEVER);
        } else {
            self.schedule_hdma(HdmaStep::Line, at + HDMA_LINE);
        }
        self.lines += 1;
        self.deadlines
            .set(Deadline::Line, at + self.line_kind.length());
        self.schedule_irq(at);
        Event::Line {
            line: self.line,
            at,
            frame_ended,
        }
    }

    /// The frame's last line: 261 on NTSC and 311 on PAL, one more with interlace on in
    /// field 0 (ADR 0020, point 2).
    // @source: anomie-regs#213c
    // @source: anomie-timing#s-cpu/clocks-and-refresh
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
