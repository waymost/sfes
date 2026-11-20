// @source: adr-0020
//! Unit tests of the scheduler: line lengths, frame lengths, the field, vertical blank,
//! the refresh schedule and the H/V position.

use alloc::vec::Vec;

use super::*;

/// Services every deadline due up to and including `end`, at the time each falls due,
/// and returns the lines started, with the frames they ended.
// @source: adr-0020
fn run_until(timing: &mut Timing, end: u64) -> Vec<(u16, u64, bool)> {
    let mut lines = Vec::new();
    while timing.next_deadline().0 <= end {
        let now = timing.next_deadline();
        if let Some(Event::Line {
            line,
            at,
            frame_ended,
        }) = timing.service(now)
        {
            lines.push((line, at.0, frame_ended));
        }
    }
    lines
}

/// The times at which the first `count` frames end, from power-on.
// @source: adr-0020
fn frame_ends(timing: &mut Timing, count: usize) -> Vec<u64> {
    let mut ends = Vec::new();
    let mut end = 0;
    while ends.len() < count {
        end += 1364;
        ends.extend(
            run_until(timing, end)
                .into_iter()
                .filter(|(_, _, frame_ended)| *frame_ended)
                .map(|(_, at, _)| at),
        );
    }
    ends
}

/// The length of each line, by line number, of the first `count` frames from power-on,
/// each counted from its line 0.
// @source: adr-0020
fn line_lengths(timing: &mut Timing, count: usize) -> Vec<Vec<u64>> {
    // Power-on starts line 0 without an event.
    let mut frames = Vec::from([Vec::from([0])]);
    let mut end = 0;
    while frames.len() <= count {
        end += 1364;
        for (line, at, _) in run_until(timing, end) {
            if line == 0 {
                frames.push(Vec::new());
            }
            if let Some(frame) = frames.last_mut() {
                frame.push(at);
            }
        }
    }
    let starts = frames.concat();
    let mut lengths = Vec::new();
    let mut index = 0;
    for frame in &frames[..count] {
        lengths.push(
            starts[index..=index + frame.len()]
                .windows(2)
                .map(|pair| pair[1] - pair[0])
                .collect::<Vec<_>>(),
        );
        index += frame.len();
    }
    lengths
}

// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: anomie-timing#s-ppu/blanking-periods
// @source: fullsnes#snestimingoscillators
// @source: fullsnes#snestiminghvcounters
// @source: fullsnes#snestiminghvevents
// @source: adr-0020
#[test]
fn ntsc_frames_alternate_with_a_short_line_240() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    // Frame 1 ends at the start of line 225. The first frame is field 1, so its line 240,
    // in the next frame's period, is 4 cycles short.
    assert_eq!(
        frame_ends(&mut timing, 3),
        [
            225 * 1364,
            225 * 1364 + 262 * 1364 - 4,
            225 * 1364 + 524 * 1364 - 4
        ]
    );
    // Two frames take (262 + 262) × 1364 − 4 master cycles (fullsnes).
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    let [field_1, field_0] = <[_; 2]>::try_from(line_lengths(&mut timing, 2)).expect("2");
    assert_eq!(field_1.len(), 262);
    assert_eq!(field_1[240], 1360);
    assert_eq!(field_1.iter().filter(|length| **length != 1364).count(), 1);
    assert_eq!(field_0.len(), 262);
    assert!(field_0.iter().all(|length| *length == 1364));
}

// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: anomie-regs#2133
// @source: anomie-regs#213c
// @source: fullsnes#snestimingoscillators
// @source: fullsnes#snestiminghvcounters
// @source: fullsnes#snestiminghvevents
// @source: adr-0020
#[test]
fn ntsc_interlace_adds_line_262_to_field_0() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    timing.write_screen_mode(SCREEN_INTERLACE);
    let [field_1, field_0] = <[_; 2]>::try_from(line_lengths(&mut timing, 2)).expect("2");
    assert_eq!(field_1.len(), 262);
    assert!(field_1.iter().all(|length| *length == 1364));
    assert_eq!(field_0.len(), 263);
    assert!(field_0.iter().all(|length| *length == 1364));
    // Two frames take (262 + 263) × 1364 master cycles (fullsnes).
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    timing.write_screen_mode(SCREEN_INTERLACE);
    let ends = frame_ends(&mut timing, 3);
    assert_eq!(ends[2] - ends[0], 525 * 1364);
}

// @source: anomie-regs#213c
// @source: fullsnes#snestimingoscillators
// @source: fullsnes#snestiminghvevents
// @source: adr-0020
#[test]
fn pal_frames_are_312_lines() {
    let mut timing = Timing::new(Region::Pal, Cycles(0));
    assert_eq!(
        frame_ends(&mut timing, 3),
        [225 * 1364, 225 * 1364 + 312 * 1364, 225 * 1364 + 624 * 1364]
    );
    let mut timing = Timing::new(Region::Pal, Cycles(0));
    for lengths in line_lengths(&mut timing, 2) {
        assert_eq!(lengths.len(), 312);
        assert!(lengths.iter().all(|length| *length == 1364));
    }
}

