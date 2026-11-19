## Summary

<!-- What changes and why. Link issues with "Closes #N". -->

## Golden changes

<!-- Required when a file in tests/assets/golden/ changes: say why each changed frame is right,
     and for a blessed entry which oracle and version it was compared with (ADR 0015).
     Write "None" otherwise. -->

## Definition of done

- [ ] Tests added or updated; golden-file changes explained below.
- [ ] Public items documented; chip explainer updated for behaviour changes.
- [ ] `@source:` tags present for hardware behaviour, with an ADR for any judgement call.
- [ ] An ADR that changes or refines `docs/design/` updates that document in this PR, citing the ADR at the changed text.
- [ ] fmt, clippy, cargo-deny and provenance checks pass.

## Attestation (required)

- [ ] I did not consult emulator source code for this change.

## AI-review checklist (required when any commit has an `Assisted-by` trailer)

- [ ] AI-review complete: every AI-written hardware function was read line by line against its cited source section, and code with no matching citation was rewritten.
- [ ] No emulator source was placed in the assistant's context or workspace.
