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

An addressing mode is the part of an instruction that finds its operand. The crate has one function per mode, in [`addressing/`](../../crates/sfes-snes-cpu/src/addressing/mod.rs). Each function performs the bus cycles of its row in the datasheet's Table 5-7, from cycle 2 up to the cycle before the data, and returns an `Operand`: the address of the data's low byte and the address of its high byte. `read_operand` then reads one byte for an 8-bit operand, or two for a 16-bit one (note 1 of the table), and `write_operand` writes them, low byte first. The modes are the `Mode` enum, named as in Table 5-6; `Address` is the same list without immediate, so a store to immediate data cannot be written.

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
- **`a,x`, `a,y` and `(d),y`** spend an internal cycle "for indexing across page boundaries, or write, or X=0" (note 4): a write always spends it, and a read spends it when adding the index crosses a page boundary or the index registers are 16 bits wide. The chip has added only the low bytes at that point, so the cycle's address is the base address with the index's low byte added to its low byte and no carry (the table's "DBR,AAH,AAL+XL"; datasheet 7.5). `al,x` and `[d],y` have no such cycle.

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

## Stores: STA, STX, STY, STZ

The stores write a register, or zero for STZ, to memory and change no flag (Table 5-5: "A → M", "00 → M", flags all unchanged). They use the same Table 5-7 rows as the loads, with RWB low in the data cycles; 16-bit data is written low byte first, as the rows list "Data Low" before "Data High". The only other difference is the indexing cycle of note 4, which a store always spends. They are in [`instructions/load_store.rs`](../../crates/sfes-snes-cpu/src/instructions/load_store.rs).

| Instruction | Width from | Modes (opcodes) |
| --- | --- | --- |
| STA | M flag | `a` 8D, `al` 8F, `d` 85, `(d)` 92, `[d]` 87, `a,x` 9D, `al,x` 9F, `a,y` 99, `d,x` 95, `(d,x)` 81, `(d),y` 91, `[d],y` 97, `d,s` 83, `(d,s),y` 93 |
| STX | X flag | `a` 8E, `d` 86, `d,y` 96 |
| STY | X flag | `a` 8C, `d` 84, `d,x` 94 |
| STZ | M flag | `a` 9C, `d` 64, `a,x` 9E, `d,x` 74 |

An 8-bit STA writes A and leaves memory above it alone; a 16-bit one writes C. Table 5-5's STA row has no `a,y` entry; the opcode matrix (Table 5-4) and row 7 of Table 5-7 both list STA `a,y` as 99, and the vectors exercise it. All 24 opcodes pass every test of their vector files in both modes, the emulation-mode stack-relative ones included under ADR 0005.

## Logic and bit tests: ORA, AND, EOR, BIT, TSB, TRB

All six work at the accumulator's width, set by the M flag. They are in [`instructions/logic.rs`](../../crates/sfes-snes-cpu/src/instructions/logic.rs).

| Instruction | Operation (Table 5-5) | Flags | Modes (opcodes) |
| --- | --- | --- | --- |
| ORA | A V M → A | N Z | `#` 09, `a` 0D, `al` 0F, `d` 05, `(d)` 12, `[d]` 07, `a,x` 1D, `al,x` 1F, `a,y` 19, `d,x` 15, `(d,x)` 01, `(d),y` 11, `[d],y` 17, `d,s` 03, `(d,s),y` 13 |
| AND | A ^ M → A | N Z | the same modes at 2x: 29, 2D, 2F, 25, 32, 27, 3D, 3F, 39, 35, 21, 31, 37, 23, 33 |
| EOR | A xv M → A | N Z | the same modes at 4x: 49, 4D, 4F, 45, 52, 47, 5D, 5F, 59, 55, 41, 51, 57, 43, 53 |
| BIT | A ^ M, result discarded | N V Z; Z only for `#` | `#` 89, `a` 2C, `d` 24, `a,x` 3C, `d,x` 34 |
| TSB | A V M → M | Z | `a` 0C, `d` 04 |
| TRB | blank in Table 5-5; the manual: M AND NOT A → M | Z | `a` 1C, `d` 14 |

ORA, AND and EOR set N and Z from the result; an 8-bit result leaves B alone. BIT and the two test-and-change instructions set Z from A AND M without keeping it. BIT's N and V take the top two bits of the data itself (bits 7 and 6, or 15 and 14), not of the AND, and immediate BIT leaves them alone (Table 5-5 note 2; the manual's BIT entry).

