# 0028. PPU state, registers and frame geometry

- **Status:** accepted
- **Date:** 2026-11-21
- **Confidence:**
  - high: the read behaviour of each register in `$2100–$213F`, and PPU1 and PPU2 open bus (points 3 and 4);
  - medium: which lines are output and when a line is wide or interlaced (points 6 and 7);
  - none claimed: what the memories and open-bus latches hold at power-on (points 1 and 4), and the frame geometry and where the outputs live (points 8 to 10). These are project choices about presentation and emulation, not claims about the hardware.

## Context

Issue #89 brings the PPU into the machine: its memories, its registers on B-bus `$00–$3F`, the open-bus behaviour of PPU1 and PPU2, a `render_line` entry point and a frame buffer. Until now the bus answered the PPU's registers itself:

- ADR 0019, point 1 made them read as open bus and ignore writes;
- ADR 0020, point 6 kept the interlace and overscan bits of `$2133` in the scheduler;
- ADR 0021, point 7 kept the H/V counter latch, and a stand-in PPU2 open bus, in the bus "until the PPU exists".

The PPU's features come in later issues: the memory ports and status registers in #90, backgrounds, sprites, windows and colour math in #91 to #96, hi-res and interlace in #97. This ADR decides what the PPU does in the meantime, and decides now how frames are presented, because hi-res and interlace are in M2 and the golden-frame files (ADR 0015) hash whatever `VideoFrame` holds.

## Sources

- **anomie-regs#registers** (the preamble): most registers, and most undefined bits of readable registers, return open bus, "the last value read over the main bus". "Registers matching $21x4-6 or $21x8-A (where x is 0-2) return the last value read from any of the PPU1 registers $2134-6, $2138-A, or $213E. This is known as PPU1 Open Bus." PPU2 open bus "involves reading registers $213B-D or $213F (NOT $21xB-D though)".
- **fullsnes#snesunpredictablethings**: PPU1 open bus "relies on the value most recently read from Ports 2134h-2136h, 2138h-213Ah, 213Eh". It shows up in all 8 bits of reads of `$21x4–$21x6` and `$21x8–$21xA` (x = 0, 1, 2), and in `$213E` bit 4. PPU2 open bus relies on the value most recently read from `$213B–$213D` and `$213F`, and shows up in the second read of `$213B` (bit 7), the second read of `$213C/$213D` (bits 7–1), and `$213F` bit 5. "Other write-only PPU registers & the HV-latch "strobe" register are acting like "normal" CPU Open Bus": `$21x0–$21x3` and `$21x7` (x = 0 to 3), and `$21xB–$21xF` (x = 0, 1, 2). The two documents agree.
- **anomie-regs#213b, #213c, #213e, #213f**: the open-bus bits of those registers, as above; `$213C/$213D` as ADR 0021 uses them.
- **fullsnes#snesiomap**: the value of each register at reset, with values in brackets "left unchanged upon Reset (but do contain the specified value upon initial Power-up)". `$2105` BGMODE is "(xFh)"; `$2133` SETINI is "00h?"; most PPU registers are "(?)". Nothing is said of VRAM, CGRAM or OAM at power-on, and no other registered source says it.
- **anomie-regs#2105**: bits 2–0 are the BG mode.
- **anomie-regs#2133**: bit 3 enables pseudo-hires, "a 512-pixel horizontal resolution" taking "pixels from the subscreen for the even-numbered pixels (zero based) and from the main screen for the odd-numbered pixels"; "Enabling this bit in Modes 5 or 6 has no effect". Bit 2, overscan, displays 239 lines instead of 224. Bit 1 makes OBJ interlaced "regardless of BG mode". Bit 0, interlace: in BG mode 5 (and probably 6) "the effective screen height will be 448 (or 478)"; in other modes "the screen will just get a bit jumpy", and "setting this bit makes the SNES output a normal interlace signal".
- **fullsnes#snesppuresolution**: lines are 256 pixels, 512 in true hi-res (BG modes 5 and 6) and with pseudo-hires (SETINI bit 3); 224 or 239 lines, doubled by interlace. Games mix them within a frame: its list includes titles with BG mode 5 "in lower screen half". In hi-res both the main and sub screen use colour 0 as their backdrop.
- **fullsnes#snesppucolorpalettememorycgramanddirectcolors**: a CGRAM entry is bits 4–0 red, 9–5 green, 14–10 blue, bit 15 unused; entry 0 is the main backdrop colour, "used when all BG/OBJ pixels are transparent".
- **anomie-timing#s-ppu/rendering**: "The PPU outputs one pixel every 4 master cycles, for dots 22-277(?) on scanlines 1-224 or 1-239". Line 0 outputs nothing (`#s-ppu/detailed-renderer-timing`: "nothing is actually output for scanline 0").
- **anomie-timing#s-ppu/blanking-periods**: V-Blank begins at line 225 or 240, at H=0, by bit 2 of `$2133`.
- **Technical design, section 4, catch-up rule 2**: the `Fast` profile renders every line whose start time is at or before `now` and has not yet been drawn, using the registers as they stand at that moment. **Section 5, shared rules**: large memories are `Box<[u8; N]>`, allocated without building on the stack; outputs are not state, and a clone gets empty outputs. **Section 5, PPU**: `render_line(&mut self, line: u16, out: &mut LineOut)`; lines are 256 or 512 pixels, frames 224 or 239 lines or 448 or 478 interlaced, and an ADR "decides how mixed-width frames are presented and hashed".
- **Feature specification, API-03**: native output is "an RGB555 buffer plus metadata (width 256 or 512, height 224, 239, 448 or 478, interlace field)"; a normalized 256 × 224 mode returns "the current field" of an interlaced frame.
- **anomie-regs#213f**: bit 7 is the interlace field, which toggles every frame (ADR 0020, point 4).
- **ADR 0015**: a golden records each checkpoint's width, height and the BLAKE3 hash of the pixels, row-major, each `u16` little-endian.

