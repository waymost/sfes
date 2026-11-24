# 0033. Sprites

- **Status:** accepted
- **Date:** 2026-11-22
- **Amended:** 2026-11-24: cites the OAM reset and renderer timing sections by their full heading paths, `anomie-timing#s-ppu/oam-reset` and `#s-ppu/detailed-renderer-timing`.
- **Confidence:**
  - high: `$2101`'s sizes and base, the OAM record, the character addressing, the palette and the colour 0 transparency, the priority order among sprites and against the backgrounds, and the 32-sprite and 34-tile limits and their flags (points 1 to 7);
  - medium: the line a sprite first appears on, which tiles are dropped by time-over, and when the flags clear (points 3, 6 and 7);
  - low: what happens in forced blank, the rotation's "major oddity", and the power-on values (points 1, 7 and 8).

## Context

Issue #94 draws the 128 sprites: `$2101 OBSEL`'s sizes and base, the OAM layout, the priority rotation of `$2103`, the per-line evaluation with its limits and the two flags in `$213E`. ADR 0029 already holds OAM, its port and the address reload, and left `$213E` bits 7 and 6 at 0 until this ADR. ADR 0030 and ADR 0032 left a place for sprites in each mode's priority order. Windows (#95), mosaic (#96), colour math and the sub screen are later issues.

The documents agree on the OAM record and the limits. They are silent or loose on:

- the output line a given Y first appears on;
- which tiles fall off when there are more than 34;
- what the evaluation does in forced blank, and the power-on values;
- how the priority rotation reads the internal OAM address.

## Sources

- **anomie-regs#2101**: `sssnnbbb`: `sss` picks two sizes, 8x8 and 16x16, 8x8 and 32x32, 8x8 and 64x64, 16x16 and 32x32, 16x16 and 64x64, 32x32 and 64x64, then "undocumented" 16x32 and 32x64, and 16x32 and 32x32. `nn` is the name select and `bbb` the name base, "Addr>>14".
- **anomie-regs#sprites**: the low table has 4 bytes per sprite, `xxxxxxxx`, `yyyyyyyy`, `cccccccc`, `vhoopppN`; the high table has 2 bits per sprite in byte `OBJ/4`, bit `0/2/4/6` the X bit 8 and bit `1/3/5/7` the size flag. Y 0 to 239 is on screen and -63 to -1 is off the top, "a really big sprite can go off the bottom and come back in the top". The flips turn the whole sprite, and a rectangular sprite is flipped vertically "as if they were two square sprites" (rows `01234567` become `32107654`). The palette starts at CGRAM 128 + `ppp`*16, the first colour is transparent. Tiles are in two 16x16 tile tables that wrap without carries; the word address of a sprite's first tile is `((Base<<13) + (cccccccc<<4) + (N ? ((Name+1)<<12) : 0)) & 0x7fff`. Priority among sprites: `$2103` bit 7 clear makes sprite 0 first; set, "take the current internal OAM word address ... and give priority to the sprite number (OAMAddr&0xFE)>>1". FirstSprite is above FirstSprite+1 and so on, wrapping at 127; only the topmost sprite's OAM priority is compared with the backgrounds. The drawing steps: an X of 256 (or -256) counts as 0 for Range and Time; Range takes the first 32 sprites from FirstSprite with -size < X < 256 and sets `$213E` bit 6 beyond that; Time "starting with the last sprite in Range, load up to 34 8x8 tiles (from left-to-right, after flipping)", counting only tiles with -8 < X < 256, and sets bit 7 beyond 34; the tile keeps its true X, palette and priority for drawing.
- **anomie-regs#2103**: `p` is the priority activation bit and `b aaaaaaaa` the OAM word address.
- **anomie-regs#213e**: bit 7 is time over, bit 6 range over, "set whether or not OBJ are actually enabled", reset at the end of V-Blank.
- **anomie-regs#backgrounds**: the priority lists with the sprites, mode by mode, in front to back order; "the SNES loads OBJ data for each scanline during the previous scanline", so the first line (line 0) is not output and games set their vertical scrolls to -1 to make a background line match a sprite's Y.
- **anomie-regs#rendering-the-screen/rendering-the-screen**: sprites are one of the layers composed per pixel; the sprite layer is enabled by `$212C` bit 4.
- **anomie-timing#s-ppu/detailed-renderer-timing**: OAM is examined during a line for the 32 sprites of the next, and 34 tiles are loaded in horizontal blank, which is why line 0 exists, "otherwise there would be no sprite data for the first scanline on the screen".
- **anomie-timing#s-ppu/oam-reset**: the internal OAM address is reset at the start of vertical blank.
- **fullsnes#snesppuspritesobjs**: `$2101` as above, with bits 4 and 3 the gap between the two tables in 4K-word steps and bits 2 to 0 the base in 8K-word steps; 8x8 tiles in a "two-dimensional array of 16x32 OBJ tiles", with "no carry-outs" from x to y or y to the next; a 64-pixel-high sprite "may wrap from lower to upper screen border". The OAM record, and attribute bit 7 vertical flip, bit 6 horizontal flip, bits 5 and 4 priority, bits 3 to 1 palette, bit 0 the tile's bit 8; OBJ palettes 4 to 7 use colour math.
- **fullsnes#snespputimersandstatus**: `$213E` bit 7 is "more than 8x34 OBJ pixels per scanline", bit 6 more than 32 OBJs; "the overflow flags are cleared at end of V-Blank, but NOT during forced blank"; they are set regardless of `$212C`. Bit 6 is set when `V=OBJ.YLOC`, `H=OAM.INDEX*2`, and bit 7 when `V=OBJ.YLOC+1`, `H=0`.
- **fullsnes#snesiomap**: `$2101` to `$2103` are "?" at power-on.
- **fullsnes#snesppuvideomemoryvram**: 4-colour-bit tile data, as for the backgrounds.
- **ADR 0028**: the first output line is line 1. **ADR 0029**: OAM and its port; `$213E`'s bit 4 and version; point 9 left bits 7 and 6 for this ADR. **ADR 0030**: how a pixel and a priority are chosen. **ADR 0032**: EXTBG and mode 7 have a place for sprites.

