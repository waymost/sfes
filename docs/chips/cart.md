# The cartridge (`sfes-snes-cart`)

This page explains the SNES cartridge and how the crate models it (QA-08). It was written issue by issue during milestone M2 (#77, #78) and covers the crate as built: header detection and the LoROM, HiROM and ExHiROM boards. Coprocessors arrive with their own issues. Every statement cites its source, by its id in [`provenance/bibliography.toml`](../../provenance/bibliography.toml):

- fullsnes (`fullsnes`), by section anchor;
- Anomie's memory-mapping doc (`anomie-memmap`);
- the SNESdev wiki (`snesdev-wiki`), by page title, archived under [`provenance/archive/snesdev-wiki/`](../../provenance/archive/snesdev-wiki/);
- the project's ADRs, listed under [Decisions](#decisions).

## What the cartridge is

To the console, a cartridge is whatever answers when the CPU drives the `/CART` line. That happens at `$8000–$FFFF` in banks `$00–$3F` and `$80–$BF`, and across all of banks `$40–$7D` and `$C0–$FF`. "LoROM", "HiROM" and the other boards are just different ways a cartridge answers those addresses (`anomie-memmap#memory-map`). The CPU reads its exception vectors from `$00:FFE0–$FFFF`, so those come from the cartridge too.

## The header

Nintendo had every cartridge carry a header at CPU address `$00:FFC0–$00:FFDF`, just before the vectors (`fullsnes#snescartridgeromheader`, `snesdev-wiki#ROM_header`). The hardware never reads it. Emulators and flash carts use it to learn which board a ROM image needs. [`Header`](../../crates/sfes-snes-cart/src/header.rs) holds its fields:

| Address | Field | Notes |
| --- | --- | --- |
| `$FFC0` | `title` | 21 bytes, ASCII `$20–$7E` or JIS X 0201, padded with spaces |
| `$FFD5` | `map_mode` | Bits 7–5 are `001`, bit 4 asks for FastROM (`Header::fast_rom`), bits 3–0 are the map mode |
| `$FFD6` | `chipset` | ROM, RAM, battery and coprocessor |
| `$FFD7` | `rom_size` | `1 SHL n` KiB, rounded up |
| `$FFD8` | `ram_size` | `1 SHL n` KiB, 0 for none |
| `$FFD9` | `country` | Implies NTSC or PAL; see [Region](#region) |
| `$FFDA` | `developer` | `$33` means an extended header at `$FFB0` |
| `$FFDB` | `version` | 0 for the first release |
| `$FFDC` | `checksum_complement` | The checksum XOR `$FFFF` |
| `$FFDE` | `checksum` | The 16-bit sum of every ROM byte |
| `$FFFC` | `reset_vector` | Where the CPU starts, in bank `$00` |

### Where the header is in a file

A ROM file holds the ROM in the order the board maps it. For each board, the header area is at a different file offset (`fullsnes#snescartridgeromheader`, `fullsnes#snescartridgeromimageinterleave`):

| Board (`Mapping`) | Header at | Map modes |
| --- | --- | --- |
| `LoRom`: 32 KiB banks at `$8000` | `$7FC0` | 0, 2 (S-DD1), 3 (SA-1) |
| `HiRom`: 64 KiB banks | `$FFC0` | 1, `$A` (SPC7110) |
| `ExHiRom`: the fast 4 MiB first, then the slow banks | `$40FFC0` | 5 |

Old copiers put a 512-byte header of their own in front of the ROM. It holds nothing an emulator needs. A file carries one exactly when its length is 512 modulo 1024, since ROMs are whole kilobytes (`fullsnes#snescartridgeromimageheadersandfileextensions`). `strip_copier_header` drops it before anything else, and `Header::copier_header` records that it was there. The ROM's identity hash is taken over the stripped image (EMU-04).

### Finding the header

Nothing in a file says which board it is for, so `Header::detect` reads all three locations that the image is long enough to hold and scores each one (ADR 0016):

1. A location whose RESET vector is outside `$8000–$FFFE` is rejected. The cartridge only answers at `$8000–$FFFF` in bank `$00`, and fullsnes gives the range even for homebrew with broken headers.
2. The rest score up to 12 points:
   - 4 when the stored checksum equals the image's checksum;
   - 4 when checksum XOR complement is `$FFFF`;
   - 2 when `$FFD5` is well formed and names this board;
   - 1 for a printable title;
   - 1 for a ROM size byte that covers the image.
3. The highest score wins. A tie, or no location left, is an error that names the boards involved, so the caller can pass `Options::mapping` and skip scoring.

The checksum comes first because both documents call it the main evidence. A real ROM's stored checksum matches at one location only. Homebrew often leaves the checksum at zero (`fullsnes#snescartridgeromheader`), so it can still be found by its map mode, title and size.

### The checksum

`checksum` adds every byte of the image into 16 bits. An image whose length is not a power of two is summed the way the cartridge mirrors it (`fullsnes#snescartridgeromheader`, `snesdev-wiki#ROM_header`; ADR 0016, ADR 0017):
- The largest power-of-two part comes first.
- The rest is summed the same way, as a window of its length rounded up to a power of two.
- That window is repeated until it is as long as the first part.

So the checksum is the sum of every byte the memory map reads in a window of the image's length rounded up to a power of two (see Mirroring).

A 5 MiB ExHiROM is therefore summed as 4 MiB plus four copies of its last 1 MiB, as fullsnes lists for *Dai Kaiju Monogatari 2*. The stored checksum and complement always add the same `$1FE` to the sum, whatever their values, so a header with a valid pair matches the plain sum.

## Region

The country byte implies the video standard (`fullsnes#snescartridgeromheader`). `Region::from_country` returns:
- **NTSC** for `$00`, `$01`, `$0D`, `$0F` and `$10`. Brazil's PAL-M is a 60 Hz standard.
- **PAL** for `$02–$0C` and `$11`. France's SECAM is a 50 Hz standard.
- **None** for the codes fullsnes marks "(?)" and for any higher code. The machine then needs `Options::region` or uses its own default.

An explicit `Options::region` always replaces the header's region (EMU-09).

## The memory map

The bus looks up what answers an address in a [`MemoryMap`](../../crates/sfes-snes-cart/src/map.rs): 2,048 pages of 8 KiB, built once when the cartridge is loaded and shared between clones (technical design section 5, ADR 0017). Each page is `Rom`, `CartRam`, `Wram`, `Io`, `Coproc` or `Open`, with ROM and RAM offsets already mirrored. `MemoryMap::resolve` adds the offset inside the page.

### The console's part

Some areas are the same whatever the cartridge (`anomie-memmap#memory-map`, `fullsnes#snesmemorymap`):

| Banks | Addresses | Page |
| --- | --- | --- |
| `$00–$3F`, `$80–$BF` | `$0000–$1FFF` | `Wram`, a mirror of `$7E:0000–$1FFF` |
| `$00–$3F`, `$80–$BF` | `$2000–$5FFF` | `Io`: Address Bus B at `$2100–$21FF` and the CPU's registers at `$4000–$43FF`, decoded by the bus |
| `$7E–$7F` | all | `Wram`, 128 KiB |

The cartridge sees all 24 address lines (`anomie-memmap#hardware/address-bus-a`). It answers `/CART` at `$8000–$FFFF` of the system banks and across `$40–$7D` and `$C0–$FF`, and may answer `$6000–$7FFF` too. Whatever the board leaves unmapped is `Open`.

### The boards

[`Board`](../../crates/sfes-snes-cart/src/board.rs) gives each address's ROM or SRAM offset, before mirroring. The header's map mode decides the board (ADR 0016). For each family, the map follows the plain board and the most common SRAM board in fullsnes's tables (`fullsnes#snescartlorommappingromdividedinto32kbanksaround1500games`, `fullsnes#snescarthirommappingromdividedinto64kbanksaround500games`):

| Board | ROM | SRAM |
| --- | --- | --- |
| `LoRom` | 32 KiB per bank at `$00–$7D,$80–$FF:8000–FFFF`, mirrored to the lower halves of `$40–$7D,$C0–$FF` | `$70–$7D,$F0–$FF:0000–7FFF`, 32 KiB per bank |
| `HiRom` | 64 KiB per bank at `$40–$7D,$C0–$FF`; the upper halves mirrored to `$00–$3F,$80–$BF` | `$20–$3F,$A0–$BF:6000–7FFF`, 8 KiB per bank |
| `ExHiRom` | as HiROM, but banks below `$80` hold the slow ROM that follows the first 4 MiB | `$80–$BF:6000–7FFF` |

Mirroring the HiROM upper halves into bank `$00` is what puts the vectors at `$00:FFE0`. So `$00:FFC0` reaches the header area that detection read: `$7FC0`, `$FFC0` or `$40FFC0` (`fullsnes#snesmemorymap`, `fullsnes#snescartridgeromimageinterleave`).

### Mirroring

A ROM smaller than its area repeats. [`mirror`](../../crates/sfes-snes-cart/src/board.rs) follows the checksum's layout (`fullsnes#snescartridgeromheader`): the largest power-of-two part, then the remainder repeated. A 3 MiB LoROM therefore reads its last 1 MiB twice. A 5 MiB ExHiROM reads its last 1 MiB in each of `$40`, `$50`, `$60` and `$70`. A remainder that is not a power of two is split the same way again (ADR 0017).

The SRAM size comes from the header, `1 SHL n` KiB. It repeats through the board's whole SRAM area, modulo its size, so even a size that is not a power of two never reaches past its end. Sizes over 128 KiB are refused (ADR 0017).

### The cartridge

[`Cartridge::load`](../../crates/sfes-snes-cart/src/cartridge.rs) detects the header, keeps the ROM without its copier header, allocates the SRAM and builds the map. It exposes the ROM as `RegionId::Rom` and the SRAM as `RegionId::CartRam`, which is the only writable region (EMU-04, FND-06).

## Not handled yet

- Interleaved images from old copiers (`fullsnes#snescartridgeromimageinterleave`).
- Board variants with other SRAM areas, SRAM across the whole LoROM bank, or open ROM banks (ADR 0017).
- Satellaview and Sufami Turbo headers.
- The checksums fullsnes lists for SPC7110 boards.

Such images may score wrongly and then need a board override.

## The code

| File | What it holds |
| --- | --- |
| [`header.rs`](../../crates/sfes-snes-cart/src/header.rs) | `Header`, `Mapping` and `Options`; `Header::detect` and `detect_with`, the score, `strip_copier_header`, `checksum` and `Region::from_country` |
| [`board.rs`](../../crates/sfes-snes-cart/src/board.rs) | `Board`: each board's ROM and SRAM offset for an address, and `mirror` |
| [`map.rs`](../../crates/sfes-snes-cart/src/map.rs) | `MemoryMap` and `Page`: the 2,048-page table and `resolve` |
| [`cartridge.rs`](../../crates/sfes-snes-cart/src/cartridge.rs) | `Cartridge::load` and `CartError`; the ROM, SRAM and map, and the regions they expose |
| [`test_support.rs`](../../crates/sfes-snes-cart/src/test_support.rs) | ROM images for tests, behind the `test-support` feature (below) |

The crate is `no_std` with `alloc` and depends only on `sfes-api` and `thiserror` (technical design section 12). Only `Cartridge` holds state that changes: the SRAM. The ROM and the map are `Arc`-shared, so cloning a machine never copies them (technical design section 5).

## Testing

- **Unit tests** sit beside the code in each file. Each cites the section its expected values come from.
- **Test images.** `test_support` builds a ROM image for a board: a plausible header, a valid checksum and, optionally, code at the RESET vector. `sfes-snes-core` enables the feature in its `[dev-dependencies]`, so the machine's tests load the same images instead of copies (AGENTS.md section 4).
- **The own cartridge.** [`tests/carts/halt`](../../tests/carts/halt/halt.s), the smallest test cartridge (ADR 0013), must be detected as LoROM with the full score of 12. Its RESET vector must resolve to its ROM through the map.

## Decisions

The documents left these points open. Each ADR quotes the sources and states its confidence.

| ADR | Decision | Confidence |
| --- | --- | --- |
| [0016](../adr/0016-cartridge-header-detection.md) | Candidate locations, the reset-vector rule, the score weights, overrides and the region table; amended for the checksum of an image whose length is not a power of two | medium |
| [0017](../adr/0017-memory-map-and-boards.md) | The page table, each board's ROM and SRAM areas, mirroring and the RAM size limit | medium |

ADRs 0013 to 0015 set up the test cartridges, the third-party test ROMs and the golden-frame files that M2's later issues use.
