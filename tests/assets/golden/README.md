# Golden frame hashes

One TOML file per approved third-party ROM (ADR 0014) and per own test cartridge (ADR 0013); the format, the runner and the commands are in [ADR 0015](../../../docs/adr/0015-golden-frame-files.md). `cargo nextest run -p sfes-test-harness` runs them all on every OS; a ROM that is not fetched is skipped, and CI sets `SFES_REQUIRE_VECTORS=1` so a missing one fails.

```bash
cargo xtask golden --new <name> <rom> <frame>…   # a new file, one unverified checkpoint per frame
cargo xtask golden --update [<name>…]            # rerun, rewrite changed checkpoints as unverified
cargo xtask golden --png <name> <frame>          # target/golden/<name>-<frame>.png, to compare with MesenCE
```

Every checkpoint is `unverified` with no `oracle` or `blessed` line until its owner has compared the frame with MesenCE's output and blessed it. A hash here shows the machine still renders what it rendered when the file was written; it does not show that the picture is right.

## Checkpoints

- Own cartridges: frames 10 and 60.
- Third-party ROMs: frames 60 and 300 (five seconds), where the screen has settled for the ones that finish.
- `joypad` holds A and Start from frame 5; `rainwarrior-twoship` presses A at frame 100 and releases it at frame 101.

## Notes on single ROMs

- Own cartridges `bus-timing`, `dma`, `hdma`, `interrupts`, `joypad` and `math` leave the screen blank (forced blank) and report in WRAM, which their own tests read. Their goldens are one blank frame, so they only show that the cartridge still boots and runs.
- `gilyon-cputest-full` stops on "Failed" at test 27 (A = 1108, X = 00EE, Y = 0078, P = B0, S = 01EF); `gilyon-cputest-basic` ends on "Success" after 452 tests. The `full` golden records the failure as it is today. It is a CPU result to investigate, not one the APU placeholder (ADR 0027) explains.
- `rainwarrior-multest_mul16` multiplies over all 16-bit inputs and prints "Pass!" at the end; it had reached 0004 after 3000 frames, so its checkpoints show the screen mid-run.

## Ruled out by the APU placeholder (#87)

None of the approved ROMs waits on the APU port handshake: they all reach a settled or animating screen without one. Commercial games and any ROM that boots an SPC700 program are the ones that will not, and join this list for M3 as they are approved.