### Read-modify-write

TSB, TRB and the memory forms of the shifts, rotates, increments and decrements share one path, `modify_operand` in [`addressing/modify.rs`](../../crates/sfes-snes-cpu/src/addressing/modify.rs). After the addressing cycles (with the note-4 indexing cycle always spent, as for a write) come the data cycles of rows 1d, 6b, 10b and 16b:

1. read the low byte, then the high byte for 16-bit data;
2. one modify cycle at the address of the last byte read;
3. write the high byte (16-bit data only), then the low byte.

MLB is active on all of these cycles and on no other, which is the "last three or five cycles" of datasheet 2.19. In native mode the modify cycle is internal. In emulation mode RWB is low in it (note 17), and the unmodified byte is written back before the result.

Table 5-7 prints the modify cycle at AA+1 for both widths. With 8-bit data the vectors put it at AA, the byte that was read, and they are the only source for the value written in emulation mode. [ADR 0006](../adr/0006-read-modify-write-modify-cycle.md) records both.

All 54 opcodes pass every test of their vector files in both modes.

## Arithmetic and compares: ADC, SBC, CMP, CPX, CPY

They are in [`instructions/arithmetic.rs`](../../crates/sfes-snes-cpu/src/instructions/arithmetic.rs).

| Instruction | Operation (Table 5-5) | Width from | Flags | Modes (opcodes) |
| --- | --- | --- | --- | --- |
| ADC | A+M+C → A | M flag | N V Z C | the 15 modes of ORA at 6x: `#` 69, `a` 6D, `al` 6F, `d` 65, `(d)` 72, `[d]` 67, `a,x` 7D, `al,x` 7F, `a,y` 79, `d,x` 75, `(d,x)` 61, `(d),y` 71, `[d],y` 77, `d,s` 63, `(d,s),y` 73 |
| SBC | A – M – (C) → A | M flag | N V Z C | the same modes at Ex: E9, ED, EF, E5, F2, E7, FD, FF, F9, F5, E1, F1, F7, E3, F3 |
| CMP | A-M | M flag | N Z C | the same modes at Cx: C9, CD, CF, C5, D2, C7, DD, DF, D9, D5, C1, D1, D7, C3, D3 |
| CPX | X-M | X flag | N Z C | `#` E0, `a` EC, `d` E4 |
| CPY | Y-M | X flag | N Z C | `#` C0, `a` CC, `d` C4 |

**Binary mode.** ADC adds the data and the carry. SBC subtracts the data and one more when the carry is clear, which is the same as adding the complement of the data and the carry; C is set when no borrow was needed. V is signed overflow: the two operands of the addition have the same sign and the result does not.

**Compares** subtract without the carry, always in binary, and keep only N, Z and C: C is set when the register is greater than or equal to the data. V and the register are unchanged.

**Decimal mode** (D set) changes ADC and SBC only, and adds no cycles (Table 7-1). The arithmetic is done one 4-bit digit at a time from the lowest: an ADC digit sum above 9 has 6 added and carries; an SBC digit difference below 0 has 6 subtracted and borrows. N, Z and C mean what they mean in binary mode. The documents disagree on V: the datasheet calls it valid in decimal mode, the manual says it is not correct, and neither defines it. The core computes it as in binary mode on the result before the top digit is adjusted, which matches every vector; the same steps applied to hex digits A to F match the vectors too. [ADR 0007](../adr/0007-decimal-mode-arithmetic.md) records the rules and the evidence.

