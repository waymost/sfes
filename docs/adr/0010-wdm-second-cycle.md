# 0010. WDM's second cycle

- **Status:** accepted
- **Date:** 2026-11-18
- **Confidence:** high

## Context

WDM (opcode `42`) is the 65C816's reserved opcode (issue #40). Every source agrees that it does nothing and is two bytes long. Table 5-7, which gives the bus cycles of every other instruction, has no row for it, so the kind of its second cycle is not settled by the datasheet.

## Sources

- `wdc-w65c816s#7.16`: "The WDM opcode may be used on future microprocessors. It performs no operation."
- `wdc-w65c816s#table-5-4`: the cell for `42` reads "WDM", mode "i" (implied), "2,2": 2 bytes and 2 cycles. Table 5-7 has no WDM row.
- `wdc-65816-manual#chapter-18-wdm`: WDM "will act like a two-byte NOP instruction"; its byte count is printed as "2*", and its cycle count is left blank.
- `6502org-65c816-opcodes#6.7`: "it is acts like a 2-byte, 2-cycle NOP … The second byte is read, but ignored."
- `sst-65816`, `42.n` and `42.e`: in all 20,000 tests the instruction is two cycles, the opcode fetch and then a cycle at PBR,PC+1 with VDA and VPA both inactive and no value recorded, an internal cycle; the program counter advances by 2 and no register or flag changes. The first tests are `42 n 1` and `42 e 1`.

The documents agree on two bytes and two cycles. The tutorial says the second byte is read; the vectors record no read.

## Decision

WDM runs one internal cycle at PBR,PC+1, the address of its second byte, then steps the program counter past that byte. It changes nothing else. The second cycle is internal, as the vectors record, rather than a program read.

## Consequences

- A bus implementation sees no access at WDM's second byte, so a debugger that counts program fetches does not count it. The cycle still takes its time and reports its address, as every internal cycle does (ADR 0004).
- The confidence is high: the cycle count and length are agreed by all documents, and every vector agrees on the cycle's signals. Only the tutorial's word "read" dissents, and it does not say which signals the read carries.
- The implementation is `wdm` in `crates/sfes-snes-cpu/src/instructions/status.rs`.
