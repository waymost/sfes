# AGENTS.md

Working rules for every contributor to sfes, human or coding assistant. Read [`PROVENANCE.md`](PROVENANCE.md) first: it is binding and overrides anything here. The [feature specification](docs/design/feature-specification.md) defines the requirement IDs (`PROV-03`, `EMU-02` …) and the [technical design](docs/design/technical-design.md) says how they are built.

## 1. Provenance rules for assistants

- Work only inside this checkout. No emulator source code exists on any path you can read, and none may be fetched, pasted, quoted or recalled into a prompt or answer.
- Implement hardware behaviour **from the cited document section or the test vectors**, never from "how emulator X does it". If you notice you are recalling another emulator's code, stop, say so, and log it in [`provenance/exposures.md`](provenance/exposures.md).
- Tag every chip-crate module, every function implementing hardware behaviour, and every test with its source:

  ```rust
  // @source: fullsnes#dma-and-hdma
  // @source: adr-0012
  ```

  The id is a key in [`provenance/bibliography.toml`](provenance/bibliography.toml) or an ADR number in `docs/adr/`. The anchor after `#` names the section.
- Code with no matching citation is rewritten. Where sources disagree or are silent, write an ADR (context, sources, decision, consequences, confidence) instead of guessing.
- Oracle emulators are black boxes: only their outputs, labelled with name and version, may enter the repo.
- Never commit ROMs, firmware, IPL dumps or test assets without a recorded license (`tests/assets/MANIFEST.toml`).

## 2. Commits, branches and pull requests

- GitHub flow: short-lived branches from `main` named `area/short-description` (areas: `cpu ppu apu cart core env py player prov infra docs`). Every change arrives through a PR with green CI; PRs merge by rebase.
- [Conventional Commits](https://www.conventionalcommits.org/): `feat(ppu): …`, `fix(cpu): …`, `docs(adr): …`, `ci: …`, `chore: …`.
- Every commit is signed off (`git commit -s`, DCO). When an assistant contributed, add a trailer naming it, e.g. `Assisted-by: Claude Code (claude-opus-5-5)`.
- An ADR in `docs/adr/NNNN-title.md` is required for hardware ambiguities, new dependencies in chip or core crates, and any format change.
- An ADR that changes or refines the [technical design](docs/design/technical-design.md) or the [feature specification](docs/design/feature-specification.md) updates that document in the same PR, with a citation such as "(ADR 0004)" at the changed text.
- Definition of done: tests added or updated (golden changes explained); public items documented; `@source:` tags present; all checks below pass; the AI-review checklist is complete for AI-assisted changes.

## 3. Checks to run before pushing

```bash
cargo +nightly fmt -- --config group_imports=StdExternalCrate,imports_granularity=Crate
cargo clippy --workspace --all-targets -- -D warnings
cargo nextest run --workspace
cargo deny check
cargo xtask provenance lint
cargo xtask provenance scan
cargo xtask provenance assets
cargo xtask deps
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
- **Derives:** the std traits alphabetically, then other crates' traits path-qualified and alphabetical:

  ```rust
  #[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
  ```

- **Type a `collect` with a turbofish**, not on the `let`:

  ```rust
  let names = watches.iter().map(Watch::name).collect::<Vec<_>>();
  ```

- **Convert with `Type::from(value)` or `.map(Type::from)`**, not `.into()`. `Into::into` is listed in `clippy.toml`'s `disallowed-methods`.
- **A conversion (`From` / `TryFrom`) starts by destructuring its input without `..`**, so a new field fails to compile until it is handled:

  ```rust
  impl From<RawHeader> for Header {
      fn from(raw: RawHeader) -> Self {
          let RawHeader { title, map_mode, checksum, complement } = raw;
          …
      }
  }
  ```

- **Parameters and `let` bindings are full words**: `address`, `value`, `machine`, `index`, not `addr`, `v`, `m`, `i`. Generic type parameters (`A: Accuracy`, `O: Observer`) are the exception.
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
