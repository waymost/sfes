# Exposure log

Accidental exposures to emulator source code (PROVENANCE.md, section 1). Append only; never edit or delete an entry (PROV-11).

| Date | Who | What was seen (project, file, approximate extent) | How | Modules it could touch |
| --- | --- | --- | --- | --- |
| 2026-11-12 | Claude Code (claude-opus-5-5), and the project owner while retrieving it | `apudsp_jwdonal.txt` (anomie's S-DSP doc with jwdonal's updates) quotes emulator code: a ~10-line global-counter function and its two 32-entry mask/xor tables attributed to Mednafen, and a 512-entry Gaussian table plus ~10 lines of interpolation code attributed to libopenspc | The whole document was read into the session while registering it in the bibliography | `sfes-snes-spc` (S-DSP global counter, envelope/noise rate timing, Gaussian interpolation) |
