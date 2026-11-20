# 0020. Scanlines, frames, the H/V position and the deadline table on the SNES

- **Status:** accepted
- **Date:** 2026-11-20
- **Confidence:** high for line lengths, line counts and the frame boundary; medium for the long dots of a long line, the field at power-on and reset, when `$2133` is sampled, and overscan changes inside the vertical-blank window

## Context

Issue #81 replaces the fixed lines and frames of ADR 0018, point 6, with the scheduler of the technical design (section 4):
- the H/V position derived from `Cycles`;
- NTSC and PAL line counts, with short and long lines, interlace fields and overscan;
- the deadline table;
- the frame boundary of FND-04;
- `on_scanline` and `on_frame_end`.

The documents agree on line lengths and line counts. They leave these questions open or disagree:
- where the long dots fall;
- which field the first frame after power-on or reset has;
- when the interlace and overscan bits of `$2133` take effect;
- how deadlines due at the same moment are ordered.

## Sources

- `anomie-timing#s-cpu/clocks-and-refresh`:
  - "The SNES runs 1 scanline every 1364 master cycles, except in non-interlace mode scanline $f0 of every other frame (those with $213f.7=1) is only 1360 cycles."
  - "Frames are 262 scanlines in non-interlace mode, while in interlace mode frames with $213f.7=0 are 263 scanlines."
  - "V-Blank" runs from scanline `$E1` or `$F0` to the end of the frame. The section gives no PAL line counts.
- `anomie-timing` (preamble): the SNES "returns to a known timing position on reset". A bit set "at H=X" is seen by a read that would latch X but not by one that would latch X−.5.
- `anomie-timing#s-ppu/rendering`: "There are always 340 dots ('pixels') per scanline; normally dots 323 and 327 are 6 master cycles instead of 4."
- `anomie-timing#s-ppu/blanking-periods`:
  - "V-Blank begins on scanline $E1 or $F0, at H=0, depending on bit 2 of $2133."
  - If the bit is set and then cleared between `$E0` and `$F0`, most start-of-V-Blank events "will trigger at the next appropriate H point".
  - "V-Blank ends at V=0 H=0."
- `anomie-timing#s-ppu/registers`: "$213F bit 7: Toggles every frame, at V=0 H=1."
- `anomie-regs#2133`:
  - bit 0 is screen interlace;
  - bit 2 is overscan: "239 lines will be displayed instead of the normal 224", and V-Blank "will occur that much later";
  - the overscan bit "only matters at the very end of the frame". Turning it off "at the very beginning of scanline X (for 0xE1<=X<=0xF0)" puts the NMI on line X. Turning it on matters only on line `$E1`, which moves the NMI to `$E2`.
- `anomie-regs#213c`: H counts 0–339. V counts 0–261 on NTSC (262 every other frame with interlace) and 0–311 on PAL ("312 in interlace?").
- `anomie-regs#213f`: bit 7, the interlace field, "will toggle every V-Blank".
- `fullsnes#snestimingoscillators`:
  - NTSC frame rate 21.47727 MHz / (262×1364 − 4/2);
  - PAL frame rate 21.28137 MHz / (312×1364);
  - PAL interlace "625*1364+4/2".
- `fullsnes#snestiminghvcounters`:
  - Short line: "60Hz frame rate + interlace=off + field=1 + line=240", 1360 cycles, "all dots are 4-cycles long".
  - Long line: "50Hz frame rate + interlace=on + field=1 + line=311", 1368 cycles, 341 dots (0–340).
  - Normal and long lines have "four dots are 5-cycles long".
  - "the selected picture size, 224 or 239 lines, doesn't matter".
  - Two-frame totals: NTSC (262+262)×1364−4 without interlace and (262+263)×1364 with it; PAL 312×1364 without interlace.
- `fullsnes#snestiminghvevents`:
  - "H=0, V=0, F=0 SNES starts at this time after /RESET".
  - "H=1, V=0 toggle interlace FIELD flag"; "V=225/240 Begin of Vblank Period"; "V=262/312 Extra scanline (occurs only in Interlace Field=0)".
  - The H-counter latch statistics see dots 323 and 327 "6 times" against 4 for other dots, and dot 340 only with "PAL+InterlaceOn".
- `fullsnes#snesiomap`: `$2133` is "00h?" on reset and `$213F` bit 7 is 0.
- Technical design section 4: the deadline table, `next_deadline`, events serviced "in time order", and the frame boundary of catch-up rule 5.
- FND-04, FND-08: every stop takes effect at the first instruction boundary at or after its target; the frame boundary is the first one at or after the first line of vertical blank.
- ADR 0001, points 3 and 6; ADR 0018, point 6; ADR 0019, point 6.

## Decision

