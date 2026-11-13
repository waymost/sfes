# 0002. Sectioned postcard state format, hashed with BLAKE3

- **Status:** accepted
- **Date:** 2026-11-12
- **Confidence:** high

## Context

FND-03 needs a versioned, sectioned, little-endian binary format that serializes the complete machine state, including ROM hash, seed and configuration. It is used for save-state files, state hashing and transfer between processes; in-memory snapshots use `Clone` instead.

The format must be:

- canonical, so equal states hash equally on every platform (FND-02);
- usable from `no_std` + `alloc` chip crates (FND-10);
- evolvable per chip without breaking everything at once;
- able to fail with clear errors and never panic (FND-09).

## Sources

- FSD FND-02, FND-03, FND-09, FND-10.
- TDD section 6: the envelope layout, hashing and loading.
- [postcard](https://docs.rs/crate/postcard) documentation: its wire format is documented and stable since 1.0, and the crate supports `no_std`.
- [BLAKE3](https://github.com/BLAKE3-team/BLAKE3) specification and crate (CC0-1.0 OR Apache-2.0).

## Decision

- **Envelope:** the magic `SFES`, a format `u16`, system id, core version, ROM SHA-256, accuracy name and seed. Then a sequence of sections, each `tag [u8; 4] | version u16 | length u32 | postcard bytes`, one per top-level state field (`CPU0 BUS0 PPU0 APU0 CART META`).
- **Encoding:** chip structs derive `serde::Serialize`/`Deserialize` and are encoded with postcard. serde's derive keeps field order fixed, so encodings are canonical. Memories larger than 32 elements are written as byte strings through one helper module in `sfes-api`.
- **Versioning:** each section has its own version. Older sections are loaded through a chain of tested migration functions. Before v1.0, a migration MAY be dropped and replaced by `StateError::Unsupported { section, found, supported }`.
- **Errors:** unknown or missing sections, trailing bytes, a ROM mismatch and an accuracy mismatch are distinct `StateError` variants.
- **Loading** applies state onto an existing machine, which keeps its immutable context (ROM, firmware, memory map). That context is checked against the envelope.
- **Hash:** `state_hash()` is BLAKE3 over the ROM SHA-256 followed by the section bytes, without the rest of the header. Identical state therefore hashes identically across core versions that share section versions.
- **`no_std`:** `blake3` is used with default features off. CI builds `sfes-api` with it for `thumbv7em-none-eabihf` and `wasm32-unknown-unknown` (confirmed in M0), so no std-only fallback is needed.

## Consequences

- Hashes and golden files are portable across OSes and architectures. The 32-bit CI job at M4 guards against platform-sized integers sneaking into state.
- Adding a field to a chip struct changes its section encoding, so it needs a section version bump and, after v1.0, a migration.
- postcard is not self-describing. Section versions, not field names, carry compatibility.
- Optional zstd compression applies only at the file layer, never inside the hashed bytes.
