# 0027. APU port placeholder until M3

- **Status:** accepted
- **Date:** 2026-11-20
- **Confidence:** high for the register shape (points 1 to 3), which follows anomie-regs; the placeholder's values (points 4 to 6) are a project choice, not hardware behaviour, and claim no confidence about the hardware

## Context

The four APU ports, `$2140–$2143`, are how the main CPU talks to the SPC700. The SPC700 and S-DSP arrive in M3, but ROMs read the ports during boot, and the bus must answer them somehow in M2. ADR 0019, point 1 left every B-bus register of a missing chip reading as open bus and ignoring writes, and point 2 put a sync hook before every access to B-bus `$40–$7F`, which does nothing until the APU exists.

This ADR decides exactly what reads and writes of the ports do until M3, where the placeholder sits so that M3 replaces only its inside, and which ROMs and goldens it rules out. It is not a decision about the hardware: what the ports hold before the SPC700 first writes them, and what they do at reset, are left to M3's ADR.

## Sources

- **anomie-regs#2140** (with `#2141`, `#2142`, `#2143`, one entry): the four registers are read/write, on the B bus, accessed at any time. "The value written here is not the value read back": a CPU write shows up in the SPC700's registers `$F4–$F7`, and a CPU read returns what the SPC700 last wrote to those registers. If the SPC700 writes a port during a CPU read, the CPU reads the OR of the old and new values. The registers are mirrored throughout `$2140–$217F`. Used here for the register shape only, as the issue scopes it.
- **Technical design, section 4, catch-up rules 1 and 3:** the APU is synced before an access to its ports, `$2140–$217F`, and runs only on port access, at frame end, and when the audio buffer needs samples. Section 5 gives the APU unit's shape: it owns audio RAM, the timers and the four ports, and `run_until` is what the bus calls on catch-up.
- **ADR 0019, points 1 and 2:** B-bus `$40–$7F` decodes to the APU, as port `address & 3`; the APU's sync hook runs before every such access.

No source says what the ports read before the SPC700 has written them: that depends on the SPC700 program running, which is M3's work. None was needed for the placeholder, because its values are chosen, not modelled.

A byte scan of the approved third-party test ROMs (ADR 0014; the files under `tests/assets/fetched/`) for absolute and long loads, stores and compares naming `$2140–$2143` found none. The few byte pairs `$40–$43, $21` in them sit in data, not after such an opcode. The scan cannot see an access through the direct page (with D = `$2100`) or an indexed address, so it is evidence, not proof. No own test cartridge touches the ports.

## Decision

1. **The shape.** The bus holds an APU unit, `Apu`, in a module of its own (`bus/apu.rs`), with four CPU-to-APU latches, the values the CPU last wrote, and four APU-to-CPU latches, the values a CPU read returns. A CPU read of a port never returns the CPU's own write (anomie-regs#2140).
2. **Decode and mirrors.** Every B-bus address `$40–$7F` reaches port `address & 3`, so `$2140–$217F` mirror the four ports every four bytes (ADR 0019, point 1). A general or HDMA transfer reaches the ports the same way, through the same read and write paths.
3. **Owner sync.** Before every read or write of a port, the bus calls `Apu::run_until(now)` from its APU sync hook (technical design, catch-up rule 1; ADR 0019, point 2). The hook stays the one place the bus asks the APU to catch up. In the placeholder, `run_until` does nothing.
4. **Reads.** A read returns the port's APU-to-CPU latch, and the value goes on the data bus, as any answered read does (ADR 0019, point 4). In the placeholder the four latches are `$00` from power-on and nothing changes them, so every read of a port returns `$00`. The OR of anomie-regs#2140 does not arise, since nothing writes the APU side. A peek returns the same value and reports no side effects.
5. **Writes.** A write stores the value in the port's CPU-to-APU latch, where M3's SPC700 will read it as `$F4–$F7`. Until then nothing reads it but the bus's own tests.
6. **Reset and power-on.** Power-on and the reset button both set all eight latches to `$00`. This is the placeholder's choice. M3's ADR decides what the APU does at reset, from the sources.
7. **M3 replaces the inside.** `Apu`'s methods (`power_on`, `reset`, `run_until`, `read_port`, `write_port`, `peek_port`) are the interface the bus uses. M3 replaces their bodies, and the struct's fields, with the SPC700, the S-DSP and the APU clock ratio (technical design section 4). The bus's decode, sync hook and access paths do not change. Every placeholder test cites this ADR, so M3 finds and replaces them all.
8. **What it rules out.**
   - Any ROM that waits for the APU. Almost every commercial game does, as part of uploading its sound program during boot, so none is expected to boot past that point before M3. This matches the plan: commercial games are M3's work.
   - A golden frame, or any other recorded output, from a ROM that reads the ports. Its output depends on the APU, so it would change in M3. Such a golden is not recorded until M3. The scan above found no such ROM among the approved test ROMs. A golden that later turns out to depend on the ports is deleted and re-recorded in M3, with the change explained in its PR (ADR 0015).
   - Own test cartridges that read the ports, other than to check this placeholder.

## Consequences

- Reads of the ports become deterministic `$00` instead of open bus, so a ROM polling `$2140` for a value the SPC700 would write spins forever rather than on a value that depends on its own code. A ROM that writes the ports and never reads them runs as it would with an APU, apart from the sound.
- ADR 0019, point 1's "until a chip answers its registers, they read as open bus" no longer applies to `$2140–$217F`; it still applies to the PPU's.
- M3 changes the results of every read of the ports, by design. Any test, golden or recorded output that shows a port read changes then, and only tests that cite this ADR should.
- Revisit if an approved test ROM is found to read the ports before M3, or if M2 needs a ROM past its APU handshake. Then the answer is to bring the SPC700 forward, not to fake a handshake in the placeholder.
