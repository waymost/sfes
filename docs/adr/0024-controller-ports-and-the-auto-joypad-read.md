# 0024. Controller ports and the auto-joypad read

- **Status:** accepted
- **Date:** 2026-11-20
- **Confidence:** high for the pad's serial data, `$4016/$4017` and the registers the auto-read fills; medium for when the auto-read starts and for the polled flag; low for the timing of the bits within the auto-read, for the pad at power-on and for reset

## Context

The 5A22 reads controllers in two ways:
- by hand, through the latch at `$4016` bit 0 and the serial data at `$4016/$4017`;
- automatically, when `$4200` bit 0 is set, into `$4218–$421F` once a frame.

The sources agree on the pad's data and on the registers. They give the auto-read's start only as a window and a rule, its length only as a total, and say nothing about how its 16 bits are spread over that time. They also give no reset values for the ports. ADR 0021, point 6, reads `$4212` bit 0 as 0 until this work. The technical design (section 7) asks for a per-frame `polled` flag for lag frames. `Snes::set_input` already holds a `Buttons` value for each port (ADR 0018).

## Sources

- `anomie-regs#controllers/generic`:
  - Latch is bit 0 of `$4016`, shared by both ports.
  - Reading `$4016` clocks port 1 and reading `$4017` clocks port 2. Data1 and Data2 are read, then the pad "is expected to stick its next bits" on the lines.
  - Data1 and Data2 are bits 0 and 1 of the register. IOBit is bit 6 (port 1) or bit 7 (port 2) of the I/O port.
  - The auto-read "effectively does" this: set `$4212` bit 0; write 1 then 0 to `$4016`, "there may be a delay here"; 16 times, read `$4016` and `$4017` and rotate each data bit into `$4218`, `$421C`, `$421A` and `$421E` with ROL; clear `$4212` bit 0.
- `anomie-regs#controllers/open-port`: with nothing connected to a pin, the SNES reads zeros.
- `anomie-regs#controllers/joypads`: 16 bits, `byetUDLRaxlr0000`, out Data1, "then one bits until latched again". Data2 and IOBit are not connected.
- `anomie-regs#4016`, `#4017`:
  - `$4016` reads `------ca` and writes `-------l`. `$4017` reads `---111db`: its bits 4–2 are tied to ground "and thus always 1".
  - The bit order is B, Y, Select, Start, Up, Down, Left, Right, A, X, L, R, 0, 0, 0, 0, then ones. The first bit read ends in bit 15 of `$4218/9`.
  - "The effect of reading these during auto-joypad read is unknown."
- `anomie-regs` (introduction): most undefined bits of readable registers return open bus.
- `anomie-regs#4200`: bit 0 enables the auto-read, which updates `$4218–$421F` "at about V Counter = $E3". The register is `$00` at power-on and reset.
- `anomie-regs#4212`: bit 0 is set while the auto-read runs; it "typically turns on at the start of V-Blank, and completes 3 scanlines later".
- `anomie-regs#4218`: the registers are only updated while `$4200` bit 0 is set, and reading them while `$4212` bit 0 is set "will return incorrect values".
- `anomie-timing#s-cpu/auto-joypad-read`:
  - The read begins between dots 32.5 and 95.5 of the first vertical-blank line and ends 4224 master cycles later. `$4212` bit 0 is set for that time.
  - "Specifically, it begins at dot 74.5 on the first frame, and thereafter some multiple of 256 cycles after the start of the previous read that falls within the observed range."
  - During the read, the only reliable value is that no buttons pressed reads 0. "Presumably reading $4016/7 or writing $4016 during this time will also screw things up."
- `fullsnes#snescontrollersioportsautomaticreading`:
  - The same register layout and serial order, with a pressed button read as 1.
  - `$4016` OUT0 must be 0 during the auto-read, "otherwise the shift register gets stuck on the first bit", so all 16 bits equal the B button. The section also repeats anomie-timing's paragraphs.
- `fullsnes#snescontrollersioportsmanualreading`:
  - `$4016` write: bit 0 is OUT0, the strobe on pin 3 of both ports. Bits 1 and 2 seem not to be connected.
  - `$4016` read: bits 1–0 are pins 5 and 4 of port 1, bits 7–2 unused. `$4017` read: bits 4–2 are tied to ground and read 1, bits 1–0 are port 2, bits 7–5 unused.
  - Each read generates a clock pulse on its port's pin 2.
- `fullsnes#snescontrollersjoypad`: the 16 bits as above, with a pressed button read as 1. Bits 17 and up are 1, "or 0=High when no pad connected".
- `fullsnes#snescontrollershardwareidcodes`: the 13th–16th bits are a device ID, `0000` for a pad.
- `fullsnes#snesiomap`: `$4016` write `00h`, `$4218–$421F` `00h`. `$4000–$4015` and `$4018–$41FF` are open bus.
- The technical design, section 7: input set before a frame is latched for that frame. The polled flag is set when the game reads `$4016/$4017` or `$4218–$421F`.

No source gives:
- when the auto-read takes each of its 16 bits;
- what a pad holds before its first latch;
- whether the 256-cycle rule starts again at reset;
- reset values for `$4016` and `$4218–$421F`.

## Decision

