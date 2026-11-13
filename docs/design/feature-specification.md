# SNES Library Core — Feature Specification

Nov 11, 2026 · @Matthew

## 1. Purpose, scope and how to read this spec

The project is a **headless-first, library-first SNES emulator core in Rust** whose machine state can be cloned, batched and inspected, built clean-room from hardware documentation only. It serves reinforcement-learning (RL) research first, world-model and decompilation work second, and TAS and achievement authoring third, while remaining playable for testing and reference.

**Why it exists.** Every widely used SNES core today is built as an application or a libretro plugin. libretro is single-instance by design, so RL wrappers must run one emulator per OS process ([openai/retro #64](https://github.com/openai/retro/issues/64)); the accuracy-focused cores use cooperative threads that cannot be cloned; and the most-used fast core, Snes9x, is licensed non-commercial ([libretro licenses](https://docs.libretro.com/development/licenses/)). The one serious Rust core, RustySNES, has an open, self-reported question about code derived from GPLv3 emulators ([its provenance file](https://github.com/doublegate/RustySNES)). A permissive core with clean, auditable provenance and a library API does not exist.

**Goals**

1. A Rust crate where one console is one owned value: clone it, run thousands of copies on threads, read any memory, hook any event.
2. A `pip install` Python package with a batched, Gymnasium-compatible RL API and stable-retro-compatible game integrations.
3. Provenance as a published, enforced feature from the first commit.
4. Enough accuracy for commercial games to play correctly, verified by tests, with a path to higher accuracy.
5. A design that admits a second console (Sega Genesis/Mega Drive) by adding chip crates only.
6. A codebase that teaches emulation: documented decisions, cited sources, test-first bring-up.

**Out of scope** is listed in section 15. In short: no shaders, netplay, mobile apps, Lua, HD packs or GUI debugger.

**How to read it**

- Every requirement has an ID: `PROV`, `FND`, `EMU`, `API`, `RL`, `WM`, `DEC`, `TAS`, `ACH`, `PLY`, `MS`, `QA`, plus a number (e.g. `FND-01`).
- Each carries a priority (P0–P3), a milestone (M0–M11) and the communities it serves; section 3 defines these.
- **MUST**, **SHOULD** and **MAY** follow [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119): MUST is a release blocker for its milestone, SHOULD needs a written reason to skip, MAY is optional.
- "Implementation guidance" notes inform the technical design that follows; they are strong defaults, not final designs. Where this spec settles an ambiguity, it says **Decision:**; where it cannot, the item is listed in section 15 as an open question.
- The project name is sfes (Super Ferrous Entertainment System); this spec says "the core" throughout. The name is available on crates.io and PyPI (section 15).

## 2. Communities and user stories

Seven communities are served, in priority order; the first three justify the project and the rest come almost free from the same foundations.

| Tier | Community | What they lack today | Evidence |
| --- | --- | --- | --- |
| Primary | RL researchers | In-process, batched, cloneable SNES environments; stable-retro runs one emulator per process | [openai/retro #64](https://github.com/openai/retro/issues/64); [stable-retro-turbo](https://pypi.org/project/stable-retro-turbo/1.0.0.post2/) reports \~10x from in-process batching (NES) |
| Primary | World-model researchers | Action-labelled frames paired with exact internal state | [StatePlay](https://arxiv.org/pdf/2607.26754) reads HP and meters from emulator memory; [WildWorld](https://arxiv.org/html/2603.23497) needed engine hooks for state |
| Primary | Decompilation and port developers | A steppable core to run the original alongside a reimplementation and diff RAM | [zelda3](https://github.com/snesrev/zelda3) compares RAM after each frame |
| Secondary | TAS authors | Parallel branch search; cross-emulator movie import | [BK2 format](https://tasvideos.org/Bizhawk/BK2Format) |
| Secondary | Achievement developers | Automated testing; manual tickets and save states today | [RA ticket handling](https://docs.retroachievements.org/Handling-Tickets) |
| Tertiary | Emulation learners and contributors | A modern, documented, clean-room codebase to learn from | — |
| Tertiary | Testers and verifiers | A simple player to check behaviour by hand | — |

### User stories

Each story names the requirements that satisfy it.

**RL researchers**

- **US-01** As an RL researcher, I `pip install` a wheel and create 64 SNES environments in one process that step together, so that I can train PPO without subprocess overhead. → `API-10`, `RL-01`, `RL-02`
- **US-02** As an RL researcher, I load an existing stable-retro integration (`data.json`, `scenario.json`, `.state`) unchanged, so that I can reuse community work. → `RL-07`
- **US-03** As an RL researcher, I cite an environment ID such as `SuperMarioWorld-Snes/YoshiIsland2-v1` whose frame-skip, sticky actions, start state and termination are pinned, so that others reproduce my numbers exactly. → `RL-10`
- **US-04** As an exploration researcher, I snapshot promising states into an archive and restart from any of them in microseconds, so that I can run Go-Explore-style methods. → `FND-01`, `RL-12`
- **US-05** As a multi-agent researcher, I control both controller ports with separate policies and rotate opponents from a pool, so that agents do not overfit one opponent. → `RL-09`
- **US-06** As an LLM-agent researcher, I step the game only when my model answers, and can switch to real-time to study latency. → `RL-13`
- **US-07** As an RL researcher, I get observations as NumPy or PyTorch arrays without copies. → `API-11`

**World-model researchers**

- **US-08** As a world-model researcher, I record frames, actions, per-layer images, sprite tables and named RAM variables per step into a standard dataset format, so that state supervision is free. → `WM-01`, `WM-02`
- **US-09** I roll out K different action sequences from one state, so that I get counterfactual training data. → `WM-03`
- **US-10** I automatically find clips where game state changes off-screen, so that I can test memory and persistence. → `WM-04`
- **US-11** I feed my model's action sequence into the real game and get per-step pixel and state error, so that evaluation needs no human or VLM judge. → `WM-05`

**Decompilation developers**

- **US-12** As a decomp developer, I run the original ROM in the core in lockstep with my C reimplementation and get the first frame and address where RAM differs. → `DEC-01`, `DEC-02`

**TAS authors**

- **US-13** As a TAS author, I import a BizHawk `.bk2` movie, play it back, and see exactly where it desyncs if it does. → `TAS-01`
- **US-14** I brute-force an RNG manipulation across all inputs over 20 frames using every CPU core. → `TAS-03`

**Achievement developers**

- **US-15** As an achievement developer, I run my set's conditions against a library of recorded playthroughs and see where each achievement fired. → `ACH-01`, `ACH-02`
- **US-16** I run agents that try to trigger achievements early or by unintended paths. → `ACH-03`

**Learners, testers and integrators**

- **US-17** As a tester, I play a ROM with keyboard or gamepad, record my session as a movie, and file it as a reproducible bug report. → `PLY-01`, `PLY-02`
- **US-18** As an integration author, I find the RAM address for lives or x-position by searching memory as I play. → `PLY-04`, `API-06`
- **US-19** As a learner, I read any chip module and see which document section each behaviour came from. → `PROV-03`
- **US-20** As a C or C++ project, I link the core through a stable C ABI. → `API-13`

## 3. Prioritization and milestones

The first public release (v0.1, milestone M5) is a `pip install` RL package for three games; everything marked P0 exists to reach it, and announcing at M5 tests demand before the larger investments.

**Priority levels**

| Priority | Meaning | Rule |
| --- | --- | --- |
| P0 | Required for the v0.1 public alpha (M5) | Blocks M5 |
| P1 | Required for v1.0 (M10) | Blocks v1.0 |
| P2 | Valuable after v1.0 | Scheduled by demand |
| P3 | Optional or exploratory | Only with a champion |

Priority was set by four tests, in order: does a primary community need it; does it unlock other features (foundations rank highest); is it cheap given the foundations; does it teach a core emulation concept at the right point in the build.

**Milestones and exit criteria**

| ID | Milestone | Exit criteria (all MUST hold) |
| --- | --- | --- |
| M0 | Provenance and skeleton | Repo public with `PROVENANCE.md`, bibliography and source-tag lint in CI; workspace crates and `System` trait stubs compile; license gate passes |
| M1 | 65C816 CPU | All opcodes pass the per-instruction test vectors in native and emulation mode, including cycle-by-cycle bus activity (`EMU-02`) |
| M2 | Bus, DMA and video | LoROM and HiROM maps, DMA/HDMA and the scanline PPU; listed homebrew and test ROMs reach golden frame hashes |
| M3 | Audio and base system | SPC700 passes its test vectors; S-DSP audio test ROMs pass; the internal list of 20 commercial games without coprocessors boots and plays to a checkpoint |
| M4 | Library foundations | `FND-01` to `FND-07` complete; determinism CI green across Linux, macOS and Windows |
| M5 | **v0.1 public alpha** | Python wheels on PyPI; batched pool; Gymnasium env; stable-retro loader; 3 integrations; published benchmark; announcement |
| M6 | Reference player | Playable binary with movie recording and RAM search |
| M7 | World-model features | `WM-01` to `WM-05` complete with an example dataset from homebrew or user-supplied ROMs |
| M8 | Accuracy hardening | Differential harness against oracles; public compatibility list; precise-mode decision recorded |
| M9 | Coprocessors | SA-1, Super FX (GSU) and DSP-1 games on the compatibility list play to checkpoints |
| M10 | **v1.0** | Decomp, TAS and achievement tooling; stable API and save-state format guarantees begin |
| M11 | Second console | Genesis feasibility spike using shared crates only (section 13) |

Milestones M6 to M9 can overlap once M5 ships; M1 to M5 are sequential. The roadmap below shows the order and the two gates.

&#91;embedded content: roadmap · 12 milestones, 1 demand gate\]

M0 to M5 run in sequence; after the M5 demand review, M6 to M9 can run in any order or overlap, and all four feed v1.0.

**Decision:** No feature work beyond M5 starts until the M5 announcement has run for at least 4 weeks and its signals (downloads, issues, integrations contributed, citations) are reviewed. If RL demand is weak, M7 (world models) and M9 (coprocessors) are re-ranked against each other; foundations are unaffected.

## 4. PROV — Provenance and licensing

Provenance is a published feature, not a background policy: every behaviour in the core MUST trace to a cited hardware document, test, or measurement, and no emulator's source code may be consulted, ever. All PROV items are P0 at M0 and serve every community, because a permissive license is only worth something if its provenance is credible. The lesson is concrete: RustySNES kept reference-emulator source in its workspace for many releases, then reworded about 39 citations to GPLv3 projects, and now carries an open, unresolved question about it ([RustySNES repository](https://github.com/doublegate/RustySNES), `docs/provenance-open-question.md`).

**PROV-01 Source policy, in the first commit.** `PROVENANCE.md` MUST list allowed and forbidden sources.

- Allowed: manufacturer datasheets (e.g. WDC W65C816S); community hardware documents such as [fullsnes](https://problemkaputt.de/fullsnes.htm) and anomie's documents; the [SNESdev wiki](https://snes.nesdev.org) (CC0); published die and decap research; patents; test ROMs and test vectors whose licenses permit use (PROV-09); the project's own hardware measurements; output (not code) of oracle emulators (PROV-05).
- Forbidden: source code of any emulator, under any license, including permissive ones; decompilations or disassemblies of emulator binaries; forum or issue posts that quote emulator code.
- **Decision:** leaked or confidential Nintendo documents are excluded. The SNESdev wiki links an archived copy of Nintendo's *SNES Development Manual* without stating how it was released ([wiki page](https://snes.nesdev.org/wiki/SNES_Development_Manual)); its status is uncertain, so it is not used. Confirmed by the project owner; revisit only with legal advice.
- Accidental exposure (e.g. a search result showing emulator code) MUST be logged in `provenance/exposures.md` with date, what was seen and which modules it could touch.

**PROV-02 Bibliography registry.** `provenance/bibliography.toml` MUST hold one entry per source: stable ID (e.g. `fullsnes`), title, URL, version or retrieval date, license, and the SHA-256 of an archived copy where redistribution is allowed.

**PROV-03 Source tags in code.** Every module in a chip crate and every function implementing hardware behaviour MUST carry at least one tag, e.g. `// @source: fullsnes#dma-and-hdma` or `// @source: adr-0012`.

- A CI lint MUST fail when a chip-crate file or public hardware function lacks a tag, or a tag names an unknown bibliography ID.
- Tests MUST also cite the source of their expected values.

**PROV-04 Ambiguity records.** When sources disagree or are silent, an ADR in `docs/adr/` MUST record what each source says, the deciding evidence (test ROM, measurement, oracle output), the decision and its confidence. These double as the learning journal.

**PROV-05 Oracle firewall.** Reference emulators (bsnes, Mesen, Snes9x and others) MAY be used only as black-box binaries.

- Only their outputs (frame hashes, RAM dumps, audio hashes, traces) are stored, as golden data with the oracle name and version.
- Their source MUST NOT exist in the repository, CI images, dev containers or any AI tool's context.
- A CI check SHOULD scan the tree for known emulator copyright headers and author names in code files and fail on a hit.

**PROV-06 Contributor attestation.** Every commit MUST carry a DCO `Signed-off-by` line. The PR template MUST include the statement "I did not consult emulator source code for this change". Outside contributions are not accepted until the provenance tooling is proven in use; after that, first-time contributors MUST acknowledge `PROVENANCE.md`.

**PROV-07 AI-assistance policy.** Use of coding assistants MUST be disclosed in a commit trailer (e.g. `Assisted-by: <tool>`).

- No emulator source may be placed in an assistant's context or workspace.
- AI-written chip code MUST carry PROV-03 tags and be reviewed line by line against the cited source; models may reproduce memorized emulator code, so code with no matching documentation is rewritten.

**PROV-08 Dependency license gate.** `cargo-deny` MUST allow only MIT, Apache-2.0, BSD-2/3-Clause, ISC, Zlib, Unicode and CC0 licenses, and deny GPL, LGPL, AGPL and non-commercial licenses. Python dependencies MUST follow the same list. Decision: this list applies to every library crate and the Python wheel; the reference player binary additionally allows MPL-2.0, OFL-1.1 and UFL-1.0 (fonts and platform helpers), and no dependency allowed only for the player may reach a library crate.

**PROV-09 Test asset licensing.** Every committed test ROM or vector file MUST have its license recorded in `tests/assets/MANIFEST.toml`. Assets with no stated license MUST NOT be committed; they are fetched at test time by URL and hash, pending permission. Commercial ROMs and coprocessor firmware MUST NOT be committed; tests needing them skip with a clear message.

**PROV-10 Release provenance report (P1, M10).** Each release MUST publish a generated report: bibliography, percentage of chip functions tagged, ADR count, oracles used with versions, and the exposure log.

**PROV-11 Remediation procedure.** If a violation is found: freeze the record (never edit or delete history or comments), list affected code, rewrite it from documents by someone not exposed to the source, and publish a notice. Rewording citations to hide a derivation is forbidden.

**PROV-12 Project license.** **Decision:** MIT OR Apache-2.0, the Rust ecosystem convention, for all crates and the Python package.

## 5. FND — Foundations

Ten foundation requirements carry most of the feature list: get cloning, determinism, the movie record, observers and memory views right, and RL, world models, decompilation, TAS and achievements become thin layers. They are designed from M0 and complete by M4; all serve every community.

**FND-01 Owned, cloneable machine (P0).** One emulated console MUST be one owned Rust value that implements `Clone + Send`.

- No mutable `static`, `thread_local!`, `OnceLock`-held state, `Rc` or `RefCell` in chip or core crates.
- Cartridge boards MUST be an `enum` (one variant per board or coprocessor family), not `Box<dyn Board>`, so `#[derive(Clone)]` works and dispatch can inline. This avoids RustySNES's non-cloneable `Box<dyn Board>` design.
- ROM and coprocessor firmware MUST be held in `Arc<[u8]>` and shared between clones; only mutable state is copied.
- Video and audio output buffers are outputs, not state: they are excluded from the clone and refilled by the next frame.
- Target: clone of a base-system machine in ≤ 50 µs on the reference machine (QA-05). Mutable state is roughly 258 KiB: 128 KiB WRAM, 64 KiB VRAM, 64 KiB audio RAM, 544 B OAM, 512 B CGRAM, plus registers and cartridge RAM.
- A compile-time test asserts `Snes: Clone + Send`.

**FND-02 Determinism contract (P0).** The same ROM, seed, configuration and input log MUST produce bit-identical state at every frame, on every supported OS and CPU architecture, in debug and release builds, at any thread count.

- Integer arithmetic only in the emulation path. The core outputs audio at the S-DSP's native rate as `i16` stereo; resampling to a host rate happens outside the core.
- Power-on contents of RAM and registers that hardware leaves undefined come from a seeded PRNG. **Decision:** SplitMix64 seeded from a `u64`; seed `0` is the documented default.
- No wall-clock time, OS randomness, hash-map iteration order or thread scheduling may affect state.
- CI MUST compare per-frame state hashes for every committed test ROM against golden values on Linux x86-64, macOS arm64 and Windows x86-64.

**FND-03 Canonical state serialization (P0).** A versioned, sectioned, little-endian binary format MUST serialize the complete machine state, including ROM hash, seed and configuration.

- It is used for save-state files, state hashing and transfer between processes; in-memory snapshots use `Clone` instead.
- A state hash is BLAKE3 of the canonical serialization. **Decision:** BLAKE3, chosen for speed and a permissive license.
- Before v1.0 the format MAY change with a version bump; loading an old version MUST fail with a clear error, never silently. From v1.0, older versions MUST load or fail clearly.
- Optional zstd compression applies only at the file layer.

**FND-04 Movie: the input log as the canonical record (P0).** A movie MUST contain: ROM SHA-1 and SHA-256, region, core version, accuracy profile, configuration, seed, IPL variant and coprocessor firmware hashes, start point (power-on, or an embedded serialized state), and per-frame input for each port.

- **Decision on what a frame is:** a frame ends at the first CPU instruction boundary at or after the PPU reaches the first line of vertical blank; an instruction or DMA transfer in progress completes first. Input set for frame *n* is held on the controller lines for the whole of frame *n*. Each frame records a `polled` flag (whether the game read the controller), which marks lag frames.
- Container: a zip file with `header.json`, a binary input stream (one `u16` button mask per port per frame, plus device-specific data for mouse or Super Scope), optional `markers.json`, and an optional text input log for diffing.
- Movies serve as TAS files, RL demonstrations, regression tests, bug reports and dataset recipes.

**FND-05 Observer hooks (P0).** The machine MUST be generic over an `Observer` type parameter whose default is a no-op that compiles away.

- Hook points: instruction executed (PC, opcode, CPU), CPU-bus read and write (24-bit address, value, master-clock time), DMA and HDMA transfers, scanline start, frame end, NMI and IRQ, APU-port access. SPC700-bus hooks SHOULD exist behind the same trait.
- Observers MUST NOT mutate machine state, so they cannot break determinism. State changes go through the API between steps.
- Because generics cannot cross the Python boundary, the core MUST also ship a concrete `WatchObserver` configurable at run time: address ranges, read or write, value conditions, and a bounded event queue.

**FND-06 Memory views (P0).** Each memory region MUST be readable as a slice and writable through an explicit API: WRAM, VRAM, CGRAM, OAM, audio RAM, cartridge SRAM or BW-RAM, read-only ROM, and a register snapshot.

- `peek_bus(addr)` MUST read the CPU bus without side effects. For I/O registers whose reads have side effects, it returns the last latched value and flags it as such.
- Writes between steps (`poke`) MUST be recorded in the movie when made during a recording, or recording MUST be refused, so movies stay replayable.

**FND-07 Console-agnostic `System` trait (P0).** Machine control MUST go through a trait shared by all future consoles: reset, power-cycle, the stepping calls (FND-08), input per port, memory-region enumeration, serialize and deserialize, and video and audio descriptors. SNES-specific types stay in SNES crates. See section 13.

**FND-08 Granular stepping (P0).** The API MUST offer `run_frame`, `run_frames(n)`, `run_scanlines(n)`, `step_instruction`, `run_master_cycles(n)` and `run_until(event)`. `run_until` takes a watch condition from FND-05 plus a limit in frames, and returns which condition stopped it. Every stop takes effect at the first instruction boundary at or after its target, so scanline and cycle stops may overshoot by up to one instruction or DMA transfer.

**FND-09 Robust inputs (P1).** Malformed ROMs, save states and movies MUST return errors, never panic or hang. ROM, state and movie loaders MUST be fuzzed with `cargo-fuzz` in CI.

**FND-10 `no_std` core (P1).** Chip crates and the core crate SHOULD build with `no_std + alloc`, to keep WASM and embedded targets open. Design for it from M0; CI checks it from M4.

## 6. EMU — Core emulation

The core starts with one `Fast` accuracy profile: CPU-cycle-exact bus timing with scanline-granular rendering. That is enough for most commercial games, several times cheaper than per-dot lockstep, and the natural order in which to learn the hardware. A dot-level `Precise` profile is a P2 decision at M8.

**EMU-01 Accuracy profiles (P0 for `Fast`, P2 for `Precise`).** Accuracy MUST be a type parameter on the machine, so both profiles share the public API, movie format and observers.

- `Fast` MUST provide: cycle-exact 65C816 timing, including per-region memory access speeds; exact DMA and HDMA timing; H/V counters and H/V IRQ at dot granularity; per-scanline exact HDMA. Rendering is per scanline, so a PPU register write takes visible effect from the next line.
- Known limitation of `Fast`: mid-scanline raster effects render a line late. Affected games are marked on the compatibility list (QA-04).
- A movie records its profile; replaying under the other profile is allowed but MUST be reported as "not guaranteed in sync".
- **Decision:** a catch-up scheduler. Each chip runs until it touches shared state or reaches a deadline, then the others catch up. RustySNES's per-master-clock lockstep measured 3.27 ms per headless frame (its `docs/benchmarks.md`); the `Fast` target is in QA-05.

**EMU-02 65C816 CPU (P0, M1).** All 256 opcodes in native and emulation mode, with every addressing mode and flag behaviour, MUST match the per-instruction vectors in the [SingleStepTests 65816 set](https://github.com/SingleStepTests): 20,000 tests per opcode (10,000 per mode), including the cycle-by-cycle bus record. Interrupts (NMI, IRQ, BRK, COP, ABORT not used on SNES), WAI and STP MUST be covered by separate tests. The repository is MIT-licensed, so the vectors can be committed with that license recorded in the test-asset manifest (PROV-09) (section 15).

**EMU-03 5A22 on-chip I/O (P0, M2).** Multiply and divide unit with its result latency, H/V timers, NMI and IRQ enables, auto-joypad read with its timing, WRIO/RDIO, FastROM selection (`$420D`).

**EMU-04 Memory map and cartridge (P0 LoROM/HiROM, P1 ExHiROM).**

- Header detection MUST score candidate header locations and accept an explicit override.
- A 512-byte copier header MUST be detected and stripped before hashing. **Decision:** the canonical ROM identity is SHA-1 and SHA-256 of the ROM without copier header. When matching stable-retro integrations, the loader computes both the stripped and the raw-file hash and accepts either.
- Cartridge SRAM is part of machine state and exportable as a `.srm` file.

**EMU-05 DMA and HDMA (P0, M2).** All 8 channels, every transfer mode, HDMA table setup at frame start and per-line transfers, HDMA pre-empting general DMA, and the bus-cycle cost of each.

**EMU-06 PPU (P0 core features, M2).**

- MUST: BG modes 0–7, sprites with per-line limits and the time-over and range-over flags, windows, colour math, mosaic, Mode 7 including EXTBG, overscan (224 or 239 lines), brightness and forced blank.
- P1: hi-res modes 5 and 6, pseudo-hi-res and interlace.
- Per-layer output (BG1–BG4, sprites, before compositing) MUST be producible on request for WM-02, with no cost when off.
- Render skip MUST skip pixel composition only. Sprite evaluation still runs every line, because its overflow flags are readable state (`$213E`).

**EMU-07 APU (P0, M3).**

- SPC700 MUST pass the MIT-licensed SPC700 vectors from [SingleStepTests/spc700](https://github.com/SingleStepTests); timers and the four communication ports with correct cross-clock behaviour.
- S-DSP: 8 voices, BRR decoding, Gaussian interpolation, ADSR and GAIN envelopes, noise, pitch modulation, echo with FIR filter. Lookup tables used by the hardware are factual data and MUST be cited to a document per PROV-03.
- Output: stereo `i16` at the S-DSP's native rate (about 32 kHz, derived from the APU clock), without resampling (FND-02).
- **Decision:** the CPU and APU oscillators are related by a fixed rational ratio of their nominal frequencies, so timing is deterministic.
- The 64-byte IPL boot ROM is Nintendo code and is not committed. Decision: a clean-room boot program written from the documented upload protocol is the default, and a user-supplied dump is an option for exact timing. The variant in use is part of machine state and is recorded in movies.

**EMU-08 Controllers and peripherals.** Standard pad (P0); Multitap for up to 4 players (P1, needed for RL-09 beyond two agents); mouse and Super Scope (P2).

**EMU-09 Regions (P0 NTSC, P1 PAL).** NTSC and PAL frame timing and line counts. Region comes from the header, with an override.

**EMU-10 Coprocessors (P1, M9), ordered by how many popular games need them.**

1. SA-1: a second 65C816 core plus its memory controller and arithmetic.
2. Super FX (GSU-1 and GSU-2).
3. DSP-1 family (NEC µPD77C25): its program and data ROM is chip firmware, supplied by the user, never committed (PROV-09).

- P2: CX4, S-DD1, OBC1, DSP-2/3/4, ST010/ST011.
- P3: SPC7110, ST018, S-RTC, Satellaview, Super Game Boy.

**EMU-11 Test-driven bring-up (P0).** Each chip's tests MUST land with or before its implementation. Where documents are silent, the project writes its own small test cartridges in 65816 or SPC700 assembly, with expected values from documents, hardware measurement or oracle output, each cited (PROV-03, PROV-04). These cartridges are permissively licensed and committed.

## 7. API — Library API and bindings

The Rust crate is the product; Python, C and WASM are thin bindings over it, so every feature is written once. Python is the scripting layer: there is no Lua.

**API-01 Rust crate and stability (P0).** The public entry point is a `Snes<A: Accuracy = Fast, O: Observer = NoopObserver>` type implementing the `System` trait (FND-07).

- Semantic versioning. Before v1.0 minor versions MAY break the API, with a changelog entry. From v1.0 breaking changes need a major version.
- **Decision:** the minimum supported Rust version is the stable release current at M0. It rises only in minor releases and is tested in CI.

**API-02 Loading (P0).** Load from bytes or a path with options: region override, accuracy profile, seed, header override, and a firmware map for coprocessors. Failures return typed errors (FND-09).

**API-03 Video output (P0).**

- Native output: an RGB555 buffer plus metadata (width 256 or 512, height 224, 239, 448 or 478, interlace field).
- Helpers convert to RGB24, RGBA8 and 8-bit grayscale.
- A normalized mode MUST always return 256×224 so RL code sees one shape. **Decision:** 512-wide frames are halved by averaging horizontal pixel pairs; 239-line frames lose their bottom 15 lines; interlaced frames return the current field. Each rule is documented and covered by tests.
- Layer buffers are available on request (EMU-06, WM-02).

**API-04 Audio output (P0).** Stereo `i16` samples at the native S-DSP rate; the per-frame sample count varies. A resampler lives in a separate helper crate, not the core. Audio generation can be switched off (RL-04).

**API-05 Input (P0).** One `u16` per port in the controller's serial bit order: B, Y, Select, Start, Up, Down, Left, Right, A, X, L, R, with 4 unused bits. Input is set before a frame and held for that frame (FND-04). Opposite d-pad directions pressed together are allowed and recorded as given; a helper MAY filter them.

**API-06 Memory API and RAM search (P0).** Region access per FND-06, plus a RAM search engine shared by Python and the player.

- Search over snapshots: equal to, not equal, changed, unchanged, increased, decreased, by value, by delta.
- Value sizes of 1, 2, 3 or 4 bytes, little- or big-endian, signed or unsigned, BCD.
- Candidates narrow over successive filters, with undo.

**API-07 State API (P0).** `clone()`, `save_state() -> Vec<u8>`, `load_state(&[u8])`, `state_hash()`.

**API-08 Movie API (P0).** Start, stop and append to a recording; play back with desync detection (the first frame whose state hash differs from a recorded checkpoint); seek by replaying from the nearest stored state.

**API-09 Batched pool in Rust (P0).** `Pool<M>` owns N machines and steps them in parallel on a thread pool.

- One call takes N actions and writes all observations into one contiguous, caller-provided buffer (N × H × W × C, `u8`).
- Results MUST be identical for any thread count (FND-02).
- Per-machine auto-reset to a start state on episode end, without a separate call.
- Lesson: in-process batching is what EnvPool reports as \~20x over subprocess vector envs on 256 cores ([EnvPool](https://www.Github.com/sail-sg/envpool)), and stable-retro-turbo as \~10x on NES ([stable-retro-turbo](https://pypi.org/project/stable-retro-turbo/1.3.0/)).

**API-10 Python package (P0, M5).** Built with PyO3 and maturin as stable-ABI (`abi3`) wheels.

- **Decision:** minimum Python 3.14, via `abi3-py314`. It balances maturity against support life: 3.14 gets security fixes until about October 2030 (PEP 745). It also clears PyO3's limited-API minimum of 3.11 for the buffer protocol that zero-copy output (API-11) needs ([PyO3 building and distribution](https://pyo3.rs/main/building-and-distribution)).
- Wheels: Linux x86-64 and aarch64 (manylinux), macOS arm64, Windows x86-64. P2: free-threaded CPython builds, since `abi3` wheels cannot load on free-threaded interpreters; on 3.14 these are version-specific builds, and PyO3's abi3t covers free-threaded Python from 3.15.
- Stepping MUST release the GIL.
- No compiler or system library is needed to install it; this avoids mGBA's build-from-source path in PyGBA ([PyGBA](https://pypi.org/project/pygba/0.2.0/)).

**API-11 Zero-copy arrays (P0 NumPy, P1 DLPack).** Observations are exposed through the buffer protocol as NumPy arrays without copying, and via DLPack for PyTorch and JAX. **Decision:** layout is NHWC `uint8`.

**API-12 WASM (P2).** The core and reference player compile to `wasm32-unknown-unknown` for browser demos (PLY-07).

**API-13 C ABI (P1, M10).** Opaque handles and a versioned header generated with `cbindgen`, covering load, step, input, memory, state, movie and a RAM-diff callback (DEC-01).

**API-14 Documentation (P0).** Every public item has rustdoc. The Python package has a typed stub file (`.pyi`) and runnable examples for each user story in section 2.

**API-15 Threading contract (P0).** `Snes` is `Send`; it needs no `Sync` restriction, because shared references only read. A Python environment object MUST NOT be stepped from two threads at once; the binding detects it and raises an error instead of locking.

## 8. RL — Reinforcement-learning environment

The RL layer is a Python package over the batched pool that speaks the Gymnasium and PettingZoo APIs, reads stable-retro integration files, and builds in the lessons of earlier SNES and Atari work. Primary community: RL researchers. All items are P0 at M5 unless noted.

**Lessons this section applies**

| Lesson | Source | Requirement |
| --- | --- | --- |
| One emulator per process cripples throughput | [openai/retro #64](https://github.com/openai/retro/issues/64); [EnvPool](https://www.Github.com/sail-sg/envpool) | RL-02 |
| SNES has 700+ useful button combinations versus Atari's 18 | [RLE](https://arxiv.org/pdf/1611.02205) | RL-05 |
| Delayed and misleading rewards (F-Zero laps, Mario score) | [RLE](https://arxiv.org/pdf/1611.02205) | RL-08 |
| Agents overfit to one opponent and forget | [RLE](https://arxiv.org/pdf/1611.02205); [FightLadder](https://sites.google.com/view/fightladder/home) | RL-09 |
| Protocol differences change results; pin them | [ALE overview](https://api.emergentmind.com/topics/arcade-learning-environment-ale) | RL-10 |
| RAM types, multi-frame updates and done conditions cause bugs | [stable-retro integration](https://stable-retro.farama.org/integration) | RL-07, RL-08 |
| Return to promising states, then explore | [Go-Explore](https://arxiv.org/abs/2004.12919) | RL-12 |
| Slow agents need paused stepping | [VideoGameBench](https://arxiv.org/pdf/2505.18134) | RL-13 |
| Learned action duration matches tuned frame-skip | [Action-duration paper](https://arxiv.org/pdf/2605.20911) | RL-06 |

**RL-01 Single environment.** A `gymnasium.Env` subclass with declared observation and action spaces, `reset(seed, options)`, `step(action)`, and `render_mode` of `rgb_array` or `human`.

**RL-02 Vector environment.** A Gymnasium `VectorEnv` backed by the Rust pool (API-09), not by Python subprocesses. It MUST support the `NEXT_STEP` (Gymnasium's default) and `SAME_STEP` autoreset modes, with `final_obs` in `info` for `SAME_STEP` ([Gymnasium autoreset](https://farama.org/Vector-Autoreset-Mode)). `DISABLED` is P1.

**RL-03 Observation types.** Selected per environment spec and combinable in a `Dict` space:

- `pixels`: normalized 256×224 RGB (API-03), optionally grayscale and resized (e.g. 84×84) inside Rust.
- `ram`: the 128 KiB WRAM as `uint8`.
- `vars`: named variables from the integration (RL-08).
- `structured`: layers, sprite table and tilemaps (WM-02), P1.

**RL-04 Speed controls in the core.** Frame-skip k repeats the action for k frames inside Rust. Frame aggregation is `last` or `max_last_two` (pixel-wise max of the final two frames). Rendering happens only on frames that are returned. Audio is off unless requested, and pixel rendering can be off entirely for RAM-only agents.

**RL-05 Action sets.** Each integration MAY define named action sets.

- `full`: every 12-button combination (4,096).
- `filtered`: no Start or Select, no opposite d-pad directions, matching stable-retro's default of disallowing Start.
- `minimal`: a per-game list of button masks.
- `multi_discrete`: d-pad (9 positions) × face and shoulder buttons.
- **Decision:** the default is `minimal` when the integration defines one, else `filtered`.

**RL-06 Variable action duration (P1).** An optional action space pairs each action with a hold duration from a configured set (e.g. 4, 8, 16, 32 frames).

**RL-07 stable-retro compatibility.** The loader MUST read stable-retro integration folders unchanged: `data.json` (variables with address and type string), `scenario.json` (reward, done, actions), `metadata.json` (default state) and `rom.sha`.

- **Decision:** stable-retro `.state` files are savestates of the Snes9x core it bundles, so they cannot load into this core. Ported integrations MUST recreate each start state as a movie from power-on, recorded in the reference player (PLY-02), plus a cached serialized state. The movie keeps start states portable across core versions.
- **Decision:** stable-retro Lua scenario scripts are not supported. Their logic is ported to Python reward functions (RL-08).
- The project SHOULD ship a script that converts an integration folder and reports which parts need manual porting.

**RL-08 Rewards and termination.**

- Variable types MUST accept stable-retro's type strings: endianness (`<`, `>`, `|`), format (`u` unsigned, `i` signed, `d` BCD, `n` low-nybble BCD) and byte count, e.g. `>u4`.
- Reward = Σ coefficient × change in variable, plus an optional per-step time reward or penalty. Done conditions use stable-retro's operators combined with `any` or `all`.
- A variable MAY set `settle_frames: k`: a change counts only after the value is stable for k frames. This addresses scores that update over several frames.
- Event conditions: done or reward when an address is written with a value, via `WatchObserver` (FND-05). It is evaluated during the step and reported at its end, so it cannot miss a one-frame event.
- Python reward functions are an escape hatch. They are slower, and specs that use them say so.

**RL-09 Multi-agent.** A PettingZoo Parallel API environment for two players, where `step` takes a dict of actions per agent and returns observation, reward, termination, truncation and info dicts ([PettingZoo Parallel API](https://pettingzoo.farama.org/api/parallel/)). Four players via Multitap is P1 (EMU-08).

- Opponent-pool helper: sample the port-2 policy per episode from a pool of checkpoints and built-in AI.
- Side swap: randomize or alternate which port the learner controls. FightLadder found agents trained on one side are weaker on the other.

**RL-10 Versioned environment specs.** An environment ID such as `SuperMarioWorld-Snes/YoshiIsland2-v1` MUST pin:

- ROM SHA-1, start state (movie hash) and seed policy;
- frame-skip and aggregation, sticky-action probability, action set;
- observation type and size;
- reward and done definitions, maximum episode frames, and whether life loss ends an episode;
- core version range and accuracy profile.

Any change to a pinned field MUST bump the version. The full spec is returned in `info` on reset and saved with results. **Decision:** follow ALE v5's defaults where they translate (frame-skip 4, sticky actions 0.25), with exceptions stated per spec ([ALE environments](https://ale.farama.org/environments/)).

**RL-11 Seeded stochasticity.** Sticky actions, random no-op starts, start-state sampling and power-on seed randomization all derive from the seed passed to `reset`. Same seed, same episode. Power-on seed randomization applies only to specs that start from power-on; named start states always replay with the seed they were recorded with, because a different power-on seed can desync their movie.

**RL-12 Exploration support.** `snapshot(env_index) -> handle` and `restore(env_index, handle)` on vector environments, using in-memory clones (FND-01). A cell-key helper builds archive keys from named variables, e.g. room ID plus x/16 and y/16, as in Go-Explore and the coordinate-based exploration reward in [Pokémon Red RL](https://www.github.com/PWhiddy/PokemonRedExperiments).

**RL-13 Agent timing modes.** Paused stepping is the default. A real-time wrapper (P1) advances emulation by wall-clock time while the agent computes, and reports action latency in frames.

**RL-14 Start-state library.** Each integration has named start states. Sampling is uniform, weighted, or a curriculum schedule. RLE attributed generalization failures to training on single stages.

**RL-15 Initial integrations at M5.** **Decision:** Super Mario World (platformer, misleading score), F-Zero (racing, delayed reward) and Street Fighter II (fighting, two-player). None needs a coprocessor; Super Mario Kart and Pilotwings wait for DSP-1 (M9). ROMs are user-supplied and matched by hash.

**RL-16 Baselines and benchmarks.** Publish steps per second for 1, 16 and 64 environments (QA-05), and a reference PPO script for each M5 integration with learning curves.

## 9. WM — World-model and dataset features

The core's value to world-model research is that it is a ground-truth simulator: exact internal state alongside every frame, cheap branching, and an evaluation harness that needs no human or model judge. All items are P1 at M7 and serve world-model researchers first, RL researchers second.

**Why this matters now.** A 2026 benchmark of 23 video world models found state scores of roughly 0.6–0.7 when an object changes off-screen and returns, and scaling Wan 2.1 from 1.3B to 14B parameters did not fix it ([Persistent State Core](https://arxiv.org/html/2606.20545)). StatePlay improved mechanics fidelity from 63.7% to 82.3% by training on HP, meter and timer values read from emulator memory ([StatePlay](https://arxiv.org/pdf/2607.26754)). WildWorld needed engine hooks, shader capture and heavy filtering to record state from a commercial game ([WildWorld](https://arxiv.org/html/2603.23497)). Exploration-driven data improved world-model quality over random-agent data ([CVPR 2025](https://openaccess.thecvf.com/content/CVPR2025/papers/Savov_Exploration-Driven_Generative_Interactive_Environments_CVPR_2025_paper.pdf)).

**WM-01 State-paired dataset writer.** A recorder that writes, per step: frame, action per port, reward, named variables, optional layer images, sprite table, tilemaps and audio.

- **Decision:** Parquet for tabular columns and WebDataset-style tar shards for images and audio, so both PyTorch and JAX loaders work. Shards target about 1 GB.
- Every dataset MUST embed its recipe: the environment spec (RL-10), the policy identifier, seeds, and the movie (FND-04) for each episode, so any episode can be regenerated exactly.
- Writing runs on a background thread and MUST NOT change emulation results.

**WM-02 Structured observations.** Available per step, at no cost when off:

- Per-layer images: BG1–BG4 and sprites before colour math and windows, plus each layer's scroll offsets. Mode 7 adds its matrix parameters.
- Sprite table: for each of the 128 OAM entries, x, y, size, tile, palette, priority, flip and on-screen flag, decoded from OAM.
- Tilemaps: each enabled BG layer as an integer grid of tile, palette, priority and flip.
- Audio: per-voice key-on events and pitch, from S-DSP state.

These give segmentation masks and object positions without manual annotation. Layer separation also addresses RLE's finding that scrolling backgrounds act as noise.

**WM-03 Counterfactual branching.** From one state, roll out K action sequences in parallel via clones (FND-01) and write them as sibling episodes sharing a parent ID. Sequences come from a policy, a random process, or a list.

**WM-04 Hidden-event mining.** Given named variables and the sprite table, detect intervals where a variable changes while its associated object is off-screen or occluded, and emit labelled clips. **Decision:** "associated object" is declared in the integration (e.g. variable `boss_hp` belongs to sprite slots 4–7). There is no automatic inference in v1.

**WM-05 World-model evaluation harness.** Feed a model's predicted actions (and optionally its predicted states) into the real core from the same start state. Report per-step pixel error (PSNR, SSIM), variable error and divergence horizon, from one Python call. Results are reproducible from the spec and seed.

**WM-06 Exploration-driven collection (P2).** A built-in collector that maximizes coverage of cell keys (RL-12) instead of acting randomly, as a baseline data source.

**WM-07 Legal guardrail (P1).** Dataset tools MUST NOT bundle ROMs. Generated datasets contain derived game imagery; the docs MUST state that redistribution of such datasets is the user's legal responsibility.

## 10. DEC — Decompilation and reimplementation support

Decompilation projects need the original game running in lockstep with their rewrite and a precise report of the first divergence; zelda3 does exactly this, comparing RAM after each frame, and borrows another project's PPU and DSP to render ([zelda3](https://github.com/snesrev/zelda3)). This section packages both needs. Community: decompilation and port developers; items are P1 at M10 unless noted, delivered through the C ABI (API-13) and Python.

**DEC-01 Lockstep comparison mode.** The core runs the original ROM from a movie while an external implementation runs the same inputs.

- At each comparison point, the external side supplies its memory image through a callback, and the core diffs it against its own.
- Comparison granularity: per frame (P1); per N instructions or at named PC addresses (P2).
- Regions compared and ignore masks (stack, scratch RAM, known-benign differences) are configured per project in a TOML file.

**DEC-02 Divergence report.** On the first mismatch the core MUST report the frame, region, address, both values, and the last 16 writes to that address with the writing PC and instruction. These come from a `WatchObserver` ring buffer, so it costs nothing when unused. Reports export as JSON.

**DEC-03 Shared input source.** The movie (FND-04) drives both sides. The core exports each frame's input to the external side, so the two never disagree about input timing.

**DEC-04 Standalone PPU and S-DSP components (P2).** The PPU and S-DSP crates MUST be usable without the rest of the machine: write registers and memory, then request a frame or audio samples. Native ports then get correct rendering and audio from a permissive, clean-room source instead of borrowing another emulator's code.

**DEC-05 Instruction trace (P1).** Stream PC, opcode, registers and cycle count to a file, filtered by address range or frame range, for the CPU, SA-1 and SPC700.

**DEC-06 Code/data log (P2).** Mark each ROM byte as executed, read as data, or used as a DMA source over a session, and export the map. This guides disassembly of unexplored code.

## 11. TAS and ACH — Tool-assisted speedruns and achievements

Both secondary communities are served by thin layers over the movie record, cloning, the pool and observers; neither gets its own subsystem. Items are P1 at M10 unless noted.

### TAS

**TAS-01 BK2 import with honest sync checking.** Read BizHawk `.bk2` movies: a zip with a key-value `Header` (including `SHA1`, `Platform`, `Core`), an `Input Log` whose first line is a log key naming the buttons and whose frame lines start with `|`, `SyncSettings` and an optional `CoreState` ([BK2 format](https://tasvideos.org/Bizhawk/BK2Format)).

- Buttons are mapped by name from the log key, never by position.
- Movies anchored to a `CoreState` (savestate-anchored) are rejected, because another core's state cannot load here.
- A movie made on another emulator MAY desync from tiny timing differences, and a BK2 carries no state hashes to check against. The importer MUST therefore offer a lag-frame alignment report, a start-offset adjustment of ±N frames, and a user-supplied end check (a RAM condition that should hold at the last frame). It MUST NOT claim sync it cannot verify.

**TAS-02 Snes9x `.smv` import (P2).** After studying the format from its public documentation and recording the findings in an ADR.

**TAS-03 Branch search API.** Given a start state, an input alphabet per frame, a depth in frames and an objective over named variables, search in parallel on the pool.

- States reached by different inputs are deduplicated by state hash (FND-03), which prunes the search sharply because many input sequences converge.
- Strategies: exhaustive within limits, beam search on the objective, and random restarts.
- Output: the best input sequences as movie fragments.

**TAS-04 Lag and polling data (P0, via FND-04).** Every movie frame records whether the controller was polled, and the API reports lag-frame counts per range.

**TAS-05 Greenzone seeking (P2).** In the reference player, cache states every N frames so seeking and branching edits are instant. Input-roll editing beyond insert, delete and overwrite of frames is out of scope.

### ACH

Achievement developers today reproduce tickets by hand with save states on the player's core ([RA ticket handling](https://docs.retroachievements.org/Handling-Tickets)), and the set-revision process documents votes but no automated testing ([RA revisions](https://docs.retroachievements.org/Achievement-Set-Revisions)).

**ACH-01 Headless condition evaluation.** Integrate [rcheevos](https://www.github.com/RetroAchievements/rcheevos) (MIT) to evaluate achievement and leaderboard definitions against core memory, offline, from a local definitions file.

- The core does no networking; rcheevos itself makes no network requests.
- Memory MUST be exposed in RetroAchievements' published SNES address layout. The design phase MUST confirm that layout from RetroAchievements documentation.

**ACH-02 Replay regression suite.** Run a set against a library of movies and output, per achievement, the frames where it triggered. Compare against an expected-triggers file and fail on differences.

**ACH-03 False-trigger fuzzing.** Drive random and RL agents, plus start states sampled from the movie library, and report any achievement that triggers outside its expected window, with a movie to reproduce it.

**ACH-04 Revision diff.** Run two versions of a set over the same suite and report every achievement whose trigger frames changed.

**ACH-05 Scope limit.** **Decision:** these are developer and QA tools only. Player-facing achievement support and hardcore mode are out of scope (section 15). Fixes found here still need confirming on the cores players use, because RA asks developers to reproduce on the reporter's core.

## 12. PLY — Reference player and authoring tools

The player is deliberately small: it exists to watch the core during development, play games for verification, and record movies, which makes it the authoring tool for start states, demonstrations, tests and bug reports. It grows in three steps, at M2, M5 and M6. Communities: testers, learners, integration authors, TAS authors.

**PLY-01 Development viewer (P0, M2).** A window that shows the framebuffer at integer scale with keyboard input, pause, frame advance and reset. Audio arrives at M3. It is needed as soon as the PPU draws anything, and is the main learning feedback loop.

**PLY-02 Recording and start-state authoring (P0, M5).** Record any session as a movie (FND-04). Mark the current frame as a named start state, saved as a movie from power-on plus a cached serialized state. Play movies back with a frame counter and desync indicator. This is how RL-07 and RL-14 start states are made, so it MUST exist by M5.

**PLY-03 Reference player (P1, M6).**

- Gamepad support and configurable key maps.
- Speed control from 0.25x to unlimited, plus fast-forward.
- Correct aspect ratio and integer scaling, with no shaders.
- Audio through a resampling helper with dynamic rate control, so audio does not crackle when frame pacing varies.
- **Implementation guidance:** `winit`, `wgpu`, `cpal` and `gilrs`, with `egui` for tool panels. All are permissively licensed, and `wgpu` keeps the WASM path open (PLY-07).

**PLY-04 RAM search and watch panel (P1, M6).** The API-06 search engine as a live panel, with filters, candidate list and undo. A watch list shows typed variables as they change. Exports straight to a `data.json` variable entry, so finding an address and writing the integration are one step.

**PLY-05 Inspection views (P2).** Toggle layers, show the decoded sprite table and tilemaps (WM-02), and show VRAM tiles and palettes. These are views of data the core already exposes, not a debugger. Breakpoints and stepping debuggers are out of scope; traces (DEC-05) go to notebooks.

**PLY-06 Bug-report bundle (P1).** One action exports a zip with the movie, configuration, core version, accuracy profile, a final state and a screenshot. Bug reports become reproducible by construction.

**PLY-07 Browser player (P2).** The player built for WASM (API-12), loading a user-chosen ROM locally, with no upload. Used for demos and quick verification.

## 13. MS — Second-console readiness

A second console MUST be addable by writing new chip crates only; the pool, environment, dataset, movie, bindings and player layers stay unchanged apart from additive changes. **Decision:** the planned second console is the Sega Genesis/Mega Drive, which has proven RL demand and whose two common libretro cores, Genesis Plus GX and PicoDrive, are both listed as non-commercial ([libretro licenses](https://docs.libretro.com/development/licenses/)). Items are P0 in design from M0; the spike is P3 at M11.

**MS-01 Layered crates.** Shared crates know nothing about the SNES. Decision: every crate carries the sfes prefix: sfes-\* for shared crates, sfes-snes-\* for SNES crates, and sfes-md-\* for the future Genesis crates. The diagram below shows the layers.

- Shared: `sfes-api` (System and Observer traits, memory-region descriptors, input-device descriptors, state container, hashing), `sfes-movie`, `sfes-pool`, `sfes-env` (specs, variables, rewards, action sets), `sfes-dataset`, `sfes-py`, `sfes-player`, plus the sfes facade, sfes-capi, sfes-cheevos and sfes-test-harness (full list in the technical design).
- SNES: `sfes-snes-cpu` (65C816, generic over a bus trait so the SA-1 reuses it), `sfes-snes-spc` (SPC700 and S-DSP), `sfes-snes-ppu`, `sfes-snes-cart` (boards and coprocessors), `sfes-snes-core` (machine assembly).
- Future Genesis: 68000, Z80, VDP, YM2612 FM and SN76489 PSG crates, plus a sfes-`md-core`.

&#91;embedded content: crate layers · dependencies point down only\]

Everything above `sfes-api` is written once; a second console implements the same contract with its own chip crates.

**MS-02 Enforced direction.** Shared crates MUST NOT depend on console crates, and chip crates MUST NOT depend on each other except through traits or a declared exception. Decision: the one exception is sfes-snes-cart depending on sfes-snes-cpu, because the SA-1 contains a 65C816. A CI check compares the Cargo dependency graph with the declared allow-list.

**MS-03 Console-neutral descriptors.**

- Input devices declare their button names in order. These names are the movie's log key and the action-set vocabulary.
- Video declares possible resolutions and the normalized size; audio declares its native rate.
- Memory regions declare names, sizes and their mapping into stable-retro and RetroAchievements address spaces.

**MS-04 System-tagged formats.** Movies, environment specs and dataset recipes MUST carry a `system` field from their first version, so formats never need a breaking change to add a console.

**MS-05 Genesis spike exit criteria (P3, M11).** A 68000 core passing its per-instruction vectors where licensed (PROV-09), driven through the shared pool and environment layers with no changes to their public APIs beyond additions. A written estimate of the remaining work.

## 14. QA — Testing, performance and quality gates

Correctness is shown by tests anyone can rerun, and speed by published benchmarks on a named machine; the performance targets below are provisional until measured at M3.

**QA-01 Test layers (P0).**

1. Unit tests per chip.
2. Per-instruction vectors for the 65C816 and SPC700 (EMU-02, EMU-07).
3. Permissively licensed test ROMs and the project's own test cartridges, with golden frame, audio and state hashes. Before M8 a golden is blessed after manual comparison with an oracle's output and marked unverified; the M8 harness upgrades it to oracle-confirmed or flags it.
4. Property tests: `load(save(m)) == m`; a clone and its original run identically; results do not depend on pool thread count.
5. The determinism matrix of FND-02.

**QA-02 Differential testing against oracles (P1, M8).** A harness runs the same movie on the core and on oracle binaries driven through their documented command-line or scripting interfaces, compares frame hashes and RAM at checkpoints, and reports the first divergence. Only outputs are stored (PROV-05). Divergences become ADR evidence (PROV-04) or bugs.

**QA-03 CI matrix (P0).** Linux x86-64, macOS arm64 and Windows x86-64; current stable Rust and the minimum supported version; `clippy` with warnings as errors; `cargo-deny` (PROV-08); the provenance lint (PROV-03).

**QA-04 Public compatibility list (P1).** One row per game: status (perfect, minor issues, major issues, broken, untested), accuracy profile, coprocessor, notes, and the checkpoint movie used as evidence.

- Commercial ROMs never enter CI. Maintainers run checkpoint movies locally against privately owned ROMs (git-ignored) and publish the results.
- Targets: 20 games at M3; at v1.0, at least 100 widely played titles with at least 90% rated perfect or minor issues. Which titles count is an open question (section 15).

**QA-05 Performance targets (P0 to measure, provisional).** Measured on a named reference machine recorded in the benchmark report.

| Metric | Target | Context |
| --- | --- | --- |
| Headless frame, `Fast`, render on, audio off | ≤ 1.0 ms per frame on one thread | RustySNES measured 3.27 ms (lockstep); RLE reported \~530 fps on an i7-5930K in 2016 ([RLE](https://arxiv.org/pdf/1611.02205)) |
| Clone of base-system machine | ≤ 50 µs | RustySNES serialized load: \~0.3 ms |
| Pool scaling | ≥ 80% of linear up to the physical core count | EnvPool-style batching |
| No-op observer overhead | Within noise (≤ 2%) | FND-05 |
| `WatchObserver` with 0–16 watches | ≤ 5% | FND-05 |

**QA-06 Benchmark suite (P0).** `criterion` benchmarks for every QA-05 metric, results published per release. Regression gates run on a fixed self-hosted machine, or compare relative to the previous commit on the same runner, since shared CI runners are noisy.

**QA-07 Code safety (P0).** `#![forbid(unsafe_code)]` in chip and core crates; `unsafe` is allowed only in bindings, the rcheevos FFI and the player, each with a `SAFETY` comment. The core MUST NOT panic on any input (FND-09); `clippy::unwrap_used` and `clippy::expect_used` are denied in chip and core crates.

**QA-08 Explainers (P1).** Each chip crate has a `docs/<chip>.md` that explains the hardware in plain language with citations, written as the chip is built. It serves learners and contributors, and it is the project's record of what was learned.

## 15. Non-goals, risks and open questions

The project wins by not building what other emulators already do well; the open questions below must be settled in the technical design before the milestone named beside them.

### Non-goals

- CRT and upscaling shaders, HD texture packs, Mode 7 upscaling.
- Netplay and online play.
- Mobile apps.
- Lua or any second scripting language; Python is the scripting layer.
- A breakpoint-and-step GUI debugger; traces and inspection views cover the need (DEC-05, PLY-05).
- Player-facing RetroAchievements support and hardcore mode.
- A libretro core before v1.0. A thin adapter MAY follow as P3.
- Satellaview, Super Game Boy and the rare coprocessors before demand appears (EMU-10).
- Distributing ROMs, coprocessor firmware or the IPL ROM in any form.

### Risks

| Risk | Impact | Mitigation |
| --- | --- | --- |
| RL demand is weaker than expected | Effort past M5 is wasted | Announce at M5 and review signals for 4 weeks before M6+ (section 3) |
| `Fast` profile breaks too many games | Credibility with players and decomp users | Public compatibility list (QA-04); `Precise` profile as a P2 path (EMU-01) |
| Clean-room work stalls on undocumented behaviour | Slower milestones | Own test cartridges (EMU-11), oracle outputs as evidence (QA-02), ADRs (PROV-04) |
| AI tools reproduce memorized emulator code | Provenance claim fails | PROV-07 review rule; code without a matching citation is rewritten |
| Ported stable-retro integrations need new start states | Slower adoption | Converter script and recorded-movie states (RL-07, PLY-02) |
| Dataset or ROM legal exposure | Project takedown | No ROMs or firmware shipped; hash matching; WM-07 notice |
| Performance targets missed | Weaker RL pitch | Targets measured and reset at M3; render, audio and frame-skip controls (RL-04) |
| One maintainer | Stalls | Explainers (QA-08) and ADRs lower the cost for contributors |

### Open questions

- [ ] **Name** — sfes (Super Ferrous Entertainment System), available on crates.io and PyPI. Remaining: reserve both names and apply the settled crate naming (MS-01: sfes-\* shared, sfes-snes-\* SNES) before M0.
- [ ] **Test-vector licenses.** The SingleStepTests 65816 repository is MIT-licensed, so its vectors can be committed with the license recorded (PROV-09). The SPC700 set (SingleStepTests/spc700) is MIT-licensed too. Resolved; record both licenses in the test-asset manifest. Before M1.
- [ ] **IPL boot ROM.** Options: (a) user supplies a dump, as with a BIOS; (b) a clean-room functional replacement implementing the documented upload protocol, possibly with timing differences; (c) skip boot via direct state setup. Resolved: (b) clean-room by default, with (a) a user dump as an exact-timing option (EMU-07).
- [x] **Official Nintendo manuals.** Resolved: exclusion confirmed by the project owner (PROV-01). Seek legal advice before any proposal to include them.
- [x] **ROM identity.** Resolved: unheadered hashes stay canonical, and stable-retro matching accepts either the stripped or the raw-file hash (EMU-04).
- [ ] **RetroAchievements memory layout and hashing** for SNES, from RA's documentation. Before M10.
- [x] **Python 3.14 minimum** (API-10). stable-retro supports Python 3.10 and newer after dropping 3.8 and 3.9 ([release notes](https://stable-retro.farama.org/release_notes)); resolved: users on 3.10 to 3.13 are left out, and stable-retro already supports 3.14. Revisit only if a dependency blocks 3.14.
- [ ] **Reference benchmark machine** (QA-05). Before M3.
- [ ] **Compatibility title list** for the v1.0 target (QA-04), e.g. by RetroAchievements player counts. Before M8.
- [ ] **`Precise` profile:** build or not, decided from the compatibility list. At M8.
- [ ] **Coprocessor firmware UX:** where users place firmware files and how they are verified by hash. Before M9.
- [ ] **Outside contributions before M5:** resolved: the codebase stays single-author at least until the provenance tooling (PROV-03, PROV-05, PROV-06) is proven in use.

## 16. Glossary and sources

### Glossary

| Term | Meaning in this spec |
| --- | --- |
| 5A22 | The SNES main CPU chip: a 65C816 core plus DMA, timers, multiply/divide and joypad logic |
| ADR | Architecture decision record; here also used for hardware-behaviour decisions (PROV-04) |
| `abi3` | CPython's stable ABI; one wheel serves many Python versions (API-10) |
| BRR | The S-DSP's compressed audio sample format |
| Catch-up scheduler | Each chip runs ahead until it touches shared state, then the others catch up (EMU-01) |
| Clean-room | Implemented from documentation and tests only, never from another implementation's code |
| CGRAM, OAM, VRAM, WRAM, ARAM | Palette RAM, sprite attribute RAM, video RAM, work RAM, audio RAM |
| Frame | Ends when the PPU reaches the first line of vertical blank (FND-04) |
| HDMA | Per-scanline DMA used for raster effects |
| IPL ROM | The SPC700's 64-byte boot program, written by Nintendo |
| Lag frame | A frame in which the game did not read the controller |
| Movie | ROM identity, configuration, seed, start point and per-frame inputs (FND-04) |
| Oracle | Another emulator used only as a black box whose outputs are compared (PROV-05) |
| Pool | A set of machines stepped together in parallel (API-09) |
| S-DSP | The SNES audio signal processor: 8 voices, envelopes, echo |
| SPC700 | The SNES audio CPU |
| Sticky actions | With probability p the previous action repeats, adding stochasticity (RL-11) |
| TAS | Tool-assisted speedrun: a movie built with savestates, slow motion and search |
| World model | A learned model that predicts future observations from past ones and actions |

### Sources

**Ecosystem and licensing**

- [libretro core licenses](https://docs.libretro.com/development/licenses/)
- [openai/retro issue #64: one emulator per process](https://github.com/openai/retro/issues/64)
- [RustySNES repository](https://github.com/doublegate/RustySNES) and its [architecture overview](https://github.com/doublegate/RustySNES/wiki/Architecture-Overview)
- [snesrev/zelda3](https://github.com/snesrev/zelda3)

**Hardware documentation and tests**

- [SNESdev wiki (CC0)](https://snes.nesdev.org) and its [SNES Development Manual page](https://snes.nesdev.org/wiki/SNES_Development_Manual)
- [fullsnes](https://problemkaputt.de/fullsnes.htm)
- [SingleStepTests](https://github.com/SingleStepTests) (65816 and ProcessorTests/spc700)

**RL and environments**

- [Playing SNES in the Retro Learning Environment](https://arxiv.org/pdf/1611.02205)
- [stable-retro: game integration](https://stable-retro.farama.org/integration) and [release notes](https://stable-retro.farama.org/release_notes)
- [stable-retro-turbo](https://pypi.org/project/stable-retro-turbo/1.3.0/)
- [EnvPool](https://www.Github.com/sail-sg/envpool)
- [ALE environments](https://ale.farama.org/environments/) and [ALE overview](https://api.emergentmind.com/topics/arcade-learning-environment-ale)
- [Gymnasium vector autoreset modes](https://farama.org/Vector-Autoreset-Mode)
- [PettingZoo Parallel API](https://pettingzoo.farama.org/api/parallel/)
- [Go-Explore: First return, then explore](https://arxiv.org/abs/2004.12919)
- [FightLadder](https://sites.google.com/view/fightladder/home)
- [For How Long Should We Be Punching?](https://arxiv.org/pdf/2605.20911)
- [VideoGameBench](https://arxiv.org/pdf/2505.18134)
- [PokemonRedExperiments](https://www.github.com/PWhiddy/PokemonRedExperiments)
- [PyGBA](https://pypi.org/project/pygba/0.2.0/)

**World models**

- [Current World Models Lack a Persistent State Core](https://arxiv.org/html/2606.20545)
- [StatePlay](https://arxiv.org/pdf/2607.26754)
- [WildWorld](https://arxiv.org/html/2603.23497)
- [Exploration-Driven Generative Interactive Environments (CVPR 2025)](https://openaccess.thecvf.com/content/CVPR2025/papers/Savov_Exploration-Driven_Generative_Interactive_Environments_CVPR_2025_paper.pdf)

**TAS and achievements**

- [BizHawk BK2 format](https://tasvideos.org/Bizhawk/BK2Format)
- [rcheevos](https://www.github.com/RetroAchievements/rcheevos)
- [RetroAchievements: handling tickets](https://docs.retroachievements.org/Handling-Tickets) and [set revisions](https://docs.retroachievements.org/Achievement-Set-Revisions)

**Tooling and standards**

- [PyO3: building and distribution](https://pyo3.rs/main/building-and-distribution)
- [RFC 2119](https://www.rfc-editor.org/rfc/rfc2119)

* [PEP 745: Python 3.14 release schedule](https://peps.python.org/pep-0745/)
* [PyO3 features reference](https://pyo3.rs/main/features)
