# The PPU (`sfes-snes-ppu`)

This page explains the SNES's picture processing unit and how the crate models it (QA-08). It is written issue by issue during milestone M2: #89, #90 and #91 so far. Every statement cites its source, by its id in [`provenance/bibliography.toml`](../../provenance/bibliography.toml):

- Anomie's register doc (`anomie-regs`), by register or heading path;
- Anomie's timing doc (`anomie-timing`), by heading path;
- fullsnes (`fullsnes`), by section anchor;
- the project's ADRs, listed under [Decisions](#decisions).

## What the chip is

The PPU is two chips, PPU1 (5C77) and PPU2 (5C78), clocked off the same oscillator as the CPU (`anomie-timing#s-ppu`, `anomie-regs#213e`, `#213f`). It draws one pixel every 4 master cycles, for dots 22–277 of lines 1–224, or 1–239 with overscan. Line 0 outputs nothing (`anomie-timing#s-ppu/rendering`). Its memories are reached only through its registers on Address Bus B, `$2100–$213F`, and are freely accessible only in vertical blank or forced blank (`fullsnes#snespictureprocessingunitppu`).

The crate models both chips as one `Ppu`. It holds no reference to the machine: the bus decodes addresses, syncs the PPU before each access to its registers, passes it the [signals](#signals) it needs, and draws and presents its lines ([`core.md`](core.md#drawing-and-presenting-frames)).

## State

[`Ppu`](../../crates/sfes-snes-ppu/src/ppu.rs) holds (ADR 0028):

| Field | What | Power-on | Reset |
| --- | --- | --- | --- |
| `vram` | Video RAM, 64 KiB | filled from the seed | kept |
| `cgram` | Palette RAM, 256 colours of 2 bytes | filled from the seed | kept |
| `oam` | Sprite attributes, 544 bytes | filled from the seed | kept |
| `display_control` | `$2100 INIDISP` | `$80` | `$80` |
| `oam_port` | `$2102/$2103` reload value and priority bit, the OAM address and latch | 0 | kept |
| `vram_port` | `$2115 VMAIN`, the VRAM address and prefetch | `$0F`, 0, 0 | kept |
| `cgram_port` | The CGRAM address, flip-flop and latch | 0 | kept |
| `mode7` | The Mode 7 write-twice latch, `$211B M7A` and `$211C M7B` | `$FF`, `$FFFF`, `$FFFF` | kept |
| `background_mode` | `$2105 BGMODE` | `$0F` | kept |
| `backgrounds` | `$2107–$210A BGnSC`, `$210B/$210C BGnnNBA`, the eight scroll registers and their latch | 0 | kept |
| `main_screen`, `color_select` | `$212C TM`, `$2130 CGWSEL` | 0 | kept |
| `screen_mode` | `$2133 SETINI` | `$00` | `$00` |
| `ppu1_open_bus`, `ppu2_open_bus` | The two open-bus latches | 0 | kept |
| `counters` | The H/V counter latch, `$213C/$213D`, and the latch flag | `$1FF` | kept |
| `next_line` | The next line of the frame to draw | 1 | 1 |
| `signals` | What the bus passed in before the latest access | | |

- **The memories.** Each is a heap-allocated byte slice, never built on the stack (technical design section 5). No source says what they hold at power-on, so they are filled from the machine's power-on stream, continuing after WRAM: VRAM, then CGRAM, then OAM (FND-02; ADR 0028, point 1). The machine exposes them as `RegionId::Vram`, `Cgram` and `Oam`.
- **The registers.** fullsnes's I/O map gives each register's value at reset, and in brackets the ones reset leaves unchanged, with their power-on value (`fullsnes#snesiomap`). `$2105` is "(xFh)", kept on reset; its unknown high nibble is taken as 0. `$2133` is "00h?" (ADR 0028, point 5).
- **The ports' values.** fullsnes gives `$2100` as "8xh", `$2115` as "(?Fh)" and `$211B/$211C` as "(FFh)(w2)"; the unknown nibbles are taken as 0. The other port values are "(?)": 0 at power-on, a project choice, and kept on reset (ADR 0029, points 2, 3, 8 and 12).
- **What is kept so far.** Besides the ports, the PPU keeps `$2100`, `$2105` and `$2133`. Writes to the other registers have no effect until their issues: the layers and effects in #91 to #96.

## Registers

[`Register`](../../crates/sfes-snes-ppu/src/register.rs) names all 64 registers. Each variant's discriminant is its B-bus address, and its doc line gives the address and the documents' mnemonic (AGENTS.md section 4). `Register::ALL` lists them in address order, and the bus decodes B-bus `$00–$3F` with `Register::try_from`, which fails above `$3F`; nothing else in the PPU sees an address.

### Reads

Most PPU registers are write-only. What a read returns depends on where the register is (`anomie-regs#registers`, `fullsnes#snesunpredictablethings`; ADR 0028, point 3; ADR 0029):

| Registers | A read returns |
| --- | --- |
| `$21x4–$21x6`, `$21x8–$21xA` (x = 0, 1, 2) | PPU1 open bus, all 8 bits |
| `$21x0–$21x3`, `$21x7` (x = 0 to 3), `$21xB–$21xF` (x = 0, 1, 2) | CPU open bus, the data bus's last value |
| `$2134–$2136 MPYL–MPYH` | the [multiplier](#the-multiplier)'s product |
| `$2138`, `$2139/$213A`, `$213B` | the [memory ports](#the-memory-ports) |
| `$213C OPHCT`, `$213D OPVCT` | the latched counter, low byte then high, with PPU2 open bus in bits 7–1 of the high byte (`anomie-regs#213c`) |
| `$213E STAT77`, `$213F STAT78` | the [status registers](#the-status-registers) |

The two documents' lists agree. `$2137 SLHV` is a read strobe: it latches the counters while `$4201` bit 7 is set, and the data read is CPU open bus (`anomie-regs#2137`; ADR 0021, point 7).

### PPU1 and PPU2 open bus

Each chip keeps the last value read from its own readable ports (`fullsnes#snesunpredictablethings`; ADR 0028, point 4):

- **PPU1** remembers reads of `$2134–$2136`, `$2138–$213A` and `$213E`. The value shows up in the PPU1 open-bus registers above and in `$213E` bit 4.
- **PPU2** remembers reads of `$213B–$213D` and `$213F`. The value shows up in the second read of `$213B` (bit 7), the second read of `$213C/$213D` (bits 7–1) and `$213F` bit 5.

A read sets the latch to the whole byte it returned, so every read of these registers has side effects, and `peek_bus` says so (ADR 0029, point 11). A DMA transfer's B-bus reads count too, since they reach the PPU as the CPU's do (ADR 0025, point 7). Both latches are 0 at power-on, a choice no source settles, and reset keeps them.

## Signals

Some registers depend on things the PPU does not track in this design. Before each access the bus passes a [`Signals`](../../crates/sfes-snes-ppu/src/signals.rs) value (ADR 0029, point 1):

| Signal | From | Used by |
| --- | --- | --- |
| `period` | the scheduler: vertical blank, else horizontal blank (H=274 to H=1), else active (`anomie-timing#s-ppu/blanking-periods`) | which memory writes land |
| `field` | `$213F` bit 7 as the scheduler keeps it (ADR 0020, point 4) | `$213F` |
| `pal` | the video standard | `$213F` bit 4 |
| `external_latch` | `$4201` bit 7, wired to the PPU's latch input | whether a `$213F` read clears the latch flag |

A peek cannot sync, so `Ppu::peek_with` takes signals worked out for that moment.

## The memory ports

[`memory.rs`](../../crates/sfes-snes-ppu/src/memory.rs) holds the three ports (ADR 0029, points 3 to 5).

### VRAM

- **`$2115 VMAIN`.** Bit 7 picks the byte whose access increments the address: the low byte (`$2118`, `$2139`) or the high byte (`$2119`, `$213A`). Bits 1–0 give the step: 1, 32, 128 or 128 words. Bits 3–2 rotate the low 8, 9 or 10 bits of the address left by three, so `aaaaaaaaBBBccccc` becomes `aaaaaaaacccccBBB` (`anomie-regs#2115`, `fullsnes#snesmemoryvramaccesstileandbgmap`).
- **The address.** `$2116/$2117` set a 16-bit word address. An access uses it translated, with bit 15 dropped, since that line is not connected; the register itself never changes by translation. Each write to `$2116` or `$2117` loads the prefetch register from the new address.
- **Writes.** `$2118` writes the low byte of the word, `$2119` the high byte, and the address increments if `$2115` names that byte. Writes leave the prefetch alone.
- **Reads.** `$2139` and `$213A` return a byte of the prefetch register. If `$2115` names that byte, the prefetch is then loaded from the current address and the address increments. That is why the first word read after setting the address arrives twice (`anomie-regs#2139`; fullsnes's "Prefetch BEFORE Increment").

### CGRAM

- **`$2121 CGADD`** sets the colour, a word address, and resets the first/second-access flip-flop.
- **`$2122 CGDATA`.** The first access holds the low byte in a latch. The second writes the latch and the high byte together, without bit 7, which CGRAM does not store (`fullsnes#snesmemorycgramaccesspalettememory`).
- **`$213B CGDATAREAD`** returns the addressed byte; on the second access bit 7 is PPU2 open bus (`anomie-regs#213b`).
- Each access increments the 9-bit byte address.

### OAM

- **`$2102/$2103 OAMADDL/OAMADDH`** set a 9-bit reload value, and `$2103` bit 7 the priority bit, kept for the sprites (#94). Either write copies the reload value, doubled, into the 10-bit byte address (`fullsnes#snesmemoryoamaccessspriteattributes`).
- **`$2104 OAMDATA`.** An even byte is held in a latch. An odd byte below `$200` writes the latch and itself as one word. A byte from `$200` up writes itself into the high table at once, at `$200 + (address & $1F)` (`anomie-regs#2104`).
- **`$2138 OAMDATAREAD`** returns the addressed byte, the high table mirrored the same way.
- Each access increments the address, wrapping at `$400`. anomie's two examples, "01 02 01 03" and "00 00 01 02 01 03", are unit tests.

### When the memories can be written

anomie's flag columns mark which registers can be accessed in forced blank, vertical blank, horizontal blank and at any time (`anomie-regs#registers`). fullsnes: in forced blank the memories "can be freely accessed (otherwise it's accessible only during Vblank)" (`fullsnes#snesppucontrol`). The PPU applies them to the memory writes only (ADR 0029, point 7):

| Port | Lands in |
| --- | --- |
| `$2118/$2119` VRAM, `$2104` OAM | forced blank or vertical blank |
| `$2122` CGRAM | forced blank, vertical blank or horizontal blank |

A write outside those periods changes no memory, but the port still runs: the address increments and the latch is set. Reads are not restricted, since no source says what they return, and the address and control registers can be written at any time. This is the least certain decision of the issue.

### The OAM address reload

The OAM address is reloaded from the reload value (ADR 0029, point 6):

- when vertical blank begins, unless forced blank is on (`anomie-regs#2102`, `anomie-timing#oam-reset`);
- when a `$2100` write clears forced blank (`anomie-regs#sprites/oam`, `anomie-timing#oam-reset`).

fullsnes limits the second case to the first line of vertical blank (`fullsnes#snesmemoryoamaccessspriteattributes`); anomie does not. Not modelled: the reported H=10 of the first case, anomie-regs#2100's reload on any `$2100` write during that line, and the scrambling of the address during rendering, which no source describes.

## The multiplier

[`mode7.rs`](../../crates/sfes-snes-ppu/src/mode7.rs) (ADR 0029, point 8):

- **The latch.** `$210D`, `$210E` and `$211B–$2120` are write-twice registers sharing one latch: a write makes the register `(value << 8) | latch`, then sets the latch to the value (`anomie-regs#211b`). Until #91 and #93 only M7A and M7B keep their values.
- **The product.** `$2134–$2136` read the 24-bit two's-complement product of M7A, signed 16-bit, and the last byte written to `$211C`, signed 8-bit (`anomie-regs#2134`, `fullsnes#snesmathsmultiplydivide`). It is ready at once.
- **Power-on.** M7A and M7B are `$FFFF`, so the product is `$000001`, as fullsnes's I/O map lists.
- fullsnes reports garbage in Mode 7 outside blanking; the PPU always returns the product.

## The status registers

| Bit | `$213E STAT77` | `$213F STAT78` |
| --- | --- | --- |
| 7 | time over, 0 until #94 | the field |
| 6 | range over, 0 until #94 | the latch flag |
| 5 | master/slave, 0 | PPU2 open bus |
| 4 | PPU1 open bus | set on a PAL console |
| 3–0 | version 1 | version 3 |

Sources: `anomie-regs#213e`, `#213f`, `fullsnes#snespputimersandstatus`; ADR 0029, points 9 and 10.

- **The PPU2 version.** anomie has seen 2 and 3, fullsnes 1 to 3; no source ties a version to a console, so the PPU reports 3.
- **A `$213F` read** resets both counter flip-flops to the low byte. It clears the latch flag only while `$4201` bit 7 is set, as anomie says; fullsnes clears it on every read.

## Lines

[`Ppu::render_line`](../../crates/sfes-snes-ppu/src/ppu.rs) draws one line into a [`LineOut`](../../crates/sfes-snes-ppu/src/line.rs), from the registers and memories as they stand (technical design section 5; ADR 0028, point 7):

- **Width.** A line's `LineWidth` is `Wide`, 512 pixels, in BG modes 5 and 6 (`$2105` bits 2–0) or with pseudo-hires (`$2133` bit 3), otherwise `Normal`, 256 (`anomie-regs#2105`, `#2133`, `fullsnes#snesppuresolution`).
- **Interlace.** The line records whether `$2133` bit 0 was set.
- **Pixels.** A pixel is a CGRAM colour, an `sfes_api::Rgb555`: red in bits 4–0, green in 9–5, blue in 14–10, bit 15 clear (`fullsnes#snesppucolorpalettememorycgramanddirectcolors`). CGRAM does not store bit 15, so the PPU drops it, and `Rgb555` refuses a value with it set. Where no layer shows a pixel it is the backdrop, CGRAM entry 0 (`fullsnes#snesppuresolution`, "Hires Notes"). [The backgrounds](#the-backgrounds) draw the rest in BG modes 0 to 4; in the other modes every pixel is the backdrop until #93 and #97.

The machine decides when a line is drawn and how lines make a frame ([`core.md`](core.md#drawing-and-presenting-frames)).

## The backgrounds

[`background.rs`](../../crates/sfes-snes-ppu/src/background.rs) holds BG1 to BG4's registers and finds one pixel of one background; `render_line` puts them in order (ADR 0030). The main screen of BG modes 0 to 4 is drawn; everything else waits for its issue.

### Where a pixel comes from

- **Scroll.** The pixel at column `x` of line `L` is the background's (`x + HOFS`, `L + VOFS`), each taken modulo the background's size: 256, 512 or 1024 pixels, from 32 or 64 tiles of 8 or 16 pixels (`anomie-regs#backgrounds/bg-scrolling`). A scroll register is written twice, low byte then high, with one latch for all eight, which the Mode 7 registers do not share (`anomie-regs#210d`). Games set `VOFS` to −1 because line 0 is not output, so the row shown is `L + VOFS`, not the frame's row.
- **Tilemap.** `$2107–$210A` give the map's address in 1K-word steps and, in bits 1–0, a second 32×32 screen to the right, below, or both, in the order A B / C D, each 0x400 words (`anomie-regs#2107`, `fullsnes#snesppubgcontrol`). An entry is `vhopppcc cccccccc`: flips, priority, palette and a 10-bit tile number (`anomie-regs#backgrounds/tile-maps-and-character-maps`).
- **Characters.** `$210B/$210C` give each background's base in 8 KiB steps, and a tile is `8 × planes` bytes from there. A row is a byte per plane, the leftmost pixel in bit 7; planes come in pairs, a pair to sixteen bytes. With `$2105`'s size bit set a tile is 16×16: the tile, the next, and the two sixteen on, flipped as a whole.

### Colour

A value of 0 is transparent. Otherwise:

| Depth | Where | CGRAM entry |
| --- | --- | --- |
| 2 planes | mode 0, BG*n* | `32 × (n − 1) + palette × 4 + value` |
| 2 planes | modes 1 and 4 | `palette × 4 + value` |
| 4 planes | modes 1 to 3 | `palette × 16 + value` |
| 8 planes | modes 3 and 4 | `value`, or direct colour |

Direct colour, `$2130` bit 0, makes the colour from the character `BBGGGRRR` and the palette `bgr`: `BBb00 RRRr0 GGGg0` (`anomie-regs#backgrounds/direct-color-mode`).

### Which layer shows

The first of these, front to back, that `$212C` turns on and that is not transparent; else the backdrop. "Low" and "high" are the entry's priority bit. Sprites will join them in #94.

| Mode | Order |
| --- | --- |
| 0 | BG1 high, BG2 high, BG1 low, BG2 low, BG3 high, BG4 high, BG3 low, BG4 low |
| 1 | BG1 high, BG2 high, BG1 low, BG2 low, BG3 high, BG3 low; with `$2105` bit 3, BG3 high first |
| 2, 3, 4 | BG1 high, BG2 high, BG1 low, BG2 low |

### Not yet

- Offset-per-tile (#92): modes 2 and 4 use each background's own scroll.
- Modes 5, 6 and 7 (#97, #93): the backdrop.
- Sprites (#94), windows (#95), the sub screen, colour math, mosaic, brightness and forced blank (#96): not applied. A wide line repeats each main-screen pixel.

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
| [`ppu.rs`](../../crates/sfes-snes-ppu/src/ppu.rs) | `Ppu`: the state, power-on and reset, `peek`, `after_read`, `write`, the status registers, the OAM reload, the counter latch and `render_line` |
| [`register.rs`](../../crates/sfes-snes-ppu/src/register.rs) | `Register`, `ReadKind` and `NotAPpuRegister`: the 64 registers, the address conversion and what a read of each returns |
| [`signals.rs`](../../crates/sfes-snes-ppu/src/signals.rs) | `Signals` and `Period`: what the bus passes in before each access |
| [`memory.rs`](../../crates/sfes-snes-ppu/src/memory.rs) | The VRAM, CGRAM and OAM ports |
| [`background.rs`](../../crates/sfes-snes-ppu/src/background.rs) | BG1 to BG4's registers, the pixel fetch and each mode's priority order |
| [`mode7.rs`](../../crates/sfes-snes-ppu/src/mode7.rs) | The Mode 7 latch, M7A, M7B and the multiplier |
| [`counters.rs`](../../crates/sfes-snes-ppu/src/counters.rs) | The H/V counter latch, its flip-flops and the latch flag |
| [`line.rs`](../../crates/sfes-snes-ppu/src/line.rs) | `LineOut` and `LineWidth`: one drawn line and its width |

The crate is `no_std` with `alloc` and depends only on `sfes-api` and `thiserror` (technical design section 12). The machine implements its bus's `Registers` trait for `Ppu` in [`bus/ppu.rs`](../../crates/sfes-snes-core/src/bus/ppu.rs).

## Testing

- **Unit tests** in each file cover:
  - the register table and each register's read kind against both documents' lists;
  - the power-on fill and reset;
  - the open-bus latches and the counters;
  - each line's width and colour;
  - each port's increments, translation and prefetch, with anomie's OAM examples;
  - which writes land in each period, the OAM reload, the multiplier and the status bits;
  - a background's tiles, planes, flips, tilemap sizes, scroll writes and wrap, each mode's palettes and priority order, direct colour, and a line's composition.
- **The machine's tests** cover reads through the bus, when lines are drawn, overscan and interlaced frames, the frame geometry, VRAM writes by period, the OAM reload at vertical blank, `$213F` through the bus and a DMA into VRAM ([`core.md`](core.md#testing)).

- **Test cartridges** `bg-mode0` to `bg-mode4` (`tests/carts/`) draw each mode; `carts.rs` in `sfes-snes-core` compares each picture with values computed from the documents. Goldens follow in #100.

## Decisions

| ADR | Decision | Confidence |
| --- | --- | --- |
| [0028](../adr/0028-ppu-state-registers-and-frame-geometry.md) | The memories and their power-on fill, the registers and their reads, PPU1 and PPU2 open bus, which lines are drawn and when, a line's width and pixels, the frame geometry, and where the outputs live | high for the reads and open bus; medium for which lines are output and their width; the power-on values and the geometry are project choices |
| [0029](../adr/0029-ppu-memory-ports-multiplier-and-status-registers.md) | The signals, `$2100`, the VRAM, CGRAM and OAM ports, the OAM reload, which writes land in each period, the multiplier, `$213E` and `$213F`, and the ports' power-on values | high for the ports, the multiplier and the status bits; medium for the reload and `$2100`'s reset value; low for access by period, the PPU2 version and the latch flag rule |
| [0030](../adr/0030-backgrounds-in-bg-modes-0-to-4.md) | The background registers, the scroll write rule, where a tile and a pixel are, the palette and direct colour of each mode and depth, each mode's priority order, which row a line shows, and what is left to the other layer issues | high for the formats, palettes and orders; medium for the scroll write and the row |

ADR 0021, point 7 covers the counter latch, which moved into the PPU with this issue.
