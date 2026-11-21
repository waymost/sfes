# 0022. The IRQ check after a change of I, and the end of WAI

- **Status:** accepted
- **Date:** 2026-11-20
- **Confidence:** high for the delayed I flag; medium for the two cycles that end WAI and for which instructions besides the five named ones delay

## Context

ADR 0004 has `step` read `interrupts()` once, before the opcode fetch, and test the I flag as it stands at that moment. ADR 0011, point 3, has a woken WAI start the interrupt sequence or the next instruction in the same step, with no cycles added. ADR 0004 said it would be revisited "if … an instruction … delays recognition of a changed I flag". ADR 0021, point 2, settles when the SNES bus sees a changed line, and leaves two CPU-core questions to this ADR:
- which I flag the IRQ check uses after an instruction that changes I;
- how long WAI takes to end once a request arrives.

## Sources

- `anomie-timing#s-cpu/interrupts`:
  - the check "occurs just before the final CPU cycle of the instruction";
  - PLP, CLI, SEI, SEP #$04 and REP #$04 "update the flags during their final CPU cycle", so the check uses the old I flag. RTI, BRK and COP "do not have this issue";
  - four examples, each after an IRQ set up to arrive during a WAI:
    1. `SEI; WAI; STZ $00; LDA #$01; CLI; LDA #$42; STP`, handler `STA $00; RTI`: `$00` ends as `42`, not 0 or 1;
    2. `SEI; WAI; CLI; SEI`: the IRQ is taken after the SEI, and the P it pushes has I set;
    3. `SEI; WAI; CLI; RTI`, with an RTI that sets I: no IRQ is taken;
    4. `SEI; WAI; STZ $00; LDA #$01; RTI`, with an RTI that clears I and returns to `LDA #$42; STP`, same handler: `$00` ends as 1.
  - The first cycle of the interrupt sequence is "an opcode fetch cycle from PB:PC", not an IO cycle as the datasheet shows.
- `anomie-timing#s-cpu/instructions`: WAI "takes 12 master cycles (2 IO cycles) to end", after which the handler may run.
- `wdc-w65c816s#table-5-7-19d` and `#table-5-7-note-9`: WAI is three cycles, the last two internal at PBR,PC+1. Note 9 on cycle 2: "Wait at cycle 2 for 2 cycles after NMIB or IRQB active input."
- `wdc-w65c816s#table-5-7-19a`, `#table-5-7-18`, `#table-5-7-22b`: the flag instructions, REP/SEP and PLP. In each, the cycle that delivers the new P (the internal cycle, REP/SEP's third cycle, the pull) is the last.
- `wdc-w65c816s#table-5-7-22g`, `#table-5-7-22j`: RTI pulls P in its third of six or seven cycles; BRK and COP push P before the two vector cycles.
- `wdc-w65c816s#7.13`, `#2.24`: an NMIB or IRQB ends the wait; with I set, an IRQB runs the next instruction.
- `6502org-65c816-opcodes#6.9`: after WAI, the processor responds "without any additional delay". ADR 0011 followed this.

anomie-timing and the datasheet agree on the delayed I flag. On the end of WAI, anomie-timing and 6502.org disagree. Note 9 fits both readings: ADR 0011 counted its two cycles as the first two of row 22a. anomie-timing says the first of those is an opcode fetch, not a wait, and that the handler runs after the two IO cycles.

## Decision

1. **The IRQ check uses the I flag from before the previous step's last cycle.**
   - `Cpu65816` gains a public field `delayed_irq_disable: Option<bool>`. An instruction that changes P in its final cycle stores the I flag as it was before that cycle. Those are PLP, REP, SEP and the seven flag instructions of row 19a.
   - Of these, only CLI, SEI, PLP, REP #$04 and SEP #$04 can change I. The others store a value equal to I, which changes nothing.
   - `Cpu65816::irq_disabled_at_boundary()` returns the stored value, or P's I flag when there is none. `step` takes it and clears the field at its start, so it applies to one boundary only. `reset` clears it too.
   - RTI, BRK, COP, the hardware interrupt sequence and XCE store nothing. RTI pulls P, and BRK and COP set I, before their last cycle. XCE does not touch I.
   - NMIs are not masked by I and are unaffected.
   - `sfes-snes-core`'s step classification for observer events uses the same method.
2. **WAI ends two internal cycles after the request.** A waiting processor's step that finds an NMI or an IRQ pending makes two internal cycles at PBR,PC, the same cycles as its halted steps (ADR 0011, point 2). It then becomes running, and in the same step runs the interrupt sequence (row 22a, from its opcode fetch) or, for an IRQ with I set, the next instruction. The choice uses the requests that ended the wait, read at the start of the step. This supersedes ADR 0011, point 3.
3. **Format.** The new field is part of `Cpu65816`'s serialized state.

ADR 0004 is amended at two points:
- "What one `step` covers", case 2 (waiting): the wake-up adds the two cycles of point 2.
- "When `interrupts()` is sampled": the requests are still read once per step, before the opcode fetch. The I flag they are tested against is that of point 1.

## Consequences

- A woken WAI takes two cycles longer than before: 12 master cycles on the SNES, where internal cycles cost 6.
- The SingleStepTests vectors are unaffected. The WAI files stop at the halt record, and every test is a single instruction, so no test reaches a second boundary.
- The four anomie-timing examples, the five delaying instructions, BRK and COP, and an NMI ending a wait are unit tests in `crates/sfes-snes-cpu/src/step.rs`. Without point 1, examples 1 to 3 fail.
- Debuggers and save states see the new field. A state saved before it does not load.
- The SA-1 uses the same core and inherits both behaviours. No source describes the SA-1's timing here. A test ROM that shows a difference reopens this ADR for that bus.
- We revisit point 2 if a test ROM measures the time from the request to the first handler cycle, and point 1 if one shows another instruction delaying the check.
- The technical design's CPU section and `docs/chips/cpu.md` cite this ADR.
- ADR 0021's "not modelled here" note about the I flag and WAI points to this ADR, and `docs/chips/core.md` replaces its "Not modelled yet" note.
- The `interrupts` test cartridge's IRQ handler starts 12 master cycles later. Its `STA $0000` now meets the refresh one cycle earlier, so `$2137` latches dot 153, not 150, and the expected values in `crates/sfes-snes-core/src/carts.rs` change with it. The IRQ is still stamped at 7242, the start of the step that ends the wait.
