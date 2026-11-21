# 0021. NMI, IRQ, the H/V timer and the counter latch on the SNES

- **Status:** accepted
- **Date:** 2026-11-20
- **Confidence:**
  - high: NMI and IRQ trigger points, and the register bits and reset values;
  - medium: when a changed line is seen, and the point in an access at which a register read sees the machine;
  - low: the PPU2 open bus of `$213C`/`$213D` until the PPU exists.

## Context

Issue #82 adds the interrupt sources of the 5A22 to the machine:
- `$4200 NMITIMEN`;
- `$4207–$420A` HTIME and VTIME;
- `$4210 RDNMI`, `$4211 TIMEUP` and `$4212 HVBJOY`;
- the H/V counter latch read at `$213C`/`$213D`, which `$2137 SLHV` and `$4201` bit 7 trigger.

ADR 0004 left one question to the machine: how late in an instruction a changed NMI or IRQ line is still seen at the next instruction boundary. That ADR gives the bus an NMI edge latch and has the CPU read `interrupts()` once per step.

The documents agree on most trigger points. They leave these questions open or disagree:
- the point in a CPU access at which a register read sees the machine;
- whether a write to `$4211` acknowledges the IRQ;
- `$4201`'s value after reset;
- the PPU2 open bus bits of the counter registers before the PPU exists.

## Sources

- `anomie-timing` (preamble): a bit set "at H=X" is seen by a read "that would latch X were it reading $2137" and not by one that would latch X−.5. A `$4201` latch "will latch 1 dot later than if the same memory access cycle were reading $2137".
- `anomie-timing#s-cpu/interrupts`:
  - The timer sets its NMI output low "at H=0.5 at the beginning of V-Blank". It sets it high "at H=0 V=0, or when register $4210 is read. Possibly also when $4200 is written?" Clearing `$4200` bit 7 forces /NMI high.
  - IRQ trigger points:
    - yx=01, H-IRQ: "every scanline, H=HTIME+~3.5";
    - yx=10, V-IRQ: "V=VTIME, H=~2.5";
    - yx=11, HV-IRQ: "V=VTIME, H=HTIME+~3.5".
  - "If H=0, $4211 bit 7 gets set 1374 master cycles after dot 0.0 of the previous scanline. Otherwise, it gets set 14+H*4 master cycles after dot 0.0 of the current scanline."
  - "no IRQ will trigger for dot 153 on the short scanline in non-interlace mode, and no IRQ will trigger for dot 153 on the last scanline of any frame".
  - The IRQ output goes high "when $4211 is read, or when IRQs are disabled by a write to $4200". An enable write in "the exact cycle when the IRQ is scheduled to trigger" still sets it.
  - "The actual check occurs just before the final CPU cycle of the instruction", so the jump begins "at the earliest 6 to 12 master cycles after /NMI or /IRQ".
  - PLP, CLI, SEI, SEP #$04 and REP #$04 change I in their final cycle, so the check uses the old I.
  - "If /NMI and /IRQ are both pending, NMI takes precedence."
  - A DMA delays the NMI until after the transfer.
- `anomie-timing#s-cpu/instructions`: WAI "takes 12 master cycles (2 IO cycles) to end".
- `anomie-timing#s-cpu/registers`:
  - `$4210` bit 7 shows the NMI output, and reading it sets the output high.
  - `$4211` bit 7 shows the IRQ output, and reading it sets the output high.
  - `$4212` bit 6 is "Set at H=274 of every scanline, and cleared at H=1". Bit 7 is "Set at H=0 at the beginning of V-Blank, and cleared at H=0 V=0".
- `anomie-timing#s-ppu/blanking-periods`: "H-Blank begins at H=274 of every scanline, and ends at H=1."
- `anomie-timing#s-ppu/registers`: `$2137` is read "to latch the PPU dot position". `$213F` bit 6 is set when the counter is latched and "cleared on read".
- `anomie-regs#4200`:
  - n enables the NMI;
  - x and y (bits 4 and 5) select the IRQ;
  - a (bit 0) is auto-joypad read;
  - "initialized to $00 on power on or reset".
- `anomie-regs#4201`:
  - Bit 7 is "connected to ... the PPU Latch line"; "the latch happens on the transition to 0", and "When bit 'a' is 0, no latching can occur."
  - "initialized as if this register were written with all 1-bits at power up, unchanged on reset(?)".
- `anomie-regs#4207`, `#4208`, `#4209`, `#420a`:
  - HTIME and VTIME are 9 bits;
  - an HTIME over 339, or a VTIME past the frame's last line, means "no IRQ firing";
  - both are "$1ff on power on, unchanged on reset".
- `anomie-regs#4210`:
  - The NMI flag "is not affected by bit 7 of $4200".
  - Bits 3–0 are the 5A22 version: "at least 2, maybe 1".
  - "The '-' bits are open bus."
