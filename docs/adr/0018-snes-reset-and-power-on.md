# 0018. Reset, power-on and observer events of the SNES machine

- **Status:** accepted
- **Date:** 2026-11-20
- **Confidence:** medium for the reset timing and the default region; high for the page-1 reads, the seeded WRAM and the observer events

## Context

Issue #79 builds the `Snes` machine around the 65C816 core. M1 left the machine three questions (`docs/chips/cpu.md`, "Notes for M2"; ADRs 0004 and 0011):
- How long is RESB held low before `Cpu65816::reset` runs?
- Do the three page-1 stack reads of the reset sequence have side effects on this bus?
- How do the CPU's steps become `on_instruction` and `on_interrupt` events, given the core calls no observer and keeps its interrupt kinds private?

The machine must also say what power-on fills with the seed (FND-02) and which region it runs when neither the header nor the caller names one (ADR 0016 left that to this issue). The bus, the scheduler and the interrupt registers arrive in #80, #81 and #82. This ADR also records what the machine does until they do.

## Sources

- `anomie-timing` (introduction): the SNES "returns to a known timing position on reset". A deterministic ROM therefore gives the same results after every reset.
- `anomie-timing#s-cpu/dma`: DMA aligns to "a multiple of 8 master cycles since reset".
- `anomie-timing#s-cpu/clocks-and-refresh`: a scanline is 1364 master cycles and a frame 262 scanlines without interlace. A memory access takes 6, 8 or 12 master cycles by region and `$420D` bit 0; an internal cycle takes 6.
- `anomie-memmap#memory-map`:
  - In banks `$00–$3F` and `$80–$BF`, `$0000–$1FFF` mirrors `$7E:0000–$1FFF`, the first 8 KiB of WRAM. Page 1 of bank `$00` is WRAM.
  - The speed column gives Slow (8) for WRAM, `$6000–$7FFF`, ROM and banks `$40–$7F`; Fast (6) for `$2000–$3FFF` and `$4200–$5FFF`; XSlow (12) for `$4000–$41FF`. Banks `$80–$BF:8000–FFFF` and `$C0–$FF` follow `$420D` bit 0, and are Slow when it is clear.
- `anomie-memmap#hardware/data-bus`: when nothing drives the data bus, a read returns the last value placed on it ("Open Bus").
- `fullsnes#snesiomap`: `$420D` bit 0 is 0 on reset. Values in brackets are left unchanged by reset but hold the given value at power-up.
- `fullsnes#snescontrollersotherinputs`: the reset button "Resets various CPU/PPU/APU registers, but leaves WRAM intact".
- `fullsnes#snestimingoscillators` and `fullsnes#snestiminghvcounters`: 1364-cycle scanlines; PAL frames are 312 scanlines, NTSC 262. Short and long lines depend on interlace and the field.
- `wdc-w65c816s#2.25`: while RESB is low the processor holds its reset values; when RESB rises the reset sequence runs. No document gives how long the SNES holds RESB, at power-on or for the reset button.
- ADR 0004: `step` reads `interrupts()` once, before the opcode fetch. It takes the interrupt sequence if an NMI is latched or the IRQ line is asserted with I clear. A waiting core with only an IRQ and I set resumes with the next instruction; a stopped core spends an idle cycle.
- ADR 0011: reset reads 0,S, 0,S−1 and 0,S−2 in page 1 without writing, after a discarded read of PBR,PC with PBR `00`. The cycles in which RESB is held are the machine's to spend.
- `vigna-splitmix` and FND-02: the generator and the seed-0 default.
- `fullsnes#snescartridgeromheader` and ADR 0016: country codes that imply no region.

## Decision

1. **RESB rises at time 0.** No document gives the hold time, so the machine spends none. `Cycles(0)` is the moment RESB rises after power-on, and the reset sequence's seven cycles are the first ones charged. The reset button works the same way: `reset` runs the sequence at once and restarts the timing position (line and frame) at that moment, as anomie-timing's "known timing position on reset" requires. `Cycles` keeps counting since power-on, as `sfes_api::Cycles` defines it.
2. **The page-1 reads have no side effects.** `$00:0100–$01FF` is WRAM (`anomie-memmap#memory-map`), whatever S holds. They go through the bus as ordinary reads, so observers see them as `on_read`. The discarded read of `$00`,PC also goes through the bus. After the reset button that address is the old PC with PBR `00`, and a register with read side effects there would see the read. After power-on PC is 0, which is WRAM.
3. **Reset and power-on.**
   - Power-on fills WRAM from `SplitMix64::new(seed)`, eight bytes per output, little-endian, from `$7E:0000` up. No document gives WRAM's power-on contents, and FND-02 decides they come from the seed.
   - Other state that hardware leaves undefined stays as its owner defines it until a source or an ADR changes it. The CPU's uninitialized registers are zero (ADR 0011), and cartridge RAM is zero (ADR 0017).
   - The reset button keeps WRAM, cartridge RAM and the data bus value, and resets the CPU. Registers that reset changes are reset as their issues add them.
4. **The default region is NTSC.** It applies when the header's country code implies none (ADR 0016) and the builder names none. NTSC is the more common standard among the documented country codes. An explicit region always wins (EMU-09).
5. **Observer events.** Before each `step` the machine works out which of ADR 0004's cases the step will take, from the CPU's run state and I flag and the bus's `Pending`:
   - an instruction step (including each byte of a block move) calls `on_instruction` once, with the program address, the opcode and the time its fetch began;
   - an interrupt entry calls `on_interrupt` with `Interrupt::Nmi` when an NMI is latched, else `Interrupt::Irq`, at the time the step began. This is the mapping from the core's NMI and IRQ kinds; BRK and COP are instructions and reset is not an interrupt event;
   - a halted step, waiting or stopped, calls neither;
   - every `read` and `write` calls `on_read` or `on_write` on `BusId::Cpu`, with the time the access began. Idle cycles call nothing.
6. **Until #80, #81 and #82 land:** (amended by ADR 0019, which replaces the first two items: access speed with `$420D`, refresh, decode and open bus; and by ADR 0020, which replaces the third: lines, frames and the frame boundary)
   - Each access costs anomie-memmap's speed for its address, with `$420D` bit 0 at its reset value, 0. Idle cycles cost 6.
   - Reads that nothing answers return the last value on the data bus, and so do the I/O pages. Writes to them do nothing.
   - Lines are 1364 master cycles and frames 262 or 312 lines, with no short or long lines. The frame boundary is the end of the frame's last line.
   - `Stop::Instructions(n)` counts steps. `Stop::Watch` runs its frame limit and returns `StopReason::FrameLimit`, since no watch exists yet.
   - Nothing sets the NMI latch or the IRQ line.

## Consequences

- A test ROM that measures the time from reset to the first opcode fetch reopens point 1.
- Point 2 holds as long as nothing but WRAM answers page 1 of bank `$00`; the memory map makes that true for every board.
- The provisional timing of point 6 is replaced, item by item, by #80 (access speed, `$420D`, refresh, open bus details), #81 (H/V position, short and long lines, the frame boundary at vertical blank) and #82 (NMI and IRQ). Each of those issues amends this ADR's point 6 as it lands.
- Instruction events from block moves arrive once per byte, as ADR 0004 foresaw.