Where the sources differ or say too little:

- anomie's rows are "Y" and its limit is "-size < X < 256"; fullsnes says the Y flag is set at `V=YLOC+1`, the line the sprite is first on, which with anomie's "line 0 is not output" gives a first line of `Y + 1`. Neither document states the formula; this is what they imply together.
- anomie's Time step loads from the last sprite in Range. It does not say which tiles are lost beyond 34; the loading order is the only thing it gives, so the tiles after the 34th in that order are the ones lost.
- fullsnes says the flags are not cleared in forced blank. Neither says whether evaluation runs in forced blank.
- Neither gives `$2101`'s power-on value.
- anomie's rotation text has a "major oddity" in which the sprite with priority depends on how many bytes were written and on the line. Its description is conjectural, and it is not modelled (point 6).
- The documents give the two undocumented sizes 6 and 7 identically.

## Decision

1. **Registers.** `$2101 OBSEL` is a plain byte, 0 at power-on and kept on reset. Its bits 7 to 5 choose the sizes of small and large sprites as the table in anomie-regs#2101 gives them (the pairs `8x8/16x16`, `8x8/32x32`, `8x8/64x64`, `16x16/32x32`, `16x16/64x64`, `32x32/64x64`, `16x32/32x64`, `16x32/32x32`); bits 4 and 3 are the name select and bits 2 to 0 the base. `$2103` and the OAM port are as in ADR 0029.
2. **The OAM record.** Sprite `index` has the low-table bytes at `index*4` (X bits 7 to 0, Y, tile bits 7 to 0, attributes) and the high-table bits in byte `0x200 + index/4`, at bit `(index%4)*2` (X bit 8) and the bit above it (size, 0 small and 1 large). X is a 9-bit two's complement number, -256 to 255; the tile is `cccccccc`, the name bit is attribute bit 0, the palette bits 3 to 1, the priority bits 5 and 4, and bits 6 and 7 are the horizontal and vertical flips.
3. **Which lines a sprite is on.** For output line `L` (the first is 1, ADR 0028) a sprite of height `H` and Y `Y` is on the line if `row = (L - 1 - Y) mod 256` is below `H`. A sprite with Y 0 is first on line 1, a sprite with Y 255 shows its row 1 on line 1, and a tall sprite wraps from the bottom of the screen to its top. Interlace is not modelled.
4. **The tiles of a sprite.** A sprite of width `W` is `W/8` tile columns across and `H/8` rows down. A vertical flip maps `row` to `(row / S)*S + (S - 1 - row mod S)` where `S` is the smaller of `W` and `H`, which is the whole sprite when it is square and each square half when it is not. A horizontal flip reverses the columns, and the pixels of each tile. The tile in column `c` and row `r` (after the flips) is `(((cccccccc >> 4) + r) & 15) << 4 | (((cccccccc & 15) + c) & 15)`; its first word is `((base << 13) + (tile << 4) + (name bit ? ((name + 1) << 12) : 0)) & 0x7FFF`; the pixel row is word `first + (row mod 8)` for planes 0 and 1 and `first + 8 + (row mod 8)` for planes 2 and 3, the same 4-bit encoding as the backgrounds (ADR 0030). A pixel of 0 is transparent; otherwise the colour is CGRAM entry `128 + palette*16 + pixel`. Sprites are never in direct colour.
5. **Range and Time.** For each line, the sprites are taken in priority order from the first sprite (point 6), and a sprite is *in range* when it is on the line and its X, with -256 read as 0, is greater than minus its width and below 256.
   - Range: the first 32 sprites in range are kept; a 33rd sets `$213E` bit 6.
   - Time: the kept sprites are taken from the last to the first and each one's tiles from the left of the screen to the right after flipping. A tile is counted if its X, the sprite's (with -256 read as 0) plus 8 times its column, is greater than -8 and below 256. The first 34 counted tiles are loaded; a 35th sets `$213E` bit 7. A loaded tile keeps the sprite's true X, so a sprite at -256 uses time but draws nothing.
   - Evaluation always runs, whether `$212C` enables the sprites or not, and before any composition (EMU-06).
