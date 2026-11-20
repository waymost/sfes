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
| `State::bus` | WRAM, the cartridge, the data bus value, the WRAM port address, `$420D`, the NMI latch and IRQ line, the clock, the line position and refresh schedule, and the held buttons | yes |
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
- **The reset button.** It "leaves WRAM intact" (`fullsnes#snescontrollersotherinputs`). The SNES "returns to a known timing position on reset" (`anomie-timing`), so `reset` restarts the line and frame position and the refresh schedule, clears `$420D` and the WRAM port address (`fullsnes#snesiomap`), then runs the CPU's reset sequence. Cartridge RAM is kept as well.
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

Until #81 and #82 (ADR 0018, point 6):

- **Lines and frames.** A line is 1364 master cycles. A frame is 262 lines on NTSC and 312 on PAL (`fullsnes#snestimingoscillators`, `fullsnes#snestiminghvcounters`), with no short or long lines yet. The frame boundary is the end of a frame's last line.
- **Stops.** A stop takes effect at the first step boundary at or after its target (FND-08). `Stop::Instructions(n)` counts steps. `Stop::Watch` runs to its frame limit, since watches arrive in M4.
- **Interrupts.** The NMI latch is cleared by the vector-pull read of the NMI vector (ADR 0004). Nothing sets it, or the IRQ line, until #82.
- **Video.** No PPU renders yet, so `video()` is a 0 × 0 frame.

### Test cartridge

[`tests/carts/bus-timing`](../../tests/carts/bus-timing/bus-timing.s) runs FastROM code from bank `$80`, drives the WRAM port, reads a write-only register and runs `INC $2180` in emulation mode. A test in `src/carts.rs` checks the start time of each of its 28 instructions, including the first refresh, and the WRAM it leaves.

## Decisions

- [ADR 0018](../adr/0018-snes-reset-and-power-on.md): RESB timing, the page-1 reads, power-on and reset, the default region, observer events and the provisional timing.
- [ADR 0019](../adr/0019-snes-bus-decode-and-refresh.md): decode, access speed, open bus, the WRAM port, refresh and read-modify-write writes to registers.
