#!/usr/bin/env python3
"""Print one bibliography entry (or the id list) without loading the whole file.

Usage: source.py <id> | --list
"""
import subprocess
import sys
import textwrap
import tomllib
from pathlib import Path


def main() -> int:
    if len(sys.argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    root = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
    sources = tomllib.loads((root / "provenance/bibliography.toml").read_text())["source"]

    if sys.argv[1] == "--list":
        for source in sources:
            print(f"{source['id']:24} {source['title']}")
        return 0

    for source in sources:
        if source["id"] == sys.argv[1]:
            for key in ("id", "title", "url", "license", "redistributable", "retrieved", "version"):
                if key in source:
                    print(f"{key}: {source[key]}")
            print("notes:")
            print(textwrap.fill(" ".join(source.get("notes", "").split()), 110, initial_indent="  ", subsequent_indent="  "))
            return 0

    ids = ", ".join(source["id"] for source in sources)
    print(f"no such id; registered: {ids}", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
