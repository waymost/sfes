# 0012. Misprinted DEX and TAY entries

- **Status:** accepted
- **Date:** 2026-11-18
- **Confidence:** high

## Context

Table 5-5 of the datasheet ("Operation, Operation Codes, and Status Register") lists each instruction's operation and opcode. Two of its rows disagree with the rest of the datasheet and with the other sources, and the manual's TAY entry has a misprint of its own. The M1 code followed the agreeing sources and said so only in doc comments (issues #37 and #40). The M1 review asked for the conflicts to be recorded here, as rule 4 of issue #25 requires.

## Sources

- `wdc-w65c816s#table-5-5`: the DEX row prints the operation "X-1 → A", opcode `CA`. The TAY row prints "A→Y" with the opcode `AB`.
- `wdc-w65c816s#table-5-4`: the opcode matrix puts TAY at `A8` and PLB at `AB`. The Table 5-5 row for PLB also gives `AB`, so the TAY row repeats an opcode that belongs to another instruction.
- `wdc-65816-manual#chapter-18-dex`: DEX "decrement[s] by one the contents of index register X", opcode `CA`.
- `wdc-65816-manual#chapter-18-tay`: the text transfers the accumulator to index register Y, but the entry's opcode box prints the TAX row, "TAX AA", in place of its own: a third misprint. The assembly listings of `wdc-65816-manual#chapter-6` assemble TAY as `A8`.
- `6502org-65c816-opcodes#6.1.1.3`: DEX decrements the X register. `6502org-65c816-opcodes#6.10.1`: TAY transfers A to Y.
- `sst-65816`:
  - `ca.n` and `ca.e`: in all 20,000 tests, X ends as its initial value minus one at the width the X flag selects, and A is unchanged (first tests `ca n 1` and `ca e 1`).
  - `a8.n` and `a8.e`: in all 20,000 tests, Y ends as the low byte of A, or all of C with 16-bit index registers, and DBR is unchanged (first tests `a8 n 1` and `a8 e 1`).
  - `ab.n` and `ab.e`: in all 20,000 tests, A and Y are unchanged, and DBR changes in 19,924 of them: opcode `AB` behaves as PLB, not as TAY (first tests `ab n 1` and `ab e 1`).

## Decision

DEX decrements X, and TAY is opcode `A8`. Opcode `AB` is PLB. The datasheet's Table 5-5 rows for DEX and TAY, and the opcode box of the manual's TAY entry, are treated as misprints: every other place in both documents, the tutorial and the vectors agree.

## Consequences

- `dex` in `crates/sfes-snes-cpu/src/instructions/shift.rs` writes X, and the opcode table in `crates/sfes-snes-cpu/src/step.rs` maps `A8` to `tay` and `AB` to `plb`. `dex` and `tay` cite this ADR.
- A later revision of either document that corrects or confirms these entries does not change the decision, since every other source agrees.
- Further misprints of this kind found later are added here rather than in new ADRs.
