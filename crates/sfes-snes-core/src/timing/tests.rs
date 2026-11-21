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
    timing.write_screen_mode(SCREEN_INTERLACE, Cycles(0));
    let [field_1, field_0] = <[_; 2]>::try_from(line_lengths(&mut timing, 2)).expect("2");
    assert_eq!(field_1.len(), 262);
    assert!(field_1.iter().all(|length| *length == 1364));
    assert_eq!(field_0.len(), 263);
    assert!(field_0.iter().all(|length| *length == 1364));
    // Two frames take (262 + 263) × 1364 master cycles (fullsnes).
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    timing.write_screen_mode(SCREEN_INTERLACE, Cycles(0));
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
    timing.write_screen_mode(SCREEN_INTERLACE, Cycles(0));
    let [field_1, field_0] = <[_; 2]>::try_from(line_lengths(&mut timing, 2)).expect("2");
    assert_eq!(field_1.len(), 312);
    assert_eq!(field_1[311], 1368);
    assert_eq!(field_1.iter().filter(|length| **length != 1364).count(), 1);
    assert_eq!(field_0.len(), 313);
    assert!(field_0.iter().all(|length| *length == 1364));
    // Two frames take 625 × 1364 + 4 master cycles (fullsnes).
    let mut timing = Timing::new(Region::Pal, Cycles(0));
    timing.write_screen_mode(SCREEN_INTERLACE, Cycles(0));
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
    timing.write_screen_mode(OVERSCAN, Cycles(0));
    assert_eq!(frame_ends(&mut timing, 1), [240 * 1364]);
    // Cleared during line 230: vertical blank begins with line 231.
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    timing.write_screen_mode(OVERSCAN, Cycles(0));
    assert!(
        run_until(&mut timing, 230 * 1364)
            .iter()
            .all(|(_, _, ended)| !ended)
    );
    timing.write_screen_mode(0, Cycles(230 * 1364));
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
// @source: adr-0026
#[test]
fn restart_returns_to_line_0_field_0_and_a_cleared_screen_mode() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    timing.write_screen_mode(SCREEN_INTERLACE | OVERSCAN, Cycles(0));
    run_until(&mut timing, 300 * 1364);
    assert_eq!(timing.frames(), 1);
    timing.restart(Cycles(500_000));
    assert_eq!(timing.position(Cycles(500_000)), (0, 0));
    assert!(!timing.field(Cycles(500_003)));
    assert!(timing.field(Cycles(500_004)));
    assert_eq!(timing.screen_mode, 0);
    assert_eq!(timing.next_refresh(), Cycles(500_538));
    // HDMA's setup at H=6 comes first (ADR 0026, point 2).
    assert_eq!(timing.next_deadline(), Cycles(500_024));
    let lines = timing.lines();
    let started = run_until(&mut timing, 500_000 + 1364);
    assert_eq!(started, [(1, 500_000 + 1364, false)]);
    assert_eq!(timing.lines(), lines + 1);
    assert_eq!(timing.frames(), 1);
    assert_eq!(frame_ends(&mut timing, 1), [500_000 + 225 * 1364]);
}

/// Services every deadline due up to and including `end`, at the time each falls due,
/// and returns the NMI, IRQ and latch events with the times they fell due.
// @source: adr-0021
fn interrupt_events(timing: &mut Timing, end: u64) -> Vec<(u64, Event)> {
    let mut events = Vec::new();
    while timing.next_deadline().0 <= end {
        let now = timing.next_deadline();
        match timing.service(now) {
            Some(event @ (Event::Nmi | Event::Irq | Event::Latch { .. })) => {
                events.push((now.0, event));
            }
            Some(Event::Refresh | Event::Line { .. } | Event::Joypad { .. } | Event::Hdma(_))
            | None => {}
        }
    }
    events
}

/// The times of the IRQ trigger points up to `end`.
// @source: adr-0021
fn irq_times(timing: &mut Timing, end: u64) -> Vec<u64> {
    interrupt_events(timing, end)
        .into_iter()
        .filter(|(_, event)| *event == Event::Irq)
        .map(|(at, _)| at)
        .collect::<Vec<_>>()
}

/// A timer in mode `enable` with HTIME `horizontal` and VTIME `vertical`, written at `now`.
// @source: adr-0021
fn set_timer(timing: &mut Timing, enable: u8, horizontal: u16, vertical: u16, now: Cycles) {
    let [low, high] = horizontal.to_le_bytes();
    timing.write_horizontal_timer_low(low, now);
    timing.write_horizontal_timer_high(high, now);
    let [low, high] = vertical.to_le_bytes();
    timing.write_vertical_timer_low(low, now);
    timing.write_vertical_timer_high(high, now);
    timing.write_interrupt_enable(enable, now);
}

