# 0034. Windows

- **Status:** accepted
- **Date:** 2026-11-23
- **Confidence:**
  - high: the register layouts, the window edges, the four logic operations and the "neither, one or both windows" rule (points 1 to 4);
  - medium: that a window masks a whole layer, sprites included, before the layers are put in order (point 5);
  - low: the power-on values (point 1).

## Context

Issue #95 adds the two windows: `$2123–$2125` choose which windows act on which layer and whether each is inverted, `$2126–$2129` place their edges, `$212A/$212B` say how the two combine, and `$212E/$212F` say which layers are masked on the main and sub screens. ADR 0030 and ADR 0033 left them out ("windows and mosaic are not applied"). The colour window and the sub screen belong to the colour-math work (#96), but its registers are the same ones, so this ADR keeps its settings and #96 adds its area.

The documents agree on nearly everything. They differ on:

- how the two bits of a layer's window setting read (fullsnes's table and anomie's letters);
- the power-on values;
- what a window does to a sprite (neither says more than "layer").

## Sources

- **anomie-regs#2123 to #2125**: `ABCDabcd`. `c` and `a` enable window 1 and window 2 for BG1, BG3 and OBJ; `C` and `A` do the same for BG2, BG4 and the colour window. `d`, `b`, `D` and `B` invert the matching window: "W" is replaced by "~W" in the combination formulae. The letters sit in the byte as `A`=bit 7 down to `d`=bit 0, so each window's pair is "enable, then invert".
- **anomie-regs#2126 to #2129**: the left and right edges. "If the left edge is greater than the right edge, the window is considered to have no range at all (and thus "W" always is false)."
- **anomie-regs#212a and #212b**: `44332211` for the backgrounds; `----ccoo` for the colour window and OBJ. Each field is `00` OR, `01` AND, `10` XOR, `11` XNOR. W1 and W2 are true for pixels between the bounds; with neither window enabled nothing is masked; with one, that window (inverted or not); with both, `W1 op W2`. "Where the function is true, the BG will be masked."
- **anomie-regs#212e and #212f**: `---o4321` enables window masking for BG1 to BG4 and OBJ on the main (or sub) screen.
- **anomie-regs#windows**: the windows mask "a portion of any BG on the scanline", can be combined per BG, and select the region to keep or to hide. The colour window is "rather different" and is used by `$2130` bits 7 to 4 (clip colours and prevent math, each never, always, inside or outside).
- **anomie-regs#rendering-the-screen/rendering-the-screen**: step 1: the first layer that is "enabled on main, not clipped, and has a non-transparent pixel here".
- **fullsnes#snesppuwindow**: the edges X1 and X2 are inclusive, width `X2-X1+1`; "If the width is zero (or negative), then the 'inside-window' becomes empty, and the whole screen will be treated 'outside-window'". The `$2123–$2125` table is 2 bits per window: "0..1=Disable, 1=Inside, 2=Outside". For the logic: OR, AND, XOR, XNOR; it applies only if both windows are enabled; one enabled is used as it is; "If both are disabled, then the 'final' area will be empty", and XNOR is "1 XOR area1 XOR area2". `$212E/$212F`: bits 0 to 4 are BG1 to BG4 and OBJ, "0=Enable, 1=Disable"; "'Disable' forcefully disables the layer within the window area"; the backdrop is always enabled.
- **fullsnes#snesppuresolution**: window X1/X2 are always counted in full pixels, "reportedly with an odd glitch in hires mode?".
- **fullsnes#snesiomap**: `$2123` to `$212B` and `$212E/$212F` are "?" at power-on.
- **ADR 0028**: line 1 is the first output line. **ADR 0030**: how a layer's pixel and priority are found. **ADR 0033**: the sprite layer.

Where the sources differ or say too little:

- fullsnes's value table reads "0..1=Disable, 1=Inside, 2=Outside", which cannot be right as written (1 is both). Anomie's bit letters say the high bit of a pair enables and the low bit inverts, which gives 0 and 1 disabled, 2 inside and 3 outside. Fullsnes's "Disable" for 0 and 1 and its "Inside" and "Outside" fit that reading when its numbers are shifted up by one; it is a slip in the number and the bibliography's notes for fullsnes now list it. Anomie's reading is used.
- Anomie's "masked where the function is true" and fullsnes's "disable the layer within the window area" agree.
- Neither says whether the sprites are masked as a layer or sprite by sprite; both call OBJ a layer with one enable bit and one window setting, so the whole sprite layer is.
- Neither gives a power-on value.
- Neither says whether a window changes the sprite evaluation of ADR 0033. Both describe windows as a clip of what is shown, and the flags of `$213E` are "set whether or not OBJ are actually enabled" (anomie-regs#213e).

## Decision

1. **Registers.** `$2123 W12SEL`, `$2124 W34SEL`, `$2125 WOBJSEL`, `$2126–$2129 WH0–WH3`, `$212A WBGLOG`, `$212B WOBJLOG` and `$212E TMW` are plain bytes: written, kept as written, and read by the renderer on each line. They are 0 at power-on and kept on reset. `$212E` keeps its low 5 bits. `$212F TSW` is not stored until the sub screen exists (#96).
2. **A layer's windows.** The masked layers are BG1, BG2, BG3, BG4 and OBJ; the colour window has a setting of the same shape (the high half of `$2125`, logic field `$212B` bits 3 and 2) that #96 reads. Each has a 4-bit setting (BG1, BG3 and OBJ in the low half of its register, BG2, BG4 and the colour window in the high half): bits 1 and 0 are window 1's enable and invert, bits 3 and 2 window 2's. A window is *on* at `x` when it is enabled and `(left <= x <= right)` differs from its invert bit; a window with `left > right` is never inside, so an inverted one is on everywhere.
3. **The edges.** `x` is the column of the 256-pixel picture; a wide line uses the same column for both of its pixels.
4. **The area.** A layer's *area* at `x` is false when neither window is enabled; the enabled window's value when one is; and with both, `W1 op W2` with the layer's two logic bits: 0 OR, 1 AND, 2 XOR, 3 XNOR (`!(a ^ b)`). The logic fields are `$212A` bits 1 to 0, 3 to 2, 5 to 4 and 7 to 6 for BG1 to BG4, and `$212B` bits 1 to 0 for OBJ and 3 to 2 for the colour window.
5. **Masking.** A layer is masked at `x` on the main screen when its `$212E` bit is set and its area is true there. A masked layer has no pixel at `x`: for a background, neither of its two priorities, for OBJ the whole sprite layer. The layers behind it show, and the backdrop is never masked. The sprite evaluation of ADR 0033 and its flags do not depend on windows. In mode 7, BG1 uses BG1's setting and EXTBG's BG2 uses BG2's.
6. **Not yet.** `$212F` and the sub screen, and the colour window's effect on `$2130` (clip and prevent), both in #96. The colour window's settings are kept (`$2125` bits 7 to 4, `$212B` bits 3 and 2) but nothing reads them yet.

## Consequences

- Windows are a lookup per pixel on the render path; they hold no state of their own beyond the registers, so HDMA changes between lines work and a mid-line write cannot occur.
- fullsnes's "odd glitch in hires mode" is not modelled; the edges are in picture pixels. A comparison with an oracle in a hires mode (#97) could settle it.
- Whether the sprite layer is masked per sprite rather than as one layer, and the power-on values, rest on silence in the documents. A frame compared with an oracle would settle both.
