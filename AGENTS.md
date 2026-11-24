# AGENTS.md

Working rules for every contributor to sfes, human or coding assistant. Read [`PROVENANCE.md`](PROVENANCE.md) first: it is binding and overrides anything here. The [feature specification](docs/design/feature-specification.md) defines the requirement IDs (`PROV-03`, `EMU-02` …) and the [technical design](docs/design/technical-design.md) says how they are built.

## 1. Provenance rules for assistants

- Work only inside this checkout. No emulator source code exists on any path you can read, and none may be fetched, pasted, quoted or recalled into a prompt or answer.
- Implement hardware behaviour **from the cited document section or the test vectors**, never from "how emulator X does it". If you notice you are recalling another emulator's code, stop, say so, and log it in [`provenance/exposures.md`](provenance/exposures.md).
- Tag every chip-crate module, every function in a chip crate (helpers and private functions included; only methods of standard-library trait impls are exempt), and every test in a chip crate or `sfes-test-harness` with its source. Tests of tooling and plumbing whose expected values come from no document (`xtask`, `sfes-api`) need no tag:

  ```rust
  // @source: fullsnes#snesdmatransfers
  // @source: adr-0012
  ```

  The id is a key in [`provenance/bibliography.toml`](provenance/bibliography.toml) or an ADR number in `docs/adr/`. The anchor after `#` names the section; each entry's `notes` give its anchor forms, and its `anchor` pattern is what `provenance lint` checks (ADR 0038). Cite the most precise place the document offers: for a 65C816 cycle sequence that is the row of the datasheet's instruction-operation table (`wdc-w65c816s#table-5-7-1a`), not the table or the chapter.
- An item's `@source:` lines list documents first, then ADRs in ascending order, each tag once; the order among documents is free. `cargo xtask provenance lint` checks it.
- An item's doc comment comes first, then its `@source:` lines, then its attributes. A module's tags come before its `//!` doc.
- Own test cartridges (`tests/carts/`, ADR 0013) are tagged the same way with `; @source:`: each source file in its leading comments, and each `.proc` and `.macro` in the comment lines directly above it. An expected value the cartridge checks itself cites its source beside it.
- A test driven by test vectors cites the vector set (`sst-65816`, or `sst-65816#a9.n` for one file). Vectors are evidence, not a specification: the function under test still cites the document it was written from.
- Read the cited section before writing the code. The source documents that cannot be redistributed are kept outside the repo by the project owner; if you cannot read the section, do not write the code.
- Code with no matching citation is rewritten. Where sources disagree or are silent, write an ADR (context, sources, decision, consequences, confidence) instead of guessing.
- Oracle emulators are black boxes: only their outputs, labelled with name and version, may enter the repo.
- Never commit ROMs, firmware, IPL dumps or test assets without a recorded license (`tests/assets/MANIFEST.toml`).

## 2. Commits, branches and pull requests

