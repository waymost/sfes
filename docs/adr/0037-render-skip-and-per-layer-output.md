# 0037. Render skip and per-layer output

- **Status:** accepted
- **Date:** 2026-11-23
- **Confidence:**
  - high: that render skip leaves sprite evaluation and its `$213E` flags running, and that the layer buffers cost nothing when off (points 1 and 2);
  - medium: that a skipped frame is not presented, and that a layer image is the main screen's view of a layer (points 3 and 5);
  - low: which layers a layer image holds when a layer is on neither screen, and the 256-pixel width in modes 5 and 6 (points 4 and 5). Both are project choices for a feature the hardware documents do not describe.

## Context

Issue #98 adds render skip and per-layer output to the scanline renderer of ADR 0028 to ADR 0036, and finishes the PPU explainer. EMU-06 says render skip "MUST skip pixel composition only", and that sprite evaluation "still runs every line, because its overflow flags are readable state (`$213E`)". It also says per-layer output (BG1 to BG4 and sprites, before compositing) must be producible on request, with no cost when off. WM-02 asks for the same images "before colour math and windows".

The documents describe hardware, which has no such features, so the shapes below are the project's. They must keep one property: neither feature changes an emulation result, the registers a program can read or the frame hash of a frame that is drawn (technical design section 12: features never change emulation results).

## Sources

- **Feature specification EMU-06**: render skip skips pixel composition only; sprite evaluation runs every line because `$213E`'s flags are readable state. Per-layer output of BG1 to BG4 and sprites, before compositing, on request, "with no cost when off".
- **Feature specification WM-02**: per-layer images of BG1 to BG4 and sprites "before colour math and windows", "at no cost when off"; also each layer's scroll offsets and, in mode 7, the matrix parameters.
- **Feature specification API-03**: layer buffers are available on request.
- **Technical design section 5, PPU**: `render_line` fetches the backgrounds into per-layer line buffers, evaluates the sprites, builds windows and applies mosaic, colour math and brightness, then writes the frame line and, when layer output is requested, copies the per-layer buffers. With render skip, the fetch, the composition and the write are skipped and the evaluation always runs.
- **fullsnes#snespputimersandstatus** and **ADR 0033, point 7**: `$213E` bits 7 and 6 are set by a line's evaluation, whatever `$212C` says, and cleared at the end of vertical blank.
- **ADR 0028, points 6, 8 and 10**: which lines are rendered and when, how lines make a frame, and that the video buffers are outputs, not state.
- **ADR 0030 and ADR 0035, point 8**: a background's pixel after scrolling and mosaic and before windows, colour math and brightness.
- **ADR 0036**: the two screens of a wide line, and the half-pixel of a tile each screen shows.

The documents are silent on everything else. No hardware has a "skipped" line, so which frame an application sees while skipping, and what a layer image holds, are decisions.

## Decision

1. **Render skip.** A skipped line runs sprite evaluation, so `$213E`'s flags are set exactly as when the line is drawn, then makes the next line the one after it. Nothing else runs: no background is fetched, no pixel composed, no line written. `Ppu::skip_line` does this; the machine calls it in place of `render_line` while its render-skip setting is on. Nothing but the output buffers differs from a drawn line. The registers, the memories, `$213E`, the counters and the mosaic's block rows are the same after a skipped frame as after a drawn one.
2. **No cost when off.** A line is rendered with layer output as an optional extra: the extra work is one branch on whether a buffer was passed. A machine with layer output off keeps no layer buffers.
3. **A skipped frame is not presented.** A frame in which any line was skipped is incomplete in the sense of ADR 0028, point 10, so `video()` keeps showing the last frame that was drawn whole (0 × 0 before the first). A program that wants pixels again turns skip off; the next whole frame is presented as usual. A frame whose lines were drawn, whatever setting was on before or after, has the hash it always had.
4. **Which layers.** A layer image holds BG1 to BG4 and the sprites, each present when the layer is on the main or the sub screen (`$212C`, `$212D`), as the hardware would fetch it, and absent otherwise. A background the mode does not have is absent. A sprite pixel is the sprite layer's front pixel, whatever its priority. The sprites are shown whatever their palette, so a palette 0 to 3 sprite appears although it takes no math.
5. **What a layer image is.** One line of one layer is 256 entries, one per column, each `None` where the layer is transparent or absent and otherwise the layer's CGRAM or direct colour. It is the layer's pixel after scrolling, offset-per-tile, flips, mosaic and interlace and before windows, the layer order, clipping, colour math and brightness, so a window or forced blank does not blank it. In BG modes 5 and 6 and with interlace it is what the main screen shows: the odd half-pixel of each tile pair (ADR 0036, point 3) and, with interlace, the half-line of the current field (ADR 0036, point 5). Pseudo-hires does not change it: the layer is what the main screen shows. This loses the sub screen's half-pixel; a program that wants the whole wide picture uses the frame.
6. **Where it is kept.** The machine keeps one layer line per output line of the frame being drawn, and the last whole frame's lines are the layer frame: `layers()` returns them, empty when layer output was not on for every line of that frame. Lines are not doubled for interlace, and a skipped frame has none. Both settings and both buffers are video outputs: a clone starts with both buffers empty and keeps the two settings, so a branched rollout renders as its parent did.
7. **Not yet.** WM-02's per-layer scroll offsets, mode 7's matrix, the sprite table and the tilemaps are registers and memory the machine already exposes, and the decoding belongs to the observation layer, not to the renderer.

## Consequences

- Render skip's saving is the whole of rendering except sprite evaluation: the pixels are the expensive part. A skipped frame shows nothing new, so an agent that needs observations while skipping turns skip off for those frames.
- Whether the sprites' pixel buffer is built for a skipped line is not observable; it is not built.
- A layer image's shape (256 entries, main screen only) can be widened later without changing the frame, if a user needs the sub screen's pixels.
- The layer images cost 5 KiB a line while on. That is the price of asking for them.
- Layer images use the same samples the compositor does, so they cannot disagree with the frame about a background's pixel; a test checks that the front layer of a plain line equals the frame.
