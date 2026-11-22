# The PPU (`sfes-snes-ppu`)

This page explains the SNES's picture processing unit and how the crate models it (QA-08). It is written issue by issue during milestone M2, starting with #89. Every statement cites its source, by its id in [`provenance/bibliography.toml`](../../provenance/bibliography.toml):

- Anomie's register doc (`anomie-regs`), by register or heading path;
- Anomie's timing doc (`anomie-timing`), by heading path;
- fullsnes (`fullsnes`), by section anchor;
- the project's ADRs, listed under [Decisions](#decisions).

## What the chip is

The PPU is two chips, PPU1 (5C77) and PPU2 (5C78), clocked off the same oscillator as the CPU (`anomie-timing#s-ppu`, `anomie-regs#213e`, `#213f`). It draws one pixel every 4 master cycles, for dots 22–277 of lines 1–224, or 1–239 with overscan. Line 0 outputs nothing (`anomie-timing#s-ppu/rendering`). Its memories are reached only through its registers on Address Bus B, `$2100–$213F`, and are freely accessible only in vertical blank or forced blank (`fullsnes#snespictureprocessingunitppu`).

The crate models both chips as one `Ppu`. It holds no reference to the machine: the bus decodes addresses, syncs the PPU before each access to its registers, and draws and presents its lines ([`core.md`](core.md#drawing-and-presenting-frames)).

## State

[`Ppu`](../../crates/sfes-snes-ppu/src/ppu.rs) holds (ADR 0028):

| Field | What | Power-on | Reset |
| --- | --- | --- | --- |
| `vram` | Video RAM, 64 KiB | filled from the seed | kept |
| `cgram` | Palette RAM, 256 colours of 2 bytes | filled from the seed | kept |
| `oam` | Sprite attributes, 544 bytes | filled from the seed | kept |
| `background_mode` | `$2105 BGMODE` | `$0F` | kept |
| `screen_mode` | `$2133 SETINI` | `$00` | `$00` |
| `ppu1_open_bus`, `ppu2_open_bus` | The two open-bus latches | 0 | kept |
| `counters` | The H/V counter latch, `$213C/$213D` | `$1FF` | kept |
| `next_line` | The next line of the frame to draw | 1 | 1 |

- **The memories.** Each is a heap-allocated byte slice, never built on the stack (technical design section 5). No source says what they hold at power-on, so they are filled from the machine's power-on stream, continuing after WRAM: VRAM, then CGRAM, then OAM (FND-02; ADR 0028, point 1). The machine exposes them as `RegionId::Vram`, `Cgram` and `Oam`.
- **The registers.** fullsnes's I/O map gives each register's value at reset, and in brackets the ones reset leaves unchanged, with their power-on value (`fullsnes#snesiomap`). `$2105` is "(xFh)", kept on reset; its unknown high nibble is taken as 0. `$2133` is "00h?" (ADR 0028, point 5).
- **What is kept so far.** The PPU keeps only `$2105` and `$2133`, the registers that decide a line's shape. Writes to the other registers have no effect until their issues: the memory ports in #90, the layers and effects in #91 to #96.

## Registers

[`Register`](../../crates/sfes-snes-ppu/src/register.rs) names all 64 registers. Each variant's discriminant is its B-bus address, and its doc line gives the address and the documents' mnemonic (AGENTS.md section 4). `Register::ALL` lists them in address order, and the bus decodes B-bus `$00–$3F` with `Register::try_from`, which fails above `$3F`; nothing else in the PPU sees an address.

### Reads

Most PPU registers are write-only. What a read returns depends on where the register is (`anomie-regs#registers`, `fullsnes#snesunpredictablethings`; ADR 0028, point 3):

| Registers | A read returns |
| --- | --- |
| `$21x4–$21x6`, `$21x8–$21xA` (x = 0, 1, 2) | PPU1 open bus, all 8 bits |
| `$21x0–$21x3`, `$21x7` (x = 0 to 3), `$21xB–$21xF` (x = 0, 1, 2) | CPU open bus, the data bus's last value |
| `$213C OPHCT`, `$213D OPVCT` | the latched counter, low byte then high, with PPU2 open bus in bits 7–1 of the high byte (`anomie-regs#213c`) |
| `$2134–$2136`, `$2138–$213B`, `$213E`, `$213F` | CPU open bus until #90 implements them |

The two documents' lists agree. `$2137 SLHV` is a read strobe: it latches the counters while `$4201` bit 7 is set, and the data read is CPU open bus (`anomie-regs#2137`; ADR 0021, point 7).

### PPU1 and PPU2 open bus

Each chip keeps the last value read from its own readable ports (`fullsnes#snesunpredictablethings`; ADR 0028, point 4):

- **PPU1** remembers reads of `$2134–$2136`, `$2138–$213A` and `$213E`. The value shows up in the PPU1 open-bus registers above and in `$213E` bit 4.
- **PPU2** remembers reads of `$213B–$213D` and `$213F`. The value shows up in the second read of `$213B` (bit 7), the second read of `$213C/$213D` (bits 7–1) and `$213F` bit 5.

A read sets the latch to the whole byte it returned. A DMA transfer's B-bus reads count too, since they reach the PPU as the CPU's do (ADR 0025, point 7). Both latches are 0 at power-on, a choice no source settles, and reset keeps them. Until #90 only `$213C` and `$213D` set a latch.

## Lines

[`Ppu::render_line`](../../crates/sfes-snes-ppu/src/ppu.rs) draws one line into a [`LineOut`](../../crates/sfes-snes-ppu/src/line.rs), from the registers and memories as they stand (technical design section 5; ADR 0028, point 7):

- **Width.** A line's `LineWidth` is `Wide`, 512 pixels, in BG modes 5 and 6 (`$2105` bits 2–0) or with pseudo-hires (`$2133` bit 3), otherwise `Normal`, 256 (`anomie-regs#2105`, `#2133`, `fullsnes#snesppuresolution`).
- **Interlace.** The line records whether `$2133` bit 0 was set.
- **Pixels.** A pixel is a CGRAM colour, an `sfes_api::Rgb555`: red in bits 4–0, green in 9–5, blue in 14–10, bit 15 clear (`fullsnes#snesppucolorpalettememorycgramanddirectcolors`). CGRAM does not store bit 15, so the PPU drops it, and `Rgb555` refuses a value with it set. Until the layers arrive, every pixel is the backdrop, CGRAM entry 0, which is what a line shows where every BG and sprite pixel is transparent. In a wide line the sub screen's pixels use colour 0 as well (`fullsnes#snesppuresolution`, "Hires Notes").

The machine decides when a line is drawn and how lines make a frame ([`core.md`](core.md#drawing-and-presenting-frames)).

## Frame geometry

How lines are presented as a `VideoFrame` and hashed (ADR 0028, points 8 and 9):

| Frame | Size |
| --- | --- |
| Plain | 256 × 224, or 256 × 239 with overscan |
| Any line in BG mode 5 or 6, or with pseudo-hires | 512 wide; a 256-pixel line has each pixel twice |
| Any line with interlace on | twice as high, 448 or 478; each line fills two rows |

- **Mixed frames.** Games mix line widths in one frame (fullsnes lists BG mode 5 "in lower screen half"), so the width is decided per frame, by its widest line. Doubling loses nothing.
- **No weaving.** An interlaced frame shows only its own field, never rows kept from the frame before, so a frame depends only on the machine's state. `VideoFrame::field` names the field, from `$213F` bit 7 (API-03).
- **Overscan cleared late.** A frame whose overscan bit was cleared during lines 225–239 has as many rows as the lines it output (ADR 0020, point 5).
- **Goldens.** They hash the frame as presented (ADR 0015).

Two alternatives were rejected. A fixed 512 × 478 canvas would cost four times the pixels for every ordinary frame. Weaving two fields would make a frame depend on the frame before, which a clone does not have.

## The code

| File | What it holds |
| --- | --- |
| [`ppu.rs`](../../crates/sfes-snes-ppu/src/ppu.rs) | `Ppu`: the state, power-on and reset, `peek`, `after_read`, `write`, the counter latch and `render_line` |
| [`register.rs`](../../crates/sfes-snes-ppu/src/register.rs) | `Register`, `ReadKind` and `NotAPpuRegister`: the 64 registers, the address conversion and what a read of each returns |
| [`counters.rs`](../../crates/sfes-snes-ppu/src/counters.rs) | The H/V counter latch and its flip-flops |
| [`line.rs`](../../crates/sfes-snes-ppu/src/line.rs) | `LineOut` and `LineWidth`: one drawn line and its width |

The crate is `no_std` with `alloc` and depends only on `sfes-api` and `thiserror` (technical design section 12). The machine implements its bus's `Registers` trait for `Ppu` in [`bus/ppu.rs`](../../crates/sfes-snes-core/src/bus/ppu.rs).

## Testing

- **Unit tests** in each file cover:
  - the register table and each register's read kind against both documents' lists;
  - the power-on fill and reset;
  - the open-bus latches and the counters;
  - each line's width and colour.
- **The machine's tests** cover reads through the bus, when lines are drawn, overscan and interlaced frames, and the frame geometry ([`core.md`](core.md#testing)).

No test cartridge exercises this issue: a frame is one colour until the layers arrive, and CGRAM can be written only through #90's port. Goldens follow in #100.

## Decisions

| ADR | Decision | Confidence |
| --- | --- | --- |
| [0028](../adr/0028-ppu-state-registers-and-frame-geometry.md) | The memories and their power-on fill, the registers and their reads, PPU1 and PPU2 open bus, which lines are drawn and when, a line's width and pixels, the frame geometry, and where the outputs live | high for the reads and open bus; medium for which lines are output and their width; the power-on values and the geometry are project choices |

ADR 0021, point 7 covers the counter latch, which moved into the PPU with this issue.
