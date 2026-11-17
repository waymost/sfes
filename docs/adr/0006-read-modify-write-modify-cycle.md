# 0006. The modify cycle of read-modify-write instructions

- **Status:** accepted
- **Date:** 2026-11-17
- **Confidence:** high for the address; medium for the value written in emulation mode

## Context

TSB and TRB (issue #35) are the first read-modify-write instructions in the core; the shifts, rotates, increments and decrements of #37 follow. Each one reads its operand, spends one cycle modifying it, and writes the result back. Table 5-7 settles most of that sequence, but leaves two things open:

1. **The address of the modify cycle.** The table prints it as "AA+1" (row 1d) and "D+DO+1" (row 10b) for both widths, although with an 8-bit operand nothing is read at AA+1.
2. **The data bus in emulation mode.** Note 17 says RWB is low in the modify cycle there, which makes it a write under ADR 0004, but no source says what value is written.

## Sources

- `wdc-w65c816s#table-5-7-1d` (Absolute R-M-W, "6 & 8 cycles"): cycles 4 "DBR,AA" Data Low, 4a (note 1) "DBR,AA+1" Data High, 5 (notes 3 and 17) "DBR,AA+1" IO, 6a (note 1) "DBR,AA+1" Data High written, 6 "DBR,AA" Data Low written. MLB is low from cycle 4 to cycle 6. `#table-5-7-10b` (Direct R-M-W) has the same shape at "0,D+DO" and "0,D+DO+1".
- `wdc-w65c816s#table-5-7-note-1`: cycles 4a and 6a exist only for 16-bit data ("add 1 cycle for M=0").
- `wdc-w65c816s#table-5-7-note-17`: "In the emulation mode, during a R-M-W instruction the RWB is low during both write and modify cycles."
- `wdc-w65c816s#2.19`: MLB is low "during the last three or five cycles" of these instructions, "depending on the state of the M flag".
- `wdc-65816-manual#chapter-18-tsb` and `#chapter-18-trb`, `6502org-65c816-opcodes#6.1.2.3`: the operation and flags only; nothing on the bus cycles.
- `sst-65816`, files `04`, `0c`, `14` and `1c`, both modes, all 80,000 tests:
  - In every test with 8-bit data (`04 n 1`, `0c n 3` and 60,029 more, in both modes), the modify cycle is at the address of the one byte read, AA, not AA+1. In every 16-bit test (`04 n 2`, `0c n 1` and 19,967 more), it is at AA+1, the address of the high byte read.
  - In every emulation-mode test (`04 e 1`, `0c e 1` and 39,998 more), the modify cycle has RWB low, VDA and VPA inactive, MLB active, and the value just read on the data bus.

The datasheet's addresses fit 16-bit data and say nothing separate for 8-bit data. One reading is consistent with the table and with every vector: the modify cycle repeats the address of the last byte read.

## Decision

- The modify cycle is at the address of the last data byte read: AA (or D+DO) for 8-bit data, AA+1 (or D+DO+1) for 16-bit data. The writes follow, high byte first.
- In native mode the modify cycle is an internal operation (`idle`) with MLB set. In emulation mode it is a `write` with MLB set, VDA and VPA clear, and the unmodified byte as its value.
- MLB is set on every data cycle from the first read to the last write, which gives the three or five cycles of 2.19.

The rule lives in `modify_operand` (`crates/sfes-snes-cpu/src/addressing/modify.rs`), which every read-modify-write instruction uses.

## Consequences

- In emulation mode the bus sees the unmodified value written before the result. A memory-mapped register with side effects on writes sees two writes. That matters to `sfes-snes-core` at M2, and is why the value is part of this decision and not left as a detail of the harness.
- The confidence on the value is medium: it rests on the vectors alone, whose generator is undocumented. A hardware test that shows a different value in the emulation-mode modify cycle reopens this ADR.
- The indexed rows (6b and 16b) arrived with #37 and follow the same rule: the 48 files of the 24 memory-form opcodes of ASL, LSR, ROL, ROR, INC and DEC (`06` to `fe`, both modes) pass with `modify_operand` unchanged.
