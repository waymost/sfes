# 0023. The multiply and divide unit, WRIO and RDIO

- **Status:** accepted
- **Date:** 2026-11-20
- **Confidence:** high for the results and the 8 and 16 cycle latency; low for what a read sees before the result, for a new operation started while one runs, and for refresh

## Context

The 5A22 has an unsigned multiply and divide unit at `$4202–$4206`, with results in `$4214–$4217`, and an 8-bit programmable I/O port written at `$4201` and read at `$4213`. The results appear some time after the write that starts the operation. The sources agree on the arithmetic and roughly on the delay, but leave open:
- whether the delay counts master cycles or CPU cycles;
- what a read sees before the result is ready;
- what a second operation does to one still running.

ADR 0021 already models `$4201` bit 7 as the PPU latch line and sets `$4201` to `$FF` at power-on and reset.

## Sources

- `anomie-regs#4202`, `#4203`: write `$4202`, then `$4203`. "8 'machine cycles' (probably 48 master cycles)" later the product may be read from `$4216/7`. `$4202` is not altered, so a new `$4203` alone starts another multiplication. The multiplication is unsigned. `$4202` is `$FF` at power-on and unchanged on reset.
- `anomie-regs#4204`–`#4206`: write `$4204/5`, then `$4206`. "16 'machine cycles' (probably 96 master cycles)" later, the quotient is in `$4214/5` and the remainder in `$4216/7`. `$4204/5` are presumably not altered. Division by 0 gives a quotient of `$FFFF` and a remainder of the dividend. The dividend is `$FFFF` at power-on and unchanged on reset.
- `anomie-regs#4214`–`#4217`: repeat the above. The results' reset values are not given.
- `anomie-timing#s-cpu/registers`: "8 cycles (8 master cycles or somewhere between 48 and 64 master cycles?)" for the product, and "16 master cycles (or somewhere between 96 and 128 master cycles)" for the division. "What happens before the time is up (or if you initiate another multiplication/division while calculating) is unknown."
- `fullsnes#snesmathsmultiplydivide`:
  - after `$4203`, "wait 8 clk cycles" for the product. The hardware "does additionally set RDDIVL=WRMPYB, and RDDIVH=00h";
  - after `$4206`, "wait 16 clk cycles". Division by zero returns `$FFFF` and the dividend;
  - the ports "are clocked by the CPU Clock", so the wait is the same number of opcodes at either CPU speed. The 3 cycles of a `MOV r,[421xh]` opcode fetch count, so 5 more are needed for a multiplication and 13 for a division;
  - if the upper N bits of `$4202` are zero, "it seems" the wait is N cycles shorter, and a refresh during the wait "seems" to shorten it too;
  - the operations start only on `$4203` and `$4206` writes.
- `fullsnes#snesiomap`: `$4202–$4206` listed `(FFh)`, `$4214–$4215` `(0)`; `$4216–$4217` have no value.
- `anomie-regs#4201`: bit 7 is the PPU latch line and pin 6 of port 2, bit 6 pin 6 of port 1. The port is initialised as if written with all ones at power-up.
- `anomie-regs#4213`: any bit 0 in `$4201` reads 0. A bit 1 reads 1 unless a device on that line pulls it to 0.
- `fullsnes#snescontrollersioportsmanualreading`: `$4201` is an open-collector output (0 drives low, 1 is high-Z or input). `$4213` reads all 8 lines. Lines 0–5 reach no connector.

## Decision

1. **Results.** A `$4203` write multiplies `$4202` by the value, unsigned. The product goes to `$4216–$4217`, the multiplier to `$4214` and 0 to `$4215` (fullsnes). A `$4206` write divides `$4204–$4205` by the value, unsigned. The quotient goes to `$4214–$4215` and the remainder to `$4216–$4217`. Dividing by 0 gives `$FFFF` and the dividend. Neither operation changes its operands, so `$4202` and `$4204–$4205` can be reused.
2. **Latency in CPU cycles.** The unit counts CPU cycles, as fullsnes says, not master cycles. Its counts, 8 and 16, are the figures all three sources give in some unit. The results appear together at the end of the 8th (multiply) or 16th (divide) CPU cycle after the starting write. A read in the 9th or 17th cycle sees them, and a read in the 8th or 16th does not. This matches fullsnes's 3-cycle opcode fetch plus 5 or 13 more.
   - Every CPU cycle counts: reads, writes and internal cycles.
   - A refresh pause is not a cycle and counts for nothing. fullsnes only says a refresh "seems" to help, and gives no amount.
   - The upper-bits shortcut for multiplication is not modelled. fullsnes says it only "seems" to happen, and no source says what the registers hold while the operation runs.
3. **Before the result.** Until the results appear, `$4214–$4217` keep their previous values. No source describes the registers mid-operation, and this choice needs no model of the unit's internals. The extra `$4214–$4215` values of a multiplication appear with its product.
4. **A new operation while one runs.** The new one starts its own count and the old one's results are dropped. They never appear.
5. **Power-on and reset.** At power-on `$4202` is `$FF` and `$4204–$4205` are `$FFFF` (anomie-regs). `$4214–$4217` are 0, following fullsnes's `(0)` for `$4214–$4215`; no source gives `$4216–$4217`. Reset changes nothing in the unit, and an operation running at reset still finishes.
6. **`$4213` RDIO** returns `$4201` with every bit a device pulls low cleared. Until the controller ports (#85) and the lightgun, no device pulls any line low, so it reads back `$4201`. It is a full 8-bit read and drives the data bus.
7. **Reads and writes of the other registers.** `$4202–$4206` are write-only and read open bus. Reading `$4213–$4217` has no side effects. Writes to them are lost.

## Consequences

- `sfes-snes-core` gains `bus/math.rs`, clocked once per CPU cycle from `Bus::charge`.
- The unit is not yet part of any serialized state. It joins the bus state when the save-state envelope lands (M4).
- Unit tests in `bus/math.rs` and `bus/tests.rs` cover:
  - both latencies at the cycle boundary;
  - reuse of the operands;
  - division by zero;
  - the power-on operands;
  - an abandoned operation;
  - RDIO;
  - peek and reset.
- The `math` test cartridge reads each result one cycle early and exactly on time, and `src/carts.rs` checks the values against the documents.
- DMA (#84) pauses the CPU. Whether the unit keeps counting during a transfer is decided there.
- We revisit points 2–4 if a test ROM measures results mid-operation, the effect of a refresh, or the upper-bits shortcut.
- The technical design's M2 checklist and `docs/chips/core.md` cite this ADR.
