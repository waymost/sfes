# 0019. Bus decode, open bus, the WRAM port, refresh and read-modify-write writes on the SNES bus

- **Status:** accepted
- **Date:** 2026-11-20
- **Confidence:** high for decode, access speed and the WRAM port; medium for the refresh schedule, open bus on internal-register writes and the WRAM port's wrap; medium for read-modify-write writes to I/O registers

## Context

Issue #80 replaces the provisional bus of ADR 0018, point 6, with the main CPU's `BusView`: A-bus and B-bus decode, access speed with `$420D MEMSEL`, DRAM refresh, open bus, the WRAM port `$2180–$2183` and the owner-sync hooks of the technical design (section 4, catch-up rule 1). The documents leave these questions open:
- what the CPU's internal registers do to the data bus;
- where the WRAM port's address goes after `$1FFFF`;
- exactly when the refresh pause falls against CPU cycles;
- what a register with side effects does with the two writes of an emulation-mode read-modify-write instruction. ADR 0006 settled the bus cycles and left this question to M2.

## Sources

- `anomie-memmap#memory-map`:
  - WRAM answers Address Bus A while /WRAM is active, and B-bus registers `$80–$83`. PPU1 and PPU2 answer B-bus `$00–$3F`, and the APU answers `$40–$7F`. The cartridge may answer "any address on either bus that is not otherwise mapped".
  - The table's speed column gives these costs in banks `$00–$3F` and `$80–$BF`:
    - Slow for `$0000–$1FFF` (/WRAM) and `$6000–$7FFF`;
    - Fast for `$2000–$20FF`, `$2200–$3FFF` and `$4400–$5FFF` (Address Bus A), and for `$2100–$21FF` (Address Bus B);
    - XSlow for `$4000–$41FF` and Fast for `$4200–$43FF` (internal CPU registers);
    - Slow for `$8000–$FFFF` in `$00–$3F`.
  - The other areas: Slow for `$40–$7F`. Note 2 for `$80–$BF:8000–FFFF` and `$C0–$FF`: "If bit 0 of CPU register $420d is set, the speed is Fast, otherwise it is Slow".
  - An internal cycle is 6 master cycles; a Fast, Slow or XSlow access 6, 8 or 12.
  - Note 1: the CPU ignores the data bus for its internal registers. "It is also unknown whether CPU writes show up on the data bus or not. Current theory is that addresses and writes will show up, but reads may or may not."
- `anomie-memmap#hardware/data-bus`: a read that nothing answers returns "the last value placed on the bus" (open bus).
- `anomie-memmap#hardware/address-bus-b`: the 8-bit B bus, with WRAM, PPU1, PPU2 and the APU on it.
- `anomie-regs` (preamble): most registers, and most undefined bits of readable ones, read as open bus. The PPU1 and PPU2 open-bus rules apply to `$2134–$213F`.
- `anomie-regs#2180`: WMDATA reads or writes the WRAM byte at the address in `$2181–$2183`, "The address is then incremented". Mixed reads and writes are "suspected" to be "handled logically".
- `anomie-regs#2181`: a 17-bit address (`-------x xxxxxxxx xxxxxxxx`); the registers "are open bus" on reads; the value "does not wrap at page boundaries on increment".
- `anomie-regs#420d`: FastROM select. Accesses through banks `$80–$FF` take 6 master cycles when it is set. "This register is initialized to $00 on power on (or reset?)".
- `anomie-timing#s-cpu/clocks-and-refresh`:
  - A scanline is 1364 master cycles.
  - "The CPU is paused for 40 cycles beginning about 536 cycles after the start of each scanline."
  - "The exact timing is that the refresh pause begins at 538 cycles into the first scanline of the first frame, and thereafter some multiple of 8 cycles after the previous pause that comes closest to 536."
- `anomie-timing#s-cpu/instructions`: an instruction's length is the sum of 6, 8 or 12 master cycles per CPU cycle.
- `fullsnes#snesmemorycontrol`: `$420D` bit 0 is "0 on reset". It selects the speed of "Memory-2": `$80–$BF:8000–FFFF` and `$C0–$FF:0000–FFFF`.
- `fullsnes#snesmemoryworkramaccess`: WMDATA reads return "the currently addressed byte, even if one mixes it with writes", with no prefetch. The address is 17 bits.
- `fullsnes#snesiomap`:
  - `$2181–$2183` and `$420D` are `00h` on reset.
  - `$2184–$21FF`, `$2200–$3FFF`, `$4000–$4015`, `$4018–$41FF`, `$420E–$420F`, `$4220–$42FF` and `$4380–$5FFF` are unused (open bus).
  - The write-only CPU ports read as open bus.
