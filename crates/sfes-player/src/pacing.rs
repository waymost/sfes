//! Fixed-timestep pacing at the video standard's nominal frame rate (ADR 0039, points 1 and 2).

use std::time::{Duration, Instant};

use sfes_snes_core::Region;

/// Master cycles in a scanline (anomie-timing, S-CPU clocks and refresh).
const LINE_CYCLES: u64 = 1364;
/// Scanlines in a non-interlaced NTSC frame.
const NTSC_LINES: u64 = 262;
/// Scanlines in a non-interlaced PAL frame.
const PAL_LINES: u64 = 312;
/// The PAL master clock in Hz, "the best number we have" (anomie-timing).
const PAL_CLOCK: u64 = 21_281_370;
/// The most frames run at one wake-up; a longer stall drops the rest.
pub(crate) const MAX_CATCH_UP: u32 = 4;

/// The wall clock. The player paces frames against it; the machine never sees it, so
/// emulation stays deterministic (FND-02).
#[expect(
    clippy::disallowed_methods,
    reason = "the viewer paces against real time; the machine's results do not depend on it"
)]
pub(crate) fn now() -> Instant {
    Instant::now()
}

/// The nominal time one frame of `region` takes, from the master clock and the frame's cycles.
///
/// - NTSC's clock is 1.89 × 10⁹ / 88 Hz, so a frame takes `cycles × 88 / 1.89` ns, written
///   `cycles × 8800 / 189` to stay in integers.
/// - PAL's is 21 281 370 Hz.
pub(crate) fn frame_period(region: Region) -> Duration {
    let nanoseconds = match region {
        Region::Ntsc => NTSC_LINES * LINE_CYCLES * 8800 / 189,
        Region::Pal => PAL_LINES * LINE_CYCLES * 1_000_000_000 / PAL_CLOCK,
    };
    Duration::from_nanos(nanoseconds)
}

/// Turns elapsed wall time into a whole number of frames to run.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Pacer {
    period: Duration,
    accumulated: Duration,
}

impl Pacer {
    /// A pacer that releases one frame every `period`.
    pub(crate) const fn new(period: Duration) -> Self {
        Self {
            period,
            accumulated: Duration::ZERO,
        }
    }

    /// Adds `elapsed` and returns how many frames to run now: at most [`MAX_CATCH_UP`]. Time
    /// beyond that is dropped, not owed.
    pub(crate) fn advance(&mut self, elapsed: Duration) -> u32 {
        self.accumulated += elapsed;
        let due = self.accumulated.as_nanos() / self.period.as_nanos().max(1);
        let due = u32::try_from(due).unwrap_or(u32::MAX);
        self.accumulated -= self.period * due;
        if due > MAX_CATCH_UP {
            MAX_CATCH_UP
        } else {
            due
        }
    }

    /// How long until the next frame is due.
    pub(crate) fn until_next(&self) -> Duration {
        self.period.saturating_sub(self.accumulated)
    }

    /// Forgets the time gathered so far, so resuming from a pause does not release a burst.
    pub(crate) const fn clear(&mut self) {
        self.accumulated = Duration::ZERO;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_periods_match_the_documents_clocks() {
        // 357 368 cycles at 21.477 MHz is 16.639 ms, 60.0988 Hz.
        assert_eq!(frame_period(Region::Ntsc), Duration::from_nanos(16_639_356));
        // 425 568 cycles at 21.28137 MHz is 19.997 ms, 50.007 Hz.
        assert_eq!(frame_period(Region::Pal), Duration::from_nanos(19_997_208));
    }

    #[test]
    fn whole_periods_release_frames_and_the_remainder_waits() {
        let period = Duration::from_millis(10);
        let mut pacer = Pacer::new(period);
        assert_eq!(pacer.advance(Duration::from_millis(9)), 0);
        assert_eq!(pacer.until_next(), Duration::from_millis(1));
        assert_eq!(pacer.advance(Duration::from_millis(2)), 1);
        assert_eq!(pacer.until_next(), Duration::from_millis(9));
        assert_eq!(pacer.advance(Duration::from_millis(20)), 2);
    }

    #[test]
    fn a_long_stall_runs_a_few_frames_and_drops_the_rest() {
        let mut pacer = Pacer::new(Duration::from_millis(10));
        assert_eq!(pacer.advance(Duration::from_secs(5)), MAX_CATCH_UP);
        assert_eq!(pacer.advance(Duration::ZERO), 0);
    }

    #[test]
    fn clearing_drops_the_time_gathered() {
        let mut pacer = Pacer::new(Duration::from_millis(10));
        assert_eq!(pacer.advance(Duration::from_millis(9)), 0);
        pacer.clear();
        assert_eq!(pacer.advance(Duration::from_millis(9)), 0);
    }
}
