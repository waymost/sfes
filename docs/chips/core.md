# The machine (`sfes-snes-core`)

This page explains the SNES machine and how the crate models it (QA-08). It grows with each M2 machine issue. Every statement cites its source, by its id in [`provenance/bibliography.toml`](../../provenance/bibliography.toml):

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
| `State::bus` | WRAM, the cartridge, the data bus value, the WRAM port address, `$420D`, the interrupt state (`$4200` bit 7, the NMI and IRQ flags, the NMI edge latch and the recorded lines), `$4201`, the H/V counter latch, the multiply and divide unit, the clock, the scheduler (line, field, vertical blank, line and frame counts, refresh, the H/V timer and the deadline table), and the held buttons | yes |
| `State::meta` | The seed and the video standard | yes |
| `observer` | The observer | yes |
| `outputs` | The frame buffer | no: a clone starts empty (technical design section 5) |

The cartridge's ROM and memory map are `Arc`-shared, so a clone copies WRAM and registers and bumps two counts.

`Snes::builder(rom)` loads the cartridge (ADR 0016, ADR 0017) and powers on, or returns a `BuildError`. It never yields a half-built machine. Options:
- `.region(Region)` forces NTSC or PAL;
- `.board(Mapping)` skips header scoring;
- `.seed(u64)` sets the power-on seed, 0 by default (FND-02);
- `.observer(o)` attaches an observer.

Until `impl System` arrives in M4, the machine has an inherent API with the same shapes: `run(Stop)`, `set_input`, `reset`, `power_cycle`, `region` and `region_mut`, `peek_bus` and `video` (FND-06, FND-08).

## Power-on and reset

- **Power-on.** WRAM is filled from `SplitMix64::new(seed)`, eight bytes per output, little-endian, from `$7E:0000` up (`vigna-splitmix`, FND-02, ADR 0018). The CPU starts from `Cpu65816::new()`, the clock from 0, and the reset sequence runs at once.
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

Every `read` and `write` cycle calls `on_read` or `on_write` on `BusId::Cpu`, with the time the access began. Internal cycles call nothing.

The bus reports line and frame boundaries from the cycle in which they pass, after that cycle's `on_read` or `on_write` (ADR 0020, point 8):
- `on_scanline(line, at)` at the start of every line, line 0 included, with the time it started. Power-on and reset start line 0 without it.
- `on_frame_end(frame)` when vertical blank begins, with the new frame count. It comes before that line's `on_scanline`.

## The bus

[`BusView`](../../crates/sfes-snes-core/src/bus/mod.rs) implements `Bus65816` for the main CPU (ADR 0019). For each cycle it decodes the address, syncs the register's owner, performs the access, charges the access time and any refresh that fell due, and tells the observer.

### Decode

The cartridge's memory map (ADR 0017) says what answers each 8 KiB page: WRAM, ROM, cartridge RAM, nothing, or the console's I/O pages at `$2000–$5FFF` of banks `$00–$3F` and `$80–$BF`. [`decode.rs`](../../crates/sfes-snes-core/src/bus/decode.rs) splits the I/O pages by anomie-memmap's table (`anomie-memmap#memory-map`, `fullsnes#snesiomap`):

| Addresses | Bus | What answers |
| --- | --- | --- |
| `$2100–$213F` | B, `$00–$3F` | PPU1 and PPU2 |
| `$2140–$217F` | B, `$40–$7F` | The APU's four ports, mirrored (`anomie-regs#2140`) |
| `$2180–$2183` | B, `$80–$83` | The WRAM port |
| `$2184–$21FF` | B | Nothing on the console |
| `$4000–$43FF` | Internal | The 5A22's registers |
| `$2000–$20FF`, `$2200–$3FFF`, `$4400–$5FFF` | A | Nothing on the console; only a cartridge could answer |

Before an access to the PPU's or the APU's registers the bus calls that chip's sync hook, so the chip runs up to `now` (technical design, section 4, catch-up rule 1). The hooks do nothing until the chips exist, and until then their registers read as open bus.

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

PPU1 and PPU2 have their own open bus (`anomie-regs`, preamble), which arrives with the PPU.

### The WRAM port

