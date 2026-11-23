# 0032. Mode 7 and EXTBG

- **Status:** accepted
- **Date:** 2026-11-22
- **Confidence:**
  - high: the registers and their write-twice behaviour, the matrix formula (the two documents give the same one), the tilemap and character addressing, and the transparent and character 0 fills (points 1 to 4);
  - medium: the row used for the vertical flip, and the power-on values of the unlisted registers (points 2 and 1);
  - low: EXTBG outside mode 7, and the colour of BG2 pixels (points 5 and 6), where the documents are brief.

## Context

Issue #93 draws BG mode 7: one 256-colour background with a rotation and scaling matrix, a fill choice for what lies outside its 1024x1024 playfield, flips, and the EXTBG variant that adds a 128-colour BG2 from the same data. ADR 0029 point 8 already holds M7A and M7B and the write-twice latch for the multiplier; this adds the other registers and the renderer. Mosaic, windows, sprites, the sub screen and colour math belong to later issues.

## Sources

- **anomie-regs#211a**: M7SEL bit 7 (`r`) clear makes the playfield 1024x1024 so the tilemap fills it; set, the playfield is larger and bit 6 (`c`) chooses the fill, 0 transparent and 1 character 0, which is transformed like any other tile. Bits 1 and 0 flip the 256x256 screen vertically and horizontally.
- **anomie-regs#211b to #211e**: M7A to M7D are write-twice, `Reg = (Current<<8) | Prev; Prev = Current`, with one `Prev` shared by these registers and by M7HOFS and M7VOFS but not by the BGnxOFS registers; they are 8.8 fixed point.
- **anomie-regs#211f and #2120**: M7X and M7Y are 13-bit two's complement, write-twice.
- **anomie-regs#210d and #210e**: M7HOFS and M7VOFS are 13-bit two's complement and are written by the same writes as BG1HOFS and BG1VOFS; modes 0 to 6 ignore them and mode 7 ignores BG1HOFS.
- **anomie-regs#211b to #2120 and #backgrounds**: the matrix formula `[X Y] = [A B; C D] * [SX + M7HOFS - CX, SY + M7VOFS - CY] + [CX CY]`, the restriction to 0 to 1023 when bit 7 is clear, character 0 chosen by the low 3 bits of each coordinate when bits 7 and 6 are set, and a "bit-accurate formula": `CLIP(a)` sign-extends from bit 13 and keeps 10 bits, `X[0,y] = (A*CLIP(HOFS-CX) & ~63) + (B*y & ~63) + (B*CLIP(VOFS-CY) & ~63) + (CX<<8)`, `Y[0,y]` likewise with C and D, then `X[x,y] = X[x-1,y] + A`, `Y[x,y] = Y[x-1,y] + C`, all with 8 fraction bits. The tilemap is 128x128 one-byte entries in the low bytes of VRAM words and the characters are 8-bit pixels in the high bytes: the tilemap byte of a playfield position is at `(((Y&~7)<<4) + (X>>3))<<1` and the pixel at `(((TileData<<6) + ((Y&7)<<3) + (X&7))<<1) + 1`. Bits 4 to 7 of `$2105` and `$2107–$210C` are ignored in mode 7. The priorities are sprites 3, 2, 1, then BG1, then sprites 0; with EXTBG: sprites 3, 2, BG2 priority 1, sprites 1, BG1, sprites 0, BG2 priority 0. BG1 may use direct colour and BG2 may not; BG2 uses the mode 7 scroll registers.
- **anomie-regs#2133**: bit 6 is EXTBG: BG2 may be enabled in mode 7, using the same tile and character data as BG1 and the high bit of the colour data as a priority.
- **fullsnes#snesppurotationscaling**: the same registers, and the same arithmetic in another form: `ORG.X = (M7HOFS-M7X) AND NOT 1C00h`, ORed with `1C00h` if negative, `VRAM.X = (M7A*ORG.X AND NOT 3Fh) + (M7B*ORG.Y AND NOT 3Fh) + M7X*100h + (M7B*SCREEN.Y AND NOT 3Fh) + M7A*SCREEN.X`, `VRAM.Y` likewise. `SCREEN.X` is `0..255` and `SCREEN.Y` is `1..224` (or `1..239`), each XORed with `FFh` when flipped. The VRAM coordinate has the fraction in bits 7 to 0, the pixel in the tile in bits 10 to 8, the map index in bits 17 to 11, and bits 18 and up non-zero when the coordinate leaves the map. Screen over (M7SEL bits 7 and 6): 0 and 1 wrap within the 128x128 tiles, 2 is transparent outside, 3 is filled with tile 0.
- **fullsnes#snesppubgcontrol**: mode 7 supports rotation and scaling but no tile flips; CG direct select works on BG1; the tilemap and tile base registers are ignored (the base is zero, the size is 128x128); mode 7 tiles have no palette attributes, so direct colour has palette bits 0. CGRAM entries 01h to 7Fh are BG2's palette in mode 7. EXTBG: BG2 receives the same 8 bits as BG1's pixel, with bit 7 the priority and the lower 7 bits the colour; in modes 0 to 6 it shows garbage.
- **fullsnes#snesiomap**: M7A and M7B are `FFh` at power-on; M7SEL, M7C, M7D, M7X and M7Y are "?".
- **ADR 0029**, point 8: the write-twice latch and M7A, M7B; ADR 0030, point 2: how a pixel and a priority are chosen; ADR 0028: the first output line is line 1.

