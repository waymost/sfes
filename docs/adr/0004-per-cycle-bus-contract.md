# 0004. Per-cycle bus contract and step granularity for the 65C816

- **Status:** accepted
- **Date:** 2026-11-12
- **Confidence:** high for the bus contract and the block-move step; medium for interrupt sampling and for the waiting and stopped steps

## Context

EMU-02 requires every opcode to match the SingleStepTests 65816 vectors, including the bus record of every cycle. Each recorded cycle holds an address, a value and eight output signals, and cycles that transfer no data are recorded too. The `Bus65816` sketch in the technical design (section 3) cannot express that: its reads carry a four-way `Access` kind, its writes carry nothing, and `idle()` has no address.

The design also says only that `step` "executes one instruction or interrupt". Before the core is written we need to fix what one `step` covers for block moves and for the waiting and stopped states, and when `step` looks at the interrupt lines.

## Sources

### Output signals (`wdc-w65c816s`)

- **VDA and VPA** (`#2.26`): four combinations. 0/0 is an internal operation, and "the Address Bus may be invalid"; 0/1 is a valid program address; 1/0 a valid data address; 1/1 an opcode fetch.
- **RWB** (`#2.23`): high when the processor reads, low when the data bus holds data to be written.
- **VPB** (`#2.28`): active low, low "during the last two interrupt sequence cycles" while the vector is read; it "may be used to select and prioritize interrupts".
- **MLB** (`#2.19`): active low, low during the last three or five cycles of the read-modify-write instructions ASL, DEC, INC, LSR, ROL, ROR, TRB and TSB, depending on the M flag.
- **E** (`#2.17`) reflects the emulation flag. **MX** (`#2.20`) carries the M flag on one clock phase and the X flag on the other.
- **Every cycle has an address.** Table 5-7 (`#table-5-7`) lists an Address Bus entry for every cycle of every row, including the internal-operation cycles whose data bus entry is "IO".
- **Emulation-mode read-modify-write** (`#table-5-7-note-17`): "RWB is low during both write and modify cycles".

### Interrupt inputs (`wdc-w65c816s`)

- **IRQB** (`#2.18`): level sensitive. With the I flag clear, a low level "initiates an interrupt sequence after the current instruction is completed". No interrupt occurs if the source is cleared before recognition.
- **NMIB** (`#2.21`): edge sensitive. A high-to-low transition initiates the sequence "after the current instruction is completed"; no further interrupt occurs while the line stays low.
- **Priorities** (`#7.19`): RESB, then ABORTB, then NMIB, then IRQB.
- **Hardware interrupt sequence** (`#table-5-7-22a`): a row of its own, "0 bytes, 7 and 8 cycles", ending with the two vector cycles.
- **Reset** (`#2.25`): "When Reset is brought high, an interrupt sequence is initiated"; STP and WAI are cleared.
- The datasheet does not say at which point of an instruction's final cycle the input lines are sampled.

### Block moves

- `wdc-w65c816s#table-5-7-9a` and `#table-5-7-9b`: MVN and MVP are "3 bytes, 7 cycles". `#3.5.9`: X and Y step, C decrements, "then PC=PC+3".
- `wdc-65816-manual#chapter-18-mvn`: "If a block move instruction is interrupted, it may be resumed automatically via execution of an RTI"; "The value pushed onto the stack when a block move is interrupted is the address of the block move instruction. The current byte-move is completed before the interrupt is serviced."
- `6502org-65c816-opcodes#6.6`: "Each byte moved takes 7 cycles"; the instructions "can only be interrupted every seventh cycle".
- `sst-65816#54.n` and `#54.e`: one test covers the whole move. Its cycle count is always a multiple of 7, and every group of 7 starts with a fresh opcode fetch and two operand fetches at the same program counter.

The three documents and the vectors agree.

### Waiting and stopped states

- **WAI** (`wdc-w65c816s#table-5-7-19d`, `#7.13`, `#2.24`): three cycles, after which RDY is pulled low and the processor halts until NMIB, IRQB or RESB. With the I flag set, an IRQB makes "the next instruction (following the WAI instruction)" execute without entering the handler.
- **STP** (`wdc-w65c816s#table-5-7-19c`, `#7.14`): three cycles, after which the clock to the internal circuitry is disabled; "the RESB signal is the only input which can restart the processor".
- `sst-65816#cb.n`, `#db.n` (and the `.e` files): each test has three bus cycles followed by a fourth record with no address, no value and no signals (`[null, null, "--------"]`), and the final program counter is one past the opcode.

### Vector format (`sst-65816`)

The set's README defines the signal string `dpvremxl` (VDA, VPA, VPB, RWB, E, M, X, MLB; a letter when active, `r` or `w` for RWB) and says that a cycle with none of VDA, VPA and VPB active and RWB reading is recorded with a `null` value. Sampling `0e.e` shows cycles with VDA and VPA inactive and RWB writing (`---w`), with a value: the emulation-mode modify cycle of `#table-5-7-note-17`.

## Decision

### Bus trait

```rust
pub trait Bus65816 {
    fn read(&mut self, address: Addr24, pins: Pins) -> u8;
    fn write(&mut self, address: Addr24, value: u8, pins: Pins);
    fn idle(&mut self, address: Addr24, pins: Pins);
    fn interrupts(&self) -> Pending;
}
```