All 51 opcodes pass every test of their vector files in both modes, including the 300,090 decimal-mode ADC and SBC tests.

## Shifts, rotates, increments: ASL, LSR, ROL, ROR, INC, DEC, INX, INY, DEX, DEY

They are in [`instructions/shift.rs`](../../crates/sfes-snes-cpu/src/instructions/shift.rs). One `Modify` enum holds the six operations on a value; the instruction picks where the value lives.

| Instruction | Operation | Flags | Accumulator | `d` | `a` | `d,x` | `a,x` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| ASL | shift left, 0 into bit 0, top bit to C | N Z C | 0A | 06 | 0E | 16 | 1E |
| LSR | shift right, 0 into the top bit, bit 0 to C | N (always 0) Z C | 4A | 46 | 4E | 56 | 5E |
| ROL | rotate left through C | N Z C | 2A | 26 | 2E | 36 | 3E |
| ROR | rotate right through C | N Z C | 6A | 66 | 6E | 76 | 7E |
| INC | add one | N Z | 1A | E6 | EE | F6 | FE |
| DEC | subtract one | N Z | 3A | C6 | CE | D6 | DE |

The "top bit" is bit 7 or bit 15 by the M flag. The accumulator forms change A or C, keeping B in 8-bit mode, and take two cycles: the opcode and one internal cycle at PBR,PC+1 (row 8). The memory forms are read-modify-write (above), so their `a,x` form always spends the indexing cycle (row 6b).

INX, INY, DEX and DEY (E8, C8, CA, 88) add or subtract one at the width the X flag selects and set N and Z; an 8-bit result leaves the register's high byte zero. They are implied instructions of two cycles (row 19a). Table 5-5 prints DEX's operation as "X-1 → A"; the destination is X, as the tutorial (6.1.1.3) says and the vectors confirm.

All 34 opcodes pass every test of their vector files in both modes.

## Branches, jumps, calls and returns

They are in [`instructions/flow.rs`](../../crates/sfes-snes-cpu/src/instructions/flow.rs); the stack pushes and pulls they use are in [`addressing/stack.rs`](../../crates/sfes-snes-cpu/src/addressing/stack.rs).

| Instruction | Opcode | Row | Cycles | Effect |
| --- | --- | --- | --- | --- |
| BPL BMI BVC BVS BRA BCC BCS BNE BEQ | 10 30 50 70 80 90 B0 D0 F0 | 20 | 2, +1 taken, +1 more in emulation mode across a page | PC + signed 8-bit offset |
| BRL | 82 | 21 | 4 | PC + 16-bit offset |
| JMP a | 4C | 1b | 3 | PC = operand |
| JMP al (JML) | 5C | 4b | 4 | PBR:PC = operand |
| JMP (a) | 6C | 3b | 5 | PC = word at 0,AA |
| JML (a) | DC | 3a | 6 | PBR:PC = long at 0,AA |
| JMP (a,x) | 7C | 2a | 6 | PC = word at PBR,AA+X |
| JSR a | 20 | 1c | 6 | push PC-1, PC = operand |
| JSR (a,x) | FC | 2b | 8 | push PC-1, PC = word at PBR,AA+X |
| JSL al | 22 | 4c | 8 | push PBR and PC-1, PBR:PC = operand |
| RTS | 60 | 22h | 6 | pull PC, add one |
| RTL | 6B | 22i | 6 | pull PC and PBR, add one to PC |

The branch conditions are those of Table 5-5. A branch's extra cycles both repeat the address of the offset byte; the page test compares the destination with the address of the next instruction. Every new program counter wraps in its bank; only the long forms change PBR. A call pushes the address of its own last byte, high byte first, and a return adds one to what it pulls. That address wraps in the bank: a JSL at `12FFFD` pushes `12`, `00`, `00`, and an RTL that pulls `FF`, `FF`, `12` continues at `120000` (tutorial 6.2.2.1 and 6.2.2.2).

