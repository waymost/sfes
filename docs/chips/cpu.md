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

## Addressing modes

An addressing mode is the part of an instruction that finds its operand. The crate has one function per mode, in [`addressing/`](../../crates/sfes-snes-cpu/src/addressing/mod.rs). Each function performs the bus cycles of its row in the datasheet's Table 5-7, from cycle 2 up to the cycle before the data, and returns an `Operand`: the address of the data's low byte and the address of its high byte. `read_operand` then reads one byte for an 8-bit operand, or two for a 16-bit one (note 1 of the table). The modes are the `Mode` enum, named as in Table 5-6.

Three kinds of cycle appear. In the table below **P** is a program byte (read at PBR,PC with VPA active), **D** a data byte (VDA active) and **I** an internal cycle, where the chip drives an address but no memory is accessed (ADR 0004). Every instruction starts with the opcode fetch, which is not shown.

| Mode | Syntax | Row | Cycles after the opcode | Data address |
| --- | --- | --- | --- | --- |
| Immediate | `#` | 18 | none | the next one or two program bytes |
| Absolute | `a` | 1a | P P | DBR : address |
| Absolute indexed | `a,x` `a,y` | 6a, 7 | P P (I) | DBR : address, plus X or Y |
| Absolute long | `al` | 4a | P P P | the 24-bit address |
| Absolute long indexed | `al,x` | 5 | P P P | the 24-bit address plus X |
| Direct | `d` | 10a | P (I) | 0 : D + offset |
| Direct indexed | `d,x` `d,y` | 16a, 17 | P (I) I | 0 : D + offset + X or Y |
| Direct indirect | `(d)` | 12 | P (I) D D | DBR : pointer |
| Direct indexed indirect | `(d,x)` | 11 | P (I) I D D | DBR : pointer, read at D + offset + X |
| Direct indirect indexed | `(d),y` | 13 | P (I) D D (I) | DBR : pointer, plus Y |
| Direct indirect long | `[d]` | 15 | P (I) D D D | the 24-bit pointer |
| Direct indirect long indexed | `[d],y` | 14 | P (I) D D D | the 24-bit pointer plus Y |
| Stack relative | `d,s` | 23 | P I | 0 : S + offset |
| Stack relative indirect indexed | `(d,s),y` | 24 | P I D D I | DBR : pointer, plus Y |

The cycles in parentheses are optional:

- **Direct modes** spend an internal cycle after the offset when the low byte of D is not zero (note 2).
- **`a,x`, `a,y` and `(d),y`** spend an internal cycle when adding the index crosses a page boundary, or when the index registers are 16 bits wide (note 4, for instructions that read). The chip has added only the low bytes at that point, so the cycle's address is the base address with the index's low byte added to its low byte and no carry (the table's "DBR,AAH,AAL+XL"; datasheet 7.5). `al,x` and `[d],y` have no such cycle.

The internal cycles that are not optional repeat the address of the last program byte, except the last one of `(d,s),y`, which repeats the address of the pointer's high byte.

### Where addresses wrap

A 16-bit operand's high byte is at "the next address", and what that means depends on the mode. The datasheet's caveats 7.1 to 7.3 give the rules, and section 5 of the 6502.org tutorial (`6502org-65c816-opcodes`) spells them out with examples, which the unit tests use.

- **Data reached through DBR or a long address** continues into the next bank: with DBR `12`, a 16-bit `LDA $FFFF` reads `12FFFF` and `130000`. Adding X or Y to such an address also carries into the bank (7.3).
- **Direct and stack-relative addresses stay in bank 0**: the sum is taken in 16 bits, and `00FFFF` is followed by `000000` (7.1, 7.2.1). This holds for data and for the pointers of the indirect modes.
- **The program counter stays in its bank**, so an immediate operand at `FFFF` continues at `0000` of the same bank (2.10).
- **In emulation mode with the low byte of D zero, the direct page wraps at its page boundary**, as the zero page of a 65C02 does: `LDA $FE,X` with D `FF00` and X `0A` reads `00FF08`. This covers `d,x`, `d,y` and the pointers of `(d)`, `(d,x)` and `(d),y`. The pointers of `[d]` and `[d],y` are the exception and run on into the next page (7.2.1, 7.2.2). With the low byte of D not zero there is no page wrapping at all (7.2.3).
- **Stack-relative addresses are a plain 16-bit sum in both modes.** In emulation mode S is in page 1 and the sum may leave it. The datasheet's 7.1 can be read otherwise; ADR 0005 records the decision and the vectors that support it.

## Loads: LDA, LDX, LDY

The loads copy memory to a register and set N and Z from the value (Table 5-5: "M → A", flags `N . . . . . Z .`). They are in [`instructions/load_store.rs`](../../crates/sfes-snes-cpu/src/instructions/load_store.rs).

| Instruction | Width from | Modes (opcodes) |
| --- | --- | --- |
| LDA | M flag | `#` A9, `a` AD, `al` AF, `d` A5, `(d)` B2, `[d]` A7, `a,x` BD, `al,x` BF, `a,y` B9, `d,x` B5, `(d,x)` A1, `(d),y` B1, `[d],y` B7, `d,s` A3, `(d,s),y` B3 |
| LDX | X flag | `#` A2, `a` AE, `d` A6, `a,y` BE, `d,y` B6 |
| LDY | X flag | `#` A0, `a` AC, `d` A4, `a,x` BC, `d,x` B4 |

An 8-bit LDA changes only A; the B accumulator keeps its value (2.4). An 8-bit LDX or LDY leaves the register's high byte zero (2.7). N is bit 7 of an 8-bit value and bit 15 of a 16-bit one.

All 25 opcodes pass every test of their vector files in both modes.

## Testing against the vectors

`sfes-test-harness` runs the SingleStepTests 65816 vectors against the core (`crates/sfes-test-harness/src/sst65816/`). For each test it loads the initial registers, seeds a `RecordingBus` (sparse RAM over the full 24-bit space) with the listed bytes, calls `step`, and reports the first difference, in this order: bus cycles (address, value and all eight signals), registers, memory, run state. A block-move test takes one `step` per byte; a test that ends with the vectors' halt record must leave the processor waiting or stopped (ADR 0004).

The registers are loaded unchanged, with one exception. The emulation-mode tests start with a stack pointer outside page 1, a state the chip cannot be in (datasheet 2.11), and expect the instruction to behave as if S were in page 1. The harness therefore sets the high byte of S to `01` for those tests ([ADR 0005](../adr/0005-emulation-mode-stack-in-the-vectors.md)).

```bash
cargo xtask vectors fetch
cargo nextest run -p sfes-test-harness
```

| Variable | Effect |
| --- | --- |
| `SFES_VECTORS_DIR` | Where the fetched sets are. Default: `tests/assets/vectors/` |
| `SFES_REQUIRE_VECTORS=1` | A missing set fails the run. Without it the run is skipped with a message |
| `SFES_VECTOR_SAMPLE=<n>` | Run only the first `n` tests of each file |

CI caches the set, requires it, and runs all of it on Linux and the first 500 tests of each file on macOS and Windows.

Only the opcodes in the harness's `IMPLEMENTED` list are run. Each instruction family adds its opcodes, and the list is deleted when all 256 exist (#43).

## Bring-up state

Until milestone M1 closes, two placeholders stand in for missing behaviour. An opcode without a `match` arm stops the processor, and so does a recognized interrupt. Neither is hardware behaviour. The first is removed once all 256 opcodes are implemented (#43), the second with the interrupt sequence (#42).