Where the sources differ or say too little:

- The two matrix formulas agree: `CLIP` and the `ORG` clip both sign-extend from bit 13 and keep 10 bits, and the incremental `X[x] = X[x-1] + A` equals the per-pixel product `M7A*SCREEN.X`.
- Anomie's `y` and fullsnes's `SCREEN.Y` are the screen line; only fullsnes says it starts at 1, which agrees with ADR 0028's first output line.
- Anomie does not say how flips enter; fullsnes XORs the screen coordinate with `FFh`.
- Neither gives M7SEL's, M7C's, M7D's, M7X's or M7Y's power-on value, or what EXTBG does when BG2's priority and BG1 are both enabled beyond the priority list.
- fullsnes's mode 7 priority note and anomie's agree for BG1 and BG2.

## Decision

1. **Registers.** `$211A M7SEL` is a plain byte. M7C, M7D, M7X, M7Y, M7HOFS and M7VOFS are write-twice registers on the latch of ADR 0029 point 8, so a write to any of them (and to `$210D` and `$210E`, which also write the BG1 scroll) takes `(value << 8) | latch` and sets the latch. M7X, M7Y, M7HOFS and M7VOFS keep 13 bits, sign-extended; M7A to M7D are signed 16-bit. The unlisted power-on values are 0; reset keeps all of them.
2. **The coordinate.** In mode 7, for output line `L` and column `x`: `SX = x` and `SY = L`, each XORed with `FF` when the matching M7SEL flip bit is set. With `ox = CLIP(HOFS - CX)` and `oy = CLIP(VOFS - CY)` (`CLIP` sign-extends bit 13 over a 10-bit value), `X = (A*ox & ~63) + (B*oy & ~63) + (B*SY & ~63) + A*SX + (CX << 8)`, and `Y = (C*ox & ~63) + (D*oy & ~63) + (D*SY & ~63) + C*SX + (CY << 8)`, in integers with 8 fraction bits (an `i32` holds every term). The pixel's playfield position is `X >> 8` and `Y >> 8`, arithmetic shifts.
3. **Outside the playfield.** A position is outside when either coordinate is below 0 or above 1023. With M7SEL bit 7 clear the position wraps modulo 1024. With it set and bit 6 clear a pixel outside is transparent; with bits 7 and 6 set it is a pixel of character 0 at the low 3 bits of each coordinate.
4. **The pixel.** The tilemap entry is the low byte of VRAM word `((Y>>3)*128 + (X>>3))`, the character number; the pixel is the high byte of word `((character*64) + (Y&7)*8 + (X&7))`. A value of 0 is transparent. BG1 uses it as a palette index, or as direct colour with palette bits 0 when `$2130` bit 0 is set (the same conversion as in ADR 0030). Tile flips, the tilemap registers and the character bases are not used.
5. **EXTBG.** With `$2133` bit 6 set in mode 7, BG2 shows the same pixel as BG1 at each position: bit 7 is its priority bit and bits 6 to 0 are the CGRAM index, 0 being transparent; BG2 is never direct colour. Its place in the order, front to back, is: BG2 with priority 1, BG1, BG2 with priority 0. Without it BG2 shows nothing. In other modes `$2133` bit 6 has no effect.
6. **Layers.** `$212C` enables BG1 and BG2 as in other modes; BG3 and BG4 show nothing in mode 7. The line is 256 pixels wide unless pseudo-hires is on.
7. **Not yet.** Mosaic (#96), windows (#95), sprites (#94), the sub screen and colour math. M7A and M7B keep feeding the multiplier whatever the mode, as ADR 0029 point 8 has it.

## Consequences

- The renderer computes each pixel from the matrix instead of stepping along the line; this equals the documented stepping and has no accumulated state, so a mid-line write cannot occur and HDMA changes between lines work.
- The flip's row (point 2: `SY = L XOR FF`) rests on fullsnes alone; a golden compared with an oracle would settle it, as would the power-on values of the unlisted registers.
- The `ADR 0029` note that fullsnes reports garbage from the multiplier in mode 7 outside blanking still stands: the PPU returns the product.
- EXTBG's colour source and its priority (points 5 and 6) are what the documents state; a frame compared with an oracle's output could still show a different order against sprites, which arrive with #94.
