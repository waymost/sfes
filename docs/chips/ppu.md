# The PPU (`sfes-snes-ppu`)

This page explains the SNES's picture processing unit and how the crate models it (QA-08). It is written issue by issue during milestone M2: #89 to #95 so far. Every statement cites its source, by its id in [`provenance/bibliography.toml`](../../provenance/bibliography.toml):

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
| `mode7` | The Mode 7 write-twice latch, `$211A M7SEL`, `$211B–$211E M7A–M7D`, `$211F/$2120 M7X/M7Y` and the Mode 7 scroll `M7HOFS/M7VOFS` | `$FF`, then 0 except M7A and M7B, `$FFFF` | kept |
| `background_mode` | `$2105 BGMODE` | `$0F` | kept |
| `sprites` | `$2101 OBSEL` and the time-over and range-over flags of `$213E` | 0 | kept |
| `windows` | `$2123–$212B` the window settings, edges and logic, and `$212E TMW`, `$212F TSW` | 0 | kept |
| `backgrounds` | `$2107–$210A BGnSC`, `$210B/$210C BGnnNBA`, the eight scroll registers and their latch | 0 | kept |
| `main_screen`, `sub_screen` | `$212C TM`, `$212D TS` | 0 | kept |
| `color_math` | `$2130 CGWSEL`, `$2131 CGADSUB` and the fixed colour of `$2132 COLDATA` | 0 | kept |
| `mosaic` | `$2106 MOSAIC` and the line its block rows start at | 0, line 1 | kept |
| `screen_mode` | `$2133 SETINI` | `$00` | `$00` |
| `ppu1_open_bus`, `ppu2_open_bus` | The two open-bus latches | 0 | kept |
| `counters` | The H/V counter latch, `$213C/$213D`, and the latch flag | `$1FF` | kept |
| `next_line` | The next line of the frame to draw | 1 | 1 |
| `signals` | What the bus passed in before the latest access | | |

- **The memories.** Each is a heap-allocated byte slice, never built on the stack (technical design section 5). No source says what they hold at power-on, so they are filled from the machine's power-on stream, continuing after WRAM: VRAM, then CGRAM, then OAM (FND-02; ADR 0028, point 1). The machine exposes them as `RegionId::Vram`, `Cgram` and `Oam`.
- **The registers.** fullsnes's I/O map gives each register's value at reset, and in brackets the ones reset leaves unchanged, with their power-on value (`fullsnes#snesiomap`). `$2105` is "(xFh)", kept on reset; its unknown high nibble is taken as 0. `$2133` is "00h?" (ADR 0028, point 5).
- **The ports' values.** fullsnes gives `$2100` as "8xh", `$2115` as "(?Fh)" and `$211B/$211C` as "(FFh)(w2)"; the unknown nibbles are taken as 0. The other port values are "(?)": 0 at power-on, a project choice, and kept on reset (ADR 0029, points 2, 3, 8 and 12).
- **What is kept so far.** Besides the ports, the PPU keeps `$2100`, `$2105`, `$2133`, `$2101` and the registers of the backgrounds and mode 7. `$2106`, `$212C–$212F` and `$2130–$2132` are kept too (ADR 0035). Writes to the other registers have no effect until their issues.

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

- **`$2102/$2103 OAMADDL/OAMADDH`** set a 9-bit reload value, and `$2103` bit 7 the priority bit, kept for the sprites, whose priority rotation reads it (ADR 0033). Either write copies the reload value, doubled, into the 10-bit byte address (`fullsnes#snesmemoryoamaccessspriteattributes`).
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

