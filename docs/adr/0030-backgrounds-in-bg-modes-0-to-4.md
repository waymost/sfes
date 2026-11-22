# 0030. Backgrounds in BG modes 0 to 4

- **Status:** accepted
- **Date:** 2026-11-22
- **Confidence:**
  - high: the tilemap and character formats, the palette each mode and depth uses, direct colour, and the priority orders (points 3 to 6), which the two documents agree on;
  - medium: how the scroll registers keep a half-written value (point 2), and which row of the background a line shows (point 7);
  - none claimed: what is left out until its own issue (points 8 and 9).

## Context

Issue #91 draws the backgrounds of BG modes 0 to 4. ADR 0028 left `render_line` filling every pixel with the backdrop "until the layers arrive", and kept only `$2105` and `$2133`; ADR 0029, point 8 let writes to `$210D` and `$210E` move only the Mode 7 latch, for the same reason. This ADR decides how the PPU stores the background registers, how it finds a pixel of a background, and how it puts the backgrounds in front of one another and the backdrop. Sprites (#94), offset-per-tile (#92), Mode 7 (#93), windows (#95), colour math, mosaic and brightness (#96) and hi-res (#97) each add to what is decided here and are not decided by it.

## Sources

- **anomie-regs#2105**: `DCBAemmm`, the BG sizes (bit set: 16×16 tiles) and the mode; bit 3 is mode 1's BG3 priority. Its table of modes gives each BG's depth and the priorities front to back. **anomie-regs#2107 to #210a**: `aaaaaayx`, "Tilemap address in VRAM (Addr>>10)"; "x" is a second tilemap to the right, "y" one below, in the order A, B, C, D, each 32×32. **anomie-regs#210b, #210c**: `bbbbaaaa`, the character base of BG1/3 and BG2/4, "Addr>>13".
- **anomie-regs#backgrounds/tile-maps-and-character-maps**:
  - a tilemap entry is `vhopppcc cccccccc`; a tilemap word address is `(Y&0x1f)<<5 + (X&0x1f)`, plus 0x400 words for a second screen to the right or below and 0x800 for the fourth;
  - characters start at byte `(Base<<13) + TileNumber * 8 * NumBitplanes`; a row is a byte per bitplane, the leftmost pixel in bit 7, planes 0 and 1 the low and high bytes of a word, each further pair of planes the same eight words on;
  - a 16×16 tile is "Tile, Tile+1, Tile+16, and Tile+17", with no wrap in the tile number except that `$3FF` goes to 0, and flips as a whole.
- **anomie-regs#backgrounds/bg-scrolling**: a background wraps at its edges; the scroll registers are write-twice with `BGnHOFS = (NewByte<<8) | (PrevByte&~7) | ((CurrentValue>>8)&7)` and `BGnVOFS = (NewByte<<8) | PrevByte`, one `Prev` shared by all eight and not by the Mode 7 registers; the tile for a position is `(X + BGnHOFS)/Size`, `(Y + BGnVOFS)/Size`. Games set the vertical scroll to −1 "because the SNES loads OBJ data for each scanline during the previous scanline", so line 0, which is not output, is the first line of the background; "an emulator would need to add 2 instead of 1" in interlace.
- **anomie-regs#210d**: "x = The BG offset, 10 bits"; writing `$210D` writes both BG1HOFS and M7HOFS.
- **anomie-regs#backgrounds/mode-0 to #mode-4, and #bg-modes**: the depths of each BG per mode and the palette start: mode 0 `ppp*4 + (BG#-1)*32`; mode 1 `ppp*ncolors`; mode 2 `ppp*16`; mode 3 BG1 0, BG2 `ppp*16`; mode 4 BG1 0, BG2 `ppp*4`. "Color 0 in any palette is considered transparent."
- **anomie-regs#backgrounds/direct-color-mode**: for the 256-colour BGs of modes 3, 4 and 7, `$2130` bit 0 treats the character data as `BBGGGRRR` and `ppp` as `bgr` to make Red=`RRRr0`, Green=`GGGg0`, Blue=`BBb00`; data 0 is still transparent. **anomie-regs#2130**: bit 0, "Direct color mode for 256-color BGs".
- **anomie-regs#212c**: `---o4321`, enable BG1 to BG4 and OBJ on the main screen. **anomie-regs#rendering-the-screen/rendering-the-screen**: "Go down the priority list to find the first BG/OBJ layer that is enabled on main, not clipped, and has a non-transparent pixel here. You'll always bottom out on the backdrop."
- **fullsnes#snesppubgcontrol**: agrees on `$2105`, with "BgMode0..4: variable 8x8 or 16x16", and on `$2107–$210A`: "SC Base Address in VRAM (in 1K-word steps, aka 2K-byte steps)"; `$210B/$210C` "Tile Base Address (in 4K-word steps)", 4 bits each. Scroll registers: "1st Write: Lower 8bit; 2nd Write: Upper 2bit", with the same formulas as anomie.
- **fullsnes#snesppucolorpalettememorycgramanddirectcolors**: the palette indices of each mode: 4-colour BGs of mode 0 at `$01–$1F`, `$21–$3F`, `$41–$5F`, `$61–$7F`; direct colour as anomie, "Color Bit7-0 all zero --> Transparent".
- **fullsnes#snesiomap**: `$2107–$2114`, `$212C`, `$2130` have no value at power-on ("(?)"); the scroll registers `(?,?)`.
- **ADR 0028**, points 5 to 7; **ADR 0029**, points 8 and 12.

