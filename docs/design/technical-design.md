# sfes — Technical Design

Nov 11, 2026 · @Matthew

## 1. Overview

sfes is a Cargo workspace in which one emulated console is one owned, `Clone + Send` Rust value, generic over its accuracy profile and observer, synchronized by a catch-up scheduler on the bus, and serialized with a versioned postcard format; everything else (pool, RL environments, datasets, bindings, player) is a thin layer over that value. This document turns the [feature specification](feature-specification.md) into a buildable design; requirement IDs (`FND-01`, `RL-07` …) refer to it.

**Project environment** (confirmed): GitHub with GitHub Actions; regular use of AI coding assistants, so PROV-07 gets enforcement in CI; benchmarks on cloud runners only, compared relative to the previous commit; Rust edition 2024 with a pinned stable toolchain.

**Key decisions**

| Area | Decision | Why |
| --- | --- | --- |
| Machine type | `Snes<A: Accuracy, O: Observer = NoopObserver>`, plain owned struct, `#[derive(Clone)]` | Clone is the snapshot (FND-01); generics monomorphize away unused features (FND-05, EMU-01) |
| Polymorphism | Enums for closed sets (boards, coprocessors, devices); traits only at extension points | Enums derive `Clone`, inline, and need no allocation; the set of SNES boards is closed |
| Scheduling | Catch-up on bus access, integer master-clock timestamps, no threads or coroutines | Deterministic and cloneable; no stackful coroutines in stable Rust (EMU-01) |
| CPU style | Instruction-at-a-time `match` on opcode; every bus access advances the clock and may trigger catch-up | Fast, readable, and exact at bus-cycle granularity |
| Serialization | serde + [postcard](https://docs.rs/crate/postcard) inside a sectioned, versioned envelope; BLAKE3 for hashes | Postcard has a documented stable wire format since 1.0 and is `no_std` (FND-03) |
| Parallelism | `rayon` thread pool in `sfes-pool` only; chip crates are single-threaded | Determinism by construction; one dependency where parallelism is needed (API-09) |
| Errors | `thiserror` enums per crate; no panics in core (`clippy::unwrap_used` denied) | FND-09, QA-07 |
| Unsafe | `#![forbid(unsafe_code)]` everywhere except `sfes-py`, `sfes-capi` and `sfes-player` | QA-07 |
| Bindings | PyO3 + maturin, `abi3-py314`; `cbindgen` for C; `wasm-bindgen` later | API-10, API-13, API-12 |
| Automation | `cargo xtask` for project tasks; release-plz for releases | Rust-native tooling, no Makefiles or shell scripts as the source of truth |

The design follows Rust conventions throughout: ownership instead of shared mutable state, enums and `match` instead of class hierarchies, traits with static dispatch instead of virtual calls, `Result` instead of exceptions, and newtypes for units such as master-clock cycles and addresses.

## 2. Workspace and crates

One Cargo workspace holds every crate, the Python package and the tooling; console-specific code lives only in `sfes-snes-*` crates, and dependencies point one way: apps → services → `sfes-api` ← chip crates (MS-02), with one declared exception: the cart crate uses the CPU crate for the SA-1. cargo xtask deps checks the graph against that allow-list in CI.

```text
sfes/
├─ Cargo.toml            # [workspace], shared [workspace.package], [workspace.lints], [workspace.dependencies]
├─ rust-toolchain.toml   # pinned stable + components (clippy, rustfmt, llvm-tools)
├─ deny.toml  clippy.toml  rustfmt.toml  release-plz.toml  cliff.toml
├─ crates/
│  ├─ sfes/              # facade: re-exports with features (snes, pool, env, movie)
│  ├─ sfes-api/  sfes-movie/  sfes-pool/  sfes-env/  sfes-dataset/  sfes-cheevos/
│  ├─ sfes-snes-cpu/  sfes-snes-spc/  sfes-snes-ppu/  sfes-snes-cart/  sfes-snes-core/
│  ├─ sfes-py/  sfes-capi/  sfes-player/
│  └─ sfes-test-harness/  # publish = false
├─ python/sfes/          # pure-Python layer: gymnasium/pettingzoo wrappers, typing stubs
├─ pyproject.toml        # maturin build backend
├─ xtask/                # cargo xtask: provenance lint, golden updates, release helpers
├─ fuzz/                 # cargo-fuzz targets (excluded from the workspace)
├─ provenance/           # bibliography.toml, exposures.md
├─ tests/assets/         # MANIFEST.toml, licensed vectors and test ROMs, golden hashes
└─ docs/                 # adr/, chips/<chip>.md explainers, book (mdBook)
```

| Crate | Role | `no_std` | Key dependencies | Spec |
| --- | --- | --- | --- | --- |
| `sfes` | Facade for Rust users: `cargo add sfes` | follows features | the crates below | API-01 |
| `sfes-api` | Traits and shared types: `System`, `Observer`, `Accuracy`, `Cycles`, region and device descriptors, state envelope, hashing | yes, `alloc` | serde, postcard, blake3, thiserror | FND-03, FND-05, FND-07 |
| `sfes-snes-cpu` | 65C816 core, generic over `Bus65816` (main CPU and SA-1) | yes | sfes-api | EMU-02 |
| `sfes-snes-spc` | SPC700 and S-DSP | yes | sfes-api | EMU-07 |
| `sfes-snes-ppu` | PPU1/PPU2, scanline renderer, layer output, sprite evaluation | yes | sfes-api | EMU-06 |
| `sfes-snes-cart` | Header detection, `Board` enum, coprocessors (SA-1 uses `sfes-snes-cpu`) | yes | sfes-api, sfes-snes-cpu (declared MS-02 exception) | EMU-04, EMU-10 |
| `sfes-snes-core` | `Snes` machine: bus, scheduler, DMA/HDMA, 5A22 I/O, controller ports | yes | the four chip crates | EMU-01, EMU-03, EMU-05 |
| `sfes-movie` | Movie container, recording/playback, BK2 import | no | zip, serde\_json | FND-04, TAS-01 |
| `sfes-pool` | Batched parallel stepping, branch search | no | rayon | API-09, TAS-03 |
| `sfes-env` | Integration loading, typed variables, rewards, action sets, env specs | no | serde\_json, toml | RL-05 to RL-14 |
| `sfes-dataset` | Dataset writer and world-model evaluation | no | parquet, tar | WM-01 to WM-05 |
| `sfes-cheevos` | rcheevos bindings for headless evaluation | no | cc (builds rcheevos) | ACH-01 |
| `sfes-py` | PyO3 extension module `sfes._native` | no | pyo3, numpy-free buffer protocol | API-10, API-11 |
| `sfes-capi` | C ABI and generated header | no | cbindgen (build) | API-13 |
| `sfes-player` | Reference player binary | no | winit, wgpu, cpal, gilrs, egui | PLY-01 to PLY-07 |
| `sfes-test-harness` | Vector runners, golden-hash checks, differential harness | no | serde\_json, insta | QA-01, QA-02 |

**Conventions for every crate**

- `edition = "2024"`, version, license (`MIT OR Apache-2.0`), repository and `rust-version` inherited from `[workspace.package]`.
- Lints inherited with `[lints] workspace = true` (section 12).
- External versions declared once in `[workspace.dependencies]` and used as `dep.workspace = true`.
- `no_std` crates use `#![no_std]` with `extern crate alloc;` and a `std` feature that is on by default and only adds `std::error::Error` impls and convenience APIs.
- Features are additive only. No feature may change emulation results; accuracy is a type parameter, not a feature.

## 3. Core abstractions

Four traits carry the whole design: `System` (what any console offers), `Accuracy` (a sealed marker chosen at compile time), `Observer` (zero-cost hooks) and `Bus65816` (how a CPU core talks to its machine). Sketches below show shape and ownership; final signatures are settled in code review.

### Units and newtypes

Timing and addressing use newtypes so units cannot be mixed by accident.

```rust
/// Master-clock cycles since power-on (21.477 MHz NTSC).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct Cycles(pub u64);

/// A 24-bit CPU address; the constructor masks to 24 bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Addr24(u32);

bitflags::bitflags! {
    /// Standard pad in serial order: B, Y, Select, Start, Up, Down, Left, Right, A, X, L, R.
    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
    pub struct Buttons: u16 { const B = 1 << 0; const Y = 1 << 1; /* … */ const R = 1 << 11; }
}
```

### Accuracy: a sealed marker trait

```rust
mod sealed { pub trait Sealed {} }

pub trait Accuracy: sealed::Sealed + Copy + Default + Send + Sync + 'static {
    const NAME: &'static str;
    const DOT_PPU: bool; // false for Fast: scanline rendering
}

#[derive(Clone, Copy, Debug, Default)] pub struct Fast;
#[derive(Clone, Copy, Debug, Default)] pub struct Precise; // P2; added later without API change
```

Branches such as `if A::DOT_PPU { … }` are resolved at compile time, so each profile compiles to its own specialized code with no runtime checks. Sealing stops downstream crates from adding profiles the core does not support.

### Observer: hooks that compile away

```rust
pub trait Observer: Clone + Send {
    /// False for NoopObserver: call sites skip building event data entirely.
    const ACTIVE: bool = true;

    #[inline(always)] fn on_instruction(&mut self, _ev: &InstrEvent) {}
    #[inline(always)] fn on_read(&mut self, _bus: BusId, _addr: u32, _value: u8, _at: Cycles) {}
    #[inline(always)] fn on_write(&mut self, _bus: BusId, _addr: u32, _value: u8, _at: Cycles) {}
    #[inline(always)] fn on_dma(&mut self, _ev: &DmaEvent) {}
    #[inline(always)] fn on_scanline(&mut self, _line: u16, _at: Cycles) {}
    #[inline(always)] fn on_interrupt(&mut self, _kind: Interrupt, _at: Cycles) {}
    #[inline(always)] fn on_frame_end(&mut self, _frame: u64) {}
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NoopObserver;
impl Observer for NoopObserver { const ACTIVE: bool = false; }
```

Observers receive `&mut self` and event values, never `&mut` machine state, so the borrow checker enforces FND-05's rule that observers cannot change emulation. A tuple impl `impl<A: Observer, B: Observer> Observer for (A, B)` composes observers without boxing. Addresses are u32 so one trait covers the 24-bit CPU buses and the 16-bit SPC700 bus; BusId says which.

### System: the console-neutral contract

```rust
pub trait System: Clone + Send {
    type Input: Copy + Default;

    fn descriptor(&self) -> &'static SystemDescriptor; // name, regions, devices, video/audio
    fn reset(&mut self);
    fn power_cycle(&mut self, seed: u64);
    fn set_input(&mut self, port: usize, input: Self::Input);
    fn run(&mut self, until: Stop) -> StopReason;      // FND-08

    fn region(&self, id: RegionId) -> &[u8];
    fn region_mut(&mut self, id: RegionId) -> &mut [u8];
    fn peek_bus(&self, addr: u32) -> Peek;              // never has side effects

    fn video(&self) -> VideoFrame<'_>;
    fn take_audio(&mut self) -> AudioChunk<'_>;

    fn save_state(&self) -> Vec<u8>;
    fn load_state(&mut self, bytes: &[u8]) -> Result<(), StateError>;
    fn state_hash(&self) -> StateHash;

    // Provided convenience methods, defined once for every console:
    fn run_frame(&mut self) -> StopReason { self.run(Stop::Frames(1)) }
}

pub enum Stop { Frames(u32), Scanlines(u32), Instructions(u32), Cycles(u64), Watch { max_frames: u32 } }
```

`RegionId` is an enum (`Wram`, `Vram`, `Cgram`, `Oam`, `Aram`, `CartRam`, `Rom`); `StateError` is a `thiserror` enum implementing `core::error::Error`, which works in `no_std`.

### The SNES machine

```rust
/// Everything that is machine state: cloned, serialized and hashed.
#[derive(Clone, Serialize, Deserialize)]
pub(crate) struct State {
    cpu: Cpu65816,   // registers and run state only
    bus: Bus,        // WRAM, PPU, APU, cart, DMA, 5A22 I/O, ports, deadlines
    meta: Meta,      // seed, region, frame counter, polled flag
}

pub struct Snes<A: Accuracy = Fast, O: Observer = NoopObserver> {
    state: State,
    obs: O,
    out: Outputs,    // frame, layer and audio buffers: not state (section 5)
    _profile: PhantomData<A>,
}

impl<A: Accuracy, O: Observer> Clone for Snes<A, O> {
    fn clone(&self) -> Self {
        Self { state: self.state.clone(), obs: self.obs.clone(), out: Outputs::default(), _profile: PhantomData }
    }
}

impl<A: Accuracy, O: Observer> Snes<A, O> {
    fn step_cpu(&mut self) {
        // Disjoint field borrows: the CPU gets the bus, observer and outputs, nothing else.
        let State { cpu, bus, .. } = &mut self.state;
        let mut view = BusView::<A, O> { bus, obs: &mut self.obs, out: &mut self.out };
        cpu.step(&mut view);
    }
}
```

Construction uses a builder that returns `Result`, so invalid ROMs never yield a half-built machine:

```rust
let mut snes = Snes::<Fast>::builder(rom)          // rom: Arc<[u8]>
    .region(Region::Auto)
    .seed(0)
    .firmware(FirmwareSet::default())
    .build()?;
```

### Bus65816: one CPU core for two machines

```rust
pub trait Bus65816 {
    fn read(&mut self, addr: Addr24, kind: Access) -> u8; // advances the clock by the region's speed
    fn write(&mut self, addr: Addr24, value: u8);
    fn idle(&mut self);                                   // internal-operation cycle
    fn interrupts(&self) -> Pending;                      // NMI, IRQ, sampled per the 65C816 rules
}
```

The main CPU's `BusView` and the SA-1's bus inside `sfes-snes-cart` both implement it, so one `Cpu65816` type serves both (MS-01). Static dispatch through `&mut impl Bus65816` keeps every access inlinable.

## 4. Timing and scheduling

The 65C816 drives time: every bus access advances the master clock by its cost, and other components run only when the CPU touches their registers or a scheduled deadline is reached. This catch-up model replaces per-cycle lockstep (EMU-01), needs no threads or coroutines, and keeps the whole machine one cloneable value.

### Clock domains

| Component | Clock | How time advances |
| --- | --- | --- |
| Master clock | 21.477 MHz NTSC, 21.281 MHz PAL | `Cycles` counter in the bus |
| 65C816 (5A22) | 6, 8 or 12 master cycles per access, by address region and `$420D` | Each `read`/`write`/`idle` adds its cost |
| PPU | Dots of 4 master cycles (some lines have longer dots) | H/V position derived from `Cycles`; lines rendered on catch-up |
| SPC700 + S-DSP | Separate oscillator, 24.576 MHz nominal | Fixed ratio to master clock (below); run on catch-up |
| SA-1 | Half the master clock | Catch-up at shared-memory and register access, plus each main-CPU instruction |
| Super FX | Its own clock setting | Catch-up at register and shared-ROM/RAM access |

**APU ratio.** APU oscillator cycles = master cycles × 45056 / 39375 for NTSC (one SPC700 cycle is 24 oscillator cycles): 24.576 MHz divided by 1890/88 MHz, reduced. The remainder is kept in an integer accumulator, so no time is lost or gained over a session (FND-02). The PAL ratio is computed the same way from its nominal crystal and recorded in an ADR.

### Deadlines

The bus keeps a small fixed array of the next scheduled events, each a `Cycles` value:

- NMI at the start of vertical blank, and the frame boundary (FND-04).
- H/V IRQ at its exact dot.
- HDMA setup at frame start and the per-line HDMA transfer.
- Auto-joypad read start and completion.
- Coprocessor interrupts and timers.

`next_deadline` is the minimum. After each bus access the CPU path compares `now >= next_deadline`, one branch on the hot path, and services due events in time order.

### Catch-up rules

1. **Owner sync.** Before an access to a component's registers or shared memory, that component runs to `now`: PPU `$2100–$213F`, APU ports `$2140–$217F`, coprocessor registers and their shared RAM.
2. **Scanline rendering (`Fast`).** PPU catch-up renders every line whose start time is ≤ `now` and has not yet been drawn, using registers as they stand at that moment. A register write in the middle of line *L* therefore takes effect on line *L+1*, matching EMU-01. HDMA for a line runs as a deadline before that line renders.
3. **APU laziness.** The SPC700 and S-DSP run only on port access, at frame end, and when the audio buffer needs samples. This is the largest performance win of the model.
4. **General DMA.** A write to `$420B` executes the transfer inside the bus, advancing the clock per byte and checking the HDMA deadline between bytes, so HDMA pre-empts DMA (EMU-05).
5. **Stop conditions.** `run(Stop)` checks its target after each instruction. A `Watch` stop is raised by the observer layer and also ends at the next instruction boundary. The frame boundary is the first instruction boundary at or after vertical-blank start, which is also when the next frame's input is latched (FND-04).

The diagram below shows the path of one CPU bus access.

&#91;embedded content: one CPU bus access · 2 checks, 6 steps\]

The common path is three steps and two comparisons; PPU, APU and coprocessor work happens only on the two side branches.

## 5. Chip designs

Each chip is a plain struct of its registers and memory with `#[derive(Clone, Serialize, Deserialize)]`, advanced by methods that take whatever bus or context they need as `&mut` parameters; no chip holds references to another. Behaviour is written from the cited documents (PROV-03), and each crate ships a `docs/chips/<chip>.md` explainer (QA-08).

### Shared rules

- Large memories are `Box<[u8; N]>`, never `[u8; N]` fields, so machines never put hundreds of KiB on the stack. They are allocated with `vec![0; N].into_boxed_slice().try_into()` instead of building the array on the stack first.
- Immutable data (ROM, firmware, the memory map) is `Arc`-shared and skipped by serde, then rebuilt on load.
- **Outputs are not state.** `Snes` holds a cloneable `State` and a separate `Outputs` (frame buffer, layer buffers, audio). `Clone for Snes` is implemented by hand: it clones `State` and gives the copy empty `Outputs`, which allocate on their first rendered frame. Search and RL snapshots that never render never pay for frame buffers.

### 65C816 (`sfes-snes-cpu`)

- `Cpu65816 { a, x, y, s, d: u16, db, pb: u8, pc: u16, p: Flags, e: bool, run: RunState }`, where `Flags` uses `bitflags` and `RunState` is `Running | Waiting | Stopped`.
- `step(&mut self, bus: &mut impl Bus65816)` executes one instruction or interrupt: fetch, then one `match opcode` with 256 arms calling small functions.
- Register width uses const generics instead of duplicated code, e.g. `fn lda<const M16: bool>(&mut self, bus: &mut impl Bus65816, mode: Mode)`. The compiler emits an 8-bit and a 16-bit version.
- Addressing modes are an enum. Each mode's function performs the exact sequence of reads, including dummy reads and internal cycles, because the test vectors check every bus cycle (EMU-02).
- Decimal mode, the emulation-mode stack and direct-page quirks, and interrupt sampling follow the WDC datasheet, with ADRs where vector results decide an ambiguity.
- Testing: a `RecordingBus` in the test harness implements `Bus65816` over a sparse 16 MiB map and records `(addr, value, signals)` per cycle, so it compares against the vectors directly.

### PPU (`sfes-snes-ppu`)

- State: registers, VRAM (64 KiB), CGRAM (512 B), OAM (544 B), latches and counters.
- `render_line(&mut self, line: u16, out: &mut LineOut)`:
  1. Fetch each enabled background for the mode into per-layer line buffers of `(color, priority)`.
  2. Evaluate sprites for the line, enforcing the 32-sprite and 34-tile limits and setting the time-over and range-over flags.
  3. Build window masks, apply mosaic, then colour math and brightness.
  4. Write the line into the frame buffer; when layer output is requested (WM-02), copy the per-layer buffers too.
- With render skip, steps 1, 3 and 4 are skipped but step 2 always runs, since its flags are readable state (EMU-06).
- Mode 7 uses the documented fixed-point matrix arithmetic in integers.

### SPC700 and S-DSP (`sfes-snes-spc`)

- `Spc700` mirrors the CPU design: registers, `step(&mut self, bus: &mut impl SpcBus)`, one `match`, vector-tested (EMU-07).
- `Dsp` holds 8 `Voice` structs (BRR decoder state, envelope, pitch counter) and echo state; it produces one stereo sample every 32 SPC700 cycles into `Outputs`.
- The APU unit owns audio RAM (64 KiB), timers and the four ports. `run_until(apu_cycles)` is what the bus calls on catch-up.
- **IPL boot ROM:** `Ipl` is an enum, `CleanRoom` or `UserSupplied(Arc<[u8; 64]>)`. The clean-room variant is a 64-byte SPC700 program written by the project from the documented upload protocol, so its code is original. It is the default (decided in review); the variant is part of machine state and recorded in movie headers.

### Cartridge (`sfes-snes-cart`)

- `Header::detect(&[u8]) -> Result<Header, HeaderError>` scores the LoROM, HiROM and ExHiROM header locations: checksum and complement, map-mode byte, plausible reset vector, printable title. An explicit override bypasses scoring.
- `MemoryMap` is a precomputed table of 2,048 pages of 8 KiB covering the 24-bit space. Each entry is `Page::Rom(offset) | Page::CartRam(offset) | Page::Wram(offset) | Page::Io | Page::Coproc(id) | Page::Open`. It is built once at load and `Arc`-shared, so a read is a table lookup.
- `Board` is an enum: `LoRom`, `HiRom`, `ExHiRom`, `Sa1(Box<Sa1>)`, `SuperFx(Box<Gsu>)`, `NecDsp(Box<NecDsp>)` and later variants. Boxing large variants keeps the enum small, and `Box<T: Clone>` is `Clone`, so cloning still works.
- **Coprocessors:** the SA-1 contains a `Cpu65816` plus I-RAM, BW-RAM access and its arithmetic and DMA units. The GSU is an interpreter for its own instruction set. The NEC DSP is a µPD77C25 interpreter running user-supplied firmware (EMU-10).

### Bus and 5A22 (`sfes-snes-core`)

The bus owns WRAM (128 KiB), the PPU, the APU unit, the cartridge, DMA channels, the multiply/divide unit, H/V timers, controller ports and the deadline table (section 4). `BusView` implements `Bus65816` by looking up the page, charging the access time, running due deadlines and catch-ups, performing the access and calling the observer.

## 6. State, cloning, serialization and determinism

In-memory snapshots are `Clone`; persisted state is a sectioned envelope of postcard-encoded chip structs; hashes are BLAKE3 over that encoding; and determinism is enforced by lints and CI rather than by convention.

### State envelope (FND-03)

```text
magic "SFES" | format u16 | system id ("snes") | core version | ROM SHA-256 | accuracy name | seed
section* : tag [u8; 4] | section version u16 | length u32 | postcard bytes
           CPU0  BUS0  PPU0  APU0  CART  META   (one per top-level State field)
```

- Each section has its own version. Loading an older section runs a chain of migration functions (`fn v1_to_v2(old: BusV1) -> BusV2`), each with a test. Before v1.0 the project MAY drop migrations and return `StateError::Unsupported { section, found, supported }` instead (FND-03).
- Unknown or missing sections, trailing bytes, a ROM mismatch or an accuracy mismatch are each distinct `StateError` variants, never panics.
- [postcard](https://docs.rs/crate/postcard) is chosen because its wire format is documented and stable since 1.0 and it is `no_std`. serde's derive keeps field order fixed, so encodings are canonical.
- serde does not handle arrays larger than 32 elements directly, so `sfes-api` provides a small `serde_with`-style helper module for `Box<[u8; N]>` that writes memories as byte strings.

**Loading.** `load_state` applies onto an existing machine rather than constructing one. Chip structs separate mutable state (serde-derived) from immutable context such as the ROM, firmware and memory map, which serde skips. The target machine keeps its context, after checking that the envelope's ROM SHA-256, IPL variant and firmware hashes match it.

### Hashing

`state_hash()` is BLAKE3 over the ROM SHA-256 followed by the section bytes, not the rest of the envelope header, so the same machine state hashes the same across core versions that share section versions. The `blake3` crate is used with default features off for `no_std`, which needs confirming at M0; its license choices (CC0 or Apache-2.0) are on the PROV-08 allow-list ([BLAKE3](https://github.com/BLAKE3-team/BLAKE3)).

### Determinism, enforced by tooling (FND-02)

| Rule | Enforcement |
| --- | --- |
| No floating point in emulation | `clippy::float_arithmetic` denied in chip and core crates |
| No hash-map iteration order in state | `HashMap` and `HashSet` listed in `disallowed-types` for those crates; use arrays or `BTreeMap` |
| No wall clock or OS randomness | `Instant::now`, `SystemTime::now` and OS RNG listed in `disallowed-methods` |
| No platform-sized integers in state | Review rule plus a CI job that serializes golden states on a 32-bit target (`i686-unknown-linux-gnu`) and compares hashes |
| Seeded power-on | `SplitMix64` implemented in `sfes-api` in a few lines, no `rand` dependency |
| Thread-count independence | Pool tests run 1, 2 and N threads and compare per-machine hashes |

Golden per-frame hashes for every committed test ROM live as text files under `tests/assets/golden/`. `cargo xtask golden --update` regenerates them, and the diff is reviewed in the PR like code.

### Clone cost

Cloning `State` copies about 258 KiB of memories plus registers and bumps `Arc` counts for ROM, firmware and the memory map. A `criterion` benchmark tracks it against the 50 µs target (QA-05).

## 7. Observers, watchpoints, movies and input

Runtime-configurable watching is one concrete observer type, `WatchObserver`; movies are a separate crate that wraps any `System` and records exactly what reached the controller ports.

### WatchObserver (FND-05, RL-08, DEC-02)

```rust
#[derive(Clone, Default)]
pub struct WatchObserver {
    watches: Vec<Watch>,        // sorted by start address
    span: Option<(u32, u32)>,   // min..=max of all watches: one compare rejects most accesses
    events: EventRing,          // fixed capacity, oldest dropped and counted
    history: WriteHistory,      // last 16 writes per watched address, with PC (DEC-02)
    stop: bool,                 // makes run(Stop::Watch) return at the next instruction boundary
}

pub struct Watch { bus: BusId, range: RangeInclusive<u32>, kind: AccessKind, when: Option<ValueCond>, action: Action }
pub enum Action { Record, Stop }
```

- The hot path is: the `span` check, then a binary search only when the address is inside it. Benchmarked against the 5% target (QA-05).
- `Snes<Fast, WatchObserver>` is the type the Python bindings and the RL layer use. `Snes<Fast>` with `NoopObserver` is the fastest path for Rust users who need no watches. Python always uses WatchObserver; with no watches its cost is one comparison per bus access, covered by the 5% benchmark.
- Instruction tracing (DEC-05) is a second observer, `TraceObserver`, that appends fixed-size records to a `Vec` the caller drains each frame. It never writes to files itself, so observers stay `no_std`.

### Input and ports (API-05, EMU-08)

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Device {
    Pad(Buttons),
    Multitap([Buttons; 4]),
    Mouse { dx: i8, dy: i8, left: bool, right: bool },
    SuperScope { x: u16, y: u16, fire: bool, cursor: bool, turbo: bool, pause: bool },
    None,
}
```

Input set before a frame is latched for that frame. The frame's `polled` flag is set when the game reads `$4016`/`$4017` or the auto-read results at `$4218`–`$421F`, which is how lag frames are detected (TAS-04).

### Movies (`sfes-movie`, FND-04)

- `Movie { header, start, inputs: Vec<FrameInput>, events: Vec<(u64, MovieEvent)>, checkpoints: Vec<(u64, StateHash)>, markers }`.
- `MovieEvent` records anything besides input that changed state during recording: `Poke { region, offset, bytes }`, `Reset`, `PowerCycle`. Pokes during a recording are always captured, so movies stay replayable (FND-06).
- Checkpoints store the state hash every N frames (default 60), so playback reports the first diverging checkpoint and bisects to the frame (API-08).
- The file is a zip (`zip` crate, deflate only): `header.json` via serde\_json, `inputs.bin` as little-endian `u16` per port per frame plus device extras, optional `events.bin`, `checkpoints.bin`, `markers.json`, and `inputs.txt` as a human-readable copy.
- `Recorder<S: System>` and `Playback<S: System>` wrap any system by value, so movies work unchanged for a second console (MS-04).
- BK2 import (TAS-01) parses the header and log key into the same `Movie` type; it never accepts a `CoreState` entry.

## 8. Batched pool and parallelism

`sfes-pool` is the only crate that uses threads: it owns N machines and steps them with rayon, writing results into caller-provided buffers by index, so outcomes never depend on scheduling (API-09).

```rust
pub struct Pool<S: System> {
    machines: Vec<S>,
    starts: Vec<Arc<S>>,            // start states for auto-reset
    workers: rayon::ThreadPool,     // owned, not the global pool
    cfg: PoolConfig,                // frame-skip, aggregation, render and audio switches
}

impl<S: System> Pool<S> {
    pub fn step(&mut self, actions: &[S::Input], out: StepBuffers<'_>) -> Result<(), PoolError> {
        let frame_len = self.cfg.obs_len();
        self.workers.install(|| {
            self.machines
                .par_iter_mut()
                .zip(actions.par_iter())
                .zip(out.obs.par_chunks_exact_mut(frame_len))
                .enumerate()
                .for_each(|(i, ((m, a), obs))| step_one(m, *a, obs, &self.cfg, i));
        });
        Ok(())
    }
}
```

- **Buffers:** `StepBuffers` borrows one contiguous `&mut [u8]` of N × H × W × C for observations plus per-machine slices for rewards and done flags. `par_chunks_exact_mut` hands each worker a disjoint slice, so the borrow checker proves there is no data race and no `unsafe` or locking is needed.
- **Auto-reset:** a finished machine resets with `machine.clone_from(&*start)`. `Snes` implements `clone_from` to reuse its existing allocations, so resets in long runs allocate nothing. PoolConfig selects Gymnasium's NEXT\_STEP or SAME\_STEP timing; with SAME\_STEP the final observation goes to a separate buffer (RL-02).
- **Thread count:** defaults to `std::thread::available_parallelism()`, which counts logical CPUs; the docs recommend setting it to physical cores for best throughput. Results are identical for any count (FND-02).
- **Granularity:** `with_min_len` keeps per-task overhead low when N is large and frames are cheap, for example RAM-only observations.
- **Snapshots:** `snapshot(i) -> SnapshotId` and `restore(i, id)` store clones in a slab inside the pool (RL-12). Ids are plain indexes, so Python never holds Rust references.

### Branch search (TAS-03)

Search runs on the same pool. Each expansion clones the frontier states, applies candidate inputs in parallel and hashes results. A `HashSet<StateHash>` is used for membership only; candidate order is kept in a `Vec` sorted by objective, then by input sequence, so results are reproducible.

## 9. Bindings: Python, C and WASM

Bindings are thin: they convert types, release the GIL or catch panics at the boundary, and call the Rust API; no emulation or RL logic is written twice.

### Python (`sfes-py` + `python/sfes`, API-10)

- **Build:** maturin is the PEP 517 backend in `pyproject.toml`, with `module-name = "sfes._native"` and `python-source = "python"`. PyO3 uses features `abi3-py314` and `extension-module`, so there is one wheel per platform for Python 3.14+. Because pyproject.toml sits at the workspace root, \[tool.maturin\] sets manifest-path to crates/sfes-py/Cargo.toml ([PyO3 features](https://pyo3.rs/main/features)).
- **Native classes:** `Machine` wraps `Snes<Fast, WatchObserver>`; `Pool` wraps `Pool<Snes<Fast, WatchObserver>>`; `Movie`, `Integration` and `EnvSpec` wrap their Rust types.
- **GIL:** every call that emulates runs inside `py.detach(|| …)`, PyO3's current API for releasing the GIL ([PyO3 parallelism guide](https://pyo3.rs/main/parallelism)).
- **Concurrent use (API-15):** `#[pyclass]` objects are borrow-checked at run time. A second thread that tries to step a machine already being stepped gets a `RuntimeError` from PyO3's borrow check instead of blocking, which is exactly the required behaviour.
- **Zero-copy observations (API-11):** Python allocates the output arrays with NumPy and passes them in. Rust takes them as `PyBuffer<u8>` through the buffer protocol (available in the limited API from 3.11), checks C-contiguity and shape, and writes into them while the GIL is released. NumPy arrays already export `__dlpack__`, so PyTorch and JAX get zero-copy access with no DLPack code in sfes.
- **Pure-Python layer:** `sfes.gym` (`SnesEnv`, `SnesVectorEnv`), `sfes.pz` (PettingZoo Parallel API), `sfes.integrations` (stable-retro loader front end), `sfes.datasets`. Runtime dependencies are `numpy` and `gymnasium`; `pettingzoo` is an optional extra. Minimums: gymnasium 1.3, the first release supporting Python 3.14, and pettingzoo 1.26, which supports 3.14 but not Windows officially.
- **Types:** a hand-maintained `_native.pyi` stub, checked in CI by `mypy --strict` over the examples.
- **Wheels:** built by `maturin-action` for manylinux x86-64 and aarch64, macOS arm64 and Windows x86-64, published with PyPI trusted publishing (OIDC), so no long-lived tokens are stored.

### C ABI (`sfes-capi`, API-13, M10)

- Opaque handles: `sfes_snes_new` returns a pointer from `Box::into_raw`; `sfes_snes_free` reclaims it with `Box::from_raw`.
- Edition 2024 requires `#[unsafe(no_mangle)]` on exported functions, which makes each export an explicit, reviewable unsafe surface.
- Every export wraps its body in `std::panic::catch_unwind` and returns an `SfesStatus` code. A panic never crosses the boundary.
- `cbindgen` runs from `cargo xtask capi-header`; the generated `sfes.h` is committed, and CI fails if it is stale.

### WASM (API-12, P2)

The chip and core crates already build for `wasm32-unknown-unknown` as part of the `no_std` CI job (FND-10). The browser player adds `wasm-bindgen` in `sfes-player` behind a `web` feature.

## 10. RL, dataset, TAS and achievement layers

Everything that runs per step (rewards, done checks, sticky actions, frame aggregation) is Rust inside the pool workers; Python only configures and consumes. These layers depend on `sfes-api` traits, not on SNES types, so they serve a second console unchanged.

### `sfes-env` (RL-03 to RL-14)

- **Integrations:** `Integration::load(dir)` reads stable-retro's `data.json`, `scenario.json`, `metadata.json` and `rom.sha` with serde\_json into typed structs; unknown keys are kept and reported, not silently dropped (RL-07).
- **Variables:** type strings parse into `VarType { endian: Endian, format: Format, bytes: u8 }` with `Format::{Unsigned, Signed, Bcd, LowNybbleBcd}`, implemented with `FromStr`. Reading a variable is a pure function of a region slice.
- **Address mapping:** stable-retro addresses are translated to `(RegionId, offset)` once at load. The exact SNES address convention used by existing integrations is confirmed from shipped integrations at M5 and recorded in an ADR.
- **Rewards and done:** compiled at load into a `Vec<Term>` and a `DoneRule` enum tree, evaluated in Rust after each frame. `settle_frames` keeps a per-variable stability counter. Event conditions come from the `WatchObserver` event ring.
- **Action sets:** an action index maps through `Vec<Buttons>`; multi-discrete combines a d-pad table with a button mask.
- **Stochasticity:** each environment owns a `SplitMix64` seeded from `reset(seed)` for sticky actions, no-op starts and start-state sampling (RL-11).
- **Specs:** `EnvSpec` is a serde struct stored as TOML with a `version` field. `EnvId` parses and formats `Game-Snes/State-vN` with `FromStr` and `Display`. A registry maps ids to specs; the full spec is returned in info on reset (RL-10).
  - **Start states:** stored as a movie plus a cached serialized state. When a release changes the state format, the cache is rebuilt by replaying the movie with its recorded seed.
  - **Floats:** rewards are f64 (float lints relaxed in sfes-env). Sticky actions compare the RNG's u64 output with an integer threshold, so no float touches emulation.

### `sfes-dataset` (WM-01 to WM-05)

- The step loop sends finished steps over a bounded `std::sync::mpsc` channel to a writer thread, so disk I/O never touches emulation timing or results.
- Tabular columns go to Parquet via the `parquet` crate and images and audio to tar shards via the `tar` crate. Both sit behind an `io` feature, so the evaluation harness can be used without them.
- Counterfactual rollouts (WM-03) use pool snapshots; hidden-event mining (WM-04) is a pass over recorded variables and sprite tables; the evaluation harness (WM-05) replays a model's actions through `Playback`.

### TAS (TAS-01 to TAS-05)

BK2 import and lag data live in `sfes-movie` (section 7); branch search in `sfes-pool` (section 8).

### `sfes-cheevos` (ACH-01 to ACH-04)

- rcheevos (MIT) is vendored as a git submodule pinned to a release tag and built with the `cc` crate; its license is recorded in `THIRD_PARTY.toml` at the repository root.
- The FFI layer is the crate's only `unsafe` code: memory-peek callbacks read from region slices laid out in RetroAchievements' SNES address order, confirmed from RA documentation before M10.
- A safe wrapper exposes `CheevosRunner::evaluate(&mut self, sys: &impl System) -> Vec<Trigger>`, used by the replay suite, the false-trigger fuzzer and the revision diff.

## 11. Reference player

`sfes-player` is an ordinary consumer of the public API; it has no access to crate internals, which keeps the API honest. It grows from a development viewer at M2 to the recording and authoring tool at M5 and the full player at M6 (PLY-01 to PLY-07).

- **Event loop:** a `winit` application implementing its `ApplicationHandler` trait. Emulation runs on the main thread with fixed-timestep pacing at the region's frame rate (about 60.1 Hz NTSC), independent of display refresh.
- **Video:** each frame's RGB555 buffer is converted to RGBA8 on the CPU and uploaded to a `wgpu` texture, drawn with integer scaling and the correct aspect ratio. No shader pipeline beyond that blit (non-goal).
- **Audio:** the `cpal` callback reads from a lock-free single-producer, single-consumer ring buffer. A small resampler with dynamic rate control converts the native S-DSP rate to the device rate and nudges the ratio by ±0.5% to keep the buffer half full, so audio neither crackles nor drifts. The resampler lives in a `sfes-player::audio` module and is not part of the core (API-04).
- **Input:** keyboard through winit and gamepads through `gilrs`, mapped through a TOML config in the platform config directory.
- **Tools:** `egui` panels for recording, start-state authoring (PLY-02), RAM search and watch (PLY-04), and later inspection views (PLY-05). Panels call the same `sfes-env` and `sfes-movie` APIs the Python package uses, so a start state made in the player is byte-identical to one made in Python.
- **Errors:** `anyhow` for application-level errors, since the player is a binary; library crates never use it.

## 12. Errors, safety, lints and dependency policy

Correctness rules are encoded as compiler and clippy settings in the workspace, so they hold for AI-written and hand-written code alike.

### Errors and panics

- Library crates define error enums with `thiserror` (version 2 supports `no_std`); variants carry the data needed to act on them, e.g. `HeaderError::Checksum { stored, computed }`.
- Binaries (`sfes-player`, `xtask`) use `anyhow` with context strings.
- The core never panics on input (FND-09). `debug_assert!` checks internal invariants in tests and fuzzing.
- Memory indexing is panic-free by construction: a `u16` address converted with `usize::from` indexes a `[u8; 0x10000]`, so the bounds check is provably unnecessary and the compiler removes it. Wider buses mask the address to the region size first.
- Hardware wrap-around is written explicitly with `wrapping_add`, `wrapping_sub` and friends. Tests and fuzzing run with overflow checks on, so an unintended overflow fails loudly instead of silently wrapping.

### Workspace lints

```toml
[workspace.lints.rust]
unsafe_code = "deny"            # chip/core crates add #![forbid(unsafe_code)]; FFI modules opt in locally
missing_docs = "warn"
unreachable_pub = "warn"

[workspace.lints.clippy]
pedantic = { level = "warn", priority = -1 }
unwrap_used = "deny"
expect_used = "deny"
panic = "deny"
todo = "deny"
dbg_macro = "deny"
float_arithmetic = "deny"      # relaxed with #[allow] in player, env, dataset and pool code only
```

- `clippy.toml` adds `disallowed-types` (`HashMap`, `HashSet` in state) and `disallowed-methods` (`Instant::now`, `SystemTime::now`) for chip and core crates (section 6), plus allow-unwrap-in-tests, allow-expect-in-tests and allow-panic-in-tests so tests stay idiomatic.
- CI runs `cargo clippy --all-targets -- -D warnings`, so warnings block merges while still being suppressible with a documented `#[allow(..., reason = "…")]`.
- Exceptions to `unsafe_code`: the FFI modules of `sfes-py`, `sfes-capi`, `sfes-cheevos` and the GPU and audio boundary of `sfes-player`. Each `unsafe` block carries a `// SAFETY:` comment, checked by `clippy::undocumented_unsafe_blocks`.

### Dependency policy

- Chip and core crates may depend only on `serde`, `postcard`, `bitflags`, `thiserror` and `blake3`. Anything else needs an ADR.
- `cargo-deny` checks licenses (PROV-08: strict for every library crate and the wheel; the player alone also allows MPL-2.0, OFL-1.1 and UFL-1.0 through per-crate exceptions in deny.toml), RustSec advisories, duplicate versions and that all sources are crates.io or the vendored rcheevos submodule.
- Dependabot opens grouped weekly updates for Cargo, pip and GitHub Actions. Updates merge only with green CI.

### Toolchain

`rust-toolchain.toml` pins one stable release, which is also the MSRV declared in `rust-version`. It is raised deliberately in a minor release, never as a side effect. A non-blocking CI job builds on the latest stable to catch upcoming breakage early.

## 13. Testing, benchmarking and provenance tooling

Tests are layered from per-instruction vectors up to Python end-to-end runs; benchmarks gate regressions with instruction counts because cloud runners are too noisy for wall-clock gates; and provenance rules are checked by `cargo xtask` commands in CI.

### Test layers (QA-01)

| Layer | Tooling | Runs |
| --- | --- | --- |
| Unit tests | `#[cfg(test)] mod tests` beside the code, run with `cargo nextest` | Every push |
| CPU and SPC700 vectors | `sfes-test-harness` runner over SingleStepTests JSON, through `RecordingBus` | Every push (cached) |
| Test ROMs and own test cartridges | Golden frame, audio and state hashes under `tests/assets/golden/` | Every push |
| Properties | `proptest`: `load(save(m)) == m`, clone equivalence, pool thread-count independence | Every push |
| Determinism matrix | Golden hashes on Linux x86-64, macOS arm64, Windows x86-64 and a 32-bit target | Every push |
| Fuzzing | `cargo-fuzz` targets: ROM load, state load, movie load, CPU step on a random bus | Nightly, 10 minutes per target |
| Python | `pytest` against the built wheel: Gymnasium checker, vector env, PettingZoo API test | Every push from M5 |
| Differential (M8) | Harness runs oracle binaries from paths in environment variables, compares outputs | Manual and weekly |

- **Vector storage:** the test-vector sets are large, so they are not committed. `cargo xtask vectors fetch` downloads them at a pinned commit, verifies SHA-256 checksums from `tests/assets/MANIFEST.toml`, and CI caches them. Both sets are MIT-licensed (spec, section 15).
- **Golden updates:** `cargo xtask golden --update` rewrites golden files; changes appear in the PR diff and need a written reason in the PR description. Each golden carries a status: unverified (blessed after manual comparison with an oracle's output) or oracle-confirmed (by the M8 harness). --update resets changed entries to unverified.
- **Coverage:** `cargo llvm-cov` publishes a report; it informs reviews but does not gate merges.

### Benchmarks (QA-05, QA-06)

- `criterion` benchmarks measure wall time for every QA-05 metric: headless frame, clone, pool scaling, observer overhead. They run locally and on release tags, and results are published with each release.
- The CI regression gate uses `gungraun` (the successor to `iai-callgrind`), which counts instructions under Valgrind on Linux. Counts are stable on shared runners, unlike wall time. A PR fails if a gated benchmark's instruction count grows more than 3% over the merge-base, unless the PR is labelled `perf-accepted` with a reason.
- Absolute targets are checked by hand on a named machine before each release (cloud-only setup confirmed).

### Provenance tooling (PROV-02 to PROV-10)

| Command | Checks | Spec |
| --- | --- | --- |
| `cargo xtask provenance lint` | Parses chip crates with `syn`: every module and public function has an `@source:` tag naming a known bibliography id | PROV-03 |
| `cargo xtask provenance scan` | Searches code files for known emulator copyright headers and author names | PROV-05 |
| `cargo xtask provenance assets` | Every file under `tests/assets/` has a manifest entry with an allowed license | PROV-09 |
| `cargo xtask provenance commits` | Every commit in the PR has `Signed-off-by`; commits with `Assisted-by` touching chip crates require the PR's AI-review checklist to be ticked | PROV-06, PROV-07 |
| `cargo xtask provenance report` | Generates the release report: bibliography, tag coverage, ADR count, oracle versions, exposure log | PROV-10 |

`cargo-deny` covers dependency licenses (PROV-08). All five commands run in CI on every PR from M0.

## 14. SDLC

sfes uses GitHub flow with Conventional Commits and release-plz, the common setup for small and medium Rust projects: `main` is always releasable, every change arrives through a pull request with green CI even when the author is the only reviewer, and releases are cut by merging a release PR that release-plz maintains.

### Planning and tracking

- GitHub Issues with one GitHub Milestone per spec milestone (M0 to M11). Labels for area (`cpu`, `ppu`, `apu`, `cart`, `core`, `env`, `py`, `player`, `prov`) and priority (`P0` to `P3`).
- Issue templates: bug report (asks for the PLY-06 bundle or a movie), hardware-behaviour question (becomes an ADR), feature.
- ADRs in `docs/adr/NNNN-title.md` using a short template: context, sources, decision, consequences, confidence. An ADR is required for hardware ambiguities (PROV-04), new dependencies in chip crates, and any format change.

### Branches, commits and merges

- Short-lived branches from `main`, named `area/short-description`.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat(ppu): …`, `fix(cpu): …`, `docs(adr): …`), which release-plz reads to choose version bumps and write the changelog ([release-plz](https://docs.rs/crate/release-plz/0.3.98)).
- Every commit carries `Signed-off-by` (PROV-06) and, when an assistant contributed, `Assisted-by: <tool>` (PROV-07). Bot commits from release-plz and Dependabot are exempt when they touch only manifests, lockfiles and changelogs.
- PRs merge by rebase, keeping a linear history of signed, conventional commits. Branch protection on `main` requires all checks, linear history and no force-pushes.

### Definition of done for a PR

- [ ] Tests added or updated; golden-file changes explained.
- [ ] Public items documented; chip explainer updated for behaviour changes.
- [ ] `@source:` tags present for hardware behaviour, with an ADR for any judgement call.
- [ ] Clippy, fmt, cargo-deny and provenance checks pass.
- [ ] If AI-assisted: the AI-review checklist is complete.

### AI-assisted workflow (PROV-07, confirmed in use)

1. **Clean workspace.** Assistants run only in the sfes checkout. No emulator source code exists on paths they can read, and none is ever pasted into a prompt. Exposure is logged per PROV-01.
2. **Agent instructions in the repo.** An `AGENTS.md` at the root states the provenance rules, allowed sources and required `@source:` tags, so every assistant session starts from the same rules.
3. **Prompt from documents.** Chip work is prompted with the cited document section or test vectors, not with "how does emulator X do it".
4. **Review against the source.** Before merge, each AI-written hardware function is read against its cited section; code that does not match a citation is rewritten. The PR's AI-review checklist records this.
5. **Trailer and CI.** The `Assisted-by` trailer is required, and `cargo xtask provenance commits` blocks PRs where AI-assisted chip changes lack a completed checklist.

### Versioning and releases

- All publishable crates and the Python package share one version number, released together, which keeps compatibility simple for users. Versions stay `0.x` until v1.0 at M10 (spec section 3).
- `cargo-semver-checks` runs in the release PR from the first published version; before 1.0, breaking changes bump the minor version.
- release-plz opens and updates the release PR on every merge to `main`. Merging it publishes crates to crates.io, tags `vX.Y.Z` (release-plz configured for one workspace tag instead of its default per-crate tags) and creates a GitHub release with the git-cliff changelog.
- The tag triggers `maturin-action` to build wheels and publish to PyPI with trusted publishing. rustdoc goes to docs.rs automatically; the mdBook user guide deploys to GitHub Pages.

The pipeline below shows the checks and the release path.

&#91;embedded content: CI and release pipeline · 6 checks, 3 outputs\]

Separately, a nightly workflow runs each fuzz target for 10 minutes and builds on the latest stable Rust; failures open issues but do not block PRs.

## 15. Implementation plan, M0 to M5

The path to the v0.1 alpha builds the foundations first and the chips in the order the hardware is easiest to learn; each milestone ends with the exit criteria in the spec. Later milestones are planned when M5 ships.

### M0 — Provenance and skeleton

- [ ] Create the GitHub repo with `LICENSE-MIT`, `LICENSE-APACHE`, `PROVENANCE.md`, `AGENTS.md`, `SECURITY.md`, issue and PR templates, branch protection.
- [ ] Workspace `Cargo.toml` with `[workspace.package]`, `[workspace.lints]`, `[workspace.dependencies]`; `rust-toolchain.toml`; `deny.toml`; `clippy.toml`.
- [ ] Empty crates from section 2 with `#![no_std]` where required; `sfes-api` traits from section 3 compiling.
- [ ] `xtask` with `provenance lint|scan|assets|commits`; `provenance/bibliography.toml` seeded with fullsnes, anomie's documents, the SNESdev wiki and the WDC datasheet.
- [ ] CI workflow with fmt, clippy, build matrix, nextest, cargo-deny and provenance jobs; release-plz configured but not publishing.
- [ ] ADR 0001: catch-up scheduling. ADR 0002: postcard state format. ADR 0003: crate naming and layering.
- [ ] **`blake3` in `no_std`:** confirm the crate builds with default features off for `thumbv7em` and `wasm32`; fall back to hashing only in `std` builds if not.

### M1 — 65C816

- [ ] `cargo xtask vectors fetch` with pinned commits and checksums; `RecordingBus` and the vector runner.
- [ ] `Cpu65816` with all 256 opcodes, both modes, interrupts, `WAI` and `STP`; `docs/chips/cpu.md` explainer.
- [ ] All vectors passing, including per-cycle bus records.

### M2 — Bus, DMA and video

- [ ] Header detection, `MemoryMap`, LoROM and HiROM boards; WRAM; 5A22 I/O.
- [ ] Deadline table and catch-up in `BusView`; DMA and HDMA.
- [ ] PPU registers and the scanline renderer for BG modes 0–7, sprites, windows, colour math.
- [ ] Development viewer (PLY-01); first golden frame hashes, marked unverified, from permissively licensed test ROMs and own test cartridges.

### M3 — Audio and base system

- [ ] SPC700 passing its vectors; timers, ports and the cross-clock ratio; clean-room IPL boot program.
- [ ] S-DSP voices, envelopes, echo; audio golden hashes.
- [ ] 20 commercial games without coprocessors play to checkpoints locally; first performance measurement against QA-05, with targets reset if needed.
- [ ] **PAL APU ratio:** derive and record in an ADR.

### M4 — Library foundations

- [ ] State envelope, migrations scaffold, BLAKE3 hashing, `Clone` with empty outputs and `clone_from`.
- [ ] `WatchObserver`, `Stop::Watch`, `TraceObserver`.
- [ ] `sfes-movie` with checkpoints and desync detection; pokes recorded.
- [ ] Determinism matrix and 32-bit job green; property tests; fuzz targets running nightly.

### M5 — v0.1 public alpha

- [ ] `sfes-pool` with auto-reset and snapshots; `gungraun` gate in CI.
- [ ] `sfes-env`: stable-retro loader, variables, rewards, action sets, env specs.
- [ ] `sfes-py` and `python/sfes`: `Machine`, `Pool`, Gymnasium and PettingZoo wrappers, `.pyi` stubs.
- [ ] Player recording and start-state authoring (PLY-02); start states for Super Mario World, F-Zero and Street Fighter II.
- [ ] Wheels on PyPI, crates on crates.io, benchmark report, README with quick start; announcement and the 4-week demand review.
- [ ] **stable-retro SNES address convention:** confirm from shipped integrations how their addresses map to WRAM and cartridge RAM.
- [ ] **gungraun threshold:** 3% is a starting value; tune after a month of data.

## 16. Open questions, risks and sources

The design is complete enough to start M0; open questions for M0 to M5 sit as tasks in their milestones (section 15), and the ones below belong to later milestones.

### Open questions

- [ ] **IPL default:** resolved in review: clean-room by default, user-supplied dump as an exact-timing option.
- [ ] **RetroAchievements SNES memory layout:** confirm from RA documentation. M10.

**Resolved in design review (2026-11-12):** SA-1 dependency as a declared MS-02 exception; goldens blessed by manual oracle comparison and marked unverified until M8; clean-room IPL by default; strict licenses for libraries and the wheel, with MPL-2.0, OFL-1.1 and UFL-1.0 allowed for the player only. All four are reflected in the feature spec.

### Design risks

| Risk | Mitigation |
| --- | --- |
| Ordering bugs between deadlines, HDMA, IRQ and catch-up | Per-cycle CPU vectors, timing test ROMs, own test cartridges, differential harness at M8 |
| Scanline rendering breaks mid-line raster effects | Known `Fast` limitation, tracked on the compatibility list; `Precise` profile is a type parameter, not a rewrite |
| Compile time grows with generic instantiations (`A` × `O`) | Only a few combinations are instantiated: `Fast` with `NoopObserver` and with `WatchObserver`, plus test observers |
| Clone cost creeps up as state grows | Clone benchmark with a target; `Arc` for anything immutable |
| AI-assisted code reproduces emulator code from memory | Section 14 workflow, `@source:` lint, review against citations |
| serde handling of large arrays | One helper module in `sfes-api`, round-trip property tests |

### Sources

- [sfes feature specification](feature-specification.md)
- [postcard](https://docs.rs/crate/postcard)
- [BLAKE3](https://github.com/BLAKE3-team/BLAKE3)
- [PyO3 features reference](https://pyo3.rs/main/features)
- [PyO3 parallelism guide](https://pyo3.rs/main/parallelism)
- [release-plz](https://docs.rs/crate/release-plz/0.3.98)
- [Conventional Commits](https://www.conventionalcommits.org/)
