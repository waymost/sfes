# 0005. Emulation-mode stack pointer in the vectors, and stack-relative addresses

- **Status:** accepted
- **Date:** 2026-11-13
- **Confidence:** high for the stack pointer; medium for stack-relative addresses in emulation mode

## Context

Two questions came up when the load instructions (issue #33) first ran against the SingleStepTests 65816 vectors.

1. Almost every emulation-mode test starts with a stack pointer whose high byte is not `01`, and expects a high byte of `01` after an instruction that does not touch the stack. The harness loaded the registers unchanged (issue #31), so every `.e` file failed on its first test. Either the core forces the high byte somewhere, or the harness does.
2. In emulation mode, does a stack-relative address (`d,s` and `(d,s),y`) stay in page 1, or is S plus the offset an ordinary 16-bit sum?

## Sources

### Stack pointer (`wdc-w65c816s`)

- `#2.11`: "During Emulation mode, the S High-order byte (SH) is always equal to one."
- `#7.10`: when switching from native to emulation mode, "the high byte of the Stack is set to 01".
- `#7.21`: "When in the Emulation mode, a 01 is forced into SH", so TCS does not load B into SH.

So a processor in emulation mode never holds a stack pointer outside page 1. The state the vectors start from is not one the chip can be in.

### Stack-relative addresses

- `wdc-w65c816s#3.5.23`: the effective address is "the sum of the second byte of the instruction and the stack pointer", with a bank byte of zero. The section makes no exception for emulation mode.
- `wdc-w65c816s#7.1`: "In the Emulation mode, the Stack address range is 000100 to 0001FF. The following opcodes and addressing modes will increment or decrement beyond this range when accessing two or three bytes: JSL, JSR (a,x), PEA, PEI, PER, PHD, PLD, RTL". Stack-relative addressing is not in that list, which can be read as confining it to page 1.
- `6502org-65c816-opcodes#5.1.1`: page wrapping happens only for instructions and addressing modes that the 65C02 has; stack-relative addressing is new, so it does not wrap. `#5.20` and `#5.21` give the address as a 16-bit sum of S and the offset in bank 0, with no emulation-mode case.

The datasheet's 7.1 and the tutorial can be read as disagreeing.

### Vectors (`sst-65816`)

Surveyed: the 25 emulation-mode files of the load instructions, 250,000 tests.

- 249,027 tests start with SH other than `01`. In all 250,000 the final S is `01` over the initial low byte.
- `sst-65816#a9.e`, test `a9 e 1`: S starts as `A8B9`; LDA immediate ends with S `01B9`.
- `sst-65816#a3.e`, test `a3 e 1`: S starts as `7F98` and the offset is `B0`. The data is read at `000248`, which is `0198 + B0`: the address uses page 1 as the high byte of S, and the sum leaves page 1. 4,831 tests of that file have a sum past `01FF`, and every one reads at `0100 + SL + offset`.
- `sst-65816#b3.e`, test `b3 e 710`: the pointer's low byte is at `0001FF` and its high byte at `000200`. 4,974 tests of the file have a sum past `01FF`, all in agreement.

## Decision

1. **The harness puts S in page 1.** `State::processor` in `sfes-test-harness` sets the high byte of S to `01` when a test's emulation flag is set, and loads every other value unchanged. The core does not force SH during an instruction: it keeps the invariant where the datasheet says the chip does (`Cpu65816::new`, `set_emulation`, and, from #39 and #40, the instructions that write S).
2. **Stack-relative addresses are a 16-bit sum in both modes.** `d,s` data and the `(d,s),y` pointer are at `0,S+SO` and `0,S+SO+1`, wrapping only at the end of bank 0. In emulation mode S is `01` over SL, and the sum may leave page 1. This follows 3.5.23 and the tutorial; we read 7.1's list as covering the instructions that push and pull, not the stack-relative modes.

## Consequences

- A test's initial state is no longer loaded bit for bit. The vectors' own final states show the adjustment is what their generator assumed.
- The same adjustment serves the later families. If a `.e` file of another instruction expects a final S outside page 1, this ADR is revisited in that issue.
- The emulation-mode stack rules for pushes and pulls (which instructions wrap in page 1 and which do not) are not decided here; they belong to the stack and call issues (#38, #39).
- We revisit decision 2 if a hardware test shows stack-relative addresses confined to page 1 in emulation mode.
