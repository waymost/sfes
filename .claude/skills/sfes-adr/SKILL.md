---
name: sfes-adr
description: Write a new sfes Architecture Decision Record in docs/adr/. Use when a hardware ambiguity or source conflict needs a decision, a chip or core crate gains a dependency, a format changes, or the user asks for an ADR. Scaffolds the next-numbered file and lists what else must change in the same PR.
---

# Writing an sfes ADR

AGENTS.md section 2 says when one is required. Do not re-read it. Do not open the template or a neighbouring ADR to copy the shape; the script and the checklist below carry it.

## 1. Scaffold

```bash
.claude/skills/sfes-adr/scripts/new-adr.sh "Short title in sentence case"
```

It picks the next number, writes `docs/adr/NNNN-slug.md` from `0000-template.md` with today's date and `Status: proposed`, and prints the path and the two tag forms to use. Edit that file; do not create ADRs by hand.

## 2. Fill it in

- **Context**: the question and why it matters. Name earlier ADRs it builds on, as in "ADR 0004, point 3".
- **Sources**: one bullet per cited source, as `bibliography-id#anchor` and what it says. State the deciding evidence (test ROM, vector set, or oracle output with name and version). Where sources disagree or are silent, say that. Look up anchor forms with the `sfes-cite` skill; do not guess them.
- **Decision**: numbered points, so later ADRs can cite "point 2".
- **Consequences**: what follows, and what evidence would make us revisit.
- **Confidence**: high, medium or low. If it varies by point, say which is which (see ADR 0022's header).
- Set `Status: accepted` when the code lands.

Read the cited section before writing a claim about it. If you cannot read it, say so in the ADR and lower the confidence. Do not fill the gap from memory of another emulator (PROVENANCE.md; log it in `provenance/exposures.md`).

## 3. Same PR, same commit

An ADR ships with the code it governs; recent history is `feat(core): ... (ADR 0021)`, not a separate docs commit. A docs-only ADR is `docs(adr): ...`.

- Code that implements the decision carries `// @source: adr-NNNN`. Tests do too.
- If the ADR changes or refines `docs/design/technical-design.md` or `docs/design/feature-specification.md`, edit that text in the same PR and put "(ADR NNNN)" at the changed text. Find the spot with `grep -n "<topic>" docs/design/*.md`.
- If the ADR supersedes an older one, set the old one's status to `superseded by NNNN`.
- To correct an accepted ADR without changing its decision's intent (a slip, a detail), edit it in place and add `- **Amended:** YYYY-MM-DD: what changed (where).` under its header. A changed decision is a new ADR.
- Where a cited document is wrong, say so in Sources and add the slip to that entry's "Known slips" in `provenance/bibliography.toml`.
- Run `cargo xtask provenance lint` (or the `sfes-preflight` skill) to check that every function and module still carries a known tag.
