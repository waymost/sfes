# 0007. Decimal-mode ADC and SBC

- **Status:** accepted
- **Date:** 2026-11-17
- **Confidence:** high for results and the C, N and Z flags with valid BCD digits; medium for V and for digits above 9

## Context

With the D flag set, ADC and SBC (issue #36) work in binary-coded decimal. For operands made only of digits 0 to 9 the result is ordinary decimal arithmetic. Two things are not settled by the documents:

1. **V in decimal mode.** BCD numbers have no sign, so "signed overflow" has no obvious decimal meaning, yet the documents disagree on whether the flag is even valid.
2. **Digits above 9.** The vectors draw the accumulator and the operand at random, so two thirds of their decimal-mode tests hold at least one hex digit A to F. No document says what the processor makes of those.

Both matter to games only through flag tests and unusual code, but every vector checks them.

## Sources

- `wdc-w65c816s#table-7-1`, row "Flag Reg", column W65C816S: "N,V and Z flags valid in decimal mode." Row "Timing", item D: decimal mode adds no cycles on the W65C816S.
- `wdc-65816-manual#chapter-18-adc` and `#chapter-18-sbc`: "d flag set: Binary coded decimal (BCD) addition is performed", and the same for subtraction. Flags: N set from the most significant bit of the result, V "set if signed overflow", Z if the result is zero, C on unsigned overflow (ADC) or when no borrow was needed (SBC). `#chapter-9-decimal-mode` adds that the carry is correct in decimal mode but "the overflow flag is not, making signed decimal arithmetic, while possible, difficult". The same section says decimal mode applies to "ADC, SBC, and CMP", which its own CMP entry in chapter 18 contradicts: "decimal mode does not affect the CMP instruction".
- `6502org-65c816-opcodes#6.1.1.1`: in decimal mode N, Z and C have their binary meanings (C: the result is outside 0 to 99 or 0 to 9999); "the v flag is overwritten, but … can be considered invalid". Its example 2 (decimal SBC: `$0001 - $2003` with carry set gives `$7998`, C clear) is a unit test.
- `sst-65816`, the 60 files of ADC (`61` to `7f`) and SBC (`e1` to `ff`), both modes: 300,090 of their 600,000 tests run in decimal mode, and 200,212 of those start with a digit above 9 in the accumulator.

The documents agree on decimal arithmetic for valid digits and on N, Z and C. On V, the datasheet calls it valid and the manual calls it not correct; neither defines it.

## Decision

Decimal ADC and SBC work one 4-bit digit at a time from the lowest, at the accumulator's width (two digits or four).

- **ADC**: each digit sum is accumulator digit + data digit + carry in. A sum above 9 has 6 added; the digit is the low 4 bits, and the carry out is set when the adjusted sum exceeds 15.
- **SBC**: each digit difference is accumulator digit − data digit − borrow in. A negative difference has 6 subtracted; the digit is the low 4 bits, and a borrow goes to the next digit. C is set at the end when no borrow is left.
- **N and Z** come from the final result, as in binary mode. **C** is the final carry, or no borrow.
- **V** is the signed overflow of the operation computed on the result as it stands after the lower digits are adjusted but before the top digit is: the same "same-sign operands, different-sign result" rule as binary mode, with the data complemented for SBC.

The rules are written from the documents' digit-by-digit description; nothing in them treats digits above 9 specially, and the same steps applied to such digits give the results the vectors expect. Before the core was written, the rules were checked against the decimal-mode tests of the immediate files (`69` and `e9`, about 20,000 tests): every one matched on A, C, N, Z and V. Computing V from the final result instead fails about a quarter of them, first at `69 n 10`, `69 e 6`, `e9 n 16` and `e9 e 16`. With the core in place, all 300,090 decimal-mode tests pass.

## Consequences

- Decimal mode costs no extra cycles and needs no separate addressing path: only the arithmetic changes. The implementation is in `crates/sfes-snes-cpu/src/instructions/arithmetic.rs`.
- CMP, CPX and CPY are binary whatever the D flag says, as the manual's CMP entry and tutorial 6.1.1.2 say and chapter 9 of the manual does not. The CMP, CPX and CPY vectors include decimal-mode tests, and all pass with binary compares.
- The V rule and the treatment of digits above 9 rest on the vectors, whose generator is undocumented; the confidence for them is medium. A hardware test ROM that disagrees reopens this ADR.
- The technical design's CPU section cites this ADR for decimal mode.
