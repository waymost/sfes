# 0035. Colour math, brightness and mosaic

- **Status:** accepted
- **Date:** 2026-11-23
- **Confidence:**
  - high: the register layouts, the add and subtract with clamping, the colour window's four cases, the layer and palette rules for math, and forced blank (points 1 to 6 except the rounding);
  - medium: that half is not applied to a clipped pixel or to the fixed colour as sub backdrop; the mosaic block geometry (point 8);
  - low: the brightness rounding (point 2), the power-on values (point 1), and which mosaic writes restart the blocks (point 8).

## Context

Issue #96 adds the sub screen (`$212D TS`, `$212F TSW`), colour math (`$2130 CGWSEL`, `$2131 CGADSUB`, `$2132 COLDATA`), brightness and forced blank (`$2100 INIDISP`) and mosaic (`$2106 MOSAIC`). ADR 0030 to ADR 0034 render the main screen only and leave all of these out. ADR 0034 keeps the colour window's settings (`$2125` bits 7 to 4, `$212B` bits 3 and 2) for this work to read.

The documents agree on the shape. They differ on:

- how the two fields of `$2130` read (anomie's "never, outside, inside, always" and fullsnes's reversed sense for the math field);
- the order of the half step and the sign of the result;
- the brightness formula;
- when mosaic blocks restart.

## Sources

- **anomie-regs#2100**: `x---bbbb`, force blank on when set; brightness "F=max, 0=off". Force blank can be cleared mid-line with glitched graphics.
- **fullsnes#snesppucontrol**: `$2100` bit 3 to 0: "0=Screen Black, or N=1..15: Brightness*(N+1)/16"; forced blank gives a black picture while sync continues. `$212C/$212D`: bits 4 to 0 are OBJ and BG4 to BG1; "Backdrop (Always enabled)"; the sub screen is used only for colour math and 512-pixel hires.
- **anomie-regs#212c and #212d**: `---o4321` enable the layers on the main (sub) screen.
- **anomie-regs#2130**: `ccmm--sd`. `cc` clips colours to black before math and `mm` prevents math, each `00` never, `01` outside the colour window only, `10` inside only, `11` always. `s` adds the sub screen instead of the fixed colour; `d` is direct colour.
- **anomie-regs#2131**: `shbo4321`. `s` subtract, `h` half ("divided by 2, except when `$2130` bit 1 is set and the fixed color is used, or when color is clipped"); `4/3/2/1/o/b` enable math on BG4, BG3, BG2, BG1, OBJ and the backdrop.
- **anomie-regs#2132**: `bgrccccc`: the plane bits choose which of blue, green and red take the intensity.
- **anomie-regs#rendering-the-screen/color-math**: sprites with palettes 0 to 3 never take part; the eight operations (add or subtract, half or not, fixed colour or sub screen). "Add the subscreen pixel and divide by 2 (unless CW etc), or add the fixed color with no division." Hires modes apply math to the visible sub pixels using the previous main pixel.
- **anomie-regs#rendering-the-screen/rendering-the-screen**: find the first layer enabled on main, not clipped, with a non-transparent pixel, bottoming out on the backdrop; if the colour window clips colours here set the pixel's colour to 0; if math applies and the colour window does not clip math, do math.
- **anomie-regs#windows**: the colour window clips colours to black before math and prevents math; both "never, always, inside … or outside".
- **fullsnes#snesppucolormath**: `$2130` bits 7 to 6 "Force Main Screen Black" (3 always, 2 inside the math window, 1 outside, 0 never) and 5 to 4 "Color Math Enable" (0 always, 1 inside the window, 2 outside, 3 never); bit 1 "Sub Screen BG/OBJ Enable (0=No/Backdrop only)". Add and subtract per channel, optionally halved, "and are then saturated to Max=31 and Min=0". The sub backdrop is the `$2132` colour and is "not divided" when the pixel is the backdrop with bit 1 set. A forced-black main screen can still take additions, with no halving; subtraction then has no effect. Math happens only when the front-most main pixel's layer is enabled in `$2131`. Whether the sub pixel must have "same or higher (XXX or … lower …) priority" is unsettled in the document.
- **anomie-regs#2106 and #rendering-the-screen/mosaic**: `xxxxDCBA`: size `xxxx+1` (0 is 1x1, F is 16x16), A to D BG1 to BG4. The block's top is the "starting scanline": the line `$2106` was written, or the first visible line of the frame; "XXX: … which changes do reset it?". Blocks are "after scrolling, but before any clip windows, color windows, or math". Modes 5 and 6 double the factor. In mode 7, BG1 is as normal and EXTBG's BG2 uses bit 0 for vertical and bit 1 for horizontal mosaic.
- **fullsnes#snesppubgcontrol** (MOSAIC): the first block is at the top-left of the screen; "When changing the mosaic size mid-frame, the hardware does first finish current block (using the old vertical size) before applying the new vertical size".
- **fullsnes#snesiomap**: `$2100` is "8xh" at power-on (ADR 0029, point 2); `$2106`, `$212D`, `$212F`, `$2130`, `$2131` and `$2132` are "?".
- **ADR 0028**: line 1 is the first output line. **ADR 0030, 0032, 0033, 0034**: the layers, mode 7, sprites and windows.

