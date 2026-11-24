# 0036. Hi-res, pseudo-hi-res and interlace

- **Status:** accepted
- **Date:** 2026-11-23
- **Confidence:**
  - high: that a wide line's even half-pixels are the sub screen's and its odd ones the main screen's, that a mode 5 or 6 tile is two characters wide and shows the even columns on the sub screen and the odd on the main, and that mosaic in modes 5 and 6 acts on half-pixels (points 1 to 4 and 7);
  - medium: the half-pixel mosaic block rule, and how hi-res math chooses its operation (points 5 and 7);
  - low: which half-lines a field shows, flipping a 16-pixel tile whole, the window and math state at column 0, the hi-res sub backdrop, and mode 6's tile height (points 3, 5, 6 and 9).

## Context

Issue #97 adds BG modes 5 and 6, pseudo-hi-res (`$2133` bit 3) and screen interlace (`$2133` bit 0). Earlier ADRs left four things to it:

- ADR 0028, point 7: a wide line, and an interlaced one, are recorded but every pixel is the backdrop;
- ADR 0030, point 7: "interlace's extra row (anomie's 'add 2')";
- ADR 0031, point 6: mode 6's offset-per-tile and its 8-half-pixel tile;
- ADR 0035, point 9: math on the sub pixels of a wide line, the doubled mosaic block of modes 5 and 6, and the sub screen's pixels as the even half-pixels of pseudo-hi-res. ADR 0033 keeps sprites over a backdrop in modes 5 and 6 until now.

The documents describe the pictures well and leave the corners open: they differ on the sub backdrop in hi-res, say little of interlace's row arithmetic, and call several edge cases unknown.

## Sources

- **anomie-regs#2133**: bit 3 "creates a 512-pixel horizontal resolution by taking pixels from the subscreen for the even-numbered pixels (zero based) and from the main screen for the odd-numbered pixels. Color math behaves just as with Mode 5/6 hires. The interlace bit still has no effect. Mosaic operates as normal (not like Mode 5/6). The 'subscreen' pixel is clipped (by windows) when the main-screen pixel to the LEFT is clipped, not when the one to the RIGHT is clipped as you'd expect. What happens with pixel column 0 is unknown." Enabling it in modes 5 or 6 "has no effect". Bit 0, interlace: in BG mode 5 (and probably 6) the screen is 448 (478) high; "in any other mode, the screen will just get a bit jumpy". Bit 1 makes OBJ interlaced (half height).
- **fullsnes#snesppucontrol** (SETINI): bit 3 "shift subscreen half dot to the left"; bit 0 interlace; bit 1 OBJ resolution.
- **anomie-regs#backgrounds/mode-5**: one 16-colour BG and one 4-colour BG. Mode 5 "always takes a 16 pixel wide tile (the height may still be 8 or 16) and only uses half the pixels (zero-based, the even pixels for subscreen tiles and the odd pixels for mainscreen tiles). Then it forces pseudo-hires on". Interlace makes the screen 448 or 478 half-lines, "either the odd half-lines or the even half-lines are drawn each frame, as indicated by bit 7 of `$213F`". `$212C` and `$212D` should be set alike. The priority order is the table `priority_order` holds.
- **anomie-regs#backgrounds/mode-6**: one 16-colour BG; "the same oddities as Mode 5"; an offset-per-tile mode "just like Mode 2", "Mode 6 always uses 8 pixel (16 half-pixel) wide tiles, this applies to BG3 as well as BG1. You can't apply the offset to an 8-half-pixel tile nor to a 16-pixel wide area".
- **fullsnes#snesppubgcontrol** (BGMODE): in mode 5 "8x8 acts as 16x8"; mode 6 "fixed 16x8?"; modes 5 and 6 are "512-pix-hires".
- **anomie-regs#2105**: the BG mode and the tile size bits.
- **anomie-regs#backgrounds/bg-scrolling**: "Horizontal scrolling scrolls in units of full pixels no matter if we're rendering a 256-pixel wide screen or a 512-half-pixel wide screen. However, vertical scrolling will move in half-line increments if interlace mode is active"; an interlace screen "needs -2 rather than -1" to correct for the missing line 0, "an emulator would need to add 2 instead of 1".
- **anomie-regs#rendering-the-screen/mosaic**: modes 5 and 6 "use a 2XxX block of half-pixels. Similarly, Modes 5/6 interlaced use a 2Xx2X block of half-pixels"; `$2106 = $0F` expands the even half-pixels to cover the odd ones, "`$1F` would cover the next even-and-odd pixel over as well"; and the example of a red BG1 and a blue BG2 pixel at pixel 0, BG1 on main and BG2 on sub: nothing but blue, then both at `$03`, and "BRBR" on two lines at `$13`. Also `#2106`: size 0 "has a visible effect" in modes 5 and 6, "the same thing happens vertically with interlace mode".
- **anomie-regs#rendering-the-screen/color-math** and **#windows**: "In hires modes, color math is applied to the visible subscreen pixels as well": look at the previous main-screen pixel (pixel 5 for the sub pixel at 6); if no math was applied to it, none is applied here; if the fixed colour was added or subtracted, so it is here; if a sub pixel was added or subtracted, "add/subtract that main-screen pixel (the original value before math)". The colour window effect on a sub pixel follows the previous main-screen pixel. What happens at the left edge "is unknown". Cyan on main and magenta on sub subtract to alternating green and red. A block alone in the backdrop with half set shows "a bright line at the left edge where the fixed color was subtracted from the subscreen pixel and no 1/2 was applied".
- **anomie-regs#rendering-the-screen/rendering-the-screen**: hi-res modes "should process the visible subscreen pixels as described".
- **fullsnes#snesppuresolution** ("Hires Notes"): horizontal scrolling counts full pixels; vertical scrolling half-pixels "(only when using true hv-hires)"; mosaic counts full pixels, 1x1 acting as 2x1 half-pixels in true h-hires "and as 2x2 half-pixels in true hv-hires mode, reportedly?" and 1x2 in pseudo v-hires "presumably?"; "both main/subscreen pixels are forced to use Color 0 as backdrop color (rather than using COLDATA setting as subscreen backdrop) (this applies for both true+pseudo hires)"; window coordinates and OBJ coordinates count full pixels. Its pictures of the true, pseudo, vertical and OBJ hi-res are missing from the copy.
- **fullsnes#snespputimersandstatus** and **anomie-regs#213f**: bit 7 of `$213F` is the interlace field.
- **ADR 0028**: line 1 is the first output line; a wide line is 512 pixels; an interlaced frame's lines each fill two rows and show the frame's own field. **ADR 0030 to 0035**: the layers, scroll, sprites, windows, math and mosaic.

