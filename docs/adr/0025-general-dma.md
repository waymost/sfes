# 0025. General DMA

- **Status:** accepted
- **Date:** 2026-11-20
- **Confidence:** high for the registers, the transfer modes, the address step and the count; medium for the pause's timing and what the A bus can reach; low for where each byte falls in its slot, for open bus during a transfer, for refresh during a transfer and for the multiply and divide unit

## Context

The 5A22 has eight DMA channels. A `$420B` write starts a general-purpose transfer on the channels it sets, pausing the CPU until every channel is done. The technical design (section 4, catch-up rule 4) runs the transfer inside the bus, advancing the clock per byte and checking the HDMA deadline between bytes, and reports each byte through `on_dma`.

The sources agree on the registers, the eight transfer modes, the address step and the byte count. They give the pause's timing as a rule with worked examples. They leave open:
- when, within a byte's 8 cycles, the byte moves;
- what the A bus reaches in `$4000–$43FF`;
- what open bus reads as during a transfer;
- whether a refresh pauses a transfer.

ADR 0023, point 2's consequences leave to this work whether the multiply and divide unit counts during a transfer. HDMA, which pre-empts a transfer, is #86.

## Sources

- `anomie-regs#420b`:
  - Each set bit enables a channel. The CPU is paused until all transfers complete, and they run "in order from 0 to 7 (?)".
  - HDMA during a transfer pauses it, and ends it if it uses the same channel.
  - `$420B` is `$00` at power-on and reset.
- `anomie-regs#43x0`:
  - Bit 7 is the direction: 0 reads the A bus and writes the B bus, 1 the reverse.
  - Bit 6 is for HDMA only.
  - Bit 3 set keeps the address fixed. Otherwise bit 4 set decrements it and clear increments it.
  - Bits 2–0 are the mode: 0 is `p`; 1 is `p, p+1`; 2 and 6 are `p, p`; 3 and 7 are `p, p, p+1, p+1`; 4 is `p, p+1, p+2, p+3`; 5 is `p, p+1, p, p+1`.
  - `$FF` at power-on, unchanged on reset.
- `anomie-regs#43x1`: the B-bus address `$21pp`. Two- and four-register modes wrap `$21FF` to `$2100`. `$FF` at power-on, unchanged on reset.
- `anomie-regs#43x2`:
  - `$43x2–$43x4` are the A-bus address.
  - "Bus A does not access the Bus B registers, so pointing this address at say $00:2100 results in open bus."
  - During a transfer `$43x2/3` step and `$43x4` does not. `$FF` at power-on, unchanged on reset.
- `anomie-regs#43x5`:
  - `$43x5/6` count bytes, "a strict limit": with 5 in a four-register mode, all four registers are written and then the first again. `$0000` transfers `$10000` bytes.
  - The count is decremented and "typically" ends at 0. `$FF` at power-on, unchanged on reset.
- `anomie-regs#43x8`, `#43xa`: `$43x7–$43xA` are not used by general DMA. `anomie-regs#43xb`: `$43xB` and `$43xF` are one register of unknown effect, `$FF` at power-on, unchanged on reset.
- `anomie-regs#dma-and-hdma/dma`:
  - A transfer is 8 master cycles per byte, 8 per channel, and "12-24 cycles" for the whole transfer.
  - Each byte puts the A-bus address on bus A and the port on bus B, and the bus marked for reading drives the data bus for the other to write. PPU, APU and WRAM registers are only on bus B, so reaching them through the A-bus address is open bus.
  - WRAM through both buses fails, "with the 2180-3 access being Open Bussed".
  - "DMA cannot access the $4300-$437f registers nor $420b nor $420c. Writes will have no effect, and reads will return Open Bus."
  - Then the address steps (the bank is not changed), and the count is decremented and tested, so a count of 0 moves 65536 bytes.
- `anomie-regs#2180`, `#2181`:
  - A transfer from WRAM to `$2180` does not write WRAM.
  - A transfer from `$2180` to WRAM writes "(initially) the Open Bus value".
  - In both cases the WRAM port's address is not incremented, and a transfer from WRAM to `$2181–$2183` has no effect.
- `anomie-timing#s-cpu/dma`:
  - 8 master cycles per byte "regardless of the memory regions accessed". What a transfer to or from `$4000–$41FF` does is unknown.
  - After the `$420B` write the CPU gets one more cycle before the pause, and the speed of the CPU cycle that runs after the transfer is "the CPU Clock speed for the DMA transfer".
  - After the pause, wait 2–8 master cycles to reach a whole multiple of 8 since reset. Then 8 cycles of overhead, 8 per channel and 8 per byte. Then wait 2–8 cycles to reach a whole number of CPU Clock cycles since the pause.
  - Where the reads fall is not known. The best guess is that "2-4 of the 'whole DMA overhead'" comes before the bytes and the rest after.
  - Four worked examples ("STA $420b : NOP", 3 bytes, a 6-cycle clock) take 48, 48, 48 and 54 cycles. A wait to a whole CPU Clock that would be 0 is a whole clock instead ("0 is not an option").