Where the sources differ or say too little:

- `$2130`'s two fields: fullsnes numbers the math field the other way round and says "Enable", where anomie says "Prevent". Read as "prevent where": fullsnes's 1 ("enabled inside the window") is anomie's 01 ("prevented outside"), its 2 is anomie's 10 and its 3 is anomie's 11. They agree; the table in point 4 is anomie's.
- Fullsnes's brightness formula gives `(N+1)/16` for `N=0`, against its own "Screen Black" and anomie's "off". Neither gives a rounding.
- Whether the half step comes before the clamp for a subtraction does not change a result, since the clamp at 0 and the shift agree. The documents round a half down (an integer divide by 2); neither says so.
- Fullsnes says the block's vertical size is the old size until it finishes; anomie says the blocks restart at the line written. Neither says what restarts them.
- Neither says whether the sub pixel must be in front of the main pixel for math.
- No power-on values.

## Decision

1. **Registers.** `$212D TS` and `$212F TSW` keep their low 5 bits; `$2130 CGWSEL`, `$2131 CGADSUB` and `$2106 MOSAIC` are plain bytes; `$2132 COLDATA` sets the fixed colour's red, green and blue channels: each of bits 5, 6 and 7 that is set puts the 5-bit intensity in bits 4 to 0 into that channel, the others keep theirs. All are 0 at power-on (the fixed colour black) and kept on reset. `$212C` keeps what it kept.
2. **Brightness and forced blank.** With `$2100` bit 7 set every pixel of the line is black. Otherwise each channel `c` of every output pixel becomes `c * (N + 1) / 16`, rounded down, with `N` the brightness in bits 3 to 0, and `N = 0` is black. Forced blank and brightness apply after math. A mid-line change is not modelled (lines are rendered whole).
3. **The two screens.** The main screen's pixel is the first, front to back in ADR 0030's order, of the layers enabled in `$212C` and not masked by `$212E`, else the backdrop (CGRAM entry 0). The sub screen's is the same with `$212D` and `$212F`, else the sub backdrop, the fixed colour. The sub screen is only looked at when math adds it (point 6).
4. **The colour window and `$2130`.** The colour window's area at a column is `Windows::area` of the colour setting from ADR 0034. A region field `00`, `01`, `10`, `11` of `$2130` applies where: never; the area is false; the area is true; always. Bits 7 to 6 clip the main pixel to black; bits 5 to 4 prevent math.
5. **Which pixels take math.** A pixel takes math when it is not prevented and its layer is enabled in `$2131`: BG1 to BG4 in bits 0 to 3, OBJ in bit 4 and the backdrop in bit 5. A sprite whose palette is 0 to 3 (CGRAM entries 128 to 191) never does. A clipped pixel (black) still takes math.
6. **The operation.** With `$2130` bit 1 clear the other operand is the fixed colour; with it set, the sub screen's pixel, which is the fixed colour where the sub screen shows its backdrop. Per channel, `$2131` bit 7 clear adds and set subtracts, the result is halved (rounded down) when bit 6 is set, then clamped to 0 to 31. The half step is skipped for a clipped pixel and, with `$2130` bit 1 set, when the operand is the sub backdrop.
7. **Not looked at.** The sub pixel's priority against the main pixel does not matter: fullsnes marks it unsettled and anomie does not mention it.
8. **Mosaic.** Mosaic applies only to backgrounds. Size is bits 7 to 4 plus 1; BG1 to BG4 are bits 0 to 3. A mosaiced background shows, at column `x` of line `y`, the pixel the background has at the top-left of its block: column `x - x mod size`, and row `y - (y - start) mod size`, with `start` the first line of the block rows. Windows, math and the layer order act on the result as they do on any other pixel. `start` is 1 at the start of every frame, and a write to `$2106` that changes its value sets it to the next line to render. In mode 7, BG1 uses bit 0 as above; EXTBG's BG2 takes its horizontal block from bit 1 and its vertical block from bit 0.
9. **Not yet.** Math on the sub pixels of a hires line, the doubled block size of modes 5 and 6, and the sub screen's pixels as the even half-pixels of pseudo-hires are #97's: until then a wide line writes each main-screen pixel twice, as before. Interlace does not change mosaic.

## Consequences

- The fixed colour is the sub backdrop whether or not it is black; a game that wants a fully transparent sub backdrop sets it to black, as fullsnes says.
- Whether brightness rounds down, and what a mosaic write that changes only the size does to the blocks, rest on silence or on a conflict between the documents. A frame compared with an oracle (#97 and the later oracle runs) would settle them.
- Rendering stays a function of the registers on each line, so HDMA changes between lines work.
