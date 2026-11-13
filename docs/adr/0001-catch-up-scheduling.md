# 0001. Catch-up scheduling on bus access

- **Status:** accepted
- **Date:** 2026-11-12
- **Confidence:** high for the model; medium for the `Fast` profile's per-line granularity, which the compatibility list (QA-04) will test

## Context

The SNES has several components on independent clocks: the 65C816 (5A22) at 6, 8 or 12 master cycles per access, the PPU at 4-cycle dots, the SPC700 and S-DSP on a separate 24.576 MHz oscillator, and coprocessors (SA-1, Super FX) with their own clocks. An emulator has to interleave them.

Two constraints decide how (FSD FND-01, FND-02, EMU-01):

- One machine is one owned value that implements `Clone + Send`: no threads, coroutines, `Rc`/`RefCell`, or shared mutable state inside it.
- Results must be bit-identical on every OS, architecture, build profile and thread count.

Cooperative-thread schedulers, as used by accuracy-focused cores, cannot be cloned. Stable Rust has no stackful coroutines anyway. Per-cycle lockstep of every component is cloneable but slow (a measured lockstep Rust core took 3.27 ms per frame against our 1.0 ms target, QA-05).

## Sources

- FSD EMU-01 (scheduling and the `Fast` profile), FND-01, FND-02, QA-05.
- TDD section 4: clock domains, deadlines and catch-up rules.
- `fullsnes`: clock rates and per-region CPU access timing. These are cited in code when implemented.

## Decision

The CPU drives time, and the other components catch up.

1. Time is an integer master-clock counter, `Cycles(u64)`. Every CPU `read`, `write` and `idle` adds its cost.
2. Before an access to a component's registers or shared memory, that component runs up to `now` (owner sync): PPU at `$2100–$213F`, APU ports at `$2140–$217F`, coprocessor registers and their shared RAM.
3. The bus keeps a small fixed array of the next scheduled events (NMI and frame boundary, H/V IRQ, HDMA, auto-joypad, coprocessor timers), each a `Cycles` value. After each access the CPU path compares `now >= next_deadline`, which is one branch on the hot path.
4. The APU runs lazily: on port access, at frame end, and when audio is requested. The cross-clock ratio is an exact integer fraction (NTSC: 45056/39375 oscillator cycles per master cycle), and its remainder is carried in an accumulator, so no time drifts.
5. Under `Fast`, PPU catch-up renders whole lines. A register write in the middle of line *L* takes effect on line *L+1*.
6. All stops (`run(Stop)`) take effect at the first instruction boundary at or after their target.

## Consequences

- The machine stays one plain struct: `Clone` is a snapshot, and determinism holds by construction.
- The common path of a bus access is three steps and two comparisons; PPU, APU and coprocessor work happens only on side branches.
- Ordering bugs between deadlines, HDMA, IRQ and catch-up are the main risk. They are mitigated by per-cycle CPU vectors, timing test ROMs, our own test cartridges, and the M8 differential harness.
- Mid-line raster effects are a known `Fast` limitation. A dot-based `Precise` profile can be added as a new `Accuracy` type without changing this model (decision at M8).
- Revisit if catch-up cannot pass the timing test ROMs at M2 and M3, or if coprocessor synchronization at M9 needs finer interleaving than register-access sync.
