# 0008. The emulation-mode stack in calls and returns, and the `(a,x)` pointer cycles

- **Status:** accepted
- **Date:** 2026-11-17
- **Confidence:** high

## Context

The calls and returns (issue #38) are the first instructions that push and pull. ADR 0005 left open which emulation-mode pushes and pulls stay in page 1 and which leave it. Two further points came up while reading the rows of `JMP (a,x)` and `JSR (a,x)`.

1. **Stack in emulation mode.** In emulation mode S is in page 1 (datasheet 2.11). When a push or pull runs past the end of the page, does the address wrap in page 1 or run into page 0 or page 2, and what is S afterwards?
2. **The `(a,x)` pointer.** Where does PBR,AA+X wrap, and what kind of bus cycle reads it?

## Sources

- `wdc-w65c816s#7.1`: "In the Emulation mode, the Stack address range is 000100 to 0001FF. The following opcodes and addressing modes will increment or decrement beyond this range when accessing two or three bytes: JSL, JSR (a,x), PEA, PEI, PER, PHD, PLD, RTL."
- `wdc-w65c816s#2.11` and `#table-7-1` (row "S"): in emulation mode the stack is "always page 1, 8 bits".
- `wdc-w65c816s#table-5-7-1c`, `#table-5-7-2b`, `#table-5-7-4c`, `#table-5-7-22h`, `#table-5-7-22i`: the stack cycles are at 0,S, 0,S-1, 0,S-2 (pushes) and 0,S+1, 0,S+2, 0,S+3 (pulls).
- `wdc-w65c816s#table-5-7-2a` and `#table-5-7-2b`: the two cycles that read the new program counter are at "PBR,AA+X" and "PBR,AA+X+1", with **VDA 0 and VPA 1**, a program-address cycle.
- `sst-65816`:
  - JSR a and RTS stay in page 1: `20 e 1023` (S `0100`) writes `000100` then `0001FF`; `60 e 121` (S `01FF`) reads `000100` then `000101`. 43 tests of `20.e` and 78 of `60.e` cross the page edge, and every one wraps.
  - JSL, JSR (a,x) and RTL leave it: `22 e 61` (S `0100`) writes `000100`, `0000FF`, `0000FE`; `fc e 458` writes `000100`, `0000FF`; `6b e 104` (S `01FF`) reads `000200` to `000202`. 75, 43 and 100 tests of those files cross the edge, and every one leaves the page.
  - After every test of these five files S is `01xx`: `22 e 61` ends with S `01FD`, `6b e 104` with S `0102`.
  - In all 40,000 tests of `7c` and `fc` (both modes) the two pointer reads have VDA active and VPA inactive, a data-address cycle. `7c n 3` (AA+X = `1B0F8`) reads `EBB0F8`: the pointer wraps in the program bank.

7.1 and the vectors agree on the stack. On the pointer cycles, Table 5-7 and the vectors disagree on one signal.

## Decision

- **Pushes and pulls in emulation mode** step S in its low byte and stay in page 1, except in the instructions that 7.1 lists. Those step S as a 16-bit value while they run, so their stack cycles may be in page 0 or page 2, and at the end of the instruction the high byte of S is set back to `01`. In native mode every push and pull steps S as a 16-bit value in bank 0. The core names the two behaviours `StackWrap::PageOne` and `StackWrap::BankZero`; this PR uses them for JSR a and RTS (page one) and JSL, JSR (a,x) and RTL (bank zero), and #39 applies the same rule to PEA, PEI, PER, PHD and PLD. [ADR 0009](0009-emulation-stack-in-plb.md) adds PLB, which 7.1 does not list but the vectors show leaving page 1.
- **The `(a,x)` pointer** of JMP and JSR is read at PBR,AA+X and PBR,AA+X+1, each a 16-bit sum that wraps in the program bank, with **VDA active and VPA inactive**, as the vectors record. Table 5-7's VPA marking for these two cycles is not followed.

## Consequences

- The stack primitives (`push`, `pull`, `restore_stack_page`) are in `crates/sfes-snes-cpu/src/addressing/stack.rs`; ADR 0005's open point on pushes and pulls is settled for the calls and returns here.
- A bus implementation that treats VPA as "program fetch" (for a cache or a debugger) sees the `(a,x)` pointer as data. That matches how the pointer is used and the vectors, but not the table. If a hardware trace shows VPA on those cycles, this ADR is reopened.
- The confidence is high: for the stack, a datasheet section and every crossing vector agree; for the pointer, every vector agrees and the table's marking is the only dissent.