## Decision

1. **Memories.** `Ppu` holds VRAM (64 KiB), CGRAM (512 bytes) and OAM (544 bytes), each allocated on the heap without building it on the stack first (technical design section 5). Like WRAM, each is a `Box<[u8]>` of fixed length rather than a `Box<[u8; N]>`: converting the allocated slice to an array box returns a `Result`, and library code may not panic on an error that cannot happen. No source says what they hold at power-on, so they are undefined power-on state (FND-02). Power-on fills them from the same `SplitMix64(seed)` stream that fills WRAM (ADR 0018), continuing after WRAM: VRAM, then CGRAM, then OAM, each from its first byte up. WRAM's contents do not change. The reset button keeps all three. The machine exposes them as `RegionId::Vram`, `Cgram` and `Oam`.
2. **Registers.** Every B-bus address `$00–$3F` decodes to one of the PPU's 64 registers, `sfes_snes_ppu::Register`, an enum whose discriminants are the B-bus addresses. The PPU has a `peek`, an `after_read` and a `write` for them, and the bus reaches them through the `Registers` trait as it does every other unit's. `$2137` stays decoded and handled as ADR 0021, point 7 says: its read is a strobe that latches the counters while `$4201` bit 7 is set.
3. **Reads.** By the two documents' lists, a read of:
   - `$21x4–$21x6` or `$21x8–$21xA` (x = 0, 1, 2) returns PPU1 open bus;
   - `$21x0–$21x3`, `$21x7` (x = 0 to 3) or `$21xB–$21xF` (x = 0, 1, 2) returns CPU open bus, the data bus's value (ADR 0019, point 4);
   - `$213C` or `$213D` returns the counter latch's next byte, with PPU2 open bus in bits 7–1 of the high byte (ADR 0021, point 7);
   - `$2134–$2136`, `$2138–$213B`, `$213E` or `$213F` returns CPU open bus and changes nothing, until #90 implements those registers. This keeps ADR 0019, point 1's rule for them alone.
