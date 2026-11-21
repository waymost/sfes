#!/usr/bin/env bash
# Create the next docs/adr/NNNN-slug.md from the template.
# Usage: new-adr.sh "Title in sentence case"
set -euo pipefail

title="${1:?usage: new-adr.sh \"Title in sentence case\"}"
root="$(git rev-parse --show-toplevel)"
dir="$root/docs/adr"

last="$(find "$dir" -maxdepth 1 -name '[0-9][0-9][0-9][0-9]-*.md' -exec basename {} \; | sort | tail -1)"
next="$(printf '%04d' $((10#${last%%-*} + 1)))"
slug="$(printf '%s' "$title" | tr '[:upper:]' '[:lower:]' | sed -E 's/[^a-z0-9]+/-/g; s/^-+|-+$//g')"
path="$dir/$next-$slug.md"

[ -e "$path" ] && { echo "refusing to overwrite $path" >&2; exit 1; }

sed -e "s/^# NNNN\. Title/# $next. $title/" \
    -e "s/^- \*\*Status:\*\* .*/- **Status:** proposed/" \
    -e "s/^- \*\*Date:\*\* .*/- **Date:** $(date +%Y-%m-%d)/" \
    -e "s/^- \*\*Confidence:\*\* .*/- **Confidence:** high | medium | low/" \
    "$dir/0000-template.md" > "$path"

echo "created ${path#"$root"/}"
echo "tag in code and tests:  // @source: adr-$next"
echo "design-doc citation:    (ADR $next)"