// @source: fullsnes#snestimingoscillators
// @source: fullsnes#snestiminghvcounters
// @source: fullsnes#snestiminghvevents
// @source: adr-0020
#[test]
fn pal_interlace_has_a_long_line_311_in_field_1_and_line_312_in_field_0() {
    let mut timing = Timing::new(Region::Pal, Cycles(0));
    timing.write_screen_mode(SCREEN_INTERLACE);
    let [field_1, field_0] = <[_; 2]>::try_from(line_lengths(&mut timing, 2)).expect("2");
    assert_eq!(field_1.len(), 312);
    assert_eq!(field_1[311], 1368);
    assert_eq!(field_1.iter().filter(|length| **length != 1364).count(), 1);
    assert_eq!(field_0.len(), 313);
    assert!(field_0.iter().all(|length| *length == 1364));
    // Two frames take 625 × 1364 + 4 master cycles (fullsnes).
    let mut timing = Timing::new(Region::Pal, Cycles(0));
    timing.write_screen_mode(SCREEN_INTERLACE);
    let ends = frame_ends(&mut timing, 3);
    assert_eq!(ends[2] - ends[0], 625 * 1364 + 4);
}

// @source: anomie-timing#s-ppu/blanking-periods
// @source: anomie-regs#2133
// @source: fullsnes#snestiminghvevents
// @source: adr-0020
#[test]
fn overscan_moves_vertical_blank_to_line_240_unless_cleared_before() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    timing.write_screen_mode(OVERSCAN);
    assert_eq!(frame_ends(&mut timing, 1), [240 * 1364]);
    // Cleared during line 230: vertical blank begins with line 231.
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    timing.write_screen_mode(OVERSCAN);
    assert!(
        run_until(&mut timing, 230 * 1364)
            .iter()
            .all(|(_, _, ended)| !ended)
    );
    timing.write_screen_mode(0);
    let started = run_until(&mut timing, 232 * 1364);
    assert_eq!(started, [(231, 231 * 1364, true), (232, 232 * 1364, false)]);
}

// @source: anomie-timing#s-ppu/registers
// @source: fullsnes#snestiminghvevents
// @source: fullsnes#snesiomap
// @source: adr-0020
#[test]
fn the_field_toggles_at_h_1_of_line_0() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    assert!(!timing.field(Cycles(3)));
    assert!(timing.field(Cycles(4)));
    assert!(timing.field(Cycles(262 * 1364 - 5)));
    run_until(&mut timing, 262 * 1364 - 4);
    assert_eq!(timing.position(Cycles(262 * 1364 - 4)), (0, 0));
    assert!(timing.field(Cycles(262 * 1364 - 1)));
    assert!(!timing.field(Cycles(262 * 1364)));
}

// @source: anomie-regs#213c
// @source: anomie-timing#s-ppu/rendering
// @source: adr-0020
#[test]
fn the_position_is_the_line_and_its_dot() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    assert_eq!(timing.position(Cycles(0)), (0, 0));
    assert_eq!(timing.position(Cycles(538)), (0, 134));
    assert_eq!(timing.position(Cycles(1363)), (0, 339));
    run_until(&mut timing, 1364);
    assert_eq!(timing.position(Cycles(1364 + 1292)), (1, 323));
    run_until(&mut timing, 240 * 1364);
    assert_eq!(timing.position(Cycles(240 * 1364 + 1359)), (240, 339));
    assert_eq!(timing.position(Cycles(240 * 1364 + 1292)), (240, 323));
}

// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: adr-0019
// @source: adr-0020
#[test]
fn refreshes_alternate_between_538_and_534_into_the_line() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    let mut due = Vec::new();
    for _ in 0..4 {
        let at = timing.next_refresh();
        run_until(&mut timing, at.0 - 1);
        assert_eq!(timing.next_refresh(), at);
        assert_eq!(timing.service(at), Some(Event::Refresh));
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
// @source: adr-0020
#[test]
fn a_short_line_keeps_the_refresh_offset() {
    assert_eq!(following_refresh(538, 1360), 538);
    assert_eq!(following_refresh(534, 1360), 534);
    assert_eq!(following_refresh(538, 1368), 538);
    assert_eq!(following_refresh(538, 1364), 534);
    assert_eq!(following_refresh(534, 1364), 538);
    // Even lines are 538 and odd lines 534 until the short line 240, which is even;
    // line 241 then takes 538 too, and the alternation continues from there.
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    let mut offsets = Vec::new();
    while offsets.len() < 243 {
        let now = timing.next_deadline();
        if timing.service(now) == Some(Event::Refresh) {
            offsets.push(now.0 - timing.line_start.0);
        }
    }
    assert_eq!(offsets[239..], [534, 538, 538, 534]);
}

// @source: anomie-timing
// @source: fullsnes#snesiomap
// @source: adr-0018
// @source: adr-0019
// @source: adr-0020
#[test]
fn restart_returns_to_line_0_field_0_and_a_cleared_screen_mode() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    timing.write_screen_mode(SCREEN_INTERLACE | OVERSCAN);
    run_until(&mut timing, 300 * 1364);
    assert_eq!(timing.frames(), 1);
    timing.restart(Cycles(500_000));
    assert_eq!(timing.position(Cycles(500_000)), (0, 0));
    assert!(!timing.field(Cycles(500_003)));
    assert!(timing.field(Cycles(500_004)));
    assert_eq!(timing.screen_mode, 0);
    assert_eq!(timing.next_refresh(), Cycles(500_538));
    assert_eq!(timing.next_deadline(), Cycles(500_538));
    let lines = timing.lines();
    let started = run_until(&mut timing, 500_000 + 1364);
    assert_eq!(started, [(1, 500_000 + 1364, false)]);
    assert_eq!(timing.lines(), lines + 1);
    assert_eq!(timing.frames(), 1);
    assert_eq!(frame_ends(&mut timing, 1), [500_000 + 225 * 1364]);
}