The pointers of `JMP (a)` and `JML (a)` are in bank 0 and wrap at its end. (`JML (a)` is the datasheet's name for DC; the WDC manual and the 6502.org tutorial write it `JMP [a]`, because its pointer is 24 bits.) The `(a,x)` pointer is in the program bank and wraps there. Table 5-7 marks its two reads with VPA, but the vectors record them as data cycles (VDA), and the core follows the vectors.

**The stack in emulation mode.** S is in page 1, and a push or pull that runs past the page edge wraps inside the page, except in the instructions that datasheet 7.1 lists. Of those, JSL, JSR (a,x) and RTL are here: they step S as a 16-bit value, so a JSL with S at `0100` writes `0100`, `00FF` and `00FE`, and when they end the high byte of S goes back to `01`. [ADR 0008](../adr/0008-emulation-stack-in-calls-and-indexed-jump-pointers.md) records both points.

All 20 opcodes pass every test of their vector files in both modes.

## Stack: pushes, pulls, PEA, PEI, PER

They are in [`instructions/stack.rs`](../../crates/sfes-snes-cpu/src/instructions/stack.rs).

| Instruction | Opcode | Row | Cycles | Effect |
| --- | --- | --- | --- | --- |
| PHA | 48 | 22c | 3, +1 if m = 0 | push A or C |
| PHX PHY | DA 5A | 22c | 3, +1 if x = 0 | push X or Y |
| PHP PHB PHK | 08 8B 4B | 22c | 3 | push P, DBR or PBR (one byte) |
| PHD | 0B | 22c | 4 | push D (two bytes) |
| PLA | 68 | 22b | 4, +1 if m = 0 | pull A or C; N, Z |
| PLX PLY | FA 7A | 22b | 4, +1 if x = 0 | pull X or Y; N, Z |
| PLP | 28 | 22b | 4 | pull P |
| PLB | AB | 22b | 4 | pull DBR; N, Z |
| PLD | 2B | 22b | 5 | pull D (two bytes); N, Z |
| PEA | F4 | 22d | 5 | push the 16-bit operand |
| PEI | D4 | 22e | 6, +1 if DL ≠ 0 | push the 16-bit value at 0,D+DO |
| PER | 62 | 22f | 6 | push PC + 16-bit offset |

A push writes the high byte at 0,S and the low byte below it; a pull reads the low byte first. Pushes spend one internal cycle and pulls two, all at the address after the opcode. PHP and PLP are always one byte (note 12, datasheet 7.20), and PHD, PLD, PEA, PEI and PER always two, whatever the M and X flags. PLP goes through the same path as any change of P: in emulation mode M and X stay set, and a set X flag clears the high bytes of X and Y. PHP in emulation mode pushes the B bit as 1, since bit 4 is the X flag there.

PEA reads no memory at its operand, and PEI reads the 16-bit value at the direct address rather than the data it points to (tutorial 6.8.1). PEI's value runs on into the next page even in emulation mode with DL = 0, as the `[d]` pointer does (datasheet 7.2.1); vector `d4 e 232` reads `000CFF` and `000D00`. PER adds its offset to the address of the next instruction, wrapping in 16 bits, after an internal cycle at the offset's high byte.

**The stack in emulation mode.** PEA, PEI, PER, PHD and PLD are in datasheet 7.1's list and step S as a 16-bit value, as JSL does (ADR 0008): PHD with S at `0100` writes `0100` and `00FF`, and S ends at `01FE`. PLB is not in the list, but the vectors show it reading `000200` when S is `01FF`, and the core follows them ([ADR 0009](../adr/0009-emulation-stack-in-plb.md)). The other pushes and pulls wrap inside page 1.

All 16 opcodes pass every test of their vector files in both modes.

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
