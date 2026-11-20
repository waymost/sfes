# 0017. Memory map and the LoROM, HiROM and ExHiROM boards

- **Status:** accepted
- **Date:** 2026-11-20
- **Confidence:** medium

## Context

The bus finds what answers an address by looking up its 8 KiB page in a `MemoryMap` built once at load (technical design section 5). The console's own part of that map is fully documented. The cartridge's part depends on the board, and the documents describe the boards as tables of real PCBs that differ in small ways. The loader has to pick one map per board for ROM and cartridge RAM, and the documents leave four questions open:
- How is a ROM mirrored when it does not fill its area, or its length is not a power of two?
- Which SRAM areas does each board use?
- How does an SRAM larger than one bank's window select its bank?
- What happens to a header whose RAM size cannot be mapped?

## Sources

- `anomie-memmap#memory-map`:
  - The console's map: in banks `$00–$3F` and `$80–$BF`, `$0000–$1FFF` mirrors `$7E:0000–$1FFF` (WRAM), `$2100–$21FF` is Address Bus B, `$4000–$43FF` the CPU's registers, `$6000–$7FFF` Address Bus A, and `$8000–$FFFF` the cartridge (`/CART`). Banks `$40–$7D` and `$C0–$FF` are wholly the cartridge, and `$7E–$7F` are WRAM.
  - "LoROM", "HiROM" and the rest are "just the cart responding differently" to Address Bus A, and the cartridge "can respond to any address on either bus that is not otherwise mapped".
- `anomie-memmap#hardware/address-bus-a`: the cartridge connector carries all 24 address lines, `/CART` and `/WRAM`.
- `fullsnes#snesmemorymap`:
  - The same map, with the remark that the upper halves of the HiROM banks are "usually mirrored" to the LoROM banks, which brings the vectors from `$40FFE0` to `$00FFE0`.
  - SRAM: HiROM at `$30–$3F,$B0–$BF:6000–7FFF`, "often also mirrored to `$20–$2F`"; LoROM at `$70–$7D,$F0–$FF:0000–7FFF`. LoROM SRAM is "sometimes crippled to `$70–$71` or `$70–$77`, or extended to `$60–$7D`".
  - It gives no rule for mirroring a ROM smaller than its area.
- `fullsnes#snescartlorommappingromdividedinto32kbanksaround1500games`:
  - Plain LoROM maps ROM at `$00–$7D,$80–$FF:8000–FFFF` with mirrors at `$40–$7D,$C0–$FF:0000–7FFF`.
  - The SRAM boards map ROM in `$00–$1F`, `$00–$3F` or `$00–$7D`, and SRAM at `$70–$7D,$F0–$FF`. Older boards use the whole 64 KiB bank and newer ones the lower 32 KiB, which lets ROMs over 3 MiB use the upper half.
  - SRAMs of 64 or 128 KiB are "divided into 32K chunks, mapped to bank 70h, 71h, etc.".
- `fullsnes#snescarthirommappingromdividedinto64kbanksaround500games`:
  - HiROM maps ROM at `$40–$7D,$C0–$FF:0000–FFFF` with mirrors at `$00–$3F,$80–$BF:8000–FFFF`.
  - Most SRAM boards use `$20–$3F,$A0–$BF:6000–7FFF`. Two use `$10–$1F,$30–$3F` and their high mirrors.
  - The ExHiROM board, SHVC-LJ3M-01, maps SRAM at `$80–$BF:6000–7FFF`, and its banks `$00–$7D` hold "different ROM banks than `$80–$FF`".
- `fullsnes#snescartridgeromimageinterleave`: an ExHiROM file holds the fast banks `$C0–$FF` first (4 MiB), then the slow banks from `$40`. The header and vectors are in the upper 32 KiB of the first slow bank, at file offset `$40FFxx`.
- `fullsnes#snescartridgeromheader`:
  - A ROM whose size is not a power of two is checksummed as its larger power-of-two part, followed by the remainder repeated, "4MB + 4 x Last 1MB" for a 5 MiB ExHiROM.
  - The RAM size byte gives `1 SHL n` KiB, "usually 1=2Kbyte .. 5=32Kbyte".