6. **Priority.** The first sprite is sprite 0 when `$2103` bit 7 is clear. When it is set, it is the sprite `(OAM byte address >> 2) & 127`, from the internal OAM address as the port holds it. The "major oddity" of anomie's rotation text is not modelled. Among the loaded tiles, the sprite nearer the first sprite (wrapping 127 to 0) is in front, and for one pixel the topmost sprite with a non-transparent pixel gives the colour and the priority `oo` (0 to 3). The sprite layer is in each mode's order at its place in the list of anomie-regs#backgrounds, which includes modes 5 and 6; it appears when `$212C` bit 4 is set.
7. **The flags.** Bit 7 of `$213E` is time over and bit 6 range over; each is set by the line's evaluation, which runs when the line is rendered, not at the horizontal position fullsnes gives. They are cleared when line 0 starts, unless forced blank is on at that moment. Evaluation also runs in forced blank. Reads of `$213E` return them above the open-bus bit and the version, as ADR 0029 point 9 has it.
8. **Not yet.** Windows (#95), mosaic (#96), colour math and the sub screen, and the oddity of point 6. The sprite layer is the main screen's alone.

## Consequences

- Evaluation is a separate step from drawing, so a later render skip can run it alone while skipping the pixels (EMU-06).
- The first line of a sprite (point 3) rests on two documents implying it together, not on a statement. A frame compared with an oracle would settle it, as would a test of the Time rule's lost tiles (point 5), where the literal order may drop a high-priority sprite's tiles.
- The flags are set per line at the start of the line's evaluation in this design, where the hardware sets them at positions in the previous line; a program that reads `$213E` within a line sees the difference only at that line's boundary.
- Modes 5 and 6 draw sprites over a backdrop until #97, each sprite pixel doubled in the 512-pixel line.
- ADR 0029 point 9 held bits 7 and 6 of `$213E` at 0 until this issue; point 7 sets them.