4. **PPU1 and PPU2 open bus.** The PPU holds two latches. A read of a PPU1 port that returns a value (`$2134–$2136`, `$2138–$213A`, `$213E`) sets the PPU1 latch to the byte returned; a read of a PPU2 port (`$213B–$213D`, `$213F`) sets the PPU2 latch the same way. The byte includes any open-bus bits it carried. A read that returns PPU1 open bus leaves the latch as it is. Until #90 only `$213C` and `$213D` set a latch, so the counters' PPU2 open bus is now the PPU's, as ADR 0021, point 7 foresaw. A transfer's B-bus reads count, since they reach the PPU as the CPU's do (ADR 0025, point 7). Both latches are 0 at power-on, a choice no source settles, and the reset button keeps them.
5. **Writes and register values.** Until each register's feature issue, the PPU keeps only what rendering needs:
   - `$2105` BGMODE, `$0F` at power-on (fullsnes's "xFh", with the unknown high nibble taken as 0) and kept on reset;
   - `$2133` SETINI, 0 at power-on and reset (fullsnes's "00h?").

   The scheduler still keeps its own copy of `$2133`'s interlace and overscan bits for timing (ADR 0020, point 6); one write sets both. A write to any other PPU register has no effect until its issue.
6. **Which lines are output, and when.** A frame outputs lines 1 to the line before vertical blank begins, normally 224 or 239 lines. Line *L* is row *L* − 1. Line 0 and the lines of vertical blank output nothing (anomie-timing). Line *L* is rendered at the first of two moments after it starts: when the PPU is synced before an access to its registers (technical design, catch-up rules 1 and 2), or when vertical blank begins. It uses the registers as they stand at that moment. Since every register write syncs the PPU first, they are the registers as they stood when the line started.
7. **A line's pixels.**
   - A line is **wide**, 512 pixels, when at its render `$2105` gives mode 5 or 6, or `$2133` bit 3 is set; otherwise it is 256 pixels.
   - A line is **interlaced** when `$2133` bit 0 is set at its render.
   - A pixel is a CGRAM colour: bits 4–0 red, 9–5 green, 14–10 blue, bit 15 0. This is what `VideoFrame`'s "RGB555" means.
   - Until the layers arrive (#91 on), every pixel of a line is the backdrop, CGRAM entry 0, as on a line where every BG and OBJ pixel is transparent. In a wide line this holds for the sub-screen's pixels too. Brightness and forced blank are applied when #96 adds them.
8. **Frame geometry.** At the start of vertical blank the frame's lines are presented as one `VideoFrame`:
   - **Width.** 512 if any of its lines was wide, otherwise 256. In a 512-wide frame, each pixel of a 256-pixel line is written twice.
   - **Height.** The number of lines output, doubled if any of its lines was interlaced. In a doubled frame each line fills two rows, 2(*L* − 1) and 2(*L* − 1) + 1.
   - **No weaving.** A frame shows only the lines rendered in it. In an interlaced frame both rows of a pair show that frame's field, not one row from the field before. A viewer that wants to weave two fields can take two consecutive frames.
   - **The field.** An interlaced frame's `VideoFrame::field` names the field it shows, `$213F` bit 7 as the frame is presented: `Field::First` for 0, `Field::Second` for 1 (API-03). Other frames have `None`. The field is not part of the hash.

   So a plain frame is 256 × 224 or 256 × 239, true or pseudo hi-res makes it 512 wide, and interlace makes it 448 or 478 high. A frame whose overscan bit was cleared during lines 225–239 (ADR 0020, point 5) has between 225 and 238 lines.
9. **Hashing.** Goldens hash the presented frame exactly as ADR 0015 says: its width, its height and its pixels. The rules above make that hash a function of the machine's state and its inputs alone. The golden format does not change.
10. **Where the outputs live.** The lines of the frame being drawn and the last presented frame are outputs, not state (technical design section 5). They live in the bus, so that every place that renders can reach them, in a wrapper whose clone is empty and which serialization will skip. `Snes::video` returns the last presented frame, 0 × 0 until the first one.
    - A frame is presented only if every one of its lines was rendered into this machine's outputs. A clone taken partway through a frame, and a machine reset partway through one, therefore present nothing for that frame, and `video()` keeps the frame before.
    - From the next frame on, a clone presents the same frames as its original.

## Consequences

- `VideoFrame` gains `field` and keeps the rest of its shape, so every consumer sees one rectangular image per frame. Its doc comment gains the pixel format. API-03's normalized mode takes every other row of an interlaced frame, which is its field.
- Doubling pixels and rows loses nothing: a 256-pixel line in a 512-wide frame, and a field in a 448-row frame, can be recovered exactly. The cost is a 512 × 478 frame (about 490 KB) only for frames that use hi-res and interlace.
- Two alternatives were rejected:
  - A fixed 512 × 478 canvas would cost four times the pixels for every ordinary frame, in hashing, in RL observations and in datasets.
  - Weaving two fields would make a frame depend on the frame before. That is output, not state, so a clone's first frame and the original's would differ, and so would their hashes.
- The rendered frame is the backdrop colour until the backgrounds arrive. It is seeded garbage until a ROM writes CGRAM, which needs #90's port. No golden is recorded before #100.
- ADR 0019, point 1's open-bus rule now covers only the PPU registers #90 has yet to implement, and ADR 0021, point 7's stand-in PPU2 open bus is replaced by point 4.
- The technical design's sections 3 and 5 are updated: the outputs live in the bus rather than in `Snes`, and the geometry is this ADR's.
- To revisit: hi-res and interlace rendering (#97) checks points 7 and 8 against the mode 5 and interlace test ROMs; a PAL frame's visible lines, if a source gives more than 239.
