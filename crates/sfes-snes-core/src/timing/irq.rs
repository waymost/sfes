// @source: anomie-timing#s-cpu/interrupts
// @source: anomie-regs#4200
// @source: anomie-regs#4207
// @source: anomie-regs#4208
// @source: anomie-regs#4209
// @source: anomie-regs#420a
// @source: fullsnes#snesppuinterrupts
// @source: adr-0021
//! The H/V timer: `$4200` bits 4 and 5, HTIME and VTIME, and the master cycle at which a
//! line's IRQ trigger point falls (ADR 0021, point 5).

use sfes_api::Cycles;

use crate::timing::position::LineKind;

/// `$4207–$420A` at power-on: `$1FF` (anomie-regs, fullsnes#snesiomap).
const TIMER_POWER_ON: u16 = 0x1FF;
/// The 9 bits HTIME and VTIME hold.
const TIMER_MASK: u16 = 0x1FF;
/// `$4200` bit 4: the H condition.
const HORIZONTAL: u8 = 0x10;
/// `$4200` bit 5: the V condition.
const VERTICAL: u8 = 0x20;
/// Master cycles from the start of the previous line to an H=0 trigger point.
const H_ZERO_TRIGGER: u64 = 1374;
/// Master cycles from the start of the line to the trigger point of H=0, before 4 per dot.
const TRIGGER_BASE: u64 = 14;
/// Master cycles per dot in the trigger formula.
const TRIGGER_DOT: u64 = 4;
/// The dot whose trigger is lost on a short line and on a frame's last line.
const LOST_DOT: u16 = 153;

/// What `$4200` bits 4 and 5 select (yx).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum IrqMode {
    /// 00: no IRQ.
    Off,
    /// 01: every line at HTIME.
    Horizontal,
    /// 10: line VTIME at H=0.
    Vertical,
    /// 11: line VTIME at HTIME.
    Both,
}

impl IrqMode {
    /// The mode a `$4200` write selects.
    // @source: anomie-regs#4200
    // @source: anomie-timing#s-cpu/interrupts
    // @source: fullsnes#snesppuinterrupts
    pub(crate) const fn from_enable(value: u8) -> Self {
        match (value & VERTICAL != 0, value & HORIZONTAL != 0) {
            (false, false) => Self::Off,
            (false, true) => Self::Horizontal,
            (true, false) => Self::Vertical,
            (true, true) => Self::Both,
        }
    }
}

/// A line as the timer sees it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TimedLine {
    /// Its number, the V position.
    pub(crate) number: u16,
    /// When it started.
    pub(crate) start: Cycles,
    /// Its length.
    pub(crate) kind: LineKind,
    /// Whether it is the frame's last line.
    pub(crate) last: bool,
    /// The length of the line before it, which places an H=0 trigger.
    pub(crate) previous_length: u64,
}

/// The H/V timer's settings.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Timer {
    /// `$4200` bits 4 and 5.
    mode: IrqMode,
    /// $4207–$4208 HTIMEL, HTIMEH: the H position of the trigger.
    horizontal: u16,
    /// $4209–$420A VTIMEL, VTIMEH: the line of the trigger.
    vertical: u16,
}

impl Timer {
    /// The timer at power-on: no IRQ, HTIME and VTIME `$1FF` (ADR 0021, point 8).
    // @source: anomie-regs#4200
    // @source: anomie-regs#4208
    // @source: anomie-regs#420a
    // @source: fullsnes#snesiomap
    // @source: adr-0021
    pub(crate) const fn power_on() -> Self {
        Self {
            mode: IrqMode::Off,
            horizontal: TIMER_POWER_ON,
            vertical: TIMER_POWER_ON,
        }
    }

    /// Reset clears `$4200` and keeps HTIME and VTIME (ADR 0021, point 8).
    // @source: anomie-regs#4200
    // @source: anomie-regs#4208
    // @source: anomie-regs#420a
    // @source: fullsnes#snesiomap
    // @source: adr-0021
    pub(crate) const fn reset(&mut self) {
        self.mode = IrqMode::Off;
    }

    /// The mode a `$4200` write selects.
    // @source: anomie-regs#4200
    pub(crate) const fn write_enable(&mut self, value: u8) {
        self.mode = IrqMode::from_enable(value);
    }

