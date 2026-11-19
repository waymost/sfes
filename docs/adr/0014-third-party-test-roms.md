# 0014. Third-party test ROMs

- **Status:** accepted
- **Date:** 2026-11-18
- **Confidence:** high

## Context

M2's exit criterion asks that "listed homebrew and test ROMs reach golden frame hashes". Issue #74 surveyed candidates from their README, LICENSE and file-list pages only, and the project owner approved the list there. Some approved ROMs carry a license that allows committing them; most state no license at all. PROV-09 forbids committing those and asks that they be fetched at test time by URL and hash. The vector sets are already fetched (`cargo xtask vectors`), but they are whole git repositories pinned by commit, while these are single files.

## Sources

- FSD PROV-08 (the license list), PROV-09 (asset licensing; unlicensed assets fetched by URL and hash) and QA-01 layer 3 (test ROMs).
- `PROVENANCE.md` section 6 and the header of `tests/assets/MANIFEST.toml`.
- Issue #74: the survey, its follow-up and the owner's approval comment.
- ADR 0013, which commits the project's own test cartridges.

## Decision

- **The approved list**, as #74's approval comment states it:
  - gilyon `cputest-basic.sfc` and `cputest-full.sfc` from release v1.4 (MIT);
  - six rainwarrior ROMs from `bbbradsmith/SNES_stuff`: `extbgtest`, `twoship`, `smalltext`, `colourmath`, `elasticity`, `multest_mul16` (no license stated);
  - fifteen krom ROMs from `PeterLemon/SNES`: `BANK/*`, `Mode7/{RotZoom,Perspective}`, `Mosaic/*`, `Window/*`, `HDMA/{RedSpaceHDMA,RedSpaceIndirectHDMA,WaveHDMA}` and `Interlace/InterlaceFont` (no license stated).
- **blargg's OAM tests are not added.** Approval was given on the condition that their zip, listed before extraction, held no source other than 65816 assembly. The listing showed `oam.c` and `test.cpp` beside the ROMs, so the zip was deleted unopened and the drop is logged in `provenance/exposures.md`.
- **Committed third-party ROMs** live in `tests/assets/third-party/<collection>/`, with the license text the license requires beside them and the collection's own expected-value files where it has them (gilyon's `tests-*.txt`). `.gitignore` admits `.sfc` files there and nowhere else outside the own cartridges.
- **Fetched ROMs** are manifest file entries under `tests/assets/fetched/<collection>/` with `fetch = true`, an `https://` `source_url` at a pinned commit, and the SHA-256. Git ignores `fetched/`. `license = "NOASSERTION"` (the SPDX term for no stated license) is allowed only on such entries; `cargo xtask provenance assets` reports it anywhere else, a fetched entry outside `fetched/`, and a committed one inside it.
- **`cargo xtask roms fetch`** downloads every fetched file that is missing or differs, with `curl` (HTTPS only, redirects included), and keeps it only when its SHA-256 matches the manifest. `curl` ships with all three CI runners, so no new dependency is needed. The vector sets keep their own command and lock file.
- **Tests** load a ROM through `sfes_test_harness::roms::load`, by its path relative to `tests/assets/`. An absent ROM skips the test with a message, unless `SFES_REQUIRE_VECTORS=1`, which CI sets, makes it an error.
- **CI** runs `cargo xtask roms fetch` in every test job, with `tests/assets/fetched/` cached under a key derived from the manifest.
- These ROMs are evidence, not specification (M2 tracking rule 5). Code still cites the document it was written from. A golden from one of them is blessed by the owner against MesenCE output, and a behaviour that only a ROM settles gets its own ADR naming the ROM.

## Consequences

- Adding an approved ROM is one manifest entry, plus the file itself when its license allows committing it. A source that disappears or changes a file fails `roms fetch` until the entry is repinned.
- A collection that ships only as an archive (as blargg's did) needs an extraction step that `roms fetch` does not have. One is added when an approved ROM needs it.
- Later surveys add to this list through the owner's approval in an issue, and amend this ADR's list.
