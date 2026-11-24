---
name: sfes-test-cart
description: Add or change an sfes own test cartridge (tests/carts/, ADR 0013) and its golden-frame file (tests/assets/golden/, ADR 0015). Use when writing a 65816 test program for a chip or core feature, registering a built .sfc in the asset manifest, creating or updating a golden, or reviewing golden changes.
---

# Own test cartridges and goldens

Use the script. Do not read the carts README, ADR 0013 or ADR 0015 to find the steps; they are below.

```bash
S=.claude/skills/sfes-test-cart/scripts/cart.py
$S new <name> <lorom|hirom> ["TITLE"]    # skeleton: cart.toml + <name>.s
$S register <name> "<what it does.>"     # build, then add or refresh the MANIFEST.toml entry
$S golden <name> [--frame N]             # create the golden file and fill in its hash
```

## When a cartridge is the right test

- Use one when an instruction-level vector set (`sst-65816`) or an approved third-party ROM cannot reach the behaviour: bus timing, register side effects, interrupt timing, DMA, PPU output. Prefer a vector or third-party ROM where one exists.
- Names are lowercase letters, digits and hyphens. A cartridge is 32 KiB (`lorom`) or 64 KiB (`hirom`).

## Flow

1. **`new`.** The skeleton has the header include, `cart_header`, `cart_vectors reset, halt, halt, halt`, a `reset` proc ending in `stp`, `.include "halt.inc"` (the shared `halt` proc, last in CODE) and `TODO(cite)` markers. A cart that draws also includes `ppu.inc`, which brings the setup macros and the `zero` byte `clear_vram` reads. The title is at most 21 characters.
2. **Write the assembly.** Conventions the existing carts follow (`halt`, `bus-timing`, `interrupts`):
   - It starts in emulation mode with `D = $0000`, `DBR = $00`, `M = X = 1`, `I` set. Use `a:` absolute addressing and 8-bit accesses until you leave emulation mode.
   - **A cart checks nothing itself unless it must.** It leaves results in WRAM (bank `$00:0000–1FFF` mirrors `$7E:0000`) or in registers and ends with `stp`. A Rust test in the consuming crate does `include_bytes!("../../../tests/assets/carts/<name>.sfc")`, runs `Snes::<Fast>::builder(...)`, and compares with expected values. See `crates/sfes-snes-core/src/carts.rs`.
   - **Expected values come from the documents, never from running our emulator.** Cite the source beside each value, in the cart and in the Rust test. A marker value the cart chose itself (like `$A5` in `halt`) cites `adr-0013`.
3. **Tag everything.** The file's leading comments, and the comment lines directly above each `.proc` and `.macro`, carry `; @source:` lines. See `sfes-cite` for anchor forms. `register` refuses to run while a `TODO(cite)` remains.
4. **`register`.** It builds with ca65/ld65, writes `tests/assets/carts/<name>.sfc`, and adds the manifest entry (`license = "own"`, SHA-256, notes). The sentence you pass becomes the notes, so make it describe the behaviour the cart exercises. Run it again after any `.s` change: it refreshes only the SHA-256 and keeps the notes.
5. **Add the Rust test** in the crate that owns the feature, tagged with `// @source:` for every document and ADR it uses (see `sfes-cite`).
6. **`golden`**, only when the cart should produce a frame (PPU work). Skip it for carts that only leave WRAM or register state.
7. **Check:** `cargo xtask carts check` (the committed ROM must match a rebuild byte for byte), then `sfes-preflight`.

## Things that fail or mislead

- **Stale ROM:** edit a `.s`, forget `register`, and `carts check` fails in CI. The built `.sfc` is committed.
- **cc65:** version 2.19 prints itself as `V2.18`, which is expected (ADR 0013). `brew install cc65` on macOS.
- **No PPU yet:** until the PPU renders, `golden` produces a 0×0 frame and a meaningless hash. Do not create goldens for carts before M2's PPU work lands. Say so instead.

## Goldens (ADR 0015)

- One TOML file per ROM in `tests/assets/golden/`, covered by one manifest directory entry. `rom` is relative to `tests/assets/`.
- Frames count from 1; frame *n* is the *n*-th frame after power-on with seed 0. A `[[input]]` table (`frame`, `port`, `buttons = "START"` or `"A | B"`, `""` releases all) is applied before that frame runs.
- A new golden needs at least one checkpoint. `golden` writes a placeholder hash and runs `cargo xtask golden --update <name>` to fill it in.
- **Update:** `cargo xtask golden --update [name…]` reruns the named goldens, or all of them. Every checkpoint whose hash changes is reset to `status = "unverified"` with no `oracle` or `blessed`.
- **See the frame:** `cargo xtask golden --png <name> <frame>` writes `target/golden/<name>-<frame>.png`, for looking at it, never for hashing.
- **Never write `oracle` or `blessed` yourself.** Those mean a person compared the frame with an oracle's output (name and version) and approved it. Leave entries `unverified` and say so in your report.
- **A PR that changes a golden says why.** Add a "Golden changes" section to its description: which goldens changed and the reason. When many hashes change at once, name the cause (for example a renderer change) instead of listing the files.
- A hash changing in a PR that did not mean to touch rendering is a bug until shown otherwise. Look before running `--update`, because `--update` accepts whatever the machine now produces.
