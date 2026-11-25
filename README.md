# sfes

**sfes** (Super Ferrous Entertainment System) is a headless-first, library-first SNES emulator core in Rust. One emulated console is one owned value you can clone, run in parallel batches and inspect, built clean-room from hardware documentation only. It targets reinforcement-learning and world-model research first, then decompilation, TAS and achievement tooling.

> **Status:** pre-alpha. Milestone M0 (provenance and workspace skeleton) is in progress; nothing emulates yet.

- [`PROVENANCE.md`](PROVENANCE.md): the source policy. No emulator source code is ever consulted.
- [`AGENTS.md`](AGENTS.md): working rules for contributors and coding assistants.
- [Feature specification](docs/design/feature-specification.md): what sfes must do, with the requirement IDs (`FND-01`, `EMU-02` …) used across the repo.
- [Technical design](docs/design/technical-design.md): how it is built. Decisions that refine it are recorded in [`docs/adr/`](docs/adr/).

## Development viewer

`sfes-player` opens a ROM in a window (PLY-01), for watching the core as it develops:

```bash
cargo run --release -p sfes-player -- tests/assets/carts/bg-mode1.sfc
```

- Pixels are square at the largest whole scale that fits the window; `--aspect 8:7` stretches the picture for a TV-like view (ADR 0039). `--region ntsc|pal` overrides the header.
- Arrows are the D-pad; `Z` B, `X` A, `A` Y, `S` X, `Q` L, `W` R; `Enter` Start; `Backspace` or right `Shift` Select.
- `Space` or `P` pauses, `N` or `.` advances one frame while paused, `R` resets, `Esc` quits.
- `--frames N` runs without a window and prints the last frame's size, as a check on machines with no display.

Outside contributions are not accepted yet (see `PROVENANCE.md`, section 4).

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT license](LICENSE-MIT) at your option. Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in this work, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