    /// $4207 HTIMEL: bits 0–7 of HTIME.
    // @source: anomie-regs#4207
    // @source: anomie-regs#4208
    pub(crate) const fn write_horizontal_low(&mut self, value: u8) {
        self.horizontal = with_low(self.horizontal, value);
    }

    /// $4208 HTIMEH: bit 8 of HTIME.
    // @source: anomie-regs#4208
    pub(crate) const fn write_horizontal_high(&mut self, value: u8) {
        self.horizontal = with_high(self.horizontal, value);
    }

    /// $4209 VTIMEL: bits 0–7 of VTIME.
    // @source: anomie-regs#4209
    // @source: anomie-regs#420a
    pub(crate) const fn write_vertical_low(&mut self, value: u8) {
        self.vertical = with_low(self.vertical, value);
    }

    /// $420A VTIMEH: bit 8 of VTIME.
    // @source: anomie-regs#420a
    pub(crate) const fn write_vertical_high(&mut self, value: u8) {
        self.vertical = with_high(self.vertical, value);
    }

    /// When `line`'s trigger point falls, or `None` if it has none (ADR 0021, point 5):
    ///
    /// - V-IRQ is HV-IRQ with H=0, and an H=0 trigger is 1374 master cycles after the start
    ///   of the previous line;
    /// - any other H is 14 + 4×H master cycles after the line's start, which may be in the
    ///   next line;
    /// - an H past the line's last dot, and H=153 on a short line or a frame's last line,
    ///   never trigger.
    // @source: anomie-timing#s-cpu/interrupts
    // @source: anomie-regs#4208
    // @source: anomie-regs#420a
    // @source: fullsnes#snesppuinterrupts
    // @source: fullsnes#snestiminghvevents
    // @source: adr-0021
    pub(crate) const fn trigger(self, line: &TimedLine) -> Option<Cycles> {
        let (dot, on_line) = match self.mode {
            IrqMode::Off => return None,
            IrqMode::Horizontal => (self.horizontal, true),
            IrqMode::Vertical => (0, line.number == self.vertical),
            IrqMode::Both => (self.horizontal, line.number == self.vertical),
        };
        if !on_line || dot >= line.kind.dots() {
            return None;
        }
        if dot == 0 {
            return Some(Cycles(line.start.0 + H_ZERO_TRIGGER - line.previous_length));
        }
        if dot == LOST_DOT && (matches!(line.kind, LineKind::Short) || line.last) {
            return None;
        }
        Some(Cycles(
            line.start.0 + TRIGGER_BASE + TRIGGER_DOT * dot as u64,
        ))
    }
}

/// `timer` with its bits 0–7 replaced by `value`.
// @source: anomie-regs#4207
// @source: anomie-regs#4209
const fn with_low(timer: u16, value: u8) -> u16 {
    (timer & 0x100) | value as u16
}

