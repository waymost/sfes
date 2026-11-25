# 0039. Player frame pacing and display scaling

- **Status:** accepted
- **Date:** 2026-11-24
- **Confidence:**
  - medium: the nominal frame periods (points 1 and 2), which follow from two documents' clocks and line counts but ignore the short and long lines;
  - none claimed: the display scaling and the 8:7 option (points 3 to 5). They are presentation choices of a development viewer, not claims about the hardware.

## Context

Issue #99 turns `sfes-player` into the development viewer (PLY-01): a window that paces the machine at the region's frame rate and draws `VideoFrame`s at integer scale. Technical design section 11 asks for "fixed-timestep pacing at the region's frame rate (about 60.1 Hz NTSC, 50 Hz PAL)" and "the correct aspect ratio", without numbers. ADR 0028 decided that a 512-wide or interlaced frame is presented at the size of the logical 256 × 224 picture, and left how it is drawn to the player.

## Sources

- **anomie-timing#s-cpu/clocks-and-refresh**: the master clock "runs at about 21.477MHz NTSC (theoretically 1.89e9/88 Hz). The best number we have for PAL is 21.28137MHz." A scanline is 1364 master cycles, except that in non-interlace mode scanline `$f0` of every other frame is 1360. A frame is 262 scanlines in non-interlace mode; in interlace mode frames with `$213F` bit 7 = 0 have 263.
- **docs/chips/core.md** (the scheduler section): a PAL frame has 312 lines, 313 in the long interlace field (`anomie-regs#213c`).
- **Technical design, section 11**, **feature specification PLY-01 and PLY-03**: as in Context.
- **ADR 0028, point 8**: the frame geometry the player receives.
- No registered source gives a pixel aspect ratio. fullsnes and the two anomie documents were searched; none states one.

## Decision

1. **Nominal frame period.** The player paces at the non-interlaced, whole-line frame length of the machine's video standard (`Snes::video_standard()`): 262 × 1364 = 357 368 master cycles on NTSC and 312 × 1364 = 425 568 on PAL. With the clocks above this is about 60.0988 Hz and 50.007 Hz. The period in nanoseconds is computed with integer arithmetic only: NTSC as `cycles × 8800 / 189` (the clock is 1.89 × 10⁹ / 88 Hz), PAL as `cycles × 10⁹ / 21 281 370`. Interlace's extra line and the 4-cycle short line change a frame by under 0.4% and 0.002%, which is below what a viewer can show.
2. **Fixed timestep.** The player keeps an accumulator of elapsed time and runs one emulated frame per period, at most 4 per wake-up, dropping any further backlog, so a stall does not make the player race to catch up. Pacing is independent of the display's refresh rate.
3. **Logical picture.** The displayed picture is the frame's logical size: width 256, height `height / 2` for an interlaced frame (`VideoFrame::field` is `Some`) and `height` otherwise. A 512-wide or interlaced frame is drawn over the same area as a 256-wide, non-interlaced one.
4. **Square pixels by default.** The picture is drawn with square pixels at the largest integer scale at which the logical picture fits the window; the rest of the window is black. Every source pixel is then an identical block, which matches the framebuffer and the goldens pixel for pixel.
5. **`--aspect 8:7`.** An option stretches the picture horizontally by 8/7 for a TV-like view. The ratio is a project choice, as no source gives it. With it, the integer scale applies to the logical height and the width is `scale × logical_width × 8 / 7`, rounded down, so rows stay uniform and columns may differ by a device pixel.

## Consequences

- The player's pacing differs from the hardware by at most the interlace and short-line effects above. A viewer that needs exact frame lengths, such as movie playback at M5, uses the machine's own count of frames, not wall time.
- The default view is not what a television showed. PLY-03's "correct aspect ratio" is met by `--aspect 8:7`, whose default may be revisited at M6 if a source for the ratio is registered.
- Technical design section 11 and feature specification PLY-01 now name these rules (ADR 0039).
- To revisit: a source for the pixel aspect; audio at M3, where dynamic rate control may want the real frame length instead.