// @source: anomie-timing#s-cpu/interrupts
// @source: fullsnes#snestiminghvevents
// @source: adr-0020
// @source: adr-0021
#[test]
fn the_nmi_flag_falls_half_a_dot_into_vertical_blank() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    // Frame 1 is field 1 without interlace, so its line 240 is 4 cycles short and frame 2
    // starts at 262 × 1364 − 4 (ADR 0020).
    let frame_2 = 262 * 1364 - 4;
    let events = interrupt_events(&mut timing, frame_2 + 226 * 1364);
    assert_eq!(
        events,
        [
            (225 * 1364 + 2, Event::Nmi),
            (frame_2 + 225 * 1364 + 2, Event::Nmi),
        ]
    );
    // With overscan the flag waits for line 240.
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    timing.write_screen_mode(OVERSCAN, Cycles(0));
    assert_eq!(
        interrupt_events(&mut timing, 241 * 1364),
        [(240 * 1364 + 2, Event::Nmi)]
    );
}

// @source: anomie-timing#s-cpu/interrupts
// @source: anomie-regs#4200
// @source: fullsnes#snesppuinterrupts
// @source: adr-0021
#[test]
fn an_hv_irq_triggers_once_a_frame_at_its_dot() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    set_timer(&mut timing, 0x30, 100, 5, Cycles(0));
    let frame_2 = 262 * 1364 - 4;
    // 14 + 4 × 100 cycles into line 5.
    assert_eq!(
        irq_times(&mut timing, frame_2 + 10 * 1364),
        [5 * 1364 + 414, frame_2 + 5 * 1364 + 414]
    );
}

// @source: anomie-timing#s-cpu/interrupts
// @source: adr-0021
#[test]
fn an_h_irq_past_the_line_end_falls_in_the_next_line_and_is_kept() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    set_timer(&mut timing, 0x10, 339, 0, Cycles(0));
    // 14 + 4 × 339 = 1370: 6 cycles into the following line, every line.
    assert_eq!(
        irq_times(&mut timing, 3 * 1364 + 1000),
        [1370, 1364 + 1370, 2 * 1364 + 1370]
    );
}

// @source: anomie-timing#s-cpu/interrupts
// @source: fullsnes#snestiminghvevents
// @source: adr-0021
#[test]
fn a_v_irq_falls_10_cycles_in_or_14_after_the_short_line() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    set_timer(&mut timing, 0x20, 0, 7, Cycles(0));
    assert_eq!(irq_times(&mut timing, 8 * 1364), [7 * 1364 + 10]);
    // Line 241 follows the 1360-cycle line 240 of frame 1.
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    set_timer(&mut timing, 0x20, 0, 241, Cycles(0));
    assert_eq!(irq_times(&mut timing, 242 * 1364), [241 * 1364 - 4 + 14]);
    // Line 0 after power-on counts a normal line before it.
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    set_timer(&mut timing, 0x20, 0, 0, Cycles(0));
    assert_eq!(irq_times(&mut timing, 100), [10]);
}

// @source: anomie-timing#s-cpu/interrupts
// @source: adr-0021
#[test]
fn a_timer_write_takes_effect_from_its_access() {
    // A trigger point already passed in this line is not met.
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    run_until(&mut timing, 100);
    set_timer(&mut timing, 0x10, 10, 0, Cycles(100));
    assert_eq!(
        irq_times(&mut timing, 2 * 1364 + 100),
        [1364 + 54, 2 * 1364 + 54]
    );
    // One later in this line is.
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    run_until(&mut timing, 50);
    set_timer(&mut timing, 0x10, 10, 0, Cycles(50));
    assert_eq!(irq_times(&mut timing, 100), [54]);
    // Disabling cancels the pending one.
    timing.write_interrupt_enable(0x00, Cycles(100));
    assert_eq!(irq_times(&mut timing, 3 * 1364), []);
}

// @source: anomie-timing
// @source: anomie-regs#4201
// @source: adr-0021
#[test]
fn a_latch_takes_the_line_and_dot_when_it_falls_due() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    run_until(&mut timing, 3 * 1364);
    timing.schedule_latch(Cycles(3 * 1364 + 1300));
    // 1300 cycles in: past the long dot 323 (1292-1297), dot (1300 - 2) / 4 = 324.
    assert_eq!(
        interrupt_events(&mut timing, 4 * 1364),
        [(3 * 1364 + 1300, Event::Latch { line: 3, dot: 324 })]
    );
    // One due with a line start is taken in the new line.
    timing.schedule_latch(Cycles(5 * 1364));
    assert_eq!(
        interrupt_events(&mut timing, 5 * 1364),
        [(5 * 1364, Event::Latch { line: 5, dot: 0 })]
    );
}

// @source: anomie-timing#s-cpu/registers
// @source: anomie-timing#s-ppu/blanking-periods
// @source: adr-0021
#[test]
fn horizontal_blank_runs_from_dot_274_to_dot_1() {
    let timing = Timing::new(Region::Ntsc, Cycles(0));
    assert!(timing.in_horizontal_blank(Cycles(0)));
    assert!(timing.in_horizontal_blank(Cycles(3)));
    assert!(!timing.in_horizontal_blank(Cycles(4)));
    assert!(!timing.in_horizontal_blank(Cycles(274 * 4 - 1)));
    assert!(timing.in_horizontal_blank(Cycles(274 * 4)));
    assert!(timing.in_horizontal_blank(Cycles(1363)));
}