- `anomie-timing#s-cpu/interrupts`: an NMI edge during a transfer triggers after it, with "a 24-30 cycle delay".
- `anomie-timing#s-cpu/clocks-and-refresh`: the CPU is paused for 40 cycles each line. Neither source says whether a refresh pauses a transfer.
- `fullsnes#snesdmaandhdmastartenableregisters`:
  - A non-zero `$420B` write starts a transfer "immediately (after a few clk cycles)", channel 0 first.
  - The bits clear when the transfer completes.
  - HDMA can interrupt a transfer.
- `fullsnes#snesdmaandhdmachannel07registers`:
  - The same layout. Bit 5 of `$43x0` is unused but readable and writable.
  - The step is "0=Increment, 2=Decrement, 1/3=Fixed". The modes are as anomie gives them, with 6 and 7 "same as" 2 and 3.
  - The 16-bit count counts bytes, not units, and holds `$0000` at the end.
  - `$43xB` is read/write and unchanged by DMA. `$43xC–$43xE` are open bus, and `$43xF` mirrors `$43xB`.
- `fullsnes#snesiomap`: `$43x0–$43xB` and `$43xF` are `(FFh)`, except `$43x4` `(xxh)`.
- `fullsnes#snesdmatransfers`: a general transfer moves up to `$10000` bytes between the A bus and the B bus.
- `fullsnes#snesmemoryworkramaccess`: WRAM-to-WRAM DMA "isn't possible", in either direction.
- `fullsnes#snesunpredictablethings` ("Open Bus for DMA"):
  - Through the A bus, `$2100–$21FF` is open bus.
  - `$4000–$41FF` is open bus too, except that `$4017` returns bits 4–2 set.
  - `$4210–$421F` "do work".
  - `$4300–$437F` is a special open bus.
  - It first says a DMA read of an unused address returns `$FF`, because the hardware "forgets" the last value. Its later edit says that is the effect of the XBOO cable it measured with. Without the cable, the last value may last "possibly forever (until DMA ends, or until it reaches a used address)".
- `fullsnes#snesmemory`: Sound RAM is not reachable by DMA.

No source gives:
- where in a byte's 8 cycles the byte moves;
- whether a refresh pauses a transfer;
- whether the multiply and divide unit counts during one;
- whether a DMA write reaches `$4000–$41FF` or the `$4200–$420F` registers other than `$420B` and `$420C`.

## Decision

1. **The registers.**
   - Each channel holds `$43x0–$43x6` and the HDMA registers `$43x7–$43xA`, which general DMA does not use, plus `$43xB`, mirrored at `$43xF`. All are full 8-bit registers that read back what was last written or stepped, with no side effects. Bit 5 of `$43x0` included.
   - `$43xC–$43xE` and `$4380–$43FF` are open bus and writes to them are lost.
   - Every channel register is `$FF` at power-on, `$43x4` included, and reset keeps them all (anomie-regs).
2. **`$420B` MDMAEN** is write-only: a read is open bus. A write records its value as the channels to run, replacing any value not yet started. A value of 0 starts nothing. Reset clears it and drops a transfer not yet started.
3. **When the pause begins.** The CPU cycle after the `$420B` write runs as usual. The pause begins at the start of the next CPU cycle. That cycle's length (6, 8 or 12) is the transfer's CPU Clock, and the cycle runs once the transfer is done.
4. **The pause's length**, counted from its start `P`:
   - wait to the next multiple of 8 master cycles since power-on or the last reset, 2 to 8 cycles, never 0;
   - 8 cycles for the transfer;
   - for each set channel, from 0 to 7: 8 cycles, then 8 per byte;
   - wait to the next multiple of the CPU Clock since `P`, 1 to N cycles, never 0.

   This reproduces anomie-timing's four examples. Its 12–24 cycles "for the whole transfer" are the two waits and the 8.