- **The latch.** `$210D`, `$210E` and `$211B–$2120` are write-twice registers sharing one latch: a write makes the register `(value << 8) | latch`, then sets the latch to the value (`anomie-regs#211b`). The other registers sharing it are the Mode 7 matrix, centre and scroll ([Mode 7](#mode-7-and-extbg)).
- **The product.** `$2134–$2136` read the 24-bit two's-complement product of M7A, signed 16-bit, and the last byte written to `$211C`, signed 8-bit (`anomie-regs#2134`, `fullsnes#snesmathsmultiplydivide`). It is ready at once.
- **Power-on.** M7A and M7B are `$FFFF`, so the product is `$000001`, as fullsnes's I/O map lists.
- fullsnes reports garbage in Mode 7 outside blanking; the PPU always returns the product.

## The status registers

| Bit | `$213E STAT77` | `$213F STAT78` |
| --- | --- | --- |
| 7 | time over (more than 34 sprite tiles on a line) | the field |
| 6 | range over (more than 32 sprites on a line) | the latch flag |
| 5 | master/slave, 0 | PPU2 open bus |
| 4 | PPU1 open bus | set on a PAL console |
| 3–0 | version 1 | version 3 |

Sources: `anomie-regs#213e`, `#213f`, `fullsnes#snespputimersandstatus`; ADR 0029, points 9 and 10; ADR 0033, point 7.

- **The sprite flags** are set when a line is evaluated, whether or not the sprites are on the main screen, and cleared when line 0 starts unless forced blank is on (`anomie-regs#213e`, `fullsnes#snespputimersandstatus`).

- **The PPU2 version.** anomie has seen 2 and 3, fullsnes 1 to 3; no source ties a version to a console, so the PPU reports 3.
- **A `$213F` read** resets both counter flip-flops to the low byte. It clears the latch flag only while `$4201` bit 7 is set, as anomie says; fullsnes clears it on every read.

## Lines

[`Ppu::render_line`](../../crates/sfes-snes-ppu/src/ppu.rs) draws one line into a [`LineOut`](../../crates/sfes-snes-ppu/src/line.rs), from the registers and memories as they stand (technical design section 5; ADR 0028, point 7):

- **Width.** A line's `LineWidth` is `Wide`, 512 pixels, in BG modes 5 and 6 (`$2105` bits 2–0) or with pseudo-hires (`$2133` bit 3), otherwise `Normal`, 256 (`anomie-regs#2105`, `#2133`, `fullsnes#snesppuresolution`).
- **Interlace.** The line records whether `$2133` bit 0 was set.
- **Pixels.** A pixel is a CGRAM colour, an `sfes_api::Rgb555`: red in bits 4–0, green in 9–5, blue in 14–10, bit 15 clear (`fullsnes#snesppucolorpalettememorycgramanddirectcolors`). CGRAM does not store bit 15, so the PPU drops it, and `Rgb555` refuses a value with it set. Where no layer shows a pixel it is the backdrop, CGRAM entry 0 (`fullsnes#snesppuresolution`, "Hires Notes"). [The backgrounds](#the-backgrounds) draw the rest. In a wide line each column is two pixels, the sub screen's and then the main screen's ([Hi-res and interlace](#hi-res-and-interlace)).

The machine decides when a line is drawn and how lines make a frame ([`core.md`](core.md#drawing-and-presenting-frames)).

## The backgrounds

[`background.rs`](../../crates/sfes-snes-ppu/src/background.rs) holds BG1 to BG4's registers and finds one pixel of one background; `render_line` puts them in order (ADR 0030). Modes 0 to 7 are drawn; the sprites, windows, colour math, mosaic and hi-res have sections of their own.

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

The first of these, front to back, that `$212C` turns on and that is not transparent; else the backdrop. "Low" and "high" are the entry's priority bit; `S0`–`S3` are the sprites of that OAM priority (ADR 0033).

| Mode | Order |
| --- | --- |
| 0 | S3, BG1 high, BG2 high, S2, BG1 low, BG2 low, S1, BG3 high, BG4 high, S0, BG3 low, BG4 low |
| 1 | S3, BG1 high, BG2 high, S2, BG1 low, BG2 low, S1, BG3 high, S0, BG3 low; with `$2105` bit 3, BG3 high first |
| 2, 3, 4, 5 | S3, BG1 high, S2, BG2 high, S1, BG1 low, S0, BG2 low |
| 6 | S3, BG1 high, S2, S1, BG1 low, S0 |
| 7 | S3, S2, S1, BG1, S0; with EXTBG: S3, S2, BG2 high, S1, BG1, S0, BG2 low |

### Offset-per-tile

In modes 2 and 4 BG3 is not drawn: its tilemap holds the scrolls of BG1 and BG2 (ADR 0031).

- **Which column.** BG*n*'s visible 8-pixel columns are counted from 0 as `(x + (HOFS & 7)) / 8`. Column 0 is as normal; column *T* reads BG3's entry at pixel `(BG3HOFS & ~7) + 8 × (T − 1)` (`anomie-regs#backgrounds/mode-2`).
- **The entry.** Bits 9–0 are the offset; bit 13 applies it to BG1 and bit 14 to BG2 (`fullsnes#snesppuvideomemoryvram`). A horizontal offset replaces `HOFS` with `(HOFS & 7) | (offset & ~7)`; a vertical one replaces `VOFS` whole.
- **Mode 2** reads two entries per column: the horizontal one in BG3's row `BG3VOFS`, the vertical one in the row 8 pixels below. **Mode 4** reads one, in the first row; with bit 15 set it is the vertical offset, else the horizontal (`anomie-regs#backgrounds/mode-4`). The line number never selects BG3's row.
- The columns' alignment with BG1's fine scroll (medium) and mode 4's row and valid bit (low) are the least certain points; see the ADR.

### Mode 7 and EXTBG

[`mode7.rs`](../../crates/sfes-snes-ppu/src/mode7.rs) draws mode 7's one 256-colour background (ADR 0032). The tilemap and character registers are not used.

- **Registers.** `$211A M7SEL`: bit 7 makes the playfield larger than the tilemap, bit 6 then fills outside with character 0 instead of transparent, bits 1 and 0 flip the screen. `$211B–$211E M7A–M7D` are the signed 8.8 matrix; `$211F/$2120 M7X/M7Y` the centre and `$210D/$210E M7HOFS/M7VOFS` the scroll, each 13 bits signed (`anomie-regs#211a`, `#211b`, `#211f`). All are write-twice on the one latch.
- **The coordinate.** For line `L` and column `x`, with `ox = CLIP(HOFS − CX)` and `oy = CLIP(VOFS − CY)` (10 bits and a sign), `X = (A·ox & ~63) + (B·oy & ~63) + (B·L & ~63) + A·x + (CX << 8)` and `Y` likewise from C and D, with 8 fraction bits. Both documents give this (`anomie-regs#211f`, `fullsnes#snesppurotationscaling`). A flip XORs `x` or `L` with `$FF`.
- **The pixel.** The playfield position is `X >> 8`, `Y >> 8`. The tilemap is 128 × 128 one-byte entries in the low bytes of VRAM words; a character is 64 words, one pixel in each high byte. Outside 0–1023, the playfield wraps, is transparent, or shows character 0 at the low three bits of each coordinate. Pixel 0 is transparent. BG1 may be direct colour, with palette bits 0.
- **EXTBG.** With `$2133` bit 6, BG2 shows the same pixel: bit 7 is its priority and bits 6–0 the CGRAM index. Front to back: BG2 high, BG1, BG2 low.
- The flip's use of the line number, the unlisted registers' power-on value (0) and EXTBG outside mode 7 (nothing) are the least certain points; see the ADR.

## Sprites

[`sprite.rs`](../../crates/sfes-snes-ppu/src/sprite.rs) evaluates and draws the 128 sprites (ADR 0033).

- **OAM.** Sprite *n*'s low-table record is `X`, `Y`, tile, `vhoopppN` at `n × 4`; its high-table bits are X bit 8 (the sign of a 9-bit X) and the size flag, 2 bits at `$200 + n / 4`, bit `(n % 4) × 2` (`anomie-regs#sprites`).
- **Sizes.** `$2101 OBSEL` bits 7–5 pick the small and large sizes: 8×8 and 16×16, 8×8 and 32×32, 8×8 and 64×64, 16×16 and 32×32, 16×16 and 64×64, 32×32 and 64×64, and the undocumented 16×32 and 32×64, and 16×32 and 32×32 (`anomie-regs#2101`).
- **Which lines.** A sprite is on line `L` when `(L − 1 − Y) mod 256` is below its height, so Y 0 starts on line 1 and a tall sprite wraps from the bottom edge to the top. This rests on fullsnes setting the flag at `YLOC + 1` and anomie's line 0 that is not output.
- **Tiles.** Characters are in two 16×16 tables that wrap without carries. The first word of a sprite's first tile is `((base << 13) + (tile << 4) + (N ? (name + 1) << 12 : 0)) & $7FFF`. A flip turns the whole sprite; a rectangular sprite flips vertically as two squares (`anomie-regs#sprites`). A pixel is 4 bits, 0 is transparent, and the colour is CGRAM `128 + palette × 16 + pixel`.
- **Per line.** Evaluation takes the first 32 sprites in range from the front sprite, wrapping at 127, and sets range over for a 33rd. It then loads up to 34 of their tiles, from the last sprite to the first, left to right, counting only tiles with X above −8 and below 256, and sets time over beyond that. An X of −256 counts as 0 for both but keeps its true place for drawing. Evaluation always runs; the pixels are drawn only when `$212C` bit 4 is set (`anomie-regs#sprites/drawing-the-sprites`).
- **Priority.** The front sprite is sprite 0, or with `$2103` bit 7 the sprite `(OAM byte address >> 2) & 127`. Among the loaded tiles the sprite nearer the front covers the others, and its OAM priority picks the sprite's place in the mode's order.
- The line a sprite starts on, which tiles time over drops, evaluation in forced blank and the power-on values are the least certain points; see the ADR. anomie's "major oddity" of the rotation is not modelled.

## Windows

[`window.rs`](../../crates/sfes-snes-ppu/src/window.rs) holds the window registers and masks layers on the main and sub screens (ADR 0034; ADR 0035).

- **Edges.** `$2126–$2129` give window 1's and window 2's left and right pixel, inclusive. A window whose left edge is above its right is empty (`anomie-regs#2126`, `fullsnes#snesppuwindow`).
- **Settings.** Each layer has four bits in `$2123–$2125`: window 1's enable (bit 1) and invert (bit 0), window 2's (bits 3 and 2). BG1, BG3 and OBJ use the low half of their register, BG2, BG4 and the colour window the high half. fullsnes's value table for these bits has a slip; the bit letters of `anomie-regs#2123` are used (`fullsnes#snesppuwindow`).
- **Area.** With neither window enabled the area is empty. With one, it is that window, inverted if its bit says. With both, the layer's two logic bits in `$212A/$212B` pick OR, AND, XOR or XNOR (`anomie-regs#212a`).
- **Masking.** A layer whose `$212E` bit is set has no pixel where its area is true, so the layers behind it show; the backdrop is never masked. The sprite layer is masked as a whole, and sprite evaluation and its flags do not depend on windows.
- **The sub screen.** `$212F` masks the sub screen's layers the way `$212E` does the main's, and the colour window's area, from its own setting, is read by colour math (ADR 0035).

## Colour math, brightness and mosaic

[`color_math.rs`](../../crates/sfes-snes-ppu/src/color_math.rs) and [`mosaic.rs`](../../crates/sfes-snes-ppu/src/mosaic.rs) hold the registers, and `render_line` puts them to use (ADR 0035).

- **Two screens.** The main screen's pixel is the front one of `$212C`'s layers, the sub screen's of `$212D`'s, each with its own window mask (`$212E`, `$212F`); the main backdrop is CGRAM entry 0 and the sub backdrop the fixed colour of `$2132`. The sub screen is only looked at when `$2130` bit 1 and `$2131` ask for it.
- **The colour window.** Its area, from `$2125` and `$212B`, picks where `$2130` bits 7 to 6 clip the main pixel to black and bits 5 to 4 prevent math: never, outside, inside or always (`anomie-regs#2130`). fullsnes numbers the math field the other way round, to the same effect.
- **Math.** A pixel of BG1 to BG4, the sprites or the backdrop takes math when its bit in `$2131` is set; a sprite with palette 0 to 3 never does. The other operand is the fixed colour, or the sub screen's pixel with `$2130` bit 1. Per channel it adds or subtracts, halves (rounding down) if asked, and clamps to 0 to 31. A clipped pixel, and the fixed colour standing in for the sub backdrop, are not halved.
- **Brightness and forced blank.** Forced blank makes the line black; otherwise each channel is scaled by `(N + 1) / 16` for brightness `N`, and 0 is black. Power-on forced blank means a PPU draws black until a game turns the screen on.
- **Mosaic.** A pixelated background shows the pixel at the top-left of its block, before windows and math; block rows count from the first line of the frame, or from the line a changed `$2106` was written. Mode 7's EXTBG layer uses bit 1 for across and bit 0 for down.
- Rounding in brightness, which writes restart the blocks, and whether the sub pixel must be in front of the main pixel are not settled by the documents; see the ADR.


## Hi-res and interlace

The code is in [`background.rs`](../../crates/sfes-snes-ppu/src/background.rs) (the tile), [`mosaic.rs`](../../crates/sfes-snes-ppu/src/mosaic.rs), [`color_math.rs`](../../crates/sfes-snes-ppu/src/color_math.rs) (`Previous`) and `render_line` (ADR 0036).

- **Two pixels a column.** A wide line, BG mode 5 or 6 or `$2133` bit 3, has the sub screen's pixel `x` as output pixel `2x` and the main screen's as `2x + 1` (`anomie-regs#2133`). Bit 3 does nothing in modes 5 and 6. The sub screen is drawn in every wide line, and its backdrop there is CGRAM entry 0, not the fixed colour (`fullsnes#snesppuresolution`).
- **Modes 5 and 6 tiles.** A tile is 8 pixels, 16 half-pixels, wide whatever the size bit, and 8 or 16 rows high: its tile on the left and the next on the right (`anomie-regs#backgrounds/mode-5`). The main screen shows the odd half-pixels and the sub screen the even, so a game sets `$212C` and `$212D` alike to see the whole picture. Scrolling counts full pixels; a flip turns the 16 half-pixels as one, which swaps what the screens show. Mode 5 has a 16-colour BG1 and a 4-colour BG2; mode 6 only BG1.
- **Mode 6's offset-per-tile** works as mode 2's, BG3 giving one entry for each 8-pixel column whatever its size bit says (`anomie-regs#backgrounds/mode-6`).
- **Interlace** matters only in modes 5 and 6: line `L` shows row `2L + F + VOFS`, `F` being the field from `$213F` bit 7, so the vertical scroll counts half-lines and `VOFS = -2` shows row 0 on the first line (`anomie-regs#backgrounds/bg-scrolling`). Which field has the even half-lines is not documented; the first does here. Elsewhere interlace changes no pixel.
- **Mosaic** in modes 5 and 6 takes the even half-pixel, and with interlace the even half-line, of each block, so size 1 has an effect and anomie's "BRBR" example holds. In pseudo-hires it is as normal (`anomie-regs#rendering-the-screen/mosaic`).
- **Math on sub pixels.** A sub pixel is clipped, halved or not and given the fixed colour, or the main pixel to its left, as that main pixel was; column 0 has nothing to its left and none of it. The colour window effect is read from that main pixel too (`anomie-regs#rendering-the-screen/color-math`, `#2133`).
- **Not modelled.** OBJ interlace (`$2133` bit 1) and fullsnes's window glitch in hi-res: the documents give no rule. Sprites are the same on both screens and in both fields.

## Frame geometry

How lines are presented as a `VideoFrame` and hashed (ADR 0028, points 8 and 9):

| Frame | Size |
| --- | --- |
| Plain | 256 × 224, or 256 × 239 with overscan |
| Any line in BG mode 5 or 6, or with pseudo-hires | 512 wide; a 256-pixel line, in the same frame, has each pixel twice |
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
| [`mode7.rs`](../../crates/sfes-snes-ppu/src/mode7.rs) | The Mode 7 registers and latch, the multiplier, and mode 7's and EXTBG's pixels |
| [`sprite.rs`](../../crates/sfes-snes-ppu/src/sprite.rs) | `$2101`, the per-line evaluation with its limits and flags, and the sprites' pixels |
| [`window.rs`](../../crates/sfes-snes-ppu/src/window.rs) | The window registers, a layer's area and its masking on the main and sub screens |
| [`color_math.rs`](../../crates/sfes-snes-ppu/src/color_math.rs) | `$2130–$2132`, which pixels take math, the blend and the brightness scale |
| [`mosaic.rs`](../../crates/sfes-snes-ppu/src/mosaic.rs) | `$2106` and each background's block corner |
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
  - a background's tiles, planes, flips, tilemap sizes, scroll writes and wrap, each mode's palettes and priority order, direct colour, offset-per-tile in modes 2 and 4, mode 7's matrix, scroll, centre, flips, fills and EXTBG, the sprites' sizes, record, rows, tile addresses, flips, planes, limits and flags, the windows' edges, inversion, logic and designation, `$2132`'s planes, the clip and prevent regions, which layers and sprite palettes take math, add, subtract, half and clamping, the sub screen and its backdrop, brightness, forced blank, mosaic blocks, a line's composition, mode 5 and 6 tiles and offsets, the two screens of a wide line, math on its sub pixels, half-pixel mosaic and interlaced rows.
- **The machine's tests** cover reads through the bus, when lines are drawn, overscan and interlaced frames, the frame geometry, VRAM writes by period, the OAM reload at vertical blank, `$213F` through the bus and a DMA into VRAM ([`core.md`](core.md#testing)).

- **Test cartridges** `bg-mode0` to `bg-mode4` and `bg-opt-mode2`, `bg-opt-mode4` and `bg-mode7` (`tests/carts/`) draw each mode, its offset-per-tile and mode 7's scaling, scroll and wrap, `hires` draws a mode 5 tile as sub and main half-pixels with colour math on both, `obj-sprites` draws overlapping and flipped sprites and sets range over, `windows` masks backgrounds by window logic, inversion and designation, and `color-math` adds the sub screen or the fixed colour with half-math, clips a tile in the colour window and pixelates a background; `carts.rs` in `sfes-snes-core` compares each picture with values computed from the documents. Goldens follow in #100.

## Decisions

| ADR | Decision | Confidence |
| --- | --- | --- |
| [0028](../adr/0028-ppu-state-registers-and-frame-geometry.md) | The memories and their power-on fill, the registers and their reads, PPU1 and PPU2 open bus, which lines are drawn and when, a line's width and pixels, the frame geometry, and where the outputs live | high for the reads and open bus; medium for which lines are output and their width; the power-on values and the geometry are project choices |
| [0029](../adr/0029-ppu-memory-ports-multiplier-and-status-registers.md) | The signals, `$2100`, the VRAM, CGRAM and OAM ports, the OAM reload, which writes land in each period, the multiplier, `$213E` and `$213F`, and the ports' power-on values | high for the ports, the multiplier and the status bits; medium for the reload and `$2100`'s reset value; low for access by period, the PPU2 version and the latch flag rule |
| [0030](../adr/0030-backgrounds-in-bg-modes-0-to-4.md) | The background registers, the scroll write rule, where a tile and a pixel are, the palette and direct colour of each mode and depth, each mode's priority order, which row a line shows, and what is left to the other layer issues | high for the formats, palettes and orders; medium for the scroll write and the row |
| [0031](../adr/0031-offset-per-tile-in-bg-modes-2-and-4.md) | Which backgrounds take offsets, the BG3 entry's bits, which BG3 column and row each column reads, mode 4's single entry, and mode 6 left to #97 | high for the entry and mode 2; medium for the column and row; low for mode 4's row and valid bit |
| [0032](../adr/0032-mode-7-and-extbg.md) | The Mode 7 registers and their latch, the matrix formula, the playfield's wrap and fills, the pixel and its address, EXTBG's BG2 and its priority order | high for the registers, formula and addressing; medium for the flip and the power-on values; low for EXTBG's detail |
| [0033](../adr/0033-sprites.md) | `$2101` and the OAM record, which lines a sprite is on, the tile addresses and flips, the range and time limits and their flags, the priority rotation, and the sprites' places in each mode's order | high for the formats, limits and order; medium for the first line, which tiles time over drops and when the flags clear; low for forced blank and the rotation's oddity |
| [0034](../adr/0034-windows.md) | The window registers, a layer's area from the two windows, inversion and logic, and masking by `$212E`, the sprite layer as a whole | high for the registers and logic; medium for masking the whole sprite layer; low for power-on values |
| [0035](../adr/0035-colour-math-brightness-and-mosaic.md) | The sub screen and its backdrop, the colour window's clip and prevent regions, which pixels take math and how it blends, brightness and forced blank, and mosaic blocks | high for the registers, the blend and the colour window; medium for half and the mosaic geometry; low for brightness rounding, the block restart and power-on values |
| [0036](../adr/0036-hi-res-pseudo-hi-res-and-interlace.md) | A wide line's two screens, the mode 5 and 6 tile, interlaced rows, half-pixel mosaic, math on sub pixels, and mode 6's offsets | high for the two screens and the tile; medium for mosaic and math; low for the field, the flip and column 0 |

ADR 0021, point 7 covers the counter latch, which moved into the PPU with this issue.