1. **A pad on each port.** Until the `Device` type of the technical design's section 7 lands, both ports always have a standard pad connected, holding the buttons last given to `Snes::set_input`. The buttons are not filtered: up with down, or left with right, is passed through.
2. **The pad.**
   - While Latch is high, the pad loads its 16 bits: the 12 buttons in serial order, a pressed button as 1, then four 0 ID bits. A clock while Latch is high returns the first bit (B) and does not advance (`fullsnes#snescontrollersioportsautomaticreading`).
   - With Latch low, a clock returns the current bit and moves to the next. After the 16th bit, every clock returns 1 until the next latch.
   - Data2 and IOBit are not connected: Data2 reads 0, and the pad pulls no I/O port line low, so `$4213` still reads back `$4201` (ADR 0023, point 6).
   - At power-on a pad is as after a complete read: it returns 1 until first latched.
   - The pad takes the buttons held at the latch, so input set before a frame is what that frame's latches see.
3. **`$4016` write.** Bit 0 drives Latch on both ports. Writing 1 loads both pads, and so does writing 0 while Latch was high, so a pad holds the buttons as they were when Latch fell. Bits 1 and 2 are not connected. Bit 0 is 0 at power-on and reset (`fullsnes#snesiomap`).
4. **`$4016` and `$4017` reads.**
   - `$4016` returns port 1's Data1 in bit 0 and Data2 in bit 1, with bits 7–2 open bus.
   - `$4017` returns port 2's Data1 in bit 0 and Data2 in bit 1, with bits 4–2 set and bits 7–5 open bus.
   - Each read clocks its port.
   - A peek returns the same bits without clocking and reports a side effect.
5. **When the auto-read starts.** `$4200` bit 0 enables it. It is checked when a read would start: if it is clear, no read happens, `$4212` bit 0 stays 0 and the registers keep their values. `$4200` is 0 at power-on and reset (`anomie-regs#4200`).
   - A read starts once per frame, on the line that starts vertical blank.
   - It starts at the earliest time at or after dot 32.5 of that line (130 master cycles after the line starts) that is a multiple of 256 master cycles after the first frame's start. The first frame's start is dot 74.5 of line 225 of the first frame after power-on or reset, 307198 master cycles after it. On that frame, the rule gives dot 74.5 itself.
   - Every read so starts 130 to 385 master cycles into its line, dots 32.5 to 96.25. The end of that range is three quarters of a dot past anomie-timing's 95.5, a difference the "observed range" leaves open.
   - The 256-cycle grid starting again at reset follows "on the first frame". No source says whether reset restarts it.
6. **The read.**
   - It lasts 4224 master cycles, with `$4212` bit 0 set throughout.
   - At its start, Latch pulses: both pads load, as for a `$4016` write of 1 then 0. Latch on the ports is `$4016` bit 0 or this pulse, so a game that leaves bit 0 set gets B in all 16 bits (fullsnes).
   - Then, at 256, 512, … 4096 master cycles after the start, the read clocks both ports. Each data line's bit is rotated into its register, as anomie-regs' ROL does: `$4218/9` takes port 1 Data1, `$421A/B` port 2 Data1, `$421C/D` port 1 Data2 and `$421E/F` port 2 Data2.
   - The busy flag clears 128 master cycles after the 16th bit.
   - A register read during the read sees the bits rotated in so far, above the old value's remaining bits. A `$4016` or `$4017` read, or a `$4016` write, during the read acts on the same pads, so it moves or reloads the bits the auto-read takes.
   - Clearing `$4200` bit 0 during a read does not stop it.
   - The 256-cycle spacing is ours. It matches the grid of point 5, but no source gives the bits' timing.
7. **`$4218–$421F`** are 0 at power-on (`fullsnes#snesiomap`). Reset keeps them, as it keeps the multiply and divide unit (ADR 0023, point 5), and abandons a read in progress. Reading them has no hardware side effects; writes are lost.
8. **The polled flag.**
   - The bus sets it when the CPU reads `$4016`, `$4017` or any of `$4218–$421F`. The auto-read's own clocks and peeks do not set it.
   - At each frame end (ADR 0020, point 7) the flag is kept as the frame's result and cleared for the next frame.
   - `Snes::polled()` returns the result of the last frame that ended. A game that reads its input in the NMI handler, after the auto-read, so polls in the frame `run_frame` runs.
   - The flag lives with the ports in the bus, not in `Meta`, so that the bus can set it.
9. **Scheduling.** The auto-read is a deadline, serviced after the counter latch when both are due together (ADR 0020, point 9). Its events are serviced at the end of the CPU cycle they fall in, like the others.

## Consequences

- `sfes-snes-core` gains `bus/ports.rs` with the pads, `$4016`, the auto-read and the polled flag. `Timing` gains the auto-read deadline and its 256-cycle grid.
- `$4212` bit 0 now reads the busy flag, which supersedes ADR 0021, point 6's "reads 0".
- `Snes` gains `polled()`. The technical design's section 7 and the `State` sketch in section 3 say where the flag lives.
- The ports are not yet in any serialized state. They join the bus state with the save-state envelope (M4).
- Unit tests in `bus/ports.rs`, `bus/tests.rs` and `timing/tests.rs` cover:
  - the serial order, the ones after 16 bits and a latch held high;
  - the `$4016/$4017` bits;
  - the start rule on the first and later frames;
  - the bits' timing and the busy flag;
  - reads during the read;
  - the enable bit;
  - reset;
  - the polled flag.
- The `joypad` test cartridge reads the pads by hand and by the auto-read, and `src/carts.rs` checks the values against the documents.
- We revisit point 5 if a test ROM measures the start on frames after the first or after reset, and point 6 if one measures `$4218–$421F` or `$4016` during the read.