/// `timer` with its bit 8 replaced by bit 0 of `value`.
// @source: anomie-regs#4208
// @source: anomie-regs#420a
const fn with_high(timer: u16, value: u8) -> u16 {
    ((timer & 0xFF) | ((value as u16) << 8)) & TIMER_MASK
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Line `number`, normal, starting at 10 000 after a normal line.
    // @source: adr-0021
    const fn line(number: u16) -> TimedLine {
        TimedLine {
            number,
            start: Cycles(10_000),
            kind: LineKind::Normal,
            last: false,
            previous_length: 1364,
        }
    }

    /// A timer in `mode` with HTIME `horizontal` and VTIME `vertical`.
    // @source: adr-0021
    const fn timer(enable: u8, horizontal: u16, vertical: u16) -> Timer {
        let mut timer = Timer::power_on();
        timer.write_enable(enable);
        let [low, high] = horizontal.to_le_bytes();
        timer.write_horizontal_low(low);
        timer.write_horizontal_high(high);
        let [low, high] = vertical.to_le_bytes();
        timer.write_vertical_low(low);
        timer.write_vertical_high(high);
        timer
    }

    // @source: anomie-regs#4200
    // @source: fullsnes#snesppuinterrupts
    #[test]
    fn bits_4_and_5_select_the_mode() {
        assert_eq!(IrqMode::from_enable(0x81), IrqMode::Off);
        assert_eq!(IrqMode::from_enable(0x10), IrqMode::Horizontal);
        assert_eq!(IrqMode::from_enable(0x20), IrqMode::Vertical);
        assert_eq!(IrqMode::from_enable(0x30), IrqMode::Both);
    }

    // @source: anomie-regs#4208
    // @source: anomie-regs#420a
    // @source: fullsnes#snesiomap
    #[test]
    fn the_timers_are_nine_bits_and_start_at_1ff() {
        let mut timer = Timer::power_on();
        assert_eq!((timer.horizontal, timer.vertical), (0x1FF, 0x1FF));
        timer.write_horizontal_low(0x34);
        timer.write_horizontal_high(0xFE);
        timer.write_vertical_high(0x00);
        timer.write_vertical_low(0x12);
        assert_eq!((timer.horizontal, timer.vertical), (0x034, 0x012));
        timer.write_enable(0x30);
        timer.reset();
        assert_eq!(timer.mode, IrqMode::Off);
        assert_eq!((timer.horizontal, timer.vertical), (0x034, 0x012));
    }

    // @source: anomie-timing#s-cpu/interrupts
    // @source: fullsnes#snestiminghvevents
    #[test]
    fn a_trigger_is_14_plus_4h_cycles_into_the_line() {
        // H=HTIME+3.5: 14 + 4H = 4(H + 3.5).
        assert_eq!(timer(0x10, 1, 0).trigger(&line(7)), Some(Cycles(10_018)));
        assert_eq!(timer(0x10, 100, 0).trigger(&line(7)), Some(Cycles(10_414)));
        assert_eq!(timer(0x30, 100, 7).trigger(&line(7)), Some(Cycles(10_414)));
        assert_eq!(timer(0x30, 100, 8).trigger(&line(7)), None);
        // The formula counts cycles, not dots: H=339 falls 6 cycles into the next line.
        assert_eq!(timer(0x10, 339, 0).trigger(&line(7)), Some(Cycles(11_370)));
    }

    // @source: anomie-timing#s-cpu/interrupts
    // @source: fullsnes#snestiminghvevents
    #[test]
    fn an_h0_trigger_is_1374_cycles_after_the_previous_line_started() {
        // H=2.5 after a normal line: 1374 - 1364 = 10 cycles.
        assert_eq!(timer(0x20, 200, 7).trigger(&line(7)), Some(Cycles(10_010)));
        assert_eq!(timer(0x30, 0, 7).trigger(&line(7)), Some(Cycles(10_010)));
        assert_eq!(timer(0x10, 0, 0).trigger(&line(7)), Some(Cycles(10_010)));
        // After a short line it is 14 cycles in; after a long one 6.
        let after_short = TimedLine {
            previous_length: 1360,
            ..line(241)
        };
        assert_eq!(
            timer(0x20, 0, 241).trigger(&after_short),
            Some(Cycles(10_014))
        );
        let after_long = TimedLine {
            previous_length: 1368,
            ..line(0)
        };
        assert_eq!(timer(0x20, 0, 0).trigger(&after_long), Some(Cycles(10_006)));
    }

    // @source: anomie-regs#4208
    // @source: anomie-regs#420a
    // @source: fullsnes#snesppuinterrupts
    #[test]
    fn an_h_past_the_last_dot_never_triggers() {
        assert_eq!(timer(0x10, 340, 0).trigger(&line(7)), None);
        assert_eq!(timer(0x10, 0x1FF, 0).trigger(&line(7)), None);
        let long = TimedLine {
            kind: LineKind::Long,
            ..line(311)
        };
        assert_eq!(timer(0x10, 340, 0).trigger(&long), Some(Cycles(11_374)));
        assert_eq!(timer(0x10, 341, 0).trigger(&long), None);
        assert_eq!(timer(0x00, 100, 7).trigger(&line(7)), None);
    }

    // @source: anomie-timing#s-cpu/interrupts
    #[test]
    fn dot_153_never_triggers_on_a_short_or_last_line() {
        let short = TimedLine {
            kind: LineKind::Short,
            ..line(240)
        };
        let last = TimedLine {
            last: true,
            ..line(261)
        };
        assert_eq!(timer(0x10, 153, 0).trigger(&line(7)), Some(Cycles(10_626)));
        assert_eq!(timer(0x10, 153, 0).trigger(&short), None);
        assert_eq!(timer(0x10, 153, 0).trigger(&last), None);
        assert_eq!(timer(0x10, 152, 0).trigger(&short), Some(Cycles(10_622)));
        assert_eq!(timer(0x10, 154, 0).trigger(&last), Some(Cycles(10_630)));
    }
}
