# Provenance policy

sfes is a clean-room SNES emulator core. Every behaviour in it traces to a cited hardware document, test, or measurement, and **no emulator's source code may be consulted, ever**. This file is the source policy required by PROV-01 and is binding for every commit, human-written or AI-assisted.

## 1. Sources (PROV-01)

### Allowed

- Manufacturer datasheets, e.g. the WDC W65C816S datasheet.
- Community hardware documents such as [fullsnes](https://problemkaputt.de/fullsnes.htm) and anomie's documents.
- The [SNESdev wiki](https://snes.nesdev.org) (CC0).
- Published die-shot and decap research; patents.
- Test ROMs and test vectors whose licenses permit use (section 6).
- The project's own hardware measurements and test cartridges.
- The **output** (never the code) of oracle emulators run as black boxes (section 3).

Every source used is registered in [`provenance/bibliography.toml`](provenance/bibliography.toml) (PROV-02) and cited from code with a source tag (section 2).

### Forbidden

- Source code of any emulator, under any license, **including permissive ones**.
- Decompilations or disassemblies of emulator binaries.
- Forum posts, issue comments, blog posts or answers that quote emulator code.
- Leaked or confidential Nintendo documents. The SNESdev wiki links an archived copy of Nintendo's *SNES Development Manual* without stating how it was released; its status is uncertain, so it is **not used**. This was confirmed by the project owner and will be revisited only with legal advice.

### Accidental exposure

If emulator code is seen by accident (a search result, a linked snippet, an assistant's output that looks recalled rather than derived), add an entry to [`provenance/exposures.md`](provenance/exposures.md) with the date, what was seen, and which modules it could touch. Do not work on those modules from memory of what was seen.

## 2. Source tags and ambiguity records (PROV-03, PROV-04)

- Every module in a chip crate (`sfes-snes-*`) and every function in one carries at least one tag naming a bibliography id or an ADR. Only methods of standard-library trait impls are exempt:

  ```rust
  // @source: fullsnes#dma-and-hdma
  // @source: adr-0012
  ```

- The anchor after `#` names the place in the source, in the form the source's bibliography entry defines, and is as precise as the source allows (a table row, not a chapter).
- Tests cite the source of their expected values the same way. A test driven by test vectors cites the vector set, e.g. `sst-65816`.
- Test vectors are evidence, not a specification: code is written from a document and checked against the vectors. Behaviour that only the vectors settle is recorded in an ADR.
- `cargo xtask provenance lint` fails CI when a chip-crate file or any function in one (other than methods of standard-library trait impls) lacks a tag, or a tag names an unknown id.
- When sources disagree or are silent, an ADR in `docs/adr/` records what each source says, the deciding evidence (test ROM, measurement, oracle output), the decision and its confidence.

## 3. Oracle firewall (PROV-05)

- Reference emulators (bsnes, Mesen, Snes9x and others) may be used **only as black-box binaries**.
- Only their outputs (frame hashes, RAM dumps, audio hashes, traces) are stored, as golden data labelled with the oracle name and version.
- Their source must not exist in this repository, CI images, dev containers or any AI tool's context.
- `cargo xtask provenance scan` searches code files for known emulator copyright headers and author names and fails on a hit.

## 4. Contributors (PROV-06, PROV-07)

- Every commit carries a DCO `Signed-off-by` line (`git commit -s`), certifying the [Developer Certificate of Origin](https://developercertificate.org/).
- Every pull request includes the statement *"I did not consult emulator source code for this change."*
- **Outside contributions are not accepted yet.** The codebase stays single-author until the provenance tooling (PROV-03, PROV-05, PROV-06) is proven in use. After that, first-time contributors must acknowledge this file.
- **AI assistance** must be disclosed with an `Assisted-by: <tool>` commit trailer. No emulator source may be placed in an assistant's context or workspace. AI-written chip code must carry source tags and is reviewed line by line against the cited source before merge; models can reproduce memorized emulator code, so code with no matching documentation is rewritten. The PR template's AI-review checklist records this review, and `cargo xtask provenance commits` blocks AI-assisted chip changes without it.

## 5. Dependency licenses (PROV-08)

`cargo-deny` allows only **MIT, Apache-2.0, BSD-2-Clause, BSD-3-Clause, ISC, Zlib, Unicode and CC0** licenses and denies GPL, LGPL, AGPL and non-commercial licenses. Python dependencies follow the same list.

This list applies to every library crate and the Python wheel. The reference player binary (`sfes-player`) additionally allows **MPL-2.0, OFL-1.1 and UFL-1.0** (fonts and platform helpers). No dependency allowed only for the player may reach a library crate; `cargo xtask deps` checks this.

## 6. Test assets (PROV-09)

- Every committed test ROM or vector file has its license recorded in [`tests/assets/MANIFEST.toml`](tests/assets/MANIFEST.toml).
- Assets with no stated license are never committed; they are fetched at test time by URL and hash, pending permission.
- Commercial ROMs, coprocessor firmware and the SPC700 IPL ROM are never committed or distributed in any form. Tests needing them skip with a clear message.

## 7. Remediation (PROV-11)

If a violation is found:

1. Freeze the record: never edit or delete history, commits, issues or comments about it.
2. List the affected code.
3. Rewrite it from documents, by someone not exposed to the source.
4. Publish a notice.

Rewording citations to hide a derivation is forbidden.

## 8. Project license (PROV-12)

All crates and the Python package are licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
