#!/usr/bin/env python3
"""Scaffold, register and golden-ize an sfes own test cartridge (ADR 0013, ADR 0015).

  cart.py new <name> <lorom|hirom> ["TITLE"]   create tests/carts/<name>/ with a skeleton
  cart.py register <name> "<what it does>"     build, then add or refresh its MANIFEST.toml entry
  cart.py golden <name> [--frame N]            create tests/assets/golden/<name>.toml and fill it in

Run from anywhere inside the checkout. Nothing here blesses a golden: that is a person's step.
"""
import argparse
import hashlib
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(subprocess.check_output(["git", "rev-parse", "--show-toplevel"], text=True).strip())
CARTS = ROOT / "tests/carts"
ASSETS = ROOT / "tests/assets"
MANIFEST = ASSETS / "MANIFEST.toml"
MAP_MODE = {"lorom": "$20", "hirom": "$21"}

SKELETON = """\
; TODO(cite): one sentence on what this cartridge exercises, and which Rust test reads its result.
; @source: adr-0013
; TODO(cite): one `; @source:` line per document section the cartridge exercises.

.p816
.include "header.inc"

cart_header "{title}", {map_mode}, $01
cart_vectors reset, halt, halt, halt

.segment "CODE"

; Entered from the emulation-mode reset vector with D = $0000, DBR = $00 and M = X = 1
; (wdc-w65c816s#2.25), so every access below is 8-bit.
; TODO(cite): tag this proc with the sections it exercises, and cite the source of every
; expected value it compares, beside the value.
; @source: wdc-w65c816s#2.25
.proc reset
    stp
.endproc

; Every other vector stops the CPU too, so an unexpected interrupt is visible as a stop.
; @source: wdc-w65c816s#7.14
.proc halt
    stp
.endproc
"""


def fail(message: str) -> "None":
    print(f"error: {message}", file=sys.stderr)
    sys.exit(1)


def xtask(*words: str) -> str:
    result = subprocess.run(["cargo", "xtask", *words], cwd=ROOT, capture_output=True, text=True)
    if result.returncode != 0:
        tail = "\n".join((result.stdout + result.stderr).strip().splitlines()[-25:])
        fail(f"cargo xtask {' '.join(words)} failed:\n{tail}")
    return result.stdout


def valid_name(name: str) -> None:
    if not re.fullmatch(r"[a-z0-9]+(-[a-z0-9]+)*", name):
        fail("cartridge names are lowercase letters, digits and hyphens")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def new(args: argparse.Namespace) -> None:
    valid_name(args.name)
    directory = CARTS / args.name
    if directory.exists():
        fail(f"{directory.relative_to(ROOT)} already exists")
    title = (args.title or f"SFES {args.name.replace('-', ' ').upper()}").upper()
    if len(title) > 21:
        fail(f"title {title!r} is longer than 21 characters; pass a shorter one")
    directory.mkdir(parents=True)
    (directory / "cart.toml").write_text(
        f"# Board and linker configuration for this cartridge (ADR 0013).\nboard = \"{args.board}\"\n"
    )
    (directory / f"{args.name}.s").write_text(SKELETON.format(title=title, map_mode=MAP_MODE[args.board]))
    print(f"created tests/carts/{args.name}/cart.toml and {args.name}.s")
    print("next: write the assembly, replace every TODO(cite), then `cart.py register`")


def register(args: argparse.Namespace) -> None:
    valid_name(args.name)
    if not (CARTS / args.name / f"{args.name}.s").is_file():
        fail(f"no tests/carts/{args.name}/{args.name}.s")
    leftovers = [
        number
        for source in (CARTS / args.name).glob("*.s")
        for number, line in enumerate(source.read_text().splitlines(), 1)
        if "TODO(cite)" in line
    ]
    if leftovers:
        fail(f"TODO(cite) markers remain in tests/carts/{args.name} (lines {leftovers}); cite them first")
    xtask("carts", "build")
    rom = ASSETS / "carts" / f"{args.name}.sfc"
    digest = sha256(rom)
    text = MANIFEST.read_text()
    path_line = f'path = "carts/{args.name}.sfc"'
    notes = (
        f"Own test cartridge: {args.what} Built from tests/carts/{args.name}/ "
        "by `cargo xtask carts build` (ADR 0013)."
    )
    if path_line in text:
        # Refresh the hash of an existing entry; keep its notes.
        pattern = re.compile(rf'({re.escape(path_line)}\nlicense = "own"\nsha256 = ")[0-9a-f]{{64}}(")')
        text, count = pattern.subn(rf"\g<1>{digest}\g<2>", text)
        if count != 1:
            fail(f"found the entry for {args.name} but not in the expected shape; edit it by hand")
        action = "refreshed the sha256 of"
    else:
        entry = f'[[asset]]\n{path_line}\nlicense = "own"\nsha256 = "{digest}"\nnotes = "{notes}"\n'
        # Insert after the last own-cart entry so related entries stay together.
        blocks = list(re.finditer(r'\[\[asset\]\]\npath = "carts/[^"]+"\n(?:.*\n)*?notes = ".*"\n', text))
        if blocks:
            end = blocks[-1].end()
            text = text[:end] + "\n" + entry + text[end:]
        else:
            text = text.rstrip("\n") + "\n\n" + entry
        action = "added"
    MANIFEST.write_text(text)
    print(f"built tests/assets/carts/{args.name}.sfc ({rom.stat().st_size} bytes, sha256 {digest})")
    print(f"{action} its MANIFEST.toml entry")


def golden(args: argparse.Namespace) -> None:
    valid_name(args.name)
    rom = ASSETS / "carts" / f"{args.name}.sfc"
    if not rom.is_file():
        fail(f"{rom.relative_to(ROOT)} does not exist; run `cart.py register` first")
    target = ASSETS / "golden" / f"{args.name}.toml"
    if target.exists():
        fail(f"{target.relative_to(ROOT)} exists; use `cargo xtask golden --update {args.name}`")
    target.parent.mkdir(exist_ok=True)
    # A placeholder checkpoint: the format needs one, and --update replaces the hash and size.
    target.write_text(
        f'rom = "carts/{args.name}.sfc"\nrom_sha256 = "{sha256(rom)}"\n\n'
        f"[[checkpoint]]\nframe = {args.frame}\nwidth = 256\nheight = 224\n"
        f'blake3 = "{"0" * 64}"\nstatus = "unverified"\n'
    )
    print(xtask("golden", "--update", args.name).strip())
    print(f"wrote {target.relative_to(ROOT)}: status stays `unverified`; a person blesses it against an oracle")
    print("view the frame:  cargo xtask golden --png", args.name, args.frame)
    print('describe it in the PR\'s "Golden changes" section (ADR 0015)')


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    commands = parser.add_subparsers(dest="command", required=True)
    parser_new = commands.add_parser("new")
    parser_new.add_argument("name")
    parser_new.add_argument("board", choices=sorted(MAP_MODE))
    parser_new.add_argument("title", nargs="?")
    parser_new.set_defaults(run=new)
    parser_register = commands.add_parser("register")
    parser_register.add_argument("name")
    parser_register.add_argument("what", help="one sentence, ending in a full stop, saying what the cartridge does")
    parser_register.set_defaults(run=register)
    parser_golden = commands.add_parser("golden")
    parser_golden.add_argument("name")
    parser_golden.add_argument("--frame", type=int, default=60)
    parser_golden.set_defaults(run=golden)
    arguments = parser.parse_args()
    arguments.run(arguments)


if __name__ == "__main__":
    main()
