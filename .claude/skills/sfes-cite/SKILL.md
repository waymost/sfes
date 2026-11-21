---
name: sfes-cite
description: Choose the right `// @source:` tag and anchor for sfes code, tests and test carts. Use whenever writing or reviewing a chip-crate function, module or test, a test cartridge, or an ADR citation, or when a lint error names a missing or unknown source. Gives anchor forms per bibliography id without loading the whole bibliography.
---

# Citing sources in sfes

Do not read `provenance/bibliography.toml` whole. It is about 190 lines of vetting history. The table below gives the anchor form for each id. For one entry's full notes:

```bash
.claude/skills/sfes-cite/scripts/source.py wdc-w65c816s     # one entry
.claude/skills/sfes-cite/scripts/source.py --list           # ids and titles
```

## What gets a tag (AGENTS.md section 1)

- Every chip-crate module and every function in a chip crate, helpers and private ones included. Only methods of standard-library trait impls are exempt.
- Every test in a chip crate or `sfes-test-harness`.
- Not `xtask` or `sfes-api` tests whose expected values come from no document.
- Test cartridges use `; @source:` in the file's leading comments and in the comment lines directly above each `.proc` and `.macro`. An expected value a cart checks cites its source beside it.

```rust
// @source: wdc-w65c816s#table-5-7-1a
// @source: adr-0012
```

One `@source:` line per source. Cite the most precise place the document offers. A test driven by vectors cites `sst-65816` or `sst-65816#a9.n`. The function under test still cites the document it was written from, because vectors are evidence and not a specification.

## Anchor forms

| id | anchor | example |
|---|---|---|
| `wdc-w65c816s` | printed number: section `2.20`; `table-5-4`, `figure-2-1`; Table 5-7 row adds the Address Mode label; `-note-N` | `#table-5-7-1a`, `#table-5-7-note-4` |
| `wdc-65816-manual` | `chapter-N`, optional kebab heading | `#chapter-18-adc` |
| `6502org-65c816-opcodes` | printed section number, or `appendix` | `#5.1.1` |
| `fullsnes` | the page's own HTML anchor name | `#snesdmatransfers` |
| `anomie-regs` | register address, lower case (`2100`, `43x0`), or kebab heading path joined by `/` | `#210d`, `#backgrounds/mode-7` |
| `anomie-timing` | kebab heading path; top level is the chip (`s-cpu`, `s-ppu`, `s-apu`) | `#s-cpu/interrupts` |
| `anomie-memmap` | kebab heading path | `#hardware/data-bus` |
| `anomie-spc700`, `anomie-dsp` | no anchor scheme recorded; check `source.py <id>` | |
| `snesdev-wiki` | page title as in its URL | `#DMA_registers` |
| `sst-65816` | vector file stem, optional | `#a9.n` |
| `adr-NNNN` | an ADR number, no anchor | `adr-0022` |

Kebab case: lower-case the heading, write `&` as `and`, drop parenthesised text and other punctuation, join words with `-`.

Rules that are easy to get wrong:

- For a 65C816 cycle sequence, cite the Table 5-7 row, not the table or the chapter.
- Where the datasheet and the programming manual differ, the datasheet wins unless an ADR says otherwise.
- Where a community source (6502.org, snesdev-wiki) and the datasheet differ, write an ADR.
- When two sources disagree or are silent, write an ADR (`sfes-adr` skill) and cite it as `adr-NNNN` rather than guessing.
- Only cite a section you have read. If it is not readable (the PDFs stay outside the repo), do not write the code.

## Restricted passages

Never use these as a source. If you notice you are recalling emulator code, stop, say so, and log it in `provenance/exposures.md`.

- the "Boot ROM Disassembly" listing in `fullsnes#snesapumaincpucommunicationport`. The uploader protocol above it is fine.
- the two emulator-derived passages in `anomie-dsp`; `source.py anomie-dsp` says which.

## A source that is not registered

Do not invent an entry. A new entry needs a licence, a retrieval date and a SHA-256 of the copy the project owner retrieved, and a vetting note. Stop and ask for it. A `snesdev-wiki` page also needs its HTML archived under `provenance/archive/snesdev-wiki/` when first cited.

## Check

`cargo xtask provenance lint` checks that every chip-crate module and function has a known tag. Fix what it reports; do not guess more anchors.