- ADR 0016: the board comes from header detection or an override.

## Decision

- **Pages:** a `MemoryMap` holds 2,048 `Page`s of 8 KiB, indexed by `address >> 13`. Each page is `Rom(offset)`, `CartRam(offset)`, `Wram(offset)`, `Io`, `Coproc(id)` or `Open`. The offset is that of the page's first byte, already mirrored. `MemoryMap::resolve` adds the offset inside the page.
- **Console area**, the same on every board, from `anomie-memmap#memory-map`:
  - In banks `$00–$3F` and `$80–$BF`, `$0000–$1FFF` is `Wram(0)`, and `$2000–$5FFF` is `Io`: two pages, which the bus decodes further.
  - Banks `$7E–$7F` are WRAM.
  - The board fills the rest. What it leaves is `Open`, including `$6000–$7FFF` on boards without SRAM there.
- **ROM**, from the plain board of each family:

  | Board | ROM at | Offset of `bank:address` before mirroring |
  | --- | --- | --- |
  | LoROM | `$00–$7D,$80–$FF:8000–FFFF` and `$40–$7D,$C0–$FF:0000–7FFF` | `(bank & $7F) × $8000 + (address & $7FFF)` |
  | HiROM | `$40–$7D,$C0–$FF:0000–FFFF` and `$00–$3F,$80–$BF:8000–FFFF` | `(bank & $3F) × $10000 + address` |
  | ExHiROM | as HiROM | `(bank & $3F) × $10000 + address`, plus `$400000` for banks below `$80` |

  The table maps `$00:FFE0` to the header area each board's detection uses (`$7FE0`, `$FFE0`, `$40FFE0`).
- **ROM mirroring:** an offset at or past the ROM's end is mirrored the way fullsnes sums the checksum. Take the offset modulo the ROM length rounded up to a power of two. If it is then inside the ROM, that is the byte. Otherwise, subtract the largest power of two not above the length from both, add it back afterwards, and repeat on the remainder. fullsnes describes one level; repeating it covers remainders that are not themselves a power of two. Only the first 16 MiB of an image can be mapped.
- **SRAM areas**, the most common board of each family. The RAM is present when the header's RAM size byte is not 0:

  | Board | SRAM at | Offset of `bank:address` before mirroring |
  | --- | --- | --- |
  | LoROM | `$70–$7D,$F0–$FF:0000–7FFF` | `(bank & $0F) × $8000 + address` |
  | HiROM | `$20–$3F,$A0–$BF:6000–7FFF` | `(bank & $1F) × $2000 + (address − $6000)` |
  | ExHiROM | `$80–$BF:6000–7FFF` | `(bank & $3F) × $2000 + (address − $6000)` |

  The offset is taken modulo the RAM size, a power of two, so a small SRAM repeats through its whole area. LoROM uses the newer 32 KiB window, so ROMs over 3 MiB keep their upper halves. In banks without SRAM, LoROM's lower halves stay ROM mirrors as on the plain board.
- **RAM size:** RAM size bytes 1 to 7, 2 KiB to 128 KiB, are mapped. A larger byte is an error, `CartError::RamSize`. 128 KiB is the largest fullsnes names, and every board's area holds it.
- **Cartridge:** `Cartridge::load` detects the header (ADR 0016), keeps the image without its copier header as `RegionId::Rom`, allocates the RAM as `RegionId::CartRam`, filled with zeros, and builds the `Arc`-shared map for its board.

## Consequences

- One map per board. Boards that differ, such as SRAM at `$10–$1F`, SRAM over the whole LoROM bank, or the 3-chip SHVC-3J0N-01 with open banks, would need their own `Board` variant. A real ROM that needs one is a reason to add it, with the ROM named in an amendment.
- Mirroring, mapping and RAM banking are pure functions of the board, ROM length and RAM size, so the map is rebuilt from the ROM on load. Save states never store it.
- What the SRAM holds at power-on is not documented here. The zero fill is a placeholder until battery saves and power-on state are decided.
