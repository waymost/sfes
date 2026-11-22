# 0016. Cartridge header detection

- **Status:** accepted
- **Date:** 2026-11-20
- **Amended:** 2026-11-21: the checksum splits a remainder that is not a power of two again, as ADR 0017 mirrors it (score table, first row).
- **Confidence:** medium

## Context

A ROM image does not say which board it was made for. The internal header that describes the cartridge sits at the CPU address `$00:FFC0`, and so at a file offset that depends on the board. Before mapping the ROM the loader has to find the header: which candidate location holds a real header (EMU-04)? Which region does the header imply (EMU-09)? What happens to the 512-byte headers that copiers added? The documents list the evidence a real header shows, but they do not say how to weigh it, and they leave some country codes' video standard open.

## Sources

- `anomie-memmap#memory-map`: banks `$00–$3F` and `$80–$BF` map the cartridge at `$8000–$FFFF`, so the CPU reads its vectors, including RESET at `$FFFC`, from the cartridge.
- `fullsnes#snescartridgeromheader`:
  - The header is at file offset `$7Fxx` (LoROM), `$FFxx` (HiROM) or `$40FFxx` (ExHiROM), plus `$200` when a copier header is present. It runs from title `$FFC0` to checksum `$FFDE`, and the exception vectors fill `$FFE0–$FFFF`.
  - The checksum complement is the checksum XOR `$FFFF`. The checksum is the sum of all ROM bytes. An image whose size is not a power of two is summed with its "bigger half" first, followed by mirrors of the smaller half up to the size rounded up.
  - `$FFD5` has bits 7–6 clear and bit 5 set, bit 4 for FastROM, and map mode in bits 3–0. Modes 0, 2 and 3 are LoROM boards, 1 and `$A` HiROM boards, and 5 ExHiROM.
  - The ROM size `$FFD7` is `1 SHL n` KiB, rounded up, usually 8 to `$0C`.
  - Text fields use `$20–$7E`, or JIS.
  - Homebrew often has a broken checksum and title, but "should (hopefully) contain valid entrypoints in range 8000h..FFFEh".
  - The country byte `$FFD9` implies NTSC or PAL as tabulated: `$00`, `$01`, `$0D`, `$0F` NTSC; `$10` Brazil PAL-M, "NTSC-like 60Hz"; `$02–$0C` and `$11` PAL, `$06` being SECAM, "PAL-like 50Hz"; `$0E` and `$12–$14` marked "(?)".
- `fullsnes#snescartridgeromimageheadersandfileextensions`: copier headers are 512 bytes and "don't contain any useful information"; detect them by `(filesize AND 3FFh)=200h`.
- `fullsnes#snescartridgeromimageinterleave`: interleaved images from old copiers, and the ExHiROM file order (fast banks first, header in the first slow bank at `$40FFxx`).
- `snesdev-wiki#ROM_header` (archived in `provenance/archive/snesdev-wiki/ROM_header.html`, retrieved 2026-11-20, SHA-256 `479f3d438aacb451422535cd5ae36469aa0f8097705fbda6ce5cb698ba4423ed`):
  - It gives the same field table and checksum process as fullsnes, and says the checksum is "the primary way to verify a candidate header".
  - It lists further checks: complement, map mode against location, the ROM size not smaller than the file, a reset vector below `$8000` being invalid, and an ASCII or JIS X 0201 title.
  - It says codes `$00` and `$01` are NTSC and "most other regions are considered PAL".
  - The page quotes no emulator code, but it cites two emulators' source files as the origin of its heuristic list. Only checks that fullsnes also states are used here. The wiki's guesses about the first instruction at the reset vector are not used.

## Decision

- **Copier header:** when the image length is `512` modulo `1024`, the first 512 bytes are a copier header. They are dropped before anything else, and `Header::copier_header` records that they were there. Hashing the ROM (EMU-04's canonical identity) happens in the loader, over the stripped image.
- **Candidates:** LoROM at `$7FC0`, HiROM at `$FFC0` and ExHiROM at `$40FFC0` in the stripped image. A candidate exists only if the image holds its whole 64-byte area, header and vectors.
- **A candidate is rejected** when its RESET vector (`$FFFC`) is outside `$8000–$FFFE`. That's the range fullsnes gives, and the cartridge only answers at `$8000–$FFFF` in bank `$00`.
- **Score** of a remaining candidate, from the evidence both documents state, weighted so that a matching checksum and complement, the evidence the wiki calls primary, outweigh everything else together:

  | Evidence | Points |
  | --- | --- |
  | The stored checksum equals the image's checksum, summed as fullsnes describes, with a remainder that is not a power of two split again as ADR 0017 mirrors it | 4 |
  | Checksum XOR complement is `$FFFF` | 4 |
  | `$FFD5` bits 7–5 are `001` and its map mode belongs to the candidate's board | 2 |
  | Every title byte is in `$20–$7E` or JIS X 0201 `$A1–$DF` | 1 |
  | The ROM size byte is at most `$0D` and `1 SHL n` KiB is not smaller than the image | 1 |

  The highest score wins. A tie for the highest is an error naming the tied boards, and so is an image with no candidate left. In both cases the caller can pass an override.
- **Override:** an explicit board skips scoring and the reset-vector check. It only needs the image to hold that board's header area.
- **Region:**
  - `$00`, `$01`, `$0D`, `$0F` and `$10` give NTSC; Brazil's PAL-M runs at 60 Hz.
  - `$02–$0C` and `$11` give PAL; SECAM runs at 50 Hz.
  - Every other code (`$0E`, `$12` and up) gives no region. The machine then needs a region override or uses its default, which the machine issue decides.
  - An explicit region override replaces the header's region.
- **Not handled:** interleaved images, Satellaview and Sufami Turbo headers, and the checksums fullsnes lists for SPC7110 boards. Such an image may lose the checksum points or be reported ambiguous, and then needs an override.

## Consequences

- A ROM with a valid checksum is found on that evidence alone. Homebrew without one still needs a plausible reset vector and usually wins on map mode, title and size.
- The weights are a judgement. A real ROM detected wrongly is a reason to revisit them, with the ROM named in an amendment.
- Region codes the documents leave open stay open until a source settles them.
