# The 65C816 CPU (`sfes-snes-cpu`)

This page explains the SNES's main processor and how the crate models it (QA-08). It grows with each instruction family during milestone M1. Every statement about the chip cites its source: the W65C816S datasheet (`wdc-w65c816s` in [`provenance/bibliography.toml`](../../provenance/bibliography.toml)) or an ADR.

## What the chip is

The 65C816 is a 16-bit successor of the 6502. It has a 24-bit address bus and an 8-bit data bus, so a 16-bit value takes two memory cycles (datasheet 2.16). It starts in **emulation mode**, where it behaves like a 65C02, and software switches it to **native mode** to get 16-bit registers (datasheet 2.8, 7.10).

The same core serves the SNES main CPU and the SA-1 coprocessor. It is generic over a bus trait and holds no reference to its machine.

## Registers

The model is `Cpu65816` in [`registers.rs`](../../crates/sfes-snes-cpu/src/registers.rs), after the datasheet's Table 2-1.

| Field | Register | Width | Notes |
| --- | --- | --- | --- |
| `a` | Accumulator C | 16 bits | A is the low byte and B the high byte. With the M flag set only A takes part in operations, and B is kept (2.4). |
| `x`, `y` | Index registers | 16 bits | With the X flag set they are 8 bits wide and the high byte is forced to zero (2.7). |
| `s` | Stack pointer | 16 bits | The high byte is `01` in emulation mode. The stack is always in bank 0 (2.11). |
| `d` | Direct register | 16 bits | Added to the 8-bit operand of direct addressing; always bank 0 (2.6). |
| `dbr` | Data Bank Register | 8 bits | The bank of data addresses (2.5). |
| `pbr` | Program Bank Register | 8 bits | The bank of instruction fetches (2.9). |
| `pc` | Program counter | 16 bits | An offset in the program bank; it wraps inside the bank (2.10). |
| `p` | Processor Status | 8 bits | See below. |
| `e` | Emulation flag | 1 bit | Not part of P; exchanged with the carry by `XCE` (2.8). |
| `run` | Run state | | `Running`, `Waiting` (after `WAI`) or `Stopped` (after `STP`). |

### The status register

From bit 7 down to bit 0 the flags are **N V M X D I Z C** (Table 2-1; 2.20 gives M and X as bits 5 and 4).

| Flag | Meaning |
| --- | --- |
| N | Negative: the sign bit of the last result |
| V | Overflow |
| M | Memory select: 1 for an 8-bit accumulator and memory operands, 0 for 16-bit |
| X | Index select: 1 for 8-bit X and Y, 0 for 16-bit |
| D | Decimal mode |
| I | IRQ disable |
| Z | Zero: the last result was zero |
| C | Carry |

In emulation mode M and X are always 1. Bit 4 then shows the Break flag, but only in the copy of P written to the stack (2.8). `Flags` therefore keeps M and X set in emulation mode and has no B flag.

### Mode and width changes

Two methods keep the registers consistent with the flags:

- `set_emulation(true)` sets M and X, forces the high byte of S to `01` and clears the high bytes of X and Y. The accumulator, including B, is untouched (7.10).
- `set_status` forces M and X to 1 in emulation mode, and clears the high bytes of X and Y whenever X is 1. Clearing X again does not bring the old high bytes back (2.7).

### Reset values

`Cpu65816::new()` holds what reset establishes (2.25): emulation mode, M, X and I set, D clear, stack page `01`, and D, DBR and PBR zero. Reset leaves A, the low bytes of X, Y and S, and the N, V, Z and C flags uninitialized; the crate makes them zero so that every machine starts identically (FND-02).

## Operand width

An instruction such as `LDA` exists in an 8-bit and a 16-bit form, chosen by the M or X flag. The crate writes it once, generic over the private `Width` trait (`u8` and `u16`), and the opcode `match` picks the width. The compiler emits both versions.

## The bus and one step

[ADR 0004](../adr/0004-per-cycle-bus-contract.md) defines both.

- **Bus.** The core makes one call to `Bus65816::read`, `write` or `idle` for every clock cycle. Each call carries the cycle's 24-bit address and a `Pins` value with the eight output signals VDA, VPA, VPB, RWB, E, M, X and MLB (datasheet 2.17 to 2.28). The bus is the only clock.
- **Step.** `Cpu65816::step` runs one instruction, one interrupt entry, one byte of a block move, or one idle cycle while waiting or stopped. It reads the interrupt requests once, before the opcode fetch.

Every instruction starts the same way: a read at PBR,PC with VDA and VPA both active, which is how the chip marks an opcode fetch (2.26; cycle 1 of every row of Table 5-7).

## Testing against the vectors

`sfes-test-harness` runs the SingleStepTests 65816 vectors against the core (`crates/sfes-test-harness/src/sst65816/`). For each test it loads the initial registers unchanged, seeds a `RecordingBus` (sparse RAM over the full 24-bit space) with the listed bytes, calls `step`, and reports the first difference, in this order: bus cycles (address, value and all eight signals), registers, memory, run state. A block-move test takes one `step` per byte; a test that ends with the vectors' halt record must leave the processor waiting or stopped (ADR 0004).

```bash
cargo xtask vectors fetch
cargo nextest run -p sfes-test-harness
```

| Variable | Effect |
| --- | --- |
| `SFES_VECTORS_DIR` | Where the fetched sets are. Default: `tests/assets/vectors/` |
| `SFES_REQUIRE_VECTORS=1` | A missing set fails the run. Without it the run is skipped with a message |
| `SFES_VECTOR_SAMPLE=<n>` | Run only the first `n` tests of each file |

Only the opcodes in the harness's `IMPLEMENTED` list are run. The list is empty until the first instruction family lands (#33) and is deleted when all 256 opcodes exist (#43).

## Bring-up state

Until milestone M1 closes, two placeholders stand in for missing behaviour. An opcode without a `match` arm stops the processor, and so does a recognized interrupt. Neither is hardware behaviour. The first is removed once all 256 opcodes are implemented (#43), the second with the interrupt sequence (#42).