- **One call per cycle.** The core makes exactly one call to `read`, `write` or `idle` for every clock cycle it executes, in order. The bus is the only clock: each call advances time by whatever that cycle costs on the machine.
- **Which method.** RWB low is a `write`, whatever VDA and VPA say. RWB high with VDA or VPA active is a `read`. RWB high with both inactive is an `idle`: the address is passed, but the bus must not perform a memory access, and no value comes back.
- **`Pins`** is a `bitflags` type over `u8` with one flag per signal of the vector string: `VDA`, `VPA`, `VPB`, `RWB`, `E`, `M`, `X`, `MLB`. A flag is set when the vector format calls the signal active, not by pin voltage: `VPB` is set during the vector cycles and `MLB` during the locked cycles, though both pins are active low; `RWB` is set for a read. `RWB` is therefore always set in `read` and `idle` and always clear in `write`. It is kept so that `Pins` is the complete record of a cycle.
- **`Access` is removed.** `Pins` carries the same information in the VDA and VPA flags.
- **Cost.** The bus is always a concrete type behind `&mut impl Bus65816`, so the calls inline. An implementation that ignores `pins`, or the address of an `idle`, has them removed by the optimizer. `sfes-snes-core` may use `VPB` (see below) and ignore the rest.

### What one `step` covers

`step(&mut self, bus: &mut impl Bus65816)` does exactly one of the following, chosen in this order.

1. **Stopped.** One `idle` call and nothing else. Only `Cpu65816::reset` leaves this state (`#7.14`).
2. **Waiting.** `step` reads `interrupts()`. With nothing pending it makes one `idle` call and stays waiting. With an NMI pending, or an IRQ pending and the I flag clear, it leaves the waiting state and performs the interrupt entry below. With an IRQ pending and the I flag set, it leaves the waiting state and executes the next instruction (`#7.13`).
3. **Interrupt entry.** If the state is running and `interrupts()` reports an NMI, or an IRQ with the I flag clear, `step` performs the hardware interrupt sequence of `#table-5-7-22a` and executes no instruction. NMI wins over IRQ (`#7.19`).
4. **One instruction**, from its opcode fetch to its last cycle as listed in its Table 5-7 row. The next instruction's opcode fetch, which the table prints as a final cycle 1, belongs to the next `step`.

**Block moves.** One `step` of MVN or MVP is one 7-cycle iteration that moves one byte. While bytes remain, the program counter is left on the opcode, so the next `step` fetches the opcode and operands again and an interrupt can be taken between iterations with the instruction's own address as the return address. After the last byte the program counter moves past the instruction.

**Reset** is not delivered through `Pending`. `Cpu65816::reset(&mut self, bus: &mut impl Bus65816)` performs the reset sequence (`#2.25`, `#table-5-7-22a`) from any state.

**ABORTB** is not modelled: `Pending` has no field for it. Neither machine planned for this core raises it.

The idle cycles of the stopped and waiting states are a modelling choice, not a claim about the pins. On hardware the processor is halted; the emulated machine around it still needs time to pass, and the bus is the only clock. Their address and `Pins` values are fixed in the interrupts issue (#42) from `#table-5-7-19c` and `#table-5-7-19d`.

### When `interrupts()` is sampled

`step` calls `interrupts()` once, before the opcode fetch (cases 2 and 3 above). The core never samples in the middle of an instruction. This is the datasheet's "after the current instruction is completed", and for block moves the documents' "between moving one byte and moving the next".

`Pending` reports what the processor has recognized, not the raw lines:

- `irq` is the level of the IRQ line as the processor would see it at this instruction boundary.
- `nmi` is an edge latch that the bus owns. The bus sets it on a high-to-low transition and clears it when it sees a `read` with `Pins::VPB` at the NMI vector address. No acknowledge method is needed.

How late in an instruction a line may change and still be recognized at the next boundary is therefore decided inside the bus implementation, not in the CPU core. The datasheet is silent on it; `sfes-snes-core` settles it in M2 with its own ADR and timing tests.

### Vector harness

- `RecordingBus` records `(address, value, pins)` for every call, with no value for `idle`, and the runner compares all three fields of every cycle against the vector, all eight signals included.
- A vector record with no address is not a bus cycle. The runner checks instead that the core is waiting or stopped after the `step`.
- For the block-move files the runner calls `step` once per byte (the initial C plus one); for every other file, once.
- The vectors are evidence, not a specification. Their generator is undocumented, so a disagreement between a vector and the datasheet gets its own ADR.
- CI runs the full set on Linux and the first 500 tests of each file on macOS and Windows, as agreed for M1 (issue #25).

## Consequences

- The M0 stubs in `sfes-snes-cpu` change: `Access` goes, `Pins` arrives, and all three cycle methods take an address and pins. This lands with the CPU skeleton (#30). Nothing else implements the trait yet.
- The technical design is updated in this PR: section 3 (trait sketch), section 5 (`step`, `RecordingBus`) and section 13 (vector test layer).
- `Observer::on_instruction` (wired in `sfes-snes-core` at M2) fires once per `step`, so a long block move produces one event per byte, and a waiting CPU one event per idle cycle unless the core filters on `RunState`.
- A main loop that runs "until the next event" needs no special case for WAI or STP: each `step` still consumes one cycle of time.
- The NMI latch lives in the bus, so it is part of bus state for cloning and serialization, and the SA-1's bus needs its own.
- We revisit this ADR if a timing test ROM shows behaviour that sampling once per instruction boundary cannot reproduce, such as an instruction that delays recognition of a changed I flag, or if the WAI or STP rows in #42 need more than one kind of idle cycle.
