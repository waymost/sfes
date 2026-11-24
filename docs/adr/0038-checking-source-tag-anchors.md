# 0038. Checking source-tag anchors

- **Status:** accepted
- **Date:** 2026-11-24
- **Confidence:** high

## Context

`cargo xtask provenance lint` checked that a tag's id is a bibliography entry or an ADR, but not the anchor after `#`. The PPU review found anchors that named no place in their document: `anomie-timing#oam-reset` and `#detailed-renderer-timing` lacked the `s-ppu/` level the entry's notes require, and an earlier example tag, `fullsnes#dma-and-hdma`, matched no anchor in fullsnes. A tag whose anchor is wrong sends a reader to the wrong place, or nowhere, and PROV-03 exists so that a reader can find the section a behaviour came from.

A full check needs the documents, and most of them cannot enter the repo: the WDC datasheet and manual are "all rights reserved" and the extracted text stays outside it, and anomie's files and fullsnes are kept by the project owner as reading copies (`provenance/bibliography.toml`). CI therefore cannot read them. The project owner chose, on 2026-11-24, a form check in CI plus an optional check against local copies, so that no text derived from a document enters the repo.

## Sources

- **PROVENANCE.md**: the anchor "names the place in the source, in the form the source's bibliography entry defines".
- **`provenance/bibliography.toml`**, each entry's `notes`: the anchor forms, e.g. anomie's heading paths in kebab case, fullsnes's HTML anchor names, the datasheet's printed section, table, figure, row and note numbers.
- **ADR 0013**: test-cartridge sources carry the same tags.

## Decision

1. **Two new optional fields per bibliography entry.**
   - `anchor`: a regular expression that the whole anchor must match, written from the entry's `notes`. An entry without it takes no anchor: a tag `id#…` for it is a finding. An ADR tag takes no anchor either.
   - `file`: the file name of the project owner's reading copy (for example `regs.txt`), used only by point 3.
2. **The form check runs always**, in `provenance lint`, for every tag under `crates/` and `tests/carts/`. A `snesdev-wiki` anchor must also name a page archived as `provenance/archive/snesdev-wiki/<anchor>.html`, since the bibliography notes require that archive at first citation.
3. **The documents check runs on request**: `cargo xtask provenance lint --documents <dir>` reads each entry's `file` from `<dir>` and reports every cited anchor not found there. It is for the project owner, or anyone with the reading copies, to run before a release or after adding citations. A missing file is a finding, so a partial directory is visible. What counts as found, per entry:
   - `fullsnes`: an `<a name="…">` in the HTML, case-insensitive.
   - `anomie-regs`, `anomie-timing`, `anomie-memmap`: a heading path, built from the headings underlined with `=` (top level) and `-` (the level below) and kebab-cased as the notes say; and for `anomie-regs`, a register entry, an address at the start of a line in the REGISTERS section.
   - `wdc-w65c816s`: a section number at the start of a line followed by its title; `Table N-M` or `Figure N-M` in the text; a Table 5-7 row label (`1a.` and so on at the start of a line); a note's table must exist.
   - `wdc-65816-manual`: the chapter number must appear as `Chapter N`, and a heading must be the kebab case of a line of the text or the start of one.
   - `6502org-65c816-opcodes`: a section number printed at the start of a line of the page's text, or `appendix` when the page has one.
   - `sst-65816` and `snesdev-wiki` need no file: the form and the archive settle them.
4. **The derived anchor sets live only in memory** while the check runs. Nothing from a document is written into the repo.

## Consequences

- CI catches malformed anchors, the class the review found, but not a well-formed anchor for a heading that does not exist. Only the documents check finds those, and it runs only where the reading copies are.
- The documents check is lenient where the documents give no structure to parse: a manual heading is matched anywhere in the text, not inside its chapter, and a datasheet note is accepted if its table exists. Tightening either would need a parser per document layout.
- A new source with anchors adds an `anchor` pattern, and a `file` and a parser in `xtask/src/provenance/anchors.rs` if it is to be checked against its copy.
- Revisit if a source becomes redistributable, when its full anchor list could be checked in CI.
