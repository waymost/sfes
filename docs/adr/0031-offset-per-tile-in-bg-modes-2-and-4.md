# 0031. Offset-per-tile in BG modes 2 and 4

- **Status:** accepted
- **Date:** 2026-11-22
- **Confidence:**
  - high: which tiles of BG1 and BG2 take an offset and which bits of a BG3 entry mean what (points 1 and 2, mode 2), which the two documents agree on;
  - medium: which BG3 tile a column takes its offset from when BG1's fine scroll is not 0 (point 3), and the row of BG3 read (point 4);
  - low: mode 4's single entry, in which row it is read and how its valid bit is used (point 5);
  - none claimed: mode 6 (point 6).

## Context

Issue #92 adds offset-per-tile to the backgrounds ADR 0030 draws: in modes 2 and 4 the "tile data" of BG3 replaces the horizontal and vertical scroll of BG1 and BG2 for each 8-pixel column. Mode 6 shares the mechanism but is a hi-res mode and arrives with #97.

## Sources

- **anomie-regs#backgrounds/mode-2**: gives the formula (`HOFS = X + BGnHOFS`, `VOFS = Y + BGnVOFS`; valid bit `0x2000` for BG1, `0x4000` for BG2; the first 8x8 tile is done as normal; otherwise the `Hval` and `Vval` entries of BG3 are read at `(HOFS&7)|(((X-8)&~7)+(BG3HOFS&~7))` in the rows `BG3VOFS` and `BG3VOFS + 8`, and replace the scrolls when their valid bit is set) and the prose: visible tiles are numbered 0 to 32, tile 0 is as normal, tile T takes the data of BG3 tile T-1; the new offset replaces BGnVOFS entirely, the low 3 bits of BGnHOFS are still used; Y never selects BG3's row; BG3's own fine scroll is ignored; the offsets apply per 8x8 subtile of a 16x16 BG1 or BG3. The formula is introduced by "Hopefully these calculations are right...".
- **anomie-regs#backgrounds/mode-4**: one value is loaded, not two: if its bit 15 is set it is the vertical offset and the horizontal is 0, else the reverse.
- **anomie-regs#backgrounds/mode-6**: "works just like as Mode 2", with 8-pixel tiles only.
- **fullsnes#snesppuvideomemoryvram**: BG3's entries in these modes: bit 15 H/V (mode 4 only), bit 14 applies to BG2, bit 13 to BG1, bits 9–0 the offset, "Lower 3bit of HORIZONTAL offsets are ignored". **fullsnes#snesppuoffsetpertilemode** is "under construction (see Anomie's docs for now)" and adds nothing.
- **ADR 0030**, points 2 and 7: the scroll registers and the row a line shows.

Where the sources differ or say too little:

- Anomie's formula and prose disagree when BGnHOFS's low 3 bits are not 0. In the formula `X&~7` and `(X-8)&~7` cut at screen-aligned columns, while the visible tiles of BGn are cut at `8T - (BGnHOFS&7)`; applied to a pixel in the middle of a tile the formula would map one tile's pixels into two different tiles of the playfield, which a tile-at-a-time fetch cannot do. The prose is unambiguous.
- Neither document says how mode 4's single entry is located (the row), nor whether its valid bits are tested.
- Neither says what happens to a vertical offset's 10 bits beyond the tilemap's height; ADR 0030 point 7 wraps the row.

## Decision

1. **Which modes and layers.** In modes 2 and 4 BG1 and BG2 take offsets; BG3 only supplies them (it is not drawn, as ADR 0030 point 8 has it). Other modes are unchanged. Mode 6 is not drawn yet (point 6).
2. **The entry.** Bits 9–0 are the offset; bit 13 is BG1's valid bit and bit 14 BG2's. A set valid bit replaces the vertical scroll whole (bits 9–0, `Y + offset`) and the horizontal scroll with `(BGnHOFS&7) | (offset&~7)`; a clear one leaves the register's value. Whether BG3 is enabled on a screen makes no difference.
3. **The column.** The visible 8-pixel tiles are counted as the prose says: pixel X of BGn is in tile `T = (X + (BGnHOFS&7)) / 8`. Tile 0 takes no offset. Tile T ≥ 1 takes BG3's entry at pixel column `(BG3HOFS&~7) + 8*(T-1)`, wrapped at BG3's width. This follows the prose, and equals the formula where `BGnHOFS&7` is 0. BG3's character size and tilemap size apply, so a 16x16 BG3 gives one entry to two columns.
4. **The row.** BG3's row is `BG3VOFS` for the horizontal entry and `BG3VOFS + 8` for the vertical one (mode 2), wrapped at BG3's height; the line does not enter (the prose: "as if Y were always 0"). The BG3 entry is used as it stands in VRAM: its flip bits are not applied.
5. **Mode 4.** One entry is read, at the horizontal entry's place. Bit 15 set makes it the vertical offset (the horizontal scroll stays), else it is the horizontal offset (the vertical scroll stays); the valid bit of the background is tested on it as in mode 2.
6. **Mode 6.** Left to #97, which draws hi-res and must decide the 8-half-pixel tile there.

## Consequences

- `Backgrounds::sample` finds a pixel's scroll through one function; modes 2 and 4 read BG3's entries per pixel, which is slower than a per-tile fetch and is fine for a renderer that is not yet the accurate one.
- Point 3 is checked by the carts only for a fine scroll of 0, and by a unit test against the prose for a non-zero one; an oracle or hardware capture of a fine-scrolled offset column would settle it. Mode 4's row and valid bit (point 5) would be revisited the same way.
