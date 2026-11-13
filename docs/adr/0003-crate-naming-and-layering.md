# 0003. Crate naming and layering

- **Status:** accepted
- **Date:** 2026-11-12
- **Confidence:** high

## Context

sfes must admit a second console (the Sega Genesis/Mega Drive) by adding chip crates only (FSD section 13). The pool, environment, dataset, movie, bindings and player layers must work unchanged apart from additive changes. That holds only if shared crates never learn about the SNES, and chip crates stay independent of each other.

## Sources

- FSD MS-01, MS-02, MS-04; PROV-08 (player-only licenses).
- TDD section 2: the crate table and dependency direction; section 12: the dependency policy for chip and core crates.

## Decision

- **Names:** every crate has the `sfes` prefix. Shared crates are `sfes-*` (`sfes-api`, `sfes-movie`, `sfes-pool`, `sfes-env`, `sfes-dataset`, `sfes-cheevos`, `sfes-py`, `sfes-capi`, `sfes-player`, `sfes-test-harness`, and the `sfes` facade). SNES crates are `sfes-snes-*` (`cpu`, `spc`, `ppu`, `cart`, `core`). Future Genesis crates are `sfes-md-*`.
- **Direction:** apps → services → `sfes-api` ← chip crates. Shared crates never depend on console crates. Chip crates depend only on `sfes-api`, except for one declared exception: `sfes-snes-cart` → `sfes-snes-cpu`, because the SA-1 contains a 65C816 and reuses the same `Cpu65816` through the `Bus65816` trait. `sfes-snes-core` assembles the chips.
- **External dependencies:** chip and core crates (including `sfes-api`) may use only `serde`, `postcard`, `bitflags`, `thiserror` and `blake3`. Anything else needs an ADR.
- **Enforcement:** `xtask/layers.toml` lists each workspace crate's allowed workspace dependencies and the external-crate limit. `cargo xtask deps` checks the `cargo metadata` graph against it in CI. It also checks that no dependency allowed only for the player (MPL-2.0, OFL-1.1, UFL-1.0) is reachable from any other crate, because cargo-deny exceptions are keyed by dependency, not by consumer.
- **Formats:** movies, environment specs and dataset recipes carry a `system` field from their first version (MS-04).

## Consequences

- A new crate, or a new edge between crates, fails CI until it is declared in `xtask/layers.toml`. That makes every layering change an explicit, reviewable diff.
- Code that would be convenient to share between SNES chips goes into `sfes-api`, behind a console-neutral trait, or stays duplicated. A second exception needs a new ADR.
- Stub crates declare dependencies only once they use them, because cargo warns on unused dependencies. The allow-list, not the manifests, records the intended graph.
