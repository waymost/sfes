# The machine (`sfes-snes-core`)

This page explains the SNES machine and how the crate models it (QA-08). It was written issue by issue during milestone M2 (#79 to #87) and covers the crate as built: the machine value, the bus, the scheduler, interrupts, the math unit, the controller ports, DMA, HDMA, the APU ports' placeholder, and how the machine draws and presents the PPU's frames. The PPU itself has its own page, [`ppu.md`](ppu.md). Every statement cites its source, by its id in [`provenance/bibliography.toml`](../../provenance/bibliography.toml):

- Anomie's memory-mapping doc (`anomie-memmap`) and timing doc (`anomie-timing`), by heading path;
- fullsnes (`fullsnes`), by section anchor;
- Vigna's SplitMix64 description (`vigna-splitmix`), archived in [`provenance/archive/vigna-splitmix/`](../../provenance/archive/vigna-splitmix/);
- anomie's register doc (`anomie-regs`), by register;
- the project's ADRs, listed under [Decisions](#decisions).

## The machine value

[`Snes<A, O>`](../../crates/sfes-snes-core/src/machine.rs) is one console as one owned value, `Clone + Send` (FND-01). `A` is the accuracy profile (`Fast` by default) and `O` the observer (`NoopObserver` by default, which compiles away; FND-05). It holds:

| Part | What | Cloned? |
| --- | --- | --- |
| `State::cpu` | The 65C816's registers and run state | yes |
| `State::bus` | WRAM, the cartridge, the data bus value, the WRAM port address, `$420D`, the interrupt state (`$4200` bit 7, the NMI and IRQ flags, the NMI edge latch and the recorded lines), `$4201`, the PPU (its memories, registers and the H/V counter latch; [`ppu.md`](ppu.md)), the multiply and divide unit, the controller ports (the held buttons, the pads, `$4016`, the auto-read and `$4218–$421F`, and the polled flags), the DMA channels, a requested transfer and HDMA's flags, the APU ports' placeholder, the clock, and the scheduler (the video standard, line, field, vertical blank, line and frame counts, refresh, the H/V timer and the deadline table) | yes |
| `State::meta` | The seed of the last power-on | yes |
| `observer` | The observer | yes |
| `State::bus.video` | The frame being drawn and the last presented frame | no: the bus's clone starts it with `Video::default()` (technical design section 5; ADR 0028, point 10) |

The cartridge's ROM and memory map are `Arc`-shared, so a clone copies WRAM, the PPU's memories and the registers, and bumps two counts.

`Snes::builder(rom)` loads the cartridge (ADR 0016, ADR 0017) and powers on, or returns a `BuildError`. It never yields a half-built machine. Options:
- `.region(Region)` forces NTSC or PAL;
- `.board(Mapping)` skips header scoring;
- `.seed(u64)` sets the power-on seed, 0 by default (FND-02);
- `.observer(o)` attaches an observer.

Until `impl System` arrives in M4, the machine has an inherent API with the same shapes: `run(Stop)`, `set_input`, `polled`, `reset`, `power_cycle`, `region` and `region_mut` (WRAM, ROM, cartridge RAM, VRAM, CGRAM and OAM), `peek_bus` and `video` (FND-06, FND-08).

## Power-on and reset

- **Power-on.** WRAM is filled from `SplitMix64::new(seed)`, eight bytes per output, little-endian, from `$7E:0000` up (`vigna-splitmix`, FND-02, ADR 0018). The same stream then fills VRAM, CGRAM and OAM (ADR 0028, point 1). The CPU starts from `Cpu65816::new()`, the clock from 0, and the reset sequence runs at once.
- **RESB.** No document says how long the SNES holds RESB, so the machine spends no time on it. `Cycles(0)` is the moment RESB rises, and the reset sequence's seven cycles are the first ones charged (ADR 0018).
- **The reset button.** It "leaves WRAM intact" (`fullsnes#snescontrollersotherinputs`). The SNES "returns to a known timing position on reset" (`anomie-timing`), so `reset` restarts the line and frame position at H=0, V=0 with field 0 (`fullsnes#snestiminghvevents`) and the refresh schedule, clears `$420D`, the scheduler's copy of `$2133` and the WRAM port address (`fullsnes#snesiomap`), then runs the CPU's reset sequence. Cartridge RAM is kept as well.
- **The page-1 reads.** The reset sequence reads three stack bytes in page 1 of bank `$00` without writing (ADR 0011). On the SNES that page is WRAM (`anomie-memmap#memory-map`), so the reads have no side effects. They still reach observers as `on_read`.
- **Region.** The builder's region wins; otherwise the header's country code decides (ADR 0016); otherwise NTSC (ADR 0018).

## Observer events

The CPU core calls no observer (`docs/chips/cpu.md`, "Notes for M2"). The machine works out before each step which of ADR 0004's cases it will take, from the run state, the I flag and the pending requests, and reports it afterwards (ADR 0018):

| Step | Event |
| --- | --- |
| An instruction, or one byte of a block move | `on_instruction` with the program address, the opcode and the time its fetch began |
| An NMI or IRQ entry | `on_interrupt(Interrupt::Nmi)` if an NMI is latched, else `Interrupt::Irq`, at the time the step began |
| A waiting or stopped step | nothing |

Every `read` and `write` cycle calls `on_read` or `on_write` on `BusId::Cpu`, with the time the access began. Internal cycles call nothing. Each byte a DMA transfer moves calls `on_dma` instead (see [General DMA](#general-dma)).

The bus reports line and frame boundaries from the cycle in which they pass, after that cycle's `on_read` or `on_write` (ADR 0020, point 8):
- `on_scanline(line, at)` at the start of every line, line 0 included, with the time it started. Power-on and reset start line 0 without it.
- `on_frame_end(frame)` when vertical blank begins, with the new frame count. It comes before that line's `on_scanline`.

## The bus

[`BusView`](../../crates/sfes-snes-core/src/bus/mod.rs) implements `Bus65816` for the main CPU (ADR 0019). For each cycle it decodes the address, syncs the register's owner, performs the access, charges the access time and any refresh that fell due, and tells the observer.

Decode names the register as well as the device: `Target::Cpu(CpuRegister)` and `BBus::Ppu(Register)` are enums, and a register that belongs to a unit carries that unit's own register type (`sfes_snes_ppu::Register`, `MathRegister`, `PortRegister`, `DmaRegister`, `ApuPort`). Each unit implements the bus's `Registers` trait (`peek`, `after_read`, `write`), and the bus forwards to it. A read is written once: the bus syncs the owner, takes the value `peek` gives, then carries out the read's side effects, so `peek_bus` and a real read cannot disagree.

### Decode

The cartridge's memory map (ADR 0017) says what answers each 8 KiB page: WRAM, ROM, cartridge RAM, nothing, or the console's I/O pages at `$2000–$5FFF` of banks `$00–$3F` and `$80–$BF`. [`decode.rs`](../../crates/sfes-snes-core/src/bus/decode.rs) splits the I/O pages by anomie-memmap's table (`anomie-memmap#memory-map`, `fullsnes#snesiomap`):

| Addresses | Bus | What answers |
| --- | --- | --- |
| `$2100–$213F` | B, `$00–$3F` | PPU1 and PPU2, as one of the PPU's 64 registers (ADR 0028, point 2) |
| `$2140–$217F` | B, `$40–$7F` | The APU's four ports, mirrored (`anomie-regs#2140`) |
| `$2180–$2183` | B, `$80–$83` | The WRAM port |
| `$2184–$21FF` | B | Nothing on the console |
| `$4000–$43FF` | Internal | The 5A22's registers |
| `$2000–$20FF`, `$2200–$3FFF`, `$4400–$5FFF` | A | Nothing on the console; only a cartridge could answer |

Before an access to the PPU's or the APU's registers the bus calls that chip's sync hook, so the chip runs up to `now` (technical design, section 4, catch-up rule 1). The PPU's hook draws the lines that have started (see [Drawing and presenting frames](#drawing-and-presenting-frames)). The APU's ports answer from a placeholder (below; ADR 0027).

### Adding a register

1. Add a variant for it to the owning unit's register enum, or a new enum for a new unit, with the doc line `$XXXX MNEMONIC: what it does` (AGENTS.md section 4).
2. Decode its address to that variant in `decode.rs`, under `Target::Cpu` or `BBus`. Decode is the only place that matches on the address.
3. Handle it in the unit's `Registers` impl:
   - `peek` gives the value with no side effects. It returns `None` where open bus answers the whole register, as for a write-only one. Where only some bits come from open bus, the bus merges them in `peek_target`.
   - `after_read` carries out what a read changes (a flip-flop, a cleared flag).
   - `write` takes a CPU write; a read-only register ignores it.
4. Wire the variant into the bus's `peek_target`, `after_read` and `write_target` matches. Do not add a `read_x` or `write_x` method to the unit, and do not read the register outside `read_target`.
5. Test the unit through `Registers::read` (test-only) and the bus through a real read, and cite both (AGENTS.md section 1).

### Access time

A CPU cycle costs 6, 8 or 12 master cycles (`anomie-timing#s-cpu/clocks-and-refresh`), by anomie-memmap's speed column:

| Banks | Addresses | Master cycles |
| --- | --- | --- |
| `$00–$3F`, `$80–$BF` | `$0000–$1FFF`, `$6000–$7FFF` | 8 |
| `$00–$3F`, `$80–$BF` | `$2000–$3FFF`, `$4200–$5FFF` | 6 |
| `$00–$3F`, `$80–$BF` | `$4000–$41FF` | 12 |
| `$00–$3F` | `$8000–$FFFF` | 8 |
| `$40–$7F` | all | 8 |
| `$80–$BF` | `$8000–$FFFF` | 6 if `$420D` bit 0 is set, else 8 |
| `$C0–$FF` | all | 6 if `$420D` bit 0 is set, else 8 |

An internal cycle costs 6. `$420D MEMSEL` is write-only and 0 at power-on and reset (`fullsnes#snesmemorycontrol`). An access's cost is fixed when it starts, so a write to `$420D` speeds up the accesses after it.

### Refresh

The CPU is paused 40 master cycles per line while the DRAM refreshes (`anomie-timing#s-cpu/clocks-and-refresh`, `fullsnes#snestimingoscillators`). The first pause is due 538 cycles into the first line after power-on or reset. Each later one is due in the next line, a multiple of 8 cycles after the one before and as close to 536 as that allows, so with 1364-cycle lines the offsets alternate 538, 534, 538 … The CPU cycle running when a pause falls due completes first; the next one starts 40 cycles after it ends (ADR 0019, point 6). Observers see the pause as a gap between two access times.

### Open bus

The data bus keeps the last value placed on it (`anomie-memmap#hardware/data-bus`, ADR 0019, point 4):

- a read that something answers places its value there;
- a read that nothing answers, or of a write-only register, returns it unchanged;
- every write places its value there, the 5A22's registers included.

PPU1 and PPU2 have their own open bus as well, held by the PPU (`anomie-regs#registers`, ADR 0028, point 4; [`ppu.md`](ppu.md)).

### The WRAM port

`$2181`, `$2182` and `$2183` set a 17-bit WRAM address; only bit 0 of `$2183` counts, and all three read as open bus (`anomie-regs#2181`). Each read or write of `$2180` accesses WRAM there and then adds 1, wrapping from `$1FFFF` to 0 (`anomie-regs#2180`, `fullsnes#snesmemoryworkramaccess`, ADR 0019, point 5). Reset sets the address to 0. `peek_bus($2180)` returns the byte a read would see and reports it as latched, since a real read moves the address.

### Read-modify-write on registers

In emulation mode a read-modify-write instruction writes twice: first the value it read, then the result (ADR 0006). Both writes reach the register with their side effects (ADR 0019, point 7). `INC $2180` therefore reads WRAM at the port's address *a*, writes the value read to *a*+1 and the result to *a*+2.

### Still provisional

Until M4 (ADR 0018, point 6):

- **Watches.** `Stop::Watch` runs to its frame limit, since watches arrive in M4.
- **Video.** The PPU draws only the backdrop until the layers arrive, so a frame is one colour, CGRAM entry 0 (ADR 0028, point 7).

### Test cartridge

[`tests/carts/bus-timing`](../../tests/carts/bus-timing/bus-timing.s) runs FastROM code from bank `$80`, drives the WRAM port, reads a write-only register and runs `INC $2180` in emulation mode. A test in `src/carts.rs` checks the start time of each of its 28 instructions, including the first refresh, and the WRAM it leaves.

## Scanlines and frames

The scheduler in [`timing/`](../../crates/sfes-snes-core/src/timing/mod.rs) derives the line and dot from the clock (ADR 0020).

### Lines

A line is 1364 master cycles (`anomie-timing#s-cpu/clocks-and-refresh`). Two lines differ (`fullsnes#snestiminghvcounters`):

| Line | When | Length | Dots |
| --- | --- | --- | --- |
| Normal | Every other line | 1364 | 0–339; 323 and 327 are 6 cycles, the rest 4 |
| Short | NTSC line 240, interlace off, field 1 | 1360 | 0–339, all 4 cycles |
| Long | PAL line 311, interlace on, field 1 | 1368 | 0–340; 323 and 327 are 6 cycles, the rest 4 |

The long dots are anomie's (`anomie-timing#s-ppu/rendering`), and fullsnes's H-counter statistics agree with them. fullsnes's "four dots are 5-cycles long" is not used (ADR 0020, point 3). A line's length is fixed when it starts.

### Frames

A frame has lines 0–261 on NTSC and 0–311 on PAL (`anomie-regs#213c`). With interlace on, frames in field 0 have one more line: 262 or 312 (`anomie-timing#s-cpu/clocks-and-refresh`, `fullsnes#snestiminghvevents`). Two consecutive frames therefore take:

| Standard | Interlace off | Interlace on |
| --- | --- | --- |
| NTSC | (262 + 262) × 1364 − 4 | (262 + 263) × 1364 |
| PAL | 2 × 312 × 1364 | 625 × 1364 + 4 |

These are fullsnes's totals (`fullsnes#snestimingoscillators`, `fullsnes#snestiminghvcounters`).

- **The field.** `$213F` bit 7 toggles at H=1 of line 0 of every frame (`anomie-timing#s-ppu/registers`, `fullsnes#snestiminghvevents`). Power-on and reset start at field 0, so the first frame is field 1 (ADR 0020, point 4).
- **Vertical blank.** It begins at H=0 of line 225, or 240 with `$2133` bit 2 (overscan) set. It ends at H=0 of line 0 (`anomie-timing#s-ppu/blanking-periods`). Clearing overscan during lines 225–239 starts vertical blank with the next line (`anomie-regs#2133`). The scheduler samples the bit only at line starts (ADR 0020, point 5).
- **`$2133`.** The scheduler keeps the interlace and overscan bits of every CPU write to `$2133`, and reset clears them (ADR 0020, point 6).

### The frame boundary and stops

A frame ends when vertical blank begins, so frame 1 ends 225 × 1364 = 306 900 master cycles after power-on, without overscan (FND-04, ADR 0020, point 7). Every stop takes effect at the first step boundary at or after its target (FND-08):
- `Stop::Frames(n)` counts vertical blanks;
- `Stop::Scanlines(n)` counts line starts;
- `Stop::Cycles(n)` counts master cycles;
- `Stop::Instructions(n)` counts steps.

A stop overshoots its target by at most one step.

### The deadline table

The bus keeps the next time each scheduled event is due: the refresh pause, the next line start, the NMI flag, the next H/V timer trigger point, a `$4201` counter latch and the auto-joypad read's next event (ADR 0024, point 9), with HDMA to follow. A DMA transfer services them between its bytes, too (ADR 0025, point 9). After every CPU cycle one comparison with the earliest of them decides whether any is due. Due events are serviced in time order, and events due together in the table's order (technical design section 4, ADR 0001, ADR 0020, point 9, ADR 0021, point 9).

## Interrupts and the H/V timer

[`bus/interrupts.rs`](../../crates/sfes-snes-core/src/bus/interrupts.rs) holds the 5A22's interrupt outputs and the CPU's view of them; [`timing/irq.rs`](../../crates/sfes-snes-core/src/timing/irq.rs) works out where the H/V timer's trigger points fall (ADR 0021).

### When the CPU sees a line

The CPU checks for interrupts "just before the final CPU cycle of the instruction" (`anomie-timing#s-cpu/interrupts`). At the start of every CPU cycle the bus records the NMI edge latch and the IRQ line, and `interrupts()` returns the record from the start of the step's last cycle. Events due at the moment a cycle starts are serviced before it, so that cycle's record sees them (ADR 0021, point 1). This settles the question ADR 0004 left to the machine:
- a line that changes during an instruction's final cycle is seen one instruction later;
- the jump begins 6 to 12 master cycles after the line changes at the earliest, as anomie-timing says;
- a waiting processor's steps are single halted cycles, so it wakes on the step after the first cycle that starts with the line low.

The CPU core adds two rules of its own ([ADR 0022](../adr/0022-irq-check-after-a-flag-change-and-the-end-of-wai.md)):
- CLI, SEI, PLP, REP and SEP change I in their final cycle, so the IRQ check at the next boundary still uses the old I flag. The machine's step classification asks the CPU for that flag (`Cpu65816::irq_disabled_at_boundary`).
- The step that ends a WAI first spends two internal cycles, 12 master cycles. The observer stamps an interrupt taken from a wait with that step's start, before the two cycles.

### NMI

| Event | When | Source |
| --- | --- | --- |
| `$4210` bit 7 set | H=0.5 of the line that starts vertical blank, 2 cycles in, whether or not NMIs are enabled | `anomie-timing#s-cpu/interrupts`, `fullsnes#snestiminghvevents` |
| `$4210` bit 7 cleared | H=0 of line 0, or a `$4210` read | the same |
| /NMI low | while the flag and `$4200` bit 7 are both set | `fullsnes#snesppuinterrupts` |
| Edge latched | each high-to-low change of /NMI; the NMI vector read clears it | `wdc-w65c816s#2.21`, ADR 0004 |

Enabling NMIs while the flag is set is a fresh edge, so an NMI follows. Writing `$4200` never clears the flag (ADR 0021, point 4).

### H/V timer IRQ

`$4200` bits 4 and 5 select the trigger (`anomie-regs#4200`, `fullsnes#snesppuinterrupts`):

| yx | Trigger |
| --- | --- |
| 00 | none |
| 01 | every line at HTIME |
| 10 | line VTIME at H=0 |
| 11 | line VTIME at HTIME |

The trigger point is anomie-timing's formula (`anomie-timing#s-cpu/interrupts`):
- for H=0: 1374 master cycles after the previous line started, usually 10 cycles into the line (H=2.5);
- otherwise: 14 + 4×H cycles into the line (H=HTIME+3.5).

The formula counts cycles, so a trigger for H ≥ 338 falls in the next line. The scheduler keeps the previous line's trigger point for that case.

No trigger is met:
- for an H past the line's last dot;
- for a VTIME past the frame's last line;
- for H=153 on a short line or on a frame's last line.

At a trigger point `$4211` bit 7 is set, and /IRQ is low while it is. A `$4211` read and a `$4200` write with bits 4 and 5 clear acknowledge it; a write to `$4211` does nothing (ADR 0021, point 5). A write to `$4200` or `$4207–$420A` takes effect from the start of its access, so a trigger point inside that access is met.

HTIME and VTIME are 9 bits, `$1FF` at power-on and kept on reset (`anomie-regs#4208`, `#420a`, `fullsnes#snesiomap`).

### The status registers

Each read sees the machine at the start of its access (ADR 0021, point 3):

| Register | Bit 7 | Bits 6–0 |
| --- | --- | --- |
| `$4210 RDNMI` | NMI flag, cleared by the read | bits 6–4 open bus, bits 3–0 version 2 |
| `$4211 TIMEUP` | IRQ flag, cleared by the read | open bus |
| `$4212 HVBJOY` | vertical blank, from H=0 of its first line to H=0 of line 0 | bit 6 horizontal blank, H=274 to H=1; bits 5–1 open bus; bit 0 auto-joypad busy (ADR 0024, point 6) |

Sources: `anomie-timing#s-cpu/registers`, `anomie-regs#4210`, `#4211`, `#4212`, `fullsnes#snesppuinterrupts`.

`peek_bus` reports `$4210` and `$4211` as latched, since a read clears them.

### The counter latch

Three things latch the H/V counters, only while `$4201` bit 7 is set (`anomie-regs#2137`, `#4201`, `fullsnes#snespputimersandstatus`):
- a `$2137` read, at the dot where its access starts;
- a `$4201` write that clears bit 7, one dot later (`anomie-timing`, preamble);
- a lightgun, which arrives with the controller ports.

The latched H is the dot and the latched V is the line (ADR 0020, point 3). Each of `$213C` and `$213D` returns the low byte, then bit 8 over PPU2 open bus, on its own flip-flop. A latch does not reset the flip-flops (`anomie-regs#213c`).

The latch is PPU2 state and lives in the PPU, and the high byte's open bus is the PPU's PPU2 open bus (ADR 0028, point 4). Until #90 implements `$213F`, the read that clears the latch flag and resets the flip-flops is missing (ADR 0021, point 7).

`$4201` is `$FF` at power-on and reset (`fullsnes#snesiomap`). The latched counters are `$1FF` at power-on and kept on reset.

### Test cartridge

[`tests/carts/interrupts`](../../tests/carts/interrupts/interrupts.s) sets an HV-IRQ at H=100, V=5 and waits for it. Its handler acknowledges the IRQ and latches and reads the counters. The cartridge then enables the NMI alone and waits again; the NMI handler reads `$4210` twice and `$4212`. A test in `src/carts.rs` checks:
- the IRQ entry at master cycle 7242, worked out by hand through five refreshes;
- the NMI entry within the 6-cycle step after the flag;
- the latched dot 153 on line 5, and every value the handlers read.

## Multiply and divide, WRIO and RDIO

The 5A22's unsigned multiply and divide unit lives in [`bus/math.rs`](../../crates/sfes-snes-core/src/bus/math.rs) (`anomie-regs#4202`–`#4206`, `#4214`–`#4217`, `fullsnes#snesmathsmultiplydivide`, ADR 0023):
- a `$4203` write multiplies `$4202` by it: the product goes to `$4216–$4217`, the multiplier to `$4214` and 0 to `$4215`;
- a `$4206` write divides `$4204–$4205` by it: the quotient goes to `$4214–$4215` and the remainder to `$4216–$4217`. Dividing by 0 gives `$FFFF` and the dividend;
- the operands are kept, so a new `$4203` or `$4206` alone starts another operation.

The unit counts CPU cycles, not master cycles (fullsnes). The results appear together at the end of the 8th (multiply) or 16th (divide) CPU cycle after the starting write. Until then the result registers keep their old values. `LDA $4216` straight after `STA $4203` reads in the 4th cycle; five more cycles of waiting reach the 9th. Points the documents leave open (ADR 0023, points 2–4):
- refresh pauses count for nothing;
- the multiply shortcut for a small `$4202` is not modelled;
- a new operation abandons a running one.

`$4202` is `$FF` and `$4204–$4205` `$FFFF` at power-on. The results are 0. Reset changes nothing in the unit.

`$4213` RDIO reads the I/O port's lines: a bit `$4201` drives low reads 0, and a high bit reads 1 unless a device pulls it low (`anomie-regs#4213`). A standard pad leaves IOBit unconnected, so nothing pulls a line low and it reads back `$4201` (ADR 0023, point 6; ADR 0024, point 2). `$4201` bit 7 is also the PPU latch line (see [The counter latch](#the-counter-latch)).

### Test cartridge

[`tests/carts/math`](../../tests/carts/math/math.s) multiplies and divides, and reads each result one CPU cycle before it appears and in the cycle it does. It also divides by zero and reads `$4213` before and after a `$4201` write. A test in `src/carts.rs` checks every value it stores.

## Controller ports and the auto-joypad read

[`bus/ports.rs`](../../crates/sfes-snes-core/src/bus/ports.rs) models both controller ports, with a standard pad always plugged into each (ADR 0024, point 1).

### The pad

- **Latch.** `$4016` bit 0 drives the Latch line of both ports (`anomie-regs#controllers/generic`). While it is high, the pad loads its 16 bits: B, Y, Select, Start, Up, Down, Left, Right, A, X, L, R, then four 0 ID bits (`anomie-regs#4016`, `fullsnes#snescontrollershardwareidcodes`). A pressed button is 1 (`fullsnes#snescontrollersjoypad`).
- **Clock.** Reading `$4016` clocks port 1 and reading `$4017` clocks port 2. Each read returns the current bit, then the pad moves to the next. After 16 bits it returns 1 until latched again (`anomie-regs#controllers/joypads`).
- **Latch held high.** With Latch still high, a clock returns B and the pad does not advance (`fullsnes#snescontrollersioportsautomaticreading`).
- **Buttons.** The pad takes the buttons `set_input` last gave when it latches, so input set before a frame is what the frame reads.
- **Not connected.** Data2 and IOBit are not connected on a pad.
- **Power-on.** A pad returns 1 until first latched (ADR 0024, point 2).

| Register | Bits |
| --- | --- |
| `$4016` read | bit 0 port 1 Data1, bit 1 Data2 (0), bits 7–2 open bus |
| `$4017` read | bit 0 port 2 Data1, bit 1 Data2 (0), bits 4–2 always 1, bits 7–5 open bus |
| `$4016` write | bit 0 Latch; bits 1–2 not connected |

Sources: `anomie-regs#4016`, `#4017`, the open-bus rule in anomie-regs' introduction, and `fullsnes#snescontrollersioportsmanualreading`. A peek returns the bits without clocking the pad and reports a side effect.

### The auto-read

`$4200` bit 0 enables a read of 16 bits from each port into `$4218–$421F`, once a frame (`anomie-regs#4200`, `#4218`). Port 1 Data1 goes to `$4218/9`, port 2 Data1 to `$421A/B`, and the Data2 lines to `$421C–$421F`. The first bit read ends in bit 15, so a pad reads `byetUDLR axlr0000`.

When it runs (`anomie-timing#s-cpu/auto-joypad-read`):
- The read starts on the line that begins vertical blank, between dots 32.5 and 95.5.
- On the first frame after power-on or reset it starts at dot 74.5, then a multiple of 256 master cycles after that. The machine takes the earliest grid time at or after dot 32.5, which is at most dot 96.25 (ADR 0024, point 5).
- It lasts 4224 master cycles, with `$4212` bit 0 set throughout.

The documents give the steps of the read only as anomie-regs' pseudo-code: pulse Latch, then 16 times read `$4016/$4017` and rotate each bit into its register. The machine (ADR 0024, point 6):
- pulses Latch at the start;
- takes a bit every 256 master cycles, the 16th at 4096;
- clears the busy flag at 4224.

What the CPU sees during a read:
- `$4218–$421F` show the bits rotated in so far.
- A `$4016/$4017` read or a `$4016` write acts on the same pads, so it moves the auto-read's bits.
- A game that leaves `$4016` bit 0 set gets B in all 16 bits (fullsnes).

Power-on and reset:
- `$4016` bit 0 and `$4200` bit 0 are 0 at power-on and reset.
- `$4218–$421F` are 0 at power-on and kept on reset.
- Reset abandons a read in progress (ADR 0024, points 3, 5 and 7).

### The polled flag

A CPU read of `$4016`, `$4017` or `$4218–$421F` sets the frame's polled flag. The auto-read's own clocks and peeks do not. At each frame end the flag is kept and cleared for the next frame, and `Snes::polled()` returns the last ended frame's value. A frame that reads neither port is a lag frame (technical design section 7, ADR 0024, point 8).

### Test cartridge

[`tests/carts/joypad`](../../tests/carts/joypad/joypad.s) latches the pads and reads:
- port 1 seventeen times and port 2 twice;
- port 1 three times with the latch held high.

It then enables the auto-read, waits for `$4212` bit 0 to set and clear, and copies `$4218–$421F`. A test in `src/carts.rs` sets the buttons on both ports and checks every byte, including the open-bus `$40` in the unused bits. It also checks the polled flag over three frames.

## General DMA

[`bus/dma/mod.rs`](../../crates/sfes-snes-core/src/bus/dma/mod.rs) holds the eight channels and runs general-purpose transfers inside the bus (technical design section 4, catch-up rule 4; ADR 0025).

### The registers

| Register | What |
| --- | --- |
| `$420B MDMAEN` | write-only; each set bit starts its channel (`anomie-regs#420b`) |
| `$43x0 DMAPx` | bit 7 direction (0: A bus to B bus); bit 3 fixed address; bit 4 decrement; bits 2–0 mode (`anomie-regs#43x0`) |
| `$43x1 BBADx` | the B-bus register `$21pp` (`anomie-regs#43x1`) |
| `$43x2–$43x4 A1Tx, A1Bx` | the A-bus address; the bank is never stepped (`anomie-regs#43x2`) |
| `$43x5–$43x6 DASx` | the byte count; 0 means `$10000` (`anomie-regs#43x5`) |
| `$43x7–$43xB`, `$43xF` | HDMA's registers and the unused byte, readable and writable; `$43xF` is `$43xB` (`fullsnes#snesdmaandhdmachannel07registers`) |

Every channel register is `$FF` at power-on and kept on reset. `$43xC–$43xE` and `$4380–$43FF` are open bus (ADR 0025, point 1).

The modes, as B-bus offsets from `$21pp` (`anomie-regs#43x0`, `fullsnes#snesdmaandhdmachannel07registers`):

| Mode | Bytes |
| --- | --- |
| 0 | `p` |
| 1 | `p, p+1` |
| 2, 6 | `p, p` |
| 3, 7 | `p, p, p+1, p+1` |
| 4 | `p, p+1, p+2, p+3` |
| 5 | `p, p+1, p, p+1` |

The count is in bytes, not units: a count of 5 in mode 4 writes all four registers and then the first again (`anomie-regs#43x5`). Offsets wrap within `$21xx`.

### The pause

After a `$420B` write the CPU gets one more cycle, then pauses. The length of the cycle that runs after the pause is the transfer's CPU Clock (`anomie-timing#s-cpu/dma`). The machine runs the transfer at the start of that cycle (ADR 0025, points 3 and 4):
1. It waits to the next multiple of 8 master cycles since power-on or reset, 2 to 8 cycles.
2. It spends 8 cycles on the whole transfer.
3. For each set channel, 0 first, it spends 8 cycles, then 8 per byte.
4. It waits to the next whole number of CPU Clocks since the pause began, 1 cycle to a whole clock.

So "STA $420B : NOP" with one 3-byte channel pauses for 48 or 54 cycles, as in anomie-timing's four examples.

Each byte moves at the start of its 8-cycle slot (ADR 0025, point 5):
- the A-bus address is read or written through the A bus, and `$21pp` plus the mode's offset through the B bus;
- the value is placed on the data bus;
- then `$43x2/3` steps, by bits 4–3 of `$43x0`, and the count is decremented, until it reaches 0.

### What a transfer reaches

- **Through the A bus** (`anomie-regs#dma-and-hdma/dma`, `fullsnes#snesunpredictablethings`; ADR 0025, point 6):
  - WRAM, ROM and cartridge RAM, and the `$42xx` registers, as for the CPU.
  - Not the B-bus registers, `$420B`, `$420C` or `$4300–$437F`: they read open bus, and writes to them are lost.
  - `$4000–$41FF` reads open bus, with `$4017` bits 4–2 set, and clocks no pad.
- **Through the B bus**, the PPU, the APU and the WRAM port answer as for the CPU (ADR 0025, point 7).
- **WRAM through both buses fails**, with the B-bus side not answered and the WRAM port not moving (`anomie-regs#2180`, `#2181`, `fullsnes#snesmemoryworkramaccess`; ADR 0025, point 7).
- **Open bus** during a transfer is the data bus's last value (ADR 0025, point 8).

### Time during a transfer

- Deadlines that fall due during a transfer are serviced between bytes, so lines, the NMI and IRQ flags, latches and the auto-joypad read go on (ADR 0025, point 9).
- A refresh pauses the transfer for 40 cycles after the slot it falls in.
- The CPU sees a flag raised during the transfer from the cycle after it.
- The multiply and divide unit does not count during the pause (ADR 0025, point 10).
- HDMA runs between bytes and can end the transfer (below; ADR 0026, point 9).

### Test cartridge

[`tests/carts/dma`](../../tests/carts/dma/dma.s) runs four transfers into the WRAM port:
- ROM to WRAM in mode 0;
- a fixed-address fill and a decrementing copy, on two channels at once;
- `$2180` to WRAM, which fails;
- `$4017` through the A bus.

It then copies channel registers to WRAM. A test in `src/carts.rs` checks every byte, including the open-bus `$EA` the next opcode leaves on the data bus.

## HDMA

[`bus/dma/hdma.rs`](../../crates/sfes-snes-core/src/bus/dma/hdma.rs) runs HDMA on the same eight channels: one unit of up to four bytes per channel per line, from a table to B-bus registers (ADR 0026).

### The registers

| Register | What |
| --- | --- |
| `$420C HDMAEN` | write-only; each set bit enables its channel, a clear bit pauses it; 0 at power-on and reset (`anomie-regs#420c`) |
| `$43x0 DMAPx` | bit 7 direction; bit 6 indirect; bits 2–0 the mode, as for DMA; bits 4–3 unused (`anomie-regs#43x0`) |
| `$43x2–$43x4 A1Tx, A1Bx` | the table's start and bank, never changed by HDMA (`anomie-regs#43x2`) |
| `$43x5–$43x7 DASx, DASBx` | the indirect address and its bank (`anomie-regs#43x5`) |
| `$43x8–$43x9 A2Ax` | the current table address, in the bank of `$43x4` (`anomie-regs#43x8`) |
| `$43xA NLTRx` | bit 7 repeat, bits 6–0 the line count (`anomie-regs#43xa`) |

Each channel also has a DoTransfer flag and a terminated-for-this-frame flag, clear at power-on and reset (ADR 0026, point 1).

A table is a run of entries, each a line-count byte followed by one unit of data (direct) or a 16-bit pointer to it (indirect) (`fullsnes#snesdmaandhdmachannel07registers`):
- `$00` ends the channel for the frame.
- `$01–$80` is one unit, then X−1 lines without one.
- `$81–$FF` is X−`$80` lines with a unit on each.

### The two steps

Both are deadlines (ADR 0026, point 2):
- **Setup** at H=6 of line 0. Every channel's terminated flag clears. Each enabled channel copies `$43x2/3` into `$43x8/9`, reads its line count and, if indirect, its pointer, and sets DoTransfer (point 5).
- **A line's transfer** at H=278 of every line before vertical blank, lines 0–224 or 0–239 with overscan. For each channel that is enabled and not terminated, from 0 to 7 (point 6):
  1. If DoTransfer is set, it moves one unit, incrementing the address it reads.
  2. It decrements all of `$43xA` and sets DoTransfer to its bit 7.
  3. When the line count reaches 0, it reads the next entry. A `$00` ends the channel. If this is the last channel worked on in the line, an indirect channel reads only one pointer byte, into `$43x6`, with `$43x5` set to 0 (`anomie-regs#dma-and-hdma/hdma`).

A channel enabled mid-frame runs from its registers as they stand and moves nothing on its first line. One that has ended stays ended until the next setup (point 7).

### The pause

A step that falls due while the CPU runs waits for the CPU cycle to end, then pauses the CPU as a DMA does (point 3):
1. It waits to a multiple of 8 cycles since reset.
2. It does the work.
3. It waits to a whole number of the next cycle's clocks.

The work costs (`anomie-timing#s-cpu/hdma`; point 4):
- 18 cycles of overhead;
- 8 per channel worked on;
- 16 per indirect pointer read (8 in the last-channel case);
- 8 per byte.

A step with no channel to work on costs nothing.

A step that falls due during a DMA runs inside its pause, between bytes. If it works on the channel the DMA is running, that channel's transfer ends there, and the later channels still run (`anomie-regs#420b`; point 9).

Each data byte reports `on_dma` with `hdma: true` (point 10).

### Test cartridge

[`tests/carts/hdma`](../../tests/carts/hdma/hdma.s) enables a direct and an indirect channel in vertical blank, both into the WRAM port. A test in `src/carts.rs` checks:
- the bytes, in line and channel order;
- the table addresses and line counts the channels end with;
- the one-byte pointer read of the last-channel case.

## The APU ports until M3

The SPC700 arrives in M3, but ROMs touch `$2140–$2143` during boot. [`bus/apu.rs`](../../crates/sfes-snes-core/src/bus/apu.rs) holds a placeholder for the APU unit. It is not hardware behaviour (ADR 0027).

The shape follows `anomie-regs#2140` (points 1 and 2):
- each port has two latches, one the CPU writes and one the CPU reads, so a read never returns the CPU's own write;
- `$2140–$217F` mirror the four ports every four bytes;
- DMA and HDMA reach the ports the same way the CPU does.

What the placeholder does (points 3 to 6):
- every read returns `$00`, and the data bus takes it;
- every write is kept for the SPC700;
- the sync hook calls `Apu::run_until(now)`, which does nothing;
- power-on and reset set all eight latches to `$00`.

M3 replaces the inside of `Apu` and keeps the bus's calls into it (point 7).

What it rules out until M3 (point 8):
- any ROM that waits for the APU, which is almost every commercial game;
- goldens from any ROM that reads the ports, since M3 changes what it sees. None of the approved test ROMs is known to read them.

## Drawing and presenting frames

The bus draws the PPU's lines and presents its frames ([`video.rs`](../../crates/sfes-snes-core/src/video.rs); technical design section 4, catch-up rule 2; ADR 0028):

- **Which lines.** A frame outputs lines 1 to the line before vertical blank, normally 224 or 239. Line 0 and vertical blank output nothing (`anomie-timing#s-ppu/rendering`).
- **When.** Line *L* is drawn when the PPU is synced after *L* has started, or when vertical blank begins, with the registers as they stand then. Every register write syncs the PPU first, so those are the registers as they stood when the line started, and a write during line *L* takes effect from line *L* + 1.
- **The frame.** At the start of vertical blank the lines become one `VideoFrame`:
  - it is 512 wide if any line was, and a 256-pixel line then has each pixel twice;
  - it is as many rows high as it has lines, or twice that if any line was interlaced, each line then filling two rows;
  - an interlaced frame shows only its own field and names it in `field`.
- **Outputs.** The lines and the presented frame are outputs, not state. They live in the bus; `Video` is not `Clone`, and the bus's own clone starts it empty. A frame is presented only if every line was drawn into this machine's outputs, so a clone taken after some lines were drawn, or a reset, presents nothing for that frame, and `video()` keeps the frame before (point 10).

Goldens hash the frame exactly as presented (ADR 0015).

## The code

| File | What it holds |
| --- | --- |
| [`machine.rs`](../../crates/sfes-snes-core/src/machine.rs) | `Snes`, `SnesBuilder`, `BuildError`, the state, the step loop, the observer events and the inherent API |
| [`bus/mod.rs`](../../crates/sfes-snes-core/src/bus/mod.rs) | `Bus`, `BusView`, the `Registers` trait, `read_target`, `peek_target`, `after_read` and `write_target`, open bus, the WRAM port, and charging each access its time and any refresh |
| [`bus/decode.rs`](../../crates/sfes-snes-core/src/bus/decode.rs) | `Target`, `BBus`, `CpuRegister`, `PpuRegister` and `Speed`: decode and access time. Each unit's register enum is in its own file |
| [`bus/interrupts.rs`](../../crates/sfes-snes-core/src/bus/interrupts.rs) | `$4200` bit 7, the NMI and IRQ flags, the edge latch and the recorded lines |
| [`bus/ppu.rs`](../../crates/sfes-snes-core/src/bus/ppu.rs) | The `Registers` impl for `sfes_snes_ppu::Ppu` |
| [`bus/math.rs`](../../crates/sfes-snes-core/src/bus/math.rs) | The multiply and divide unit |
| [`bus/ports.rs`](../../crates/sfes-snes-core/src/bus/ports.rs) | The pads, `$4016/$4017`, the auto-read and the polled flag |
| [`bus/dma/mod.rs`](../../crates/sfes-snes-core/src/bus/dma/mod.rs) | The channel registers, `$420B`, `$420C` and the pause |
| [`bus/dma/hdma.rs`](../../crates/sfes-snes-core/src/bus/dma/hdma.rs) | HDMA's setup and line steps |
| [`bus/apu.rs`](../../crates/sfes-snes-core/src/bus/apu.rs) | The APU ports' placeholder |
| [`timing/mod.rs`](../../crates/sfes-snes-core/src/timing/mod.rs) | The scheduler: line, field, vertical blank, refresh, the NMI and timer points, the `$4201` latch, the auto-read and HDMA's steps, and `Timing::service`, which returns each due event |
| [`timing/position.rs`](../../crates/sfes-snes-core/src/timing/position.rs) | Line lengths and the dot at a point in a line |
| [`timing/irq.rs`](../../crates/sfes-snes-core/src/timing/irq.rs) | `$4200`'s timer bits, HTIME, VTIME and the H/V timer's trigger points |
| [`timing/deadlines.rs`](../../crates/sfes-snes-core/src/timing/deadlines.rs) | The deadline table |
| [`video.rs`](../../crates/sfes-snes-core/src/video.rs) | `Video`: the lines of the frame being drawn, presenting them, and the last frame |
| [`carts.rs`](../../crates/sfes-snes-core/src/carts.rs) | The test-cartridge runs (below) |

Each unit with CPU-visible registers implements `Registers`, and only decode matches on addresses (AGENTS.md section 4; [Adding a register](#adding-a-register)). The crate is `no_std` with `alloc` and depends on `sfes-api`, `sfes-snes-cpu`, `sfes-snes-cart`, `sfes-snes-ppu`, `bitflags` and `thiserror` (technical design section 12).

## Testing

- **Unit tests.** `bus/tests.rs` covers decode, the WRAM port, open bus, access speed, refresh, the interrupt and timer registers, the math unit, the ports, DMA and the APU placeholder. `timing/tests.rs` covers line and frame lengths, the field, vertical blank, refresh and the H/V position. Smaller tests sit beside the code in each unit. Each test cites the section its expected values come from.
- **Machine tests** in `machine.rs` cover the builder, power-on, reset, the observer events and the memory regions. `video.rs` tests the frame geometry. ROM images come from `sfes_snes_cart::test_support` (see [the cartridge page](cart.md#testing)).
- **Test cartridges.** `carts.rs` runs six of the project's own cartridges (ADR 0013), one per feature: `bus-timing`, `interrupts`, `math`, `joypad`, `dma` and `hdma`. Each section above describes its cartridge. Expected values are worked out from the documents, not taken from an emulator.
- **Cloning.** `a_clone_runs_the_same_and_starts_without_outputs` runs a machine and its clone for a frame. They must end at the same cycle with the same WRAM and the same frame, and the clone's frame buffer must start empty (FND-01, ADR 0028).

Golden frames (ADR 0015) need the PPU and follow in #100.

## Decisions

The documents left these points open or disagree on them. Each ADR quotes the sources and states its confidence.

| ADR | Decision | Confidence |
| --- | --- | --- |
| [0018](../adr/0018-snes-reset-and-power-on.md) | RESB timing, the page-1 reads, power-on and reset, the default region, observer events and the provisional timing | medium for the reset timing and the default region; high for the rest |
| [0019](../adr/0019-snes-bus-decode-and-refresh.md) | Decode, access speed, open bus, the WRAM port, refresh and read-modify-write writes to registers | high for decode, speed and the WRAM port; medium for refresh, open bus on internal-register writes, the port's wrap and read-modify-write writes |
| [0020](../adr/0020-snes-scanlines-frames-and-deadlines.md) | Line lengths and dots, frames, the field, vertical blank and overscan, the frame boundary, line and frame events, and the deadline table | high for line lengths, line counts and the frame boundary; medium for the long dots, the field at reset, when `$2133` is sampled and overscan changes in vertical blank |
| [0021](../adr/0021-snes-interrupts-timers-and-counter-latch.md) | When a changed NMI or IRQ line is seen, the NMI and H/V timer trigger points, `$4210–$4212`, the counter latch, and their reset values | high for the trigger points and register bits; medium for when a line is seen and where in an access a read sees the machine; low for PPU2 open bus |
| [0022](../adr/0022-irq-check-after-a-flag-change-and-the-end-of-wai.md) | (CPU core) The IRQ check uses the I flag from before the last cycle, and WAI ends two internal cycles after a request | high for the I flag; medium for the end of WAI |
| [0023](../adr/0023-snes-multiply-divide-unit-and-io-port.md) | The multiply and divide results, their latency in CPU cycles, reads before the result, and RDIO | high for the results and latency; low for early reads, overlapping operations and refresh |
| [0024](../adr/0024-controller-ports-and-the-auto-joypad-read.md) | The pad, `$4016/$4017`, when the auto-read starts and takes its bits, reset, and the polled flag | high for the pad and the registers; medium for the auto-read's start and the polled flag; low for the bits' timing, power-on and reset |
| [0025](../adr/0025-general-dma.md) | The channel registers, the pause, where each byte falls, what the A and B buses reach, open bus, and deadlines and the math unit during a transfer | high for the registers, modes, step and count; medium for the pause and the A bus; low for byte placement, open bus, refresh and the math unit |
| [0026](../adr/0026-hdma.md) | `$420C` and the HDMA flags, when the steps fall due, the pause and its costs, the table steps and the last-channel case, mid-frame enabling, and pre-empting a DMA; amended for anomie-timing's overscan-bit slip | high for the registers, table and steps; medium for timing and costs; low for the overhead's placement, the last-channel case at frame start, mid-frame enabling and a pre-empted DMA |
| [0027](../adr/0027-apu-port-placeholder-until-m3.md) | The APU ports' placeholder until M3: the register shape, reads of `$00`, kept writes, the sync hook, reset, and the ROMs and goldens it rules out | high for the register shape; the placeholder's values are a project choice and claim none |
| [0028](../adr/0028-ppu-state-registers-and-frame-geometry.md) | The PPU's memories and their power-on fill, its registers and their reads, PPU1 and PPU2 open bus, which lines are drawn and when, and the frame geometry and where the outputs live | high for the reads and open bus; medium for which lines are output and their width; the power-on values and the geometry are project choices |

ADR 0001 (catch-up scheduling) predates M1 and governs the bus's sync hooks and the deadline table. ADR 0004 is the CPU's bus contract, which `BusView` implements.
