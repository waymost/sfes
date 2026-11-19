# Own test cartridges

Small 65816 programs written by the project (EMU-11), assembled with ca65 and ld65 from cc65 2.19 ([ADR 0013](../../docs/adr/0013-test-cartridge-toolchain.md)). The built ROMs are committed in `tests/assets/carts/`, so running the tests needs no assembler.

## Adding a cartridge

1. Create `tests/carts/<name>/<name>.s`. Further `.s` files in the directory are assembled and linked with it.
2. Add `tests/carts/<name>/cart.toml` with `board = "lorom"` (32 KiB) or `board = "hirom"` (64 KiB).
3. Start the source with `.p816`, `.include "header.inc"`, then `cart_header` and `cart_vectors` (see [`common/header.inc`](common/header.inc)).
4. Tag the file and every `.proc` and `.macro` with `; @source:` naming the section it exercises (AGENTS.md section 1). An expected value the cartridge compares against cites its source in the comment beside it.
5. Run `cargo xtask carts build`, then add the printed SHA-256 to `tests/assets/MANIFEST.toml` as `path = "carts/<name>.sfc"`, `license = "own"`.

`cargo xtask carts check` rebuilds every cartridge and fails if a committed ROM differs; CI runs it on Linux.

## Installing cc65 2.19

- macOS: `brew install cc65`.
- Elsewhere: build the `V2.19` tag of <https://github.com/cc65/cc65> with `make bin` and put its `bin/` on the path.

The 2.19 tools report themselves as `V2.18`; `cargo xtask carts` checks for that string.