1. **Lines.**
   - A line is 1364 master cycles.
   - On NTSC, line 240 is 1360 cycles when interlace is off and the field is 1 (anomie-timing and fullsnes agree).
   - On PAL, line 311 is 1368 cycles when interlace is on and the field is 1 (fullsnes).
   - A line's length is fixed when it starts, from `$2133` bit 0 and the field at that moment.
2. **Frames.**
   - An NTSC frame has lines 0–261, a PAL frame 0–311.
   - With interlace on and field 0 the frame has one more line: 262 on NTSC, 312 on PAL.
   - Whether the extra line runs is decided when the line before it ends, from `$2133` bit 0 then.
   - The fullsnes two-frame totals follow from these rules for both regions, with and without interlace.
3. **Dots.**
   - The H position is the PPU's dot, as `$213C` reports it.
   - Dots are 4 master cycles, except dots 323 and 327, which are 6 on normal and long lines.
   - A short line has 340 dots of 4 cycles.
   - A long line has 341 dots (0–340): the normal line's dots plus dot 340 of 4 cycles.
   - fullsnes's "four dots are 5-cycles long" is not used. anomie-timing's two 6-cycle dots fit fullsnes's own latch statistics, and both add up to 1364 and 1368.
   - The V position is the line.
4. **The field.**
   - `$213F` bit 7 toggles at H=1 of line 0 of every frame, interlaced or not. anomie-timing and fullsnes give this point. anomie-regs's "every V-Blank" is the less precise statement.
   - Power-on and reset start at H=0, V=0 with field 0 (fullsnes). The field therefore becomes 1 at H=1 of the first frame, so the first frame after power-on or reset is field 1.
5. **Vertical blank and overscan.**
   - Vertical blank begins at H=0 of the first line from 225 to 240 that starts while `$2133` bit 2 is clear, and at line 240 if the bit is set throughout. It ends at H=0 of line 0.
   - This follows anomie-regs for turning the bit off inside the window.
   - Turning it on during line 225, which anomie-regs says delays the NMI to line 226, is not modelled: the bit is sampled only at line starts.
6. **`$2133`.**
   - The scheduler keeps its own copy of bits 0 and 2, taken from every CPU write to `$2133` after the PPU's owner sync.
   - Both bits are 0 at power-on and reset (fullsnes).
7. **The frame boundary.**
   - A frame ends when vertical blank begins. The frame counter advances then and `on_frame_end` is called with the new count.
   - `run(Stop::Frames(n))` returns at the first step boundary at or after the *n*-th such moment (FND-04, FND-08).
   - The first frame after power-on is the 225 or 240 lines before the first vertical blank.
   - The frame counter and the count of lines started move from the machine's metadata to the bus's timing, beside the deadlines.
8. **Observer events.**
   - `on_scanline(line, at)` is called at the start of every line, line 0 included, with the time it started. Power-on and reset start line 0 without it.
   - When a line starts vertical blank, `on_frame_end` comes before that line's `on_scanline`, since that line opens the next frame's period.
   - Both are called from the bus access in which the moment passed, not at the step boundary.
9. **The deadline table.**
   - The bus keeps a fixed array of the next time each scheduled event is due: the refresh pause (ADR 0019) and the next line start. Later events (NMI, IRQ, HDMA, auto-joypad) are added to it.
   - `next_deadline` is the earliest.
   - After each CPU cycle the bus compares the clock with `next_deadline` and services due events in time order. Events due at the same time are serviced in the array's order.
   - Servicing a refresh pauses the CPU (ADR 0019); servicing a line start moves the position and reports it.
10. **Stops.** Every `Stop` is checked between steps, so it overshoots its target by at most one instruction or interrupt entry. DMA transfers join the overshoot bound with #83.

## Consequences

- ADR 0018, point 6 is amended: the fixed line and frame lengths and the frame boundary at the end of the last line are replaced by this ADR. Only the absence of NMI and IRQ stays until #82.
- `Stop::Frames` and `on_frame_end` now count vertical blanks. Frame 1 ends after 306 900 master cycles on both standards without overscan, not 357 368 or 425 568.
- The H/V position and the field are ready for `$2137`, `$213C–$213F` and the H/V IRQ in #82. #82 also needs half-dot positions (H=0.5, H=HTIME+3.5), which it adds on top of the dot rule of point 3.
- Point 3's long line rests on fullsnes alone, and its dot 340 on the latch statistics. A test ROM that latches H through a PAL interlaced long line reopens it.
- Point 4's first frame being field 1 rests on reading fullsnes's "F=0" at H=0 literally. A test that reads `$213F` right after reset settles it.
- Point 5's sampling at line starts misses anomie-regs's line-`$E1` case and any mid-line change; a PPU test ROM that probes overscan switching reopens it.
- ADR 0001 stands: no timing test ROM has run against the scheduler yet. The approved timing ROMs (ADR 0014) are run when the PPU and NMI exist.