Where the sources differ or say too little:

- The formula anomie gives for a tilemap's word address begins `(Addr<<9)`. The same document's `$2107` says the field is `Addr>>10`, and fullsnes says 1K-word steps; a 32×32 tilemap is 1024 words, so two tilemaps side by side are 2K words apart, as the formula's later terms have them. `(Addr<<9)` is a slip, and this ADR follows the field and fullsnes. The entry's notes list it.
- The scroll formulas keep bits of the old register's value, which a 10-bit register cannot do after a first write: `$FF` then `$01` must make `$1FF`. Taking the register as only 10 bits, `((Reg>>8)&7)` would lose the bit that `$FF`'s bit 2 sets, and the usual two-write sequence would give `$1FB`. fullsnes says only that the second write is the "upper 2bit". Neither says whether a vertical or horizontal scroll above 1023 is used whole.
- Neither document says where the PPU takes the row of the background in the lines it does not output, or whether a line's number or its row in the picture is added to the vertical scroll; anomie's `Y + BGnVOFS` and the −1 convention are the evidence.
- Neither says what a pixel of a background is when a mode gives the background no pixels (BG3 in modes 2, 4 and 6 holds offset data; BG4 outside mode 0).

## Decision

1. **Registers.** The PPU keeps `$2107–$210A` BGnSC, `$210B–$210C` BGnnNBA, the eight scroll registers `$210D–$2114`, `$212C` TM and `$2130` CGWSEL. Each is 0 at power-on and kept on reset (fullsnes gives no value; ADR 0029, point 12). Of TM, `$212D–$212F` and `$2131–$2132` the renderer uses only TM's bits 3 to 0, and of CGWSEL only bit 0. `$2105`'s bits 7 to 3 are kept as ADR 0028 says (point 5) and used here.
2. **Scroll writes.** A scroll register is a 16-bit word, of which the renderer uses the low 10 bits. A write of `value`:
   - to BGnHOFS makes the word `value<<8 | (latch & 0xF8) | ((old word >> 8) & 7)`;
   - to BGnVOFS makes it `value<<8 | latch`;
   - and sets the latch to `value`.

   The latch is shared by the eight registers. It is not the Mode 7 registers' latch (ADR 0029, point 8), though a write to `$210D` or `$210E` runs both. Keeping the written byte in bits 15 to 8, and not only the 10 bits the register uses, is what lets two writes make any value from 0 to `$3FF`.