`$2181`, `$2182` and `$2183` set a 17-bit WRAM address; only bit 0 of `$2183` counts, and all three read as open bus (`anomie-regs#2181`). Each read or write of `$2180` accesses WRAM there and then adds 1, wrapping from `$1FFFF` to 0 (`anomie-regs#2180`, `fullsnes#snesmemoryworkramaccess`, ADR 0019, point 5). Reset sets the address to 0. `peek_bus($2180)` returns the byte a read would see and reports it as latched, since a real read moves the address.

### Read-modify-write on registers

In emulation mode a read-modify-write instruction writes twice: first the value it read, then the result (ADR 0006). Both writes reach the register with their side effects (ADR 0019, point 7). `INC $2180` therefore reads WRAM at the port's address *a*, writes the value read to *a*+1 and the result to *a*+2.

### Still provisional

Until M4 (ADR 0018, point 6):

- **Watches.** `Stop::Watch` runs to its frame limit, since watches arrive in M4.
- **Video.** No PPU renders yet, so `video()` is a 0 × 0 frame.

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

The bus keeps the next time each scheduled event is due: the refresh pause, the next line start, the NMI flag, the next H/V timer trigger point and a `$4201` counter latch, with HDMA and auto-joypad to follow. After every CPU cycle one comparison with the earliest of them decides whether any is due. Due events are serviced in time order, and events due together in the table's order (technical design section 4, ADR 0001, ADR 0020, point 9, ADR 0021, point 9).

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
| `$4212 HVBJOY` | vertical blank, from H=0 of its first line to H=0 of line 0 | bit 6 horizontal blank, H=274 to H=1; bits 5–1 open bus; bit 0 auto-joypad busy, 0 until #85 |

Sources: `anomie-timing#s-cpu/registers`, `anomie-regs#4210`, `#4211`, `#4212`, `fullsnes#snesppuinterrupts`.

`peek_bus` reports `$4210` and `$4211` as latched, since a read clears them.

### The counter latch

Three things latch the H/V counters, only while `$4201` bit 7 is set (`anomie-regs#2137`, `#4201`, `fullsnes#snespputimersandstatus`):
- a `$2137` read, at the dot where its access starts;
- a `$4201` write that clears bit 7, one dot later (`anomie-timing`, preamble);
- a lightgun, which arrives with the controller ports.

The latched H is the dot and the latched V is the line (ADR 0020, point 3). Each of `$213C` and `$213D` returns the low byte, then bit 8 over PPU2 open bus, on its own flip-flop. A latch does not reset the flip-flops (`anomie-regs#213c`).

Until the PPU exists:
- the latch lives in the bus;
- PPU2 open bus is the last value read from `$213C` or `$213D`;
- the `$213F` read that clears the latch flag and resets the flip-flops is missing (ADR 0021, point 7).

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

`$4213` RDIO reads the I/O port's lines: a bit `$4201` drives low reads 0, and a high bit reads 1 unless a device pulls it low (`anomie-regs#4213`). Nothing pulls a line low until the controller ports, so it reads back `$4201` (ADR 0023, point 6). `$4201` bit 7 is also the PPU latch line (see [The counter latch](#the-counter-latch)).

### Test cartridge

[`tests/carts/math`](../../tests/carts/math/math.s) multiplies and divides, and reads each result one CPU cycle before it appears and in the cycle it does. It also divides by zero and reads `$4213` before and after a `$4201` write. A test in `src/carts.rs` checks every value it stores.

## Decisions

- [ADR 0018](../adr/0018-snes-reset-and-power-on.md): RESB timing, the page-1 reads, power-on and reset, the default region, observer events and the provisional timing.
- [ADR 0019](../adr/0019-snes-bus-decode-and-refresh.md): decode, access speed, open bus, the WRAM port, refresh and read-modify-write writes to registers.
- [ADR 0020](../adr/0020-snes-scanlines-frames-and-deadlines.md): line lengths and dots, frames, the field, vertical blank and overscan, the frame boundary, line and frame events, and the deadline table.
- [ADR 0021](../adr/0021-snes-interrupts-timers-and-counter-latch.md): when a changed NMI or IRQ line is seen, the NMI and H/V timer trigger points, `$4210–$4212`, the counter latch, and their reset values.
- [ADR 0022](../adr/0022-irq-check-after-a-flag-change-and-the-end-of-wai.md) (CPU core): the IRQ check uses the I flag from before the last cycle, and WAI ends two internal cycles after a request.
- [ADR 0023](../adr/0023-snes-multiply-divide-unit-and-io-port.md): the multiply and divide results, their latency in CPU cycles, reads before the result, and RDIO.