- GitHub flow: short-lived branches from `main` named `area/short-description` (areas: `cpu ppu apu cart core env py player prov infra docs`). Every change arrives through a PR with green CI; PRs merge by rebase.
- [Conventional Commits](https://www.conventionalcommits.org/): `feat(ppu): …`, `fix(cpu): …`, `docs(adr): …`, `ci: …`, `chore: …`.
- Every commit is signed off (`git commit -s`, DCO). When an assistant contributed, add a trailer naming it, e.g. `Assisted-by: Claude Code (claude-opus-5-5)`.
- An ADR in `docs/adr/NNNN-title.md` is required for hardware ambiguities, new dependencies in chip or core crates, and any format change.
- A doc comment or explainer that says a behaviour waits for another issue ("until #N", "not drawn yet") is updated in the PR that lands #N.
- An accepted ADR is corrected in place only for a slip or a detail that leaves its decision's intent unchanged, with a dated line under its header saying what changed: `- **Amended:** 2026-11-21: …`. A changed decision is a new ADR that supersedes the old one.
- Where a cited document is wrong (a slip, a misnumbered bit), the ADR that relies on that section says so, and the entry's `notes` in `provenance/bibliography.toml` list it under "known slips".
- An ADR that changes or refines the [technical design](docs/design/technical-design.md) or the [feature specification](docs/design/feature-specification.md) updates that document in the same PR, with a citation such as "(ADR 0004)" at the changed text.
- Definition of done: tests added or updated (golden changes explained); public items documented; `@source:` tags present; all checks below pass; the AI-review checklist is complete for AI-assisted changes.

## 3. Checks to run before pushing

```bash
cargo +nightly fmt -- --config group_imports=StdExternalCrate,imports_granularity=Crate
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo deny check
cargo xtask provenance lint   # --documents <dir> also finds each anchor in the reading copies
cargo xtask provenance scan
cargo xtask provenance assets
cargo xtask deps
cargo xtask carts check     # needs cc65 2.19 (ADR 0013); CI runs it on Linux
```

CI runs all of these, plus `cargo xtask provenance commits` on pull requests and `no_std` builds for `thumbv7em-none-eabihf` and `wasm32-unknown-unknown`.

## 4. Rust conventions

The [technical design](docs/design/technical-design.md)'s code sketches show shape only; these conventions win where they differ.

- **Edition 2024**, with `style_edition = "2024"` in `rustfmt.toml`.
- **A nested `if let` / `if` is a let-chain.**

  ```rust
  if let Some(watch) = watches.first() && watch.contains(address) { … }
  ```

  Enforced by `clippy::collapsible_if`.
- **A module with submodules is `foo/mod.rs`, never `foo.rs` beside `foo/`.** When a module gains its first submodule, move it with `git mv foo.rs foo/mod.rs` so history follows. Enforced by `clippy::self_named_module_files`.
- **Imports: three groups** (`std`/`core`/`alloc`, then external crates, then one nested `use crate::{…}`), **one `use` per crate.** Use `crate::` paths, never `super::`, except `use super::*;` in a test module. To re-apply the grouping:

  ```bash
  cargo +nightly fmt -- --config group_imports=StdExternalCrate,imports_granularity=Crate
  ```

  CI checks formatting with this command.
- **No glob imports of a module's items** (`use foo::*`), outside `use super::*` in tests. Enforced by `clippy::wildcard_imports` and `clippy::enum_glob_use`.
- **Name enum variants by path** (`Background::One`, `Self::One`), not through `use Enum::{One, Two}`. A bare variant name in a pattern turns into a binding that matches anything once the import goes.
- **Derives:** the std traits alphabetically, then other crates' traits path-qualified and alphabetical:

  ```rust
  #[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
  ```

- **Type a `collect` with a turbofish**, not on the `let`:

  ```rust
  let names = watches.iter().map(Watch::name).collect::<Vec<_>>();
  ```

- **Convert with `Type::from(value)` or `.map(Type::from)`**, not `.into()`. `Into::into` is listed in `clippy.toml`'s `disallowed-methods`. A widening integer conversion is `u32::from(value)`, not `as` (`clippy::cast_lossless`).
- **A narrowing or sign-changing integer conversion is `as`**, with an `#[expect(clippy::cast_possible_truncation, reason = "…")]` (or the matching cast lint) on the smallest item around it saying why the value fits. Use `T::try_from` only where failure is a real case the code handles; never `T::try_from(value).unwrap_or_default()` (or `.ok()`, `let Ok … else`) for a value known to fit, which turns a bug into a silent 0. `#[allow]` is denied and every `#[expect]` gives a reason (`clippy::allow_attributes`, `clippy::allow_attributes_without_reason`).

  ```rust
  #[expect(clippy::cast_possible_truncation, reason = "a dot is below 341")]
  const fn dot_at(offset: u64) -> u16 { (offset / 4) as u16 }
  ```

- **A register the code reads per pixel, per access or per cycle is decoded once, on write**, into typed state (enums, bitflags, newtypes), as `VramControl` is. One read rarely can stay the byte the CPU wrote.
- **Registers whose bits index the same things share one type.** The PPU's TM, TS, TMW, TSW and CGADSUB bits 4–0 are all `Layers`, whose bit positions are the `LayerKind` values; a shift by a layer number is not written out at each use.
- **A value from a fixed set is an enum or a newtype index**, not a raw `u8` or `usize`: a register, a port, a DMA channel, an access speed. When the hardware gives each variant a number, the enum is `#[repr(..)]` with explicit discriminants and is read with `as`, not a `match` that returns the same numbers. An array indexed by such a type cannot be indexed out of range.

  ```rust
  #[repr(u8)]
  pub enum Speed { Fast = 6, Slow = 8, XSlow = 12 }
  ```

- **Raw addresses are decoded once, where they enter a unit** (the bus's `decode.rs`), into typed registers. Units take the typed register and never match on addresses.
- **State made of independent flags is a `bitflags` type**, not separate `bool`s or masks spread through the code. A register kept as the byte the CPU wrote can stay a `u8`.
- **Time is `Cycles`, and its arithmetic uses its operators**: `now + count`, `later - earlier`, and `Cycles::after` / `Cycles::since` in a `const fn`. A count added to a `Cycles`, or the difference of two, is never written through `.0` (`Cycles(now.0 + count)`, `later.0 - earlier.0`), tests included; `.0` is read only where the raw count is the point, such as a modulo or a bound passed as `u64`.
- **A hardware constant is defined once**, in the crate that owns it, and other crates import it (`sfes_snes_cart::WRAM_SIZE`). A value that follows from other constants is computed from them, not written as a literal:

  ```rust
  const AUTO_READ_FIRST: u64 = FIRST_VBLANK_LINE as u64 * LineKind::Normal.length() + 298;
  ```

- **Test fixtures shared between crates live in the crate that owns the data**, behind a `test-support` feature (`sfes_snes_cart::test_support`), and the other crates enable it in `[dev-dependencies]`. They are never copied. The feature, like every feature, changes no emulation result.
- **Use the standard combinator when there is one**: `Option::or`, `min`, `max`, `matches!` with ranges, `Option::map`, rather than the same thing written out. Pedantic clippy catches most cases.
- **A conversion (`From` / `TryFrom`) starts by destructuring its input without `..`**, so a new field fails to compile until it is handled:

  ```rust
  impl From<RawHeader> for Header {
      fn from(raw: RawHeader) -> Self {
          let RawHeader { title, map_mode, checksum, complement } = raw;
          …
      }
  }
  ```

- **Parameters, closure parameters and `let` bindings are full words**: `address`, `value`, `machine`, `index`, not `addr`, `v`, `m`, `i`, and `|sample| sample.color`, not `|s| s.color`. Generic type parameters (`A: Accuracy`, `O: Observer`) and the hardware's own coordinate names (`x`, `y`) are the exceptions.
- **Hardware registers are named in full words, and their doc comment gives the address and the documents' mnemonic**, so a search by either still finds them:

  ```rust
  /// $2100 INIDISP: forced blank and screen brightness.
  display_control: u8,
  /// $420C HDMAEN
  pub fn write_hdma_enable(&mut self, value: u8) { … }
  ```

  The identifier says what the register does (`display_control`, `write_dma_enable`), never the mnemonic alone (`inidisp`, `mdmaen`). The doc comment opens with `$XXXX MNEMONIC`, then a colon and what it does. A range is written `$2140–$2143 APUIO0–APUIO3`. A mixed-case per-channel mnemonic goes in backticks (`` `DMAPx` ``), as `clippy::doc_markdown` requires.
- **A doc comment that describes several steps or cases is a bullet list**, not a run-on paragraph.
- **A struct literal holds simple initializers.** Bind a value that needs a block (a `match`, an `if`, a closure, a multi-line chain) first and pass it by name:

  ```rust
  let region = match header.region_code { … };
  Meta { seed, region, frame: 0 }
  ```

## 5. Project rules (from the [technical design](docs/design/technical-design.md), section 12)

- No panics in library code: `unwrap`, `expect`, `panic!`, `todo!` and `dbg!` are denied outside tests. Return `thiserror` enums; binaries (`sfes-player`, `xtask`) use `anyhow`.
- Hardware wrap-around is explicit (`wrapping_add`, `wrapping_sub`, …); tests run with overflow checks on.
- Chip and core crates: no floating point, no `HashMap`/`HashSet` in state, no wall clock or OS randomness, `#![no_std]` + `alloc`, `#![forbid(unsafe_code)]`. Dependencies are limited to `serde`, `postcard`, `bitflags`, `thiserror` and `blake3`; anything else needs an ADR.
- `unsafe` appears only in the FFI boundaries of `sfes-py`, `sfes-capi`, `sfes-cheevos` and the GPU/audio boundary of `sfes-player`, each block with a `// SAFETY:` comment.
- Features are additive and never change emulation results; accuracy is a type parameter.
- Dependencies point one way: apps → services → `sfes-api` ← chip crates. The only declared exception is `sfes-snes-cart` → `sfes-snes-cpu` (SA-1). `cargo xtask deps` enforces the graph.
- A core-crate unit with CPU-visible registers implements `bus::Registers` and is read only through the bus's `read_target`; [`docs/chips/core.md`](docs/chips/core.md) has the checklist for adding a register.