3. **Where a tile is.** For a background of `columns × 32` and `rows × 32` tiles of 8 or 16 pixels (`$2105` bit 4 plus the background's number, and `$2107`'s bits 0 and 1):
   - the pixel at column `x` of line `L` is the background's (`x + HOFS`) and (`L + VOFS`), each wrapped at the background's width and height in pixels;
   - the tile is that position divided by the tile size; its entry is the word at `(register >> 2) << 10`, plus `(tile Y mod 32) * 32 + (tile X mod 32)`, plus 0x400 words if the tilemap is 64 tiles wide and the tile is in the right screen, plus 0x400 (0x800 if the tilemap is also wide) for the lower screen; VRAM word addresses have 15 bits (ADR 0029, point 3);
   - a character starts at byte `(nibble << 13) + tile * 8 * planes`, the nibble being the background's half of `$210B` or `$210C`, and byte addresses wrap at 64 KiB.
4. **Tiles.** The entry's bits 14 and 15 flip the whole tile, 8×8 or 16×16, before the tile is chosen. A 16×16 tile is the entry's tile and the tiles 1, 16 and 17 after it, each number wrapped to 10 bits, in the places a flipped tile moves them to. A pixel's value has one bit from each plane at the bit 7 − (column mod 8) of that row's byte: planes 0 and 1 at the row's two bytes, planes 2 and 3 sixteen bytes on, and so for 4, 5 and 6, 7. The depth of each background is fixed by the mode: mode 0 all two; mode 1 four, four, two; mode 2 four, four; mode 3 eight, four; mode 4 eight, two.
5. **Colour.** A value of 0 is transparent. Otherwise the colour is CGRAM entry `palette start + value`, where the palette start is:
   - two planes: `ppp * 4`, with 32 more for each background before it in mode 0;
   - four planes: `ppp * 16`;
   - eight planes: 0, and entry `value`;
   - eight planes with `$2130` bit 0 set: no CGRAM, but the 15-bit colour `BBb00 RRRr0 GGGg0` of the point's sources.
6. **Priority.** A pixel is the first, front to back, of these layers that is on in `$212C` and not transparent, else the backdrop (CGRAM entry 0). A layer is a background and whether the pixel's entry has priority bit 13 set:
   - mode 0: BG1 high, BG2 high, BG1 low, BG2 low, BG3 high, BG4 high, BG3 low, BG4 low;
   - mode 1: BG1 high, BG2 high, BG1 low, BG2 low, BG3 high, BG3 low; with `$2105` bit 3 set, BG3 high first of all;
   - modes 2, 3, 4: BG1 high, BG2 high, BG1 low, BG2 low.

   The sprites will take their places between these (#94).
7. **Which row.** The row of a background that line `L` shows is `L + VOFS`, whatever `L − 1` the frame holds for it. With the usual `VOFS = −1`, line 1, the first output, shows row 0. Interlace's extra row (anomie's "add 2") waits for #97.
8. **What is drawn.** The main screen of the modes above only. Until their issues: the sub screen and `$212D`, windows and mosaic, colour math and the fixed colour, brightness and forced blank are not applied, so a line in forced blank shows its picture; BG modes 5, 6 and 7 draw only the backdrop; sprites are absent; and a wide line, which only pseudo-hires makes in these modes, repeats each main-screen pixel twice. Offset-per-tile (#92) is not applied in modes 2 and 4: BG1 and BG2 use their own scroll registers, which is the hardware's picture while the map of BG3 sets none of the valid bits.
9. **Tests.** The renderer has unit tests of the tile, scroll, palette, direct colour and order rules, and one cartridge for each of modes 0 to 4 (`tests/carts/bg-mode0` to `bg-mode4`) whose picture a Rust test in `sfes-snes-core` compares with values computed from the documents. A hardware or oracle comparison waits for the golden suite (#100).

## Consequences

- The documents are consistent on everything but the points under "Where the sources differ"; points 2 and 7 are the least certain and a test ROM that writes a scroll byte twice or once, or sets the scroll to −1, would settle them. A visible effect of point 2 is on the low three bits of a horizontal scroll that a game writes only once.
- Frames that games draw in modes 5 to 7 are the backdrop until #93 and #97, and frames that use sprites, windows or colour math are wrong until #94 to #96; the golden suite (#100) is blessed only once those land.
- `render_line` fetches each background's pixel from VRAM one at a time. It is not fast, and the `Fast` profile's tile cache is a later change that must give the same picture.
- ADR 0028, point 7's "until the layers arrive" is now this ADR for modes 0 to 4. ADR 0029, point 8 is unchanged: the Mode 7 latch still has its own rule.
