#!/usr/bin/env bash
# Run the AGENTS.md section 3 checks and print only what failed.
#
# Usage: preflight.sh [--fix] [--bail] [--only step[,step...]]
#   --fix    apply rustfmt instead of only checking it
#   --bail   stop at the first failing step
#   --only   run just these steps: fmt clippy test deny lint scan assets deps carts
# Output: one line per step; for a failed step, the first lines of its output and the path of the full log.
set -uo pipefail

fix=0 bail=0 only=""
while [ $# -gt 0 ]; do
  case "$1" in
    --fix) fix=1 ;;
    --bail) bail=1 ;;
    --only) only="${2:?--only needs a list}"; shift ;;
    *) echo "unknown argument: $1" >&2; exit 2 ;;
  esac
  shift
done

cd "$(git rev-parse --show-toplevel)"
logs="$(mktemp -d "${TMPDIR:-/tmp}/sfes-preflight.XXXXXX")"
fmt_config="group_imports=StdExternalCrate,imports_granularity=Crate"
max_lines=40
failed=0

wanted() { [ -z "$only" ] || [[ ",$only," == *",$1,"* ]]; }

step() { # name, command...
  local name="$1"; shift
  wanted "$name" || return 0
  [ "$bail" = 1 ] && [ "$failed" != 0 ] && return 0
  local log="$logs/$name.log"
  if "$@" >"$log" 2>&1; then
    printf 'ok    %s\n' "$name"
  else
    failed=$((failed + 1))
    printf 'FAIL  %s  (full log: %s)\n' "$name" "$log"
    grep -v '^\s*$' "$log" | head -n "$max_lines" | sed 's/^/      /'
    local total; total="$(grep -vc '^\s*$' "$log")"
    [ "$total" -gt "$max_lines" ] && printf '      ... %s more lines in the log\n' "$((total - max_lines))"
  fi
}

if [ "$fix" = 1 ]; then
  step fmt cargo +nightly fmt -- --config "$fmt_config"
else
  step fmt cargo +nightly fmt --check -- --config "$fmt_config"
fi
step clippy cargo clippy --workspace --all-targets --message-format short -- -D warnings
step test   cargo nextest run --workspace --status-level fail --final-status-level fail --no-tests=warn
step deny   cargo deny check
step lint   cargo xtask provenance lint
step scan   cargo xtask provenance scan
step assets cargo xtask provenance assets
step deps   cargo xtask deps

if wanted carts; then
  if command -v ca65 >/dev/null 2>&1; then
    step carts cargo xtask carts check
  else
    printf 'skip  carts  (cc65 2.19 not installed; CI runs it on Linux, ADR 0013)\n'
  fi
fi

if [ "$failed" = 0 ]; then
  echo "all selected checks passed"
else
  echo "$failed step(s) failed"
  [ "$fix" = 0 ] && echo "formatting failures: rerun with --fix"
  exit 1
fi