- `anomie-regs#4211`: the IRQ flag is "cleared on read or write", and bits 6–0 are open bus.
- `anomie-regs#4212`:
  - v and h are the blanking flags, at approximate H values that `anomie-timing` refines;
  - a is the auto-joypad busy flag;
  - the other bits are open bus (`fullsnes#snesppuinterrupts`: "Not used").
- `anomie-regs#2137`: a read latches the counters "if bit 7 of $4201 is set". "The data actually read is open bus."
- `anomie-regs#213c`, `#213d`:
  - The counters are 9 bits, "bits 1-7 of the high byte are PPU2 Open Bus".
  - "Each register keeps seperate track of whether to return the low or high byte", reset to low by a `$213F` read and "NOT reset when the counter is latched".
- `fullsnes#snesppuinterrupts`:
  - "Disabling IRQs (via bit4-5) does additionally acknowledge IRQs. There's no such effect when disabling NMIs".
  - The NMI flag is set at the beginning of vertical blank "even if NMIs are disabled".
  - The CPU's internal NMI flag "gets set when "[4200h].7 AND [4210h].7" changes from 0-to-1, and gets cleared when the NMI gets executed".
  - `$4210` version "version 2 exists". `$4211` is listed as read-only.
  - A `$4211` read "at the very time when the IRQ condition is true (which lasts for 4-8 master cycles)" returns 1 without clearing the flag.
  - HTIME is "0..339 (+/-1 in long/short lines)".
- `fullsnes#snespputimersandstatus`:
  - Three things latch the counters "only if WRIO.Bit7 is (or was) set": a `$2137` read, a 1-to-0 change of `$4201` bit 7, and the lightgun.
  - Each latch sets `$213F` bit 6.
  - The second read of `$213C`/`$213D` returns "Upper 1bit (other 7bit PPU2 open bus; last value read from PPU2)".
- `fullsnes#snestiminghvevents`:
  - "H=0, V=225 set Vblank flag", "H=0.5, V=225 set NMI flag", "H=0, V=0 clear Vblank flag, and reset NMI flag".
  - "H=HTIME+3.5 H-IRQ", "H=2.5, V=VTIME V-IRQ (or HV-IRQ with HTIME=0)".
  - "H=274 set hblank flag", "H=1 clear hblank flag".
- `fullsnes#snesiomap`:
  - On reset `$4200` is 00h, `$4201` FFh, `$4210` bit 7 0 and `$4211` 00h.
  - HTIME and VTIME are 1FFh and OPHCT and OPVCT 01FFh, in brackets: "values in brackets are left unchanged upon Reset (but do contain the specified value upon initial Power-up)".
- `wdc-w65c816s#2.18`: IRQB is level sensitive, "no interrupt will occur if the interrupt source is cleared prior to interrupt recognition".
- `wdc-w65c816s#2.21`: a high-to-low NMIB transition "initiates an interrupt sequence after the current instruction is completed".
- `wdc-w65c816s#7.19`: NMIB has priority over IRQB.
- ADR 0004 (when `interrupts()` is sampled), ADR 0019 (access speed, open bus), ADR 0020 (lines, dots, vertical blank, the deadline table).

## Decision

1. **When a changed line is seen.**
   - At the start of every CPU cycle the bus records the NMI edge latch and the IRQ line. `interrupts()` returns the record from the start of the step's last cycle, so the check sits "just before the final CPU cycle" (anomie-timing).
   - Events due at the very moment a cycle starts are serviced before it, so that cycle's record sees them.
   - A line that changes during an instruction's final cycle is therefore seen one instruction later.
   - The NMI latch survives until the vector read clears it (ADR 0004).
   - The IRQ is taken only if the line is still low at the next boundary's record (`wdc-w65c816s#2.18`).
   - The same rule applies to a waiting processor, whose steps are single halted cycles.