Where the sources differ or say too little:

- Fullsnes forces the hi-res sub backdrop to colour 0; anomie's example subtracts the fixed colour where the sub screen shows the backdrop, as any sub backdrop does.
- Nothing says which of the two half-lines is the first field's, or which tile column a flipped 16-pixel tile puts on which half.
- Mode 6's tile height: anomie says the sizes work as in mode 5, fullsnes "fixed 16x8?".
- The column 0 cases are unknown, as the pseudo-hi-res note says.
- Interlace with sprites (`$2133` bit 1) has no rule beyond "half-height".

## Decision

1. **Wide lines.** A line is wide in BG mode 5 or 6, or with `$2133` bit 3 (as ADR 0028, point 7). Column `x` of the picture becomes two pixels: pixel `2x` is the sub screen's and pixel `2x + 1` the main screen's. In modes 5 and 6 setting bit 3 changes nothing. The sub screen is drawn whenever the line is wide, not only when math adds it. Window coordinates and sprite columns stay in full pixels.
2. **The sub screen of a wide line.** Its front pixel is chosen as ADR 0035, point 3 says, from `$212D` and `$212F`, but where it shows its backdrop the pixel is CGRAM entry 0, not the fixed colour (fullsnes). Where a main pixel's math adds the sub screen and the sub screen shows its backdrop, the operand is still the fixed colour with no half step (anomie's bright edge): the sub backdrop is entry 0 when it is seen and the fixed colour when it is added.
3. **Modes 5 and 6: the tile.** Depths: mode 5 BG1 four planes, BG2 two; mode 6 BG1 four planes. A tile is 8 full pixels, 16 half-pixels, wide whatever the size bit; the size bit makes it 8 or 16 rows high (anomie's mode 5 and "same oddities" for mode 6; fullsnes's "fixed 16x8?" is not taken). A tile's characters are its tile `n` on the left and `n + 1` on the right, and with 16 rows the two below, `n + 16` and `n + 17`, as in a 16 × 16 tile. The pixel at column `x` of the main screen is half-pixel `2 * ((x + HOFS) mod 8) + 1` of its tile, the sub screen's is `2 * ((x + HOFS) mod 8)`: the odd and even columns of the 16. The flips act on the whole 16-half-pixel tile, so a horizontal flip swaps which columns the two screens show. HOFS counts full pixels and wraps at the playfield's width, 8 pixels for each of its tiles. Palettes are as ADR 0030, point 5.
4. **Interlace.** With `$2133` bit 0 in mode 5 or 6, the row a line shows is `2L + F + VOFS` (L the line, 1 for the first, as ADR 0030, point 7): the vertical scroll counts half-lines and the line is one of two half-lines, `F` being 0 for the first field and 1 for the second (`$213F` bit 7, the PPU's signals). So `VOFS = -2` shows row 0 on the first line of the first field. In other modes interlace changes no pixel, as anomie says. The row is taken before the tile's size, so a tile is 8 or 16 half-lines high. A line's pixels fill two rows of the frame, as ADR 0028, point 8 has it.
5. **Mosaic.** In modes 5 and 6, a pixelated background takes the half-pixel at the top-left of its block: the sub screen's half of its column, so the even half-pixel for both screens, with the block's columns as ADR 0035, point 8. With interlace it takes the even half-line, the first field's, of the block's top row (`2 * row`, not `2 * row + F`). So the block is `2X` half-pixels across by `X` lines, and `2X` half-lines by `X` lines in interlace. Size 1 has this effect, as anomie says. Pseudo-hi-res and interlace in other modes pixelate as in ADR 0035.
6. **Math on a wide line's main pixels** is ADR 0035, points 4 to 6, unchanged, with the sub screen of point 2 above.
7. **Math on its sub pixels.** The sub pixel at column `x` (output pixel `2x`) looks at the previous main pixel, column `x - 1`:
   - if that pixel was clipped by the colour window, the sub pixel is clipped to black, and is not halved;
   - if math was not applied to it, none is applied;
   - if it took the fixed colour, or the sub backdrop (as a fixed colour), the sub pixel takes the fixed colour, halved only as that pixel was;
   - if it took a sub pixel, the sub pixel takes that main pixel's colour, as it stood after clipping and before math, halved as that pixel was.

   The operation, add or subtract, half or not, and the clamp are the same as that main pixel's. The sub pixel's layers are masked by `$212F` at column `x`. At column 0, where there is no previous pixel, nothing is clipped and no math is applied.
8. **Brightness and forced blank** apply to every pixel of the line.
9. **Mode 6's offset-per-tile.** As mode 2 (ADR 0031): BG3 supplies two entries per 8-pixel column, horizontal at row `BG3VOFS` and vertical at `BG3VOFS + 8`, and BG1's valid bit is bit 13. In mode 6 a BG3 entry is 8 full pixels wide, whatever BG3's size bit says, and as high as its size bit. BG3 is not drawn.
10. **Not modelled.** OBJ interlace (`$2133` bit 1), and fullsnes's "odd glitch" in window edges in hi-res: no source gives the rule, so sprites are drawn as in ADR 0033.
11. **Tests.** The renderer has unit tests of the tile, the two screens, the mosaic block, the interlaced row, math on sub pixels and mode 6's offsets, and one cartridge, `tests/carts/hires`, compared with values computed from the documents. There is no golden: the frame would be unverified until an oracle run (#100).

## Consequences

- Points 3 (the flip), 4 (which half-line is first, whether the field alters a mosaic block) and 7 (the first column, and the colour of a sub pixel that took a main pixel) rest on one or no sentence each; a frame compared with an oracle would settle them.
- A wide line's cost doubles: both screens are fetched for each column.
- Mode 6's OBJ and sprites, and OBJ interlace, are drawn as before: the sprite pixels are the same on both screens and in both fields.
- ADR 0028 is unchanged: frame geometry, hashing and the field remain as it says.
