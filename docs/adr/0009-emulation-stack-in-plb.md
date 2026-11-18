# 0009. PLB leaves page 1 in emulation mode

- **Status:** accepted
- **Date:** 2026-11-18
- **Confidence:** medium

## Context

The pushes and pulls (issue #39) apply ADR 0008's rule: in emulation mode a push or pull stays in page 1, except in the instructions that datasheet 7.1 lists (PEA, PEI, PER, PHD and PLD among this family), which step S as a 16-bit value and put the high byte of S back to `01` at the end. The vectors agree with that rule for every instruction of the family but one, PLB, which 7.1 does not list.

## Sources

- `wdc-w65c816s#7.1`: "In the Emulation mode, the Stack address range is 000100 to 0001FF. The following opcodes and addressing modes will increment or decrement beyond this range when accessing two or three bytes: JSL, JSR (a,x), PEA, PEI, PER, PHD, PLD, RTL." PLB accesses one byte and is not listed.
- `wdc-w65c816s#7.8.2`: PHK, PHB and PLB "may be used with the emulation mode even though all Bank Addresses are forced to $00". It says nothing about their stack addresses.
- `wdc-w65c816s#table-5-7-22b`: PLB's pull is "REG Low" at 0,S+1, as for every pull.
- `wdc-65816-manual#chapter-18-plb` and `6502org-65c816-opcodes#6.8.3`: "The stack pointer is first incremented. Then the byte pointed to by the stack pointer is loaded into the register." Neither mentions page 1.
- `sst-65816`, emulation-mode files, tests that start with S at the page edge (S `01FF` for a pull, S `0100` for a push):
  - `ab.e` (PLB): all 37 such tests read `000200` and end with S `0100`; the first is `ab e 75`.
  - `68.e`, `fa.e`, `7a.e`, `28.e` (PLA, PLX, PLY, PLP): all 28, 45, 46 and 40 such tests read `000100` and end with S `0100`.
  - `48.e`, `da.e`, `5a.e`, `08.e`, `8b.e`, `4b.e` (PHA, PHX, PHY, PHP, PHB, PHK): every such test writes `000100` and ends with S `01FF`.
  - `0b.e`, `f4.e`, `d4.e`, `62.e` (PHD, PEA, PEI, PER) write `000100` then `0000FF`, and `2b.e` (PLD) reads `000200` and `000201`, as 7.1 says; first tests `0b e 435`, `f4 e 123`, `d4 e 21`, `62 e 287`, `2b e 618`.
  - After every test of the 16 emulation-mode files of the family, the high byte of S is `01`.

The documents do not mention PLB's stack address in emulation mode; the vectors put it outside page 1.

## Decision

PLB pulls with the 16-bit step of the 7.1 instructions (`StackWrap::BankZero`), so with S at `01FF` it reads `000200`, and then sets the high byte of S back to `01`. Every other push and pull of the family follows ADR 0008: PEA, PEI, PER, PHD and PLD step S as a 16-bit value; PHA, PHX, PHY, PHP, PHB, PHK, PLA, PLX, PLY and PLP stay in page 1.

## Consequences

- PLB is the only instruction in the core whose emulation-mode stack behaviour rests on the vectors alone. It is a one-byte pull, so it can differ from the page-1 rule only when S is `01FF`.
- The vectors' generator is undocumented, so the confidence is medium. A hardware trace showing PLB reading `000100` reopens this ADR.
- The implementation is `plb` in `crates/sfes-snes-cpu/src/instructions/stack.rs`. The technical design's CPU section cites this ADR with 0005 and 0008.