5. **A byte.**
   - Each byte has an 8-cycle slot and moves at the slot's start. Within one channel the slots follow its 8 cycles one after another. Anomie's guess places part of the whole-transfer overhead before the bytes. We put all 8 cycles first, as its example's order does.
   - The value moved is placed on the data bus, so it is open bus afterwards.
   - The B-bus address is `$21pp` plus the mode's offset for the byte's place in this transfer: bytes moved so far, modulo the unit's length. Two- and four-register modes wrap within `$21xx`.
   - After each byte, the A-bus address in `$43x2/3` steps by `$43x0` bits 4–3, wrapping at 16 bits; `$43x4` is not changed. The count in `$43x5/6` is decremented, and the channel ends when it reaches 0, so a count of 0 moves `$10000` bytes and every channel ends with a count of 0.
6. **What the A bus reaches.**
   - WRAM, cartridge ROM and cartridge RAM, as for the CPU.
   - `$2100–$21FF` through the A bus: open bus, writes lost.
   - `$4000–$41FF`: open bus, with `$4017` bits 4–2 set (fullsnes). Reads clock no pad and writes are lost.
   - `$420B`, `$420C` and `$4300–$437F`: open bus, writes lost (anomie-regs).
   - Every other register in `$4200–$42FF`, `$4210–$421F` included, as for a CPU access, with the same side effects. Reading `$4210` clears the NMI flag, and reading `$4218–$421F` sets the polled flag (ADR 0024, point 8).
   - Addresses nothing answers: open bus.

   That the other `$42xx` registers take DMA writes follows from anomie listing only `$420B`, `$420C` and `$4300–$437F` as out of reach.
7. **The B bus.**
   - The PPU, APU and WRAM port answer as for a CPU access, with the same side effects. A transfer that reads `$2137` latches the counters as a CPU read does.
   - `$2184–$21FF` is open bus and writes are lost.
   - When the A-bus address is WRAM and the B-bus address is `$2180–$2183`, the B-bus side fails. From WRAM, WRAM is read and the B-bus write is lost. To WRAM, nothing drives the bus and WRAM is written with the open-bus value. In both cases the port's address does not change.
8. **Open bus during a transfer** is the data bus's last value, as for the CPU (anomie-memmap). The value at the start is the CPU's last, normally the `$420B` write's value or the next opcode. fullsnes's `$FF` is the XBOO cable's effect by its own later account. Without the cable the value is kept, which this matches. Its `$4300` special case is not modelled.
9. **Time during a transfer.**
   - Deadlines that fall due during a slot or a wait are serviced at its end, in table order, as at the end of a CPU cycle (ADR 0020, point 9). Lines start, the NMI and IRQ flags are set, latches and the auto-joypad read go on (ADR 0021, ADR 0024).
   - A refresh due during a transfer pauses it for 40 cycles, as it pauses the CPU (ADR 0019, point 6): the refresh is a pause of the bus, and no source exempts DMA. Because 40 is a multiple of 8, the slots stay on the 8-cycle grid.
   - The CPU sees an interrupt raised during the transfer in the cycle after it, at its next instruction boundary (ADR 0021, point 1). The 24–30 cycle delay anomie-timing gives is not modelled separately.
   - HDMA (#86) will be a deadline checked between bytes that pauses or ends the transfer.
10. **The multiply and divide unit does not count during a transfer.** It counts CPU cycles (ADR 0023, point 2). A DMA pause, like a refresh, is not one. An operation running when the transfer starts finishes after the CPU resumes.
11. **Events.**
    - Each byte reports one `on_dma` event: the channel, `hdma: false`, the source and destination as CPU addresses (`$00:21pp` for the B bus), the value and the slot's start time.
    - A transfer's accesses are not reported through `on_read` or `on_write`, which are the CPU's.

## Consequences

- `sfes-snes-core` gains `bus/dma.rs`: the channel registers, the pending `$420B` value and the transfer, run by the bus at the start of the CPU cycle that follows its pause.
- `Bus::charge` is split. The clock and deadline part is shared with the transfer. The multiply and divide unit's count and the start countdown stay with CPU cycles.
- The channels are not yet in any serialized state. They join the bus state with the save-state envelope (M4).
- Unit tests in `bus/dma.rs` and `bus/tests.rs` cover:
  - the registers and their power-on values;
  - every mode's B-bus pattern;
  - the address step and the count, including a count of 0;
  - anomie-timing's four worked examples;
  - several channels;
  - the A- and B-bus restrictions and WRAM through both buses;
  - deadlines during a transfer;
  - the multiply and divide unit;
  - reset;
  - `on_dma`.
- The `dma` test cartridge moves bytes in several modes and directions and records the registers afterwards, and `src/carts.rs` checks the values against the documents.
- We revisit point 4 or 5 if a test ROM measures a transfer's length or reads `$2137` within one, point 9 if one measures a refresh during a transfer, point 6 if one measures `$4000–$41FF` or `$42xx` through DMA, and point 10 if one measures the unit across a transfer.