2. **What is not modelled here.**
   - The old I flag that CLI, SEI, PLP, REP and SEP leave to the check is a CPU-core change, and so is the 12-cycle end of WAI. A follow-up in `sfes-snes-cpu` amends ADR 0004 for both.
   - The NMI that a DMA delays comes with DMA (#83).
   - A `$4211` read at the moment of the trigger, which fullsnes says reads 1 without clearing it, is not modelled. Neither document says how long the "condition" lasts.
3. **The moment a read or a latch sees.**
   - A read of `$4210`, `$4211` or `$4212`, and a `$2137` latch, see the machine at the start of the CPU access. anomie-timing ties both to the same point in the access, and the documents give no finer one.
   - A `$4201` write that latches does so 4 master cycles, one dot, after its access starts (anomie-timing).
   - A write to `$4200` or `$4207–$420A` takes effect at the start of its access. A trigger point inside that access is therefore met (anomie-timing's enable "at the exact cycle").
4. **NMI.**
   - The NMI flag (`$4210` bit 7) is set at H=0.5 of the line that starts vertical blank (ADR 0020, point 5): 2 master cycles after the line starts. It is set whether or not NMIs are enabled.
   - The flag is cleared at H=0 of line 0, and by a `$4210` read.
   - The CPU's /NMI input is low while the flag and `$4200` bit 7 are both set. Each high-to-low change of /NMI sets the edge latch, and the NMI vector read clears it (ADR 0004).
   - Enabling NMIs while the flag is set is therefore a fresh edge (fullsnes). Writing `$4200` does not clear the flag: fullsnes says so, and anomie-timing only asks "Possibly".
   - A `$4210` read returns the flag in bit 7, open bus in bits 6–4 (ADR 0019) and version 2 in bits 3–0, the one version both documents name.
5. **IRQ.**
   - `$4200` bits 4 and 5 select the trigger:
     - none (00);
     - every line at HTIME (01);
     - line VTIME at H=0 (10);
     - line VTIME at HTIME (11).
   - The trigger point of a line uses anomie-timing's formula:
     - H=0: 1374 master cycles after the previous line started;
     - otherwise: 14 + 4×H master cycles after the line started.
   - The formula is in master cycles. A trigger after a long dot is therefore not at an exact half-dot, and a trigger for H ≥ 338 falls in the next line.
   - No trigger:
     - for an H past the line's last dot, which is 339, or 340 on a long line (anomie-regs, and fullsnes's "+1" for long lines);
     - for a VTIME past the frame's last line;
     - for H=153 on a short line or on the frame's last line (anomie-timing).
   - The H=0 trigger of line 0 after power-on or reset counts a normal previous line, so it falls 10 cycles after the start.
   - At a trigger point the IRQ flag (`$4211` bit 7) is set. The CPU's /IRQ input is low while it is set; there is no cartridge IRQ yet.
   - The flag is cleared by a `$4211` read and by a `$4200` write with bits 4 and 5 both 0 (anomie-timing, fullsnes).
   - A write to `$4211` does nothing: anomie-regs alone says it clears the flag, and fullsnes lists the register as read-only.
   - A `$4211` read returns the flag in bit 7 and open bus in bits 6–0.
6. **`$4212`.**
   - Bit 7 is vertical blank as ADR 0020 has it, from H=0 of its first line to H=0 of line 0.
   - Bit 6 is horizontal blank, from H=274 to H=1, read from the dot at the access's start (anomie-timing over anomie-regs's approximate values).
   - Bit 0, auto-joypad busy, reads 0 until #85.
   - Bits 5–1 are open bus.
7. **The counter latch.**
   - A `$2137` read latches the counters when `$4201` bit 7 is set, and returns open bus. A `$4201` write that changes bit 7 from 1 to 0 latches them one dot later (point 3).
   - The latched H is the dot of ADR 0020, point 3, and the latched V is the line.
   - Each latch sets the `$213F` bit 6 flag, which `$213F` reads with the PPU.
   - `$213C` and `$213D` each return the low byte, then bit 8 with PPU2 open bus in bits 7–1, alternating on their own flip-flops. A latch does not reset the flip-flops; the `$213F` read that does comes with the PPU.
   - Until the PPU exists, PPU2 open bus is the last value read from `$213C` or `$213D`, 0 at power-on. The PPU issue replaces it with the PPU2 open bus of every PPU2 register.
8. **Power-on and reset.**
   - Power-on and reset clear `$4200`, the NMI and IRQ flags, the edge latch and the recorded lines.
   - HTIME and VTIME are `$1FF` at power-on and kept on reset. So are the latched counters `$1FF`, the flip-flops on the low byte and PPU2 open bus.
   - `$4201` is `$FF` at power-on and on reset: fullsnes's reset column states it, where anomie-regs writes "unchanged on reset(?)".
9. **The deadline table.**
   - The table gains three events after the refresh and the line start: the NMI flag, the IRQ trigger and a `$4201` latch. Events due together are serviced in this order.
   - The IRQ deadline is the earliest trigger point not yet passed, of the line now running and of the line before it, whose late triggers fall in this one. It is worked out again:
     - when a line starts;
     - after each trigger;
     - after a write to `$4200`, `$4207–$420A` or `$2133`.

## Consequences

- ADR 0004's open question is settled: the check sits at the start of the last cycle of each step. ADR 0004 stands otherwise; the follow-up on I and WAI amends it.
- `$4210`, `$4211`, `$4212`, `$2137`, `$213C` and `$213D` read with side effects, so `peek_bus` reports them as latched, with the value a read would return.
- The PPU issue takes over `$213C`, `$213D` and the latch flag together with `$213F` and PPU2 open bus. Until then the counter latch lives in the bus.
- Point 3's choice of the access's start is the least certain decision here. A test ROM that reads `$4212` or latches `$2137` across a refresh or a line boundary, half a dot apart, reopens it.
- The approved timing ROMs (ADR 0014) are run when the PPU exists; until then the own cartridge `interrupts` and the unit tests are the evidence.
