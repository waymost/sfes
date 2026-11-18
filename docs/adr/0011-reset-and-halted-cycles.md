# 0011. Reset sequence, halted cycles and interrupt details

- **Status:** accepted
- **Date:** 2026-11-18
- **Confidence:** medium for the reset sequence; high for the halted cycles, WAI wake-up and DBR

## Context

Issue #42 adds BRK, COP, RTI, WAI and STP, the hardware NMI and IRQ entry, and `Cpu65816::reset`. The vectors cover the five opcodes, but not reset, hardware interrupts or what happens after WAI and STP. Their tests stop at the halt record. ADR 0004 left the address and signals of the waiting and stopped cycles to this issue. The documents leave four points open:

1. the bus cycles of reset;
2. the cycles of a waiting or stopped processor;
3. how long WAI takes to wake;
4. whether an interrupt changes DBR.

## Sources

### Reset

- `wdc-w65c816s#2.25`: while RESB is low, D is `0000`, DBR and PBR `00`, SH `01`, XH and YH `00`, E 1, M and X 1, D 0 and I 1. SL, XL, YL, A and the other flags are "not initialized". The signals are VDA 0, VPA 0, VPB 1 (inactive) and RWB 1. "When Reset is brought high, an interrupt sequence is initiated": STP and WAI are cleared, "RWB remains in the high state during the stack address cycles", and the program counter is loaded from `00FFFC,D`.
- `wdc-w65c816s#table-5-7-22a`: one row covers ABORT, IRQ, NMI and RES, "0 bytes, 7 and 8 cycles":
  1. VDA and VPA 1 at PBR,PC, data "IO";
  2. internal at PBR,PC;
  3. to 6. VDA 1 at 0,S, 0,S-1, 0,S-2 and 0,S-3, data PBR, PCH, PCL and P, RWB 0;
  7. and 8. VPB active at 0,VA and 0,VA+1.

  Note 7 is on cycle 2, and note 10 on cycles 3 to 5.
- `wdc-w65c816s#table-5-7-note-7`: "Subtract 1 cycle for 6502 emulation mode (E=1)". `#table-5-7-note-10`: "RWB remains high during Reset."
- `wdc-65816-manual#chapter-18-stp`: reset "causes the emulation bit to be set to one", D zero, "stack high to be set to one", M and X set. It says nothing of the stack cycles.
- `6502org-65c816-opcodes#6.3.1.1`: RESET is one of the four hardware interrupts and "puts the 65C816 into emulation mode, thus there is no native mode RESET vector". The instruction under way "is not completed".
- No vector covers reset.

The documents agree on the register values, the emulation-mode length and the vector. Three things are open:
- **Which cycles a reset has.** Read with note 7, row 22a gives seven cycles. It does not say whether the stack cycles still carry VDA.
- **Whether S moves.** The addresses 0,S to 0,S-2 suggest it steps, but 2.25 leaves SL uninitialized.
- **Where cycle 1's address points.** The documents don't say whether it is the PBR,PC from before reset or after.

### Halted cycles

- `wdc-w65c816s#table-5-7-19d` (WAI):
  - cycles 2 and 3 are internal at PBR,PC+1, and RDY is 0 from cycle 3;
  - when IRQB or NMIB arrives, the row continues with a cycle "1" at PBR,PC+1 with VDA and VPA 1 and data "IRQ(BRK)".
- `wdc-w65c816s#table-5-7-19c` (STP): cycles 2 and 3 are internal at PBR,PC+1. While RESB is 0, cycles 1c, 1b and 1a are internal at PBR,PC+1 with data "RES (BRK)", and a cycle "1 … BEGIN" follows.
- `wdc-w65c816s#7.13`, `#7.14`, `#2.24`: WAI holds the processor until RESB, NMIB or IRQB. STP stops the clock until RESB.
- `sst-65816`, `cb.n`, `cb.e`, `db.n`, `db.e`, all 40,000 tests: three cycles (the opcode, then two internal cycles at PBR,PC+1), then the halt record. The program counter ends one past the opcode. First tests `cb n 1` and `db e 1`.

### WAI wake-up

- `wdc-w65c816s#table-5-7-note-9`, on cycle 2 of the WAI row: "Wait at cycle 2 for 2 cycles after NMIB or IRQB active input."
- `6502org-65c816-opcodes#6.9`: after WAI's third cycle, the processor "can respond to it without any additional delay". With the I flag set, "an IRQ can be responded to within one cycle".
- `wdc-65816-manual#chapter-18-wai`: WAI ensures "that an interrupt is recognized immediately".

### DBR in an interrupt

- `wdc-w65c816s#7.11`: a hardware interrupt, BRK or COP clears PBR. DBR is not mentioned.
- `wdc-w65c816s#7.4.1.3`, on ABORT asserted during an interrupt sequence: "PBR and DBR will become 00 (Emulation mode)".
- `wdc-65816-manual#chapter-18-brk`, `#chapter-18-cop` and `6502org-65c816-opcodes#6.3.1` name PBR only.
- `sst-65816`, `00.n`, `00.e`, `02.n`, `02.e`, all 40,000 tests: DBR ends as it began. First tests `00 e 1` and `02 e 1`.

## Decision

1. **Reset.** `Cpu65816::reset` runs in two parts.
   - First it sets the registers of 2.25. A, the low bytes of X, Y and S, and the N, V, Z and C flags keep their values. The run state becomes running.
   - Then it runs row 22a in emulation mode, seven cycles:
     1. a read of PBR,PC (PBR now `00`) with VDA and VPA, discarded;
     2. an internal cycle at the same address;
     3. to 5. three reads with VDA at 0,S, 0,S-1 and 0,S-2, RWB high (note 10). S steps down in page 1, so it ends three lower;
     6. and 7. the vector reads at `00FFFC` and `00FFFD` with VPB.
   - The program counter takes the vector. Nothing is written.
2. **Halted cycles.** A waiting or stopped processor's step repeats WAI's or STP's third cycle: an internal cycle at PBR,PC (the "PBR,PC+1" of the row), RWB high, with the E, M and X signals. The cycles 1c to 1a of row 19c are the cycles in which RESB is held low. The machine spends them as stopped steps before it calls `reset`.
3. **WAI wake-up adds no cycles.** The step that sees the request runs row 22a at once. Its first two cycles, at PBR,PC (WAI's PBR,PC+1), are row 19d's final cycle and the two cycles of note 9. With the I flag set, an IRQ ends the wait and the same step fetches the next opcode.
4. **DBR is kept** by BRK, COP, NMI and IRQ, as all 40,000 BRK and COP vectors show. 7.4.1.3 applies to an aborted interrupt sequence, which this core does not model (ADR 0004).

## Consequences

- Reset reads three stack bytes without changing them, so a bus with read side effects in page 1 of bank 0 would see them. Whether a machine has any is for that machine's bus to settle; `sfes-snes-core` checks it in M2.
- S after reset is the old SL minus 3, in page 1. Software must set S anyway (2.25).
- The reset sequence rests on the datasheet alone. A hardware bus trace of reset reopens point 1, as would a test ROM that measures the time from RESB to the first opcode fetch.
- How long RESB is held is the machine's choice. `sfes-snes-core` (M2) decides it with its own ADR.
- The technical design's CPU section cites this ADR next to ADR 0004 for `reset`.
- The implementation is `reset`, `wai`, `stp` and `enter_handler` in `crates/sfes-snes-cpu/src/instructions/interrupt.rs`, and `halted_cycle` and `step` in `crates/sfes-snes-cpu/src/step.rs`.