- `fullsnes#snestiminghvevents`:
  - The SNES starts at H=0, V=0 after /RESET.
  - "around here, REFRESH begins (duration 40 clks/10 dots)" at H=133.5, which is 534 master cycles.
  - "Opcodes may begin before Refresh, and end after Refresh (rather than forcefully finishing the Opcode before starting Refresh)".
- `fullsnes#snestimingoscillators`: "The CPU is paused for 40 master cycles (per 1364 cycle scanline) for memory REFRESH purposes".
- ADR 0004: one `read`, `write` or `idle` call per CPU cycle.
- ADR 0006: in emulation mode a read-modify-write instruction's modify cycle is a `write` of the unmodified value, followed by the write of the result.
- ADR 0018: `Cycles(0)` is the moment RESB rises; reset restarts the line and frame position.

## Decision

1. **Decode.** An address is looked up in the cartridge's `MemoryMap` (ADR 0017). The console's I/O pages (`$2000–$5FFF` in banks `$00–$3F` and `$80–$BF`) are decoded further:
   - `$2100–$21FF` puts its low byte on the B bus:
     - `$00–$3F` is the PPU;
     - `$40–$7F` is the APU;
     - `$80–$83` is the WRAM port;
     - anything else is unanswered.
   - `$4000–$43FF` holds the CPU's internal registers.
   - The rest of the I/O pages are Address Bus A, which no chip on the console answers.

   Until a chip answers its registers, they read as open bus and ignore writes. (The APU ports answer from a placeholder until M3: ADR 0027.)
2. **Owner sync.** Before every access to B-bus `$00–$3F` the bus calls the PPU's sync hook, and before `$40–$7F` the APU's (technical design, catch-up rule 1). The hooks do nothing until those chips exist.
3. **Access speed.** Each access costs anomie-memmap's speed for its address, and an internal cycle 6. Memory-2 is Fast while `$420D` bit 0 is set. An access's cost is fixed when it starts, so a write to `$420D` changes the speed from the next access. `$420D` is write-only, reads as open bus, and is 0 at power-on and after reset.
4. **Open bus.**
   - A read that something answers places its value on the data bus.
   - A read that nothing answers, or that hits a write-only register, returns the data bus value unchanged.
   - Every write places its value on the data bus, including writes to the CPU's internal registers. This follows the "current theory" of anomie-memmap's note 1.
   - PPU1 and PPU2 open bus come with the PPU.
5. **The WRAM port.**
   - `$2181`, `$2182` and `$2183` write the low, middle and top bits of a 17-bit address. Only bit 0 of `$2183` is kept, and reads of the three return open bus.
   - A read or write of `$2180` accesses WRAM at that address, then adds 1 to it.
   - The address wraps from `$1FFFF` to `$00000`. The documents give a 17-bit address and no rule for its carry, so the carry out of bit 16 is dropped.
   - The address is 0 at power-on and after reset.
6. **Refresh.**
   - The first refresh is due 538 master cycles after the start of the first line. That line starts at power-on, or at a reset (ADR 0018).
   - Each later refresh is due in the next line, at the offset that is a multiple of 8 cycles after the previous due time and closest to 536. With 1364-cycle lines the offsets alternate between 538 and 534.
   - The CPU cycle that is running when a refresh falls due completes. The CPU is then paused for 40 master cycles, and its next cycle begins after the pause.
   - The schedule counts from the due times, not from when the pauses began, so the CPU's cycle lengths never move it.
7. **Read-modify-write writes to I/O registers.** The two writes of an emulation-mode read-modify-write instruction reach the bus as two ordinary writes, in order: first the unmodified value, then the result. A register with side effects has them on each write, and the bus does nothing special for the pair. For example, `INC $2180` in emulation mode:
   - reads WRAM at address *a*;
   - writes the value read to *a*+1;
   - writes the result to *a*+2;
   - leaves the address at *a*+3.

## Consequences

- ADR 0018, point 6 is amended: access speed, open bus and `$420D` now follow this ADR. The fixed line lengths, the frame boundary and the absence of NMI and IRQ stay until #81 and #82.
- The refresh schedule takes the line length as a parameter, so the short line of #81 changes the offsets without changing the rule.
- A refresh pause shows in the time of the next access, so observers see the gap between two `on_read` or `on_write` times.
- Point 6 rests on anomie-timing alone. fullsnes gives H=133.5 (534) and a "stuttering" refresh that it cannot explain, which this ADR does not model. A test ROM that measures where the CPU stops across a refresh reopens point 6.
- Point 7 inherits ADR 0006's medium confidence on the value of the first write. A hardware test of `INC $2180` in emulation mode would settle both ADRs.
- Point 4 for internal-register writes and point 5's wrap are reopened by any measurement.
- The DMA rules of `$2180–$2183` (anomie-regs) belong to #83.