// @source: fullsnes#snesiomap
// @source: adr-0021
#[test]
fn restart_disables_the_irq_and_cancels_the_nmi_flag_and_latch() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    set_timer(&mut timing, 0x10, 10, 0, Cycles(0));
    run_until(&mut timing, 225 * 1364);
    timing.schedule_latch(Cycles(225 * 1364 + 100));
    timing.restart(Cycles(225 * 1364 + 1));
    assert_eq!(interrupt_events(&mut timing, 225 * 1364 + 3 * 1364), []);
    // HTIME was kept: re-enabling at the start of line 3 triggers 54 cycles into it and
    // into each line after.
    let start = 225 * 1364 + 1 + 3 * 1364;
    timing.write_interrupt_enable(0x10, Cycles(start));
    assert_eq!(
        irq_times(&mut timing, start + 1364 + 54),
        [start + 54, start + 1364 + 54]
    );
}

/// The auto-read starts of the next `count` frames, each with the start of the line it
/// falls in.
// @source: adr-0024
fn auto_read_starts(timing: &mut Timing, count: usize) -> Vec<(u64, u64)> {
    let mut starts = Vec::new();
    let mut line_start = 0;
    while starts.len() < count {
        let now = timing.next_deadline();
        match timing.service(now) {
            Some(Event::Line { at, .. }) => line_start = at.0,
            Some(Event::Joypad { at }) => starts.push((at.0, line_start)),
            _ => {}
        }
    }
    starts
}

// @source: anomie-timing#s-cpu/auto-joypad-read
// @source: adr-0024
#[test]
fn the_auto_read_starts_at_dot_74_5_then_on_a_256_cycle_grid_from_dot_32_5() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    let starts = auto_read_starts(&mut timing, 12);
    // Dot 74.5 of line 225 on the first frame.
    assert_eq!(starts[0], (225 * 1364 + 298, 225 * 1364));
    for &(at, line_start) in &starts {
        assert_eq!((at - starts[0].0) % 256, 0, "{at}");
        assert!(
            (130..386).contains(&(at - line_start)),
            "{at} in line at {line_start}"
        );
    }
}

// @source: anomie-timing#s-cpu/auto-joypad-read
// @source: adr-0024
#[test]
fn a_restart_starts_the_auto_read_grid_over_and_drops_a_pending_event() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    timing.schedule_joypad(Some(Cycles(5000)));
    timing.restart(Cycles(1001));
    let starts = auto_read_starts(&mut timing, 1);
    assert_eq!(starts[0].0, 1001 + 225 * 1364 + 298);
}

/// The HDMA steps that fall due up to `end`, each with its line and the time it fell due.
// @source: adr-0026
fn hdma_steps(timing: &mut Timing, end: u64) -> Vec<(HdmaStep, u16, u64)> {
    let mut steps = Vec::new();
    while timing.next_deadline().0 <= end {
        let now = timing.next_deadline();
        if let Some(Event::Hdma(step)) = timing.service(now) {
            steps.push((step, timing.position(now).0, now.0));
        }
    }
    steps
}

// @source: anomie-timing#s-cpu/hdma
// @source: anomie-regs#dma-and-hdma/hdma
// @source: fullsnes#snestiminghvevents
// @source: adr-0020
// @source: adr-0026
#[test]
fn hdma_sets_up_at_h_6_of_line_0_and_transfers_at_h_278_of_lines_0_to_224() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    // The first frame is field 1, with the short line 240 (ADR 0020, point 1).
    let frame = 262 * 1364 - 4;
    let steps = hdma_steps(&mut timing, frame + 2000);
    assert_eq!(steps.len(), 1 + 225 + 1 + 1);
    assert_eq!(steps[0], (HdmaStep::Setup, 0, 24));
    for (line, step) in (0..225).zip(&steps[1..226]) {
        assert_eq!(*step, (HdmaStep::Line, line, u64::from(line) * 1364 + 1112));
    }
    // None in vertical blank; the next frame sets up again.
    assert_eq!(steps[226], (HdmaStep::Setup, 0, frame + 24));
    assert_eq!(steps[227], (HdmaStep::Line, 0, frame + 1112));
}

// @source: anomie-timing#s-cpu/hdma
// @source: anomie-regs#2133
// @source: adr-0020
// @source: adr-0026
#[test]
fn with_overscan_hdma_transfers_on_lines_0_to_239() {
    let mut timing = Timing::new(Region::Ntsc, Cycles(0));
    timing.write_screen_mode(OVERSCAN, Cycles(0));
    let lines = hdma_steps(&mut timing, 262 * 1364)
        .into_iter()
        .filter(|(step, ..)| *step == HdmaStep::Line)
        .map(|(_, line, _)| line)
        .collect::<Vec<_>>();
    assert_eq!(lines, (0..240).collect::<Vec<_>>());
}
