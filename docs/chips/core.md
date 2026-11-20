# The machine (`sfes-snes-core`)

This page explains the SNES machine and how the crate models it (QA-08). It grows with each M2 machine issue. Every statement cites its source, by its id in [`provenance/bibliography.toml`](../../provenance/bibliography.toml):

- Anomie's memory-mapping doc (`anomie-memmap`) and timing doc (`anomie-timing`), by heading path;
- fullsnes (`fullsnes`), by section anchor;
- Vigna's SplitMix64 description (`vigna-splitmix`), archived in [`provenance/archive/vigna-splitmix/`](../../provenance/archive/vigna-splitmix/);
- the project's ADRs, listed under [Decisions](#decisions).

## The machine value

[`Snes<A, O>`](../../crates/sfes-snes-core/src/machine.rs) is one console as one owned value, `Clone + Send` (FND-01). `A` is the accuracy profile (`Fast` by default) and `O` the observer (`NoopObserver` by default, which compiles away; FND-05). It holds:

| Part | What | Cloned? |
| --- | --- | --- |
| `State::cpu` | The 65C816's registers and run state | yes |
| `State::bus` | WRAM, the cartridge, the data bus value, the NMI latch and IRQ line, the clock, the line position and the held buttons | yes |
| `State::meta` | The seed, the video standard and the frame count | yes |
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
- **The reset button.** It "leaves WRAM intact" (`fullsnes#snescontrollersotherinputs`). The SNES "returns to a known timing position on reset" (`anomie-timing`), so `reset` restarts the line and frame position, then runs the CPU's reset sequence. Cartridge RAM is kept as well.
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

## The bus, for now

Until #80 and #81, the bus is a minimal one (ADR 0018, point 6):

- **Decode.** Each address is looked up in the cartridge's memory map (ADR 0017). WRAM, ROM and cartridge RAM answer. The I/O pages and anything unmapped return the last value on the data bus (`anomie-memmap#hardware/data-bus`). Writes there do nothing.
- **Access time.** anomie-memmap's speed column applies, with `$420D` bit 0 at its reset value 0 (`fullsnes#snesiomap`):

  | Banks | Addresses | Master cycles |
  | --- | --- | --- |
  | `$00–$3F`, `$80–$BF` | `$0000–$1FFF`, `$6000–$FFFF` | 8 |
  | `$00–$3F`, `$80–$BF` | `$2000–$3FFF`, `$4200–$5FFF` | 6 |
  | `$00–$3F`, `$80–$BF` | `$4000–$41FF` | 12 |
  | `$40–$FF` | all | 8 |

  An internal cycle costs 6 (`anomie-timing#s-cpu/clocks-and-refresh`).
- **Lines and frames.** A line is 1364 master cycles. A frame is 262 lines on NTSC and 312 on PAL (`fullsnes#snestimingoscillators`, `fullsnes#snestiminghvcounters`), with no short or long lines yet. The frame boundary is the end of a frame's last line.
- **Stops.** A stop takes effect at the first step boundary at or after its target (FND-08). `Stop::Instructions(n)` counts steps. `Stop::Watch` runs to its frame limit, since watches arrive in M4.
- **Interrupts.** The NMI latch is cleared by the vector-pull read of the NMI vector (ADR 0004). Nothing sets it, or the IRQ line, until #82.
- **Video.** No PPU renders yet, so `video()` is a 0 × 0 frame.

## Decisions

- [ADR 0018](../adr/0018-snes-reset-and-power-on.md): RESB timing, the page-1 reads, power-on and reset, the default region, observer events and the provisional bus.
