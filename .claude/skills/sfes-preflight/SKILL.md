---
name: sfes-preflight
description: Run the sfes pre-push checks (fmt, clippy, nextest, deny, provenance lint/scan/assets, deps, carts) and see only the failures. Use before committing or pushing, after a large edit, or when asked to run CI locally or "the checks".
---

# sfes pre-push checks

Run the script. Do not run the nine commands from AGENTS.md section 3 by hand, because cargo's full output is most of the token cost.

```bash
.claude/skills/sfes-preflight/scripts/preflight.sh            # all steps, failures only
.claude/skills/sfes-preflight/scripts/preflight.sh --fix      # apply rustfmt first
.claude/skills/sfes-preflight/scripts/preflight.sh --only clippy,lint   # re-run just what failed
```

It prints one `ok` / `FAIL` / `skip` line per step. For a failure it shows the first 40 lines and the path to the full log. Read the log only when the excerpt is not enough, and then read only the part you need.

## Working through failures

- Fix, then re-run with `--only <failed steps>`. Run the full set once at the end.
- The rules clippy enforces are in AGENTS.md section 4. Fix what it reports instead of re-reading them. Clippy runs with `--message-format short`: one line per diagnostic.
- `fmt` needs the nightly toolchain and the import-grouping config; `--fix` handles both.
- `lint` failing means a missing, unknown or malformed `@source:` tag. Use the `sfes-cite` skill; do not guess an anchor.
- `scan` flagging a pattern means text that looks like emulator source. Do not reword it to dodge the pattern. Stop, and follow PROVENANCE.md and `provenance/exposures.md`.
- `carts` is skipped when `ca65` is missing. Say so when you report; CI runs it on Linux.
- `test` uses `--status-level fail`, so only failing tests print. Pass `-E 'test(name)'` to nextest directly when iterating on one test.

## Not covered

CI also runs `cargo xtask provenance commits` on pull requests (sign-off and trailers) and the `no_std` builds for `thumbv7em-none-eabihf` and `wasm32-unknown-unknown`. After touching a chip or core crate's dependencies or `std` use, run:

```bash
cargo build -p <crate> --target thumbv7em-none-eabihf
```

Report honestly which steps ran, which failed and which were skipped.
