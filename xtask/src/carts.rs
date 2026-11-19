//! `cargo xtask carts build|check`: the project's own test cartridges (EMU-11, ADR 0013).
//!
//! Each cartridge is a directory `tests/carts/<name>/` holding `<name>.s` (and any further `.s`
//! files) and a `cart.toml` naming its board. ca65 assembles every `.s` file with
//! `tests/carts/common/` on the include path, ld65 links them with the board's linker
//! configuration from that directory, and the ROM size byte and checksum are written into the
//! header. `build` writes the result to `tests/assets/carts/<name>.sfc`; `check` builds into
//! `target/carts/` and compares with the committed file.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use anyhow::{Context, Result, bail};

use crate::{files, finding::Finding};

/// Cartridge sources, relative to the workspace root.
pub(crate) const CARTS: &str = "tests/carts";
/// The directory in [`CARTS`] holding the shared include and linker configurations.
pub(crate) const COMMON: &str = "common";
/// Built ROMs, relative to `tests/assets/`.
pub(crate) const OUTPUT: &str = "carts";
/// Scratch space for `check` and for object files, relative to the workspace root.
const SCRATCH: &str = "target/carts";

/// The version the cc65 2.19 release's tools print (ADR 0013): that release still reports
/// itself as 2.18.
const TOOL_VERSION: &str = "V2.18";

/// The smallest ROM a board here links: the single 32 KiB bank of `lorom.cfg`.
const SMALLEST_ROM: usize = 32 * 1024;
/// The largest ROM accepted: 4 MiB, size byte `$0C` (`fullsnes#snescartridgeromheader`).
const LARGEST_ROM: usize = 4 * 1024 * 1024;

/// A cartridge's `cart.toml`.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CartFile {
    board: Board,
}

/// How the cartridge's ROM appears in the CPU's address space, which fixes the linker
/// configuration and the header's place in the file.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum Board {
    Lorom,
    Hirom,
}

impl Board {
    /// The linker configuration in [`COMMON`].
    fn linker_config(self) -> &'static str {
        match self {
            Self::Lorom => "lorom.cfg",
            Self::Hirom => "hirom.cfg",
        }
    }

    /// The file offset of the header that the CPU sees at `$00:FFC0`
    /// (`fullsnes#snescartridgeromheader`).
    fn header_offset(self) -> usize {
        match self {
            Self::Lorom => 0x7FC0,
            Self::Hirom => 0xFFC0,
        }
    }
}

/// One cartridge to build.
#[derive(Debug)]
struct Cart {
    name: String,
    board: Board,
    /// Every `.s` file in the cartridge's directory, sorted.
    sources: Vec<PathBuf>,
}

/// The cartridge names: every directory in [`CARTS`] other than [`COMMON`], sorted.
pub(crate) fn names(root: &Path) -> Result<Vec<String>> {
    let directory = root.join(CARTS);
    let mut names = Vec::new();
    if !directory.is_dir() {
        return Ok(names);
    }
    for entry in fs::read_dir(&directory).with_context(|| format!("reading {CARTS}"))? {
        let path = entry?.path();
        let name = path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
        if path.is_dir() && name != COMMON {
            names.push(name);
        }
    }
    names.sort();
    Ok(names)
}

/// Builds every cartridge into `tests/assets/carts/`.
pub(crate) fn build(root: &Path) -> Result<Vec<Finding>> {
    let (carts, mut findings) = discover(root)?;
    if !findings.is_empty() {
        return Ok(findings);
    }
    check_tools()?;
    let output = root.join("tests/assets").join(OUTPUT);
    fs::create_dir_all(&output).with_context(|| format!("creating {}", output.display()))?;
    for cart in &carts {
        match assemble(root, cart)? {
            Ok(rom) => {
                let path = output.join(format!("{}.sfc", cart.name));
                fs::write(&path, &rom).with_context(|| format!("writing {}", path.display()))?;
                println!(
                    "{}: {} bytes, sha256 {}",
                    files::display(root, &path),
                    rom.len(),
                    files::sha256_hex(&rom)
                );
            }
            Err(finding) => findings.push(finding),
        }
    }
    Ok(findings)
}

/// Rebuilds every cartridge and compares it with the committed ROM.
pub(crate) fn check(root: &Path) -> Result<Vec<Finding>> {
    let (carts, mut findings) = discover(root)?;
    if !findings.is_empty() {
        return Ok(findings);
    }
    check_tools()?;
    let output = root.join("tests/assets").join(OUTPUT);
    for cart in &carts {
        let built = match assemble(root, cart)? {
            Ok(rom) => rom,
            Err(finding) => {
                findings.push(finding);
                continue;
            }
        };
        let path = output.join(format!("{}.sfc", cart.name));
        let committed = if path.is_file() {
            Some(fs::read(&path).with_context(|| format!("reading {}", path.display()))?)
        } else {
            None
        };
        findings.extend(compare(&cart.name, &built, committed.as_deref()));
    }
    let names = carts
        .iter()
        .map(|cart| cart.name.clone())
        .collect::<Vec<_>>();
    let committed = files::walk(&output, &|_| false)?
        .iter()
        .map(|path| files::display(&output, path))
        .collect::<Vec<_>>();
    findings.extend(orphans(&names, &committed));
    Ok(findings)
}

/// Finds the cartridges and reads their `cart.toml`. A malformed cartridge is a finding.
fn discover(root: &Path) -> Result<(Vec<Cart>, Vec<Finding>)> {
    let mut carts = Vec::new();
    let mut findings = Vec::new();
    for name in names(root)? {
        let directory = root.join(CARTS).join(&name);
        let display = format!("{CARTS}/{name}");
        if !is_cart_name(&name) {
            findings.push(Finding::new(
                &display,
                0,
                "cartridge names are lowercase letters, digits and hyphens",
            ));
            continue;
        }
        let toml_path = directory.join("cart.toml");
        let Ok(text) = fs::read_to_string(&toml_path) else {
            findings.push(Finding::new(
                &display,
                0,
                "no cart.toml naming the board (ADR 0013)",
            ));
            continue;
        };
        let board = match toml::from_str::<CartFile>(&text) {
            Ok(CartFile { board }) => board,
            Err(error) => {
                findings.push(Finding::new(
                    format!("{display}/cart.toml"),
                    0,
                    format!("invalid: {}", error.message()),
                ));
                continue;
            }
        };
        let sources = files::walk(&directory, &|_| true)?
            .into_iter()
            .filter(|path| path.extension().is_some_and(|extension| extension == "s"))
            .collect::<Vec<_>>();
        let entry = directory.join(format!("{name}.s"));
        if !sources.contains(&entry) {
            findings.push(Finding::new(
                &display,
                0,
                format!("no entry source {name}.s (ADR 0013)"),
            ));
            continue;
        }
        carts.push(Cart {
            name,
            board,
            sources,
        });
    }
    Ok((carts, findings))
}

fn is_cart_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

/// Fails unless ca65 and ld65 are on the path and report [`TOOL_VERSION`].
fn check_tools() -> Result<()> {
    for tool in ["ca65", "ld65"] {
        let output = Command::new(tool)
            .arg("--version")
            .output()
            .with_context(|| {
                format!("running {tool}: install cc65 2.19 (ADR 0013), e.g. `brew install cc65`")
            })?;
        // The version goes to standard error.
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !text.contains(TOOL_VERSION) {
            bail!(
                "{tool} reports `{}`; ADR 0013 pins cc65 2.19, whose tools report {TOOL_VERSION}",
                text.trim()
            );
        }
    }
    Ok(())
}

/// Assembles and links one cartridge in [`SCRATCH`] and finishes its header. A tool failure
/// or a bad ROM size is a finding.
fn assemble(root: &Path, cart: &Cart) -> Result<Result<Vec<u8>, Finding>> {
    let display = format!("{CARTS}/{}", cart.name);
    let scratch = root.join(SCRATCH).join(&cart.name);
    if scratch.is_dir() {
        fs::remove_dir_all(&scratch).with_context(|| format!("clearing {}", scratch.display()))?;
    }
    fs::create_dir_all(&scratch).with_context(|| format!("creating {}", scratch.display()))?;
    let common = root.join(CARTS).join(COMMON);
    let mut objects = Vec::new();
    for source in &cart.sources {
        let stem = source
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned());
        let object = scratch.join(format!("{stem}.o"));
        let mut command = Command::new("ca65");
        command
            .arg("--cpu")
            .arg("65816")
            .arg("-I")
            .arg(&common)
            .arg("-o")
            .arg(&object)
            .arg(source);
        if let Err(message) = run_tool(&mut command)? {
            return Ok(Err(Finding::new(files::display(root, source), 0, message)));
        }
        objects.push(object);
    }
    let image = scratch.join(format!("{}.sfc", cart.name));
    let mut command = Command::new("ld65");
    command
        .arg("-C")
        .arg(common.join(cart.board.linker_config()))
        .arg("-o")
        .arg(&image)
        .args(&objects);
    if let Err(message) = run_tool(&mut command)? {
        return Ok(Err(Finding::new(&display, 0, message)));
    }
    let mut rom = fs::read(&image).with_context(|| format!("reading {}", image.display()))?;
    Ok(match finish_header(&mut rom, cart.board) {
        Ok(()) => Ok(rom),
        Err(message) => Err(Finding::new(&display, 0, message)),
    })
}

/// Runs an assembler or linker; a non-zero exit is returned as its diagnostics.
fn run_tool(command: &mut Command) -> Result<Result<(), String>> {
    let program = command.get_program().to_string_lossy().into_owned();
    let output = command
        .output()
        .with_context(|| format!("running {program}"))?;
    if output.status.success() {
        return Ok(Ok(()));
    }
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    Ok(Err(format!("{program} failed: {}", diagnostics.trim())))
}

/// Writes the ROM size byte (`$FFD7`) and the checksum and its complement (`$FFDC–$FFDF`).
/// The size byte is `n` for a ROM of `1 << n` KiB. The checksum is the sum of every byte,
/// modulo `$10000`, with `$FFDC–$FFDF` taken as `FF FF 00 00`; the complement is the checksum
/// XOR `$FFFF` (`fullsnes#snescartridgeromheader`).
fn finish_header(rom: &mut [u8], board: Board) -> Result<(), String> {
    let length = rom.len();
    let header = board.header_offset();
    if !length.is_power_of_two() || !(SMALLEST_ROM..=LARGEST_ROM).contains(&length) {
        return Err(format!(
            "ROM is {length} bytes; it must be a power of two from 32 KiB to 4 MiB"
        ));
    }
    if length < header + 0x40 {
        return Err(format!(
            "ROM is {length} bytes, too small for a {board:?} header at {header:#06X}"
        ));
    }
    let size_exponent = (length / 1024).trailing_zeros();
    rom[header + 0x17] = u8::try_from(size_exponent).map_err(|error| error.to_string())?;
    rom[header + 0x1C..header + 0x20].copy_from_slice(&[0xFF, 0xFF, 0x00, 0x00]);
    let checksum = rom
        .iter()
        .fold(0_u16, |sum, byte| sum.wrapping_add(u16::from(*byte)));
    let complement = checksum ^ 0xFFFF;
    rom[header + 0x1C..header + 0x1E].copy_from_slice(&complement.to_le_bytes());
    rom[header + 0x1E..header + 0x20].copy_from_slice(&checksum.to_le_bytes());
    Ok(())
}

/// A finding when the built ROM differs from the committed one, or none is committed.
fn compare(name: &str, built: &[u8], committed: Option<&[u8]>) -> Option<Finding> {
    let path = format!("tests/assets/{OUTPUT}/{name}.sfc");
    match committed {
        None => Some(Finding::new(
            path,
            0,
            "not committed; run `cargo xtask carts build`",
        )),
        Some(committed) if committed != built => {
            let first = built
                .iter()
                .zip(committed)
                .position(|(built, committed)| built != committed)
                .unwrap_or(built.len().min(committed.len()));
            Some(Finding::new(
                path,
                0,
                format!(
                    "stale: differs from a fresh build at offset {first:#X} \
                     (built {} bytes, committed {}); run `cargo xtask carts build`",
                    built.len(),
                    committed.len()
                ),
            ))
        }
        Some(_) => None,
    }
}

/// Files in `tests/assets/carts/` (relative to it) that no cartridge builds.
fn orphans(names: &[String], committed: &[String]) -> Vec<Finding> {
    committed
        .iter()
        .filter(|file| {
            !file
                .strip_suffix(".sfc")
                .is_some_and(|stem| names.iter().any(|name| name == stem))
        })
        .map(|file| {
            Finding::new(
                format!("tests/assets/{OUTPUT}/{file}"),
                0,
                format!("no cartridge source in {CARTS}/ builds this file"),
            )
        })
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Whether the header's checksum is the sum of every byte with `$FFDC–$FFDF` taken as
    /// `FF FF 00 00`, and its complement is the checksum XOR `$FFFF`.
    fn checksum_holds(rom: &[u8], board: Board) -> bool {
        let header = board.header_offset();
        let complement = u16::from_le_bytes([rom[header + 0x1C], rom[header + 0x1D]]);
        let checksum = u16::from_le_bytes([rom[header + 0x1E], rom[header + 0x1F]]);
        let mut assumed = rom.to_vec();
        assumed[header + 0x1C..header + 0x20].copy_from_slice(&[0xFF, 0xFF, 0x00, 0x00]);
        let sum = assumed
            .iter()
            .fold(0_u16, |sum, byte| sum.wrapping_add(u16::from(*byte)));
        sum == checksum && complement == checksum ^ 0xFFFF
    }

    #[test]
    fn lorom_header_is_finished() {
        let mut rom = vec![0_u8; 32 * 1024];
        rom[0] = 0xA9;
        rom[1] = 0x01;
        finish_header(&mut rom, Board::Lorom).unwrap();
        // 32 KiB is 1 << 5 KiB.
        assert_eq!(rom[0x7FD7], 5);
        // 0xA9 + 0x01 + 0x05 + 0xFF + 0xFF = 0x2AD.
        assert_eq!(&rom[0x7FDC..0x7FE0], &[0x52, 0xFD, 0xAD, 0x02]);
        assert!(checksum_holds(&rom, Board::Lorom));
    }

    #[test]
    fn hirom_header_is_finished() {
        let mut rom = vec![0xFF_u8; 64 * 1024];
        finish_header(&mut rom, Board::Hirom).unwrap();
        assert_eq!(rom[0xFFD7], 6);
        assert_eq!(rom[0x7FD7], 0xFF, "the LoROM header location is untouched");
        assert!(checksum_holds(&rom, Board::Hirom));
    }

    #[test]
    fn finishing_twice_gives_the_same_bytes() {
        let mut once = vec![0x5A_u8; 32 * 1024];
        finish_header(&mut once, Board::Lorom).unwrap();
        let mut twice = once.clone();
        finish_header(&mut twice, Board::Lorom).unwrap();
        assert_eq!(once, twice);
    }

    #[test]
    fn bad_sizes_are_rejected() {
        assert!(finish_header(&mut vec![0; 48 * 1024], Board::Lorom).is_err());
        assert!(finish_header(&mut vec![0; 16 * 1024], Board::Lorom).is_err());
        assert!(finish_header(&mut vec![0; 32 * 1024], Board::Hirom).is_err());
        assert!(finish_header(&mut vec![0; 8 * 1024 * 1024], Board::Hirom).is_err());
    }

    #[test]
    fn stale_and_missing_roms_are_reported() {
        assert_eq!(compare("halt", b"abc", Some(b"abc")), None);
        let stale = compare("halt", b"abc", Some(b"abd")).unwrap().to_string();
        assert!(stale.contains("tests/assets/carts/halt.sfc"), "{stale}");
        assert!(stale.contains("offset 0x2"), "{stale}");
        let shorter = compare("halt", b"abc", Some(b"ab")).unwrap().to_string();
        assert!(shorter.contains("offset 0x2"), "{shorter}");
        let missing = compare("halt", b"abc", None).unwrap().to_string();
        assert!(missing.contains("not committed"), "{missing}");
    }

    #[test]
    fn orphaned_roms_are_reported() {
        let names = [String::from("halt")];
        let committed = [
            String::from("halt.sfc"),
            String::from("gone.sfc"),
            String::from("notes.txt"),
        ];
        let findings = orphans(&names, &committed)
            .into_iter()
            .map(|finding| finding.path)
            .collect::<Vec<_>>();
        assert_eq!(
            findings,
            [
                "tests/assets/carts/gone.sfc",
                "tests/assets/carts/notes.txt"
            ]
        );
    }

    #[test]
    fn cart_files_parse() {
        let parsed = toml::from_str::<CartFile>("board = \"hirom\"\n").unwrap();
        assert_eq!(parsed.board, Board::Hirom);
        assert!(toml::from_str::<CartFile>("board = \"exhirom\"\n").is_err());
        assert!(toml::from_str::<CartFile>("board = \"lorom\"\nspeed = 1\n").is_err());
    }

    #[test]
    fn cart_names_are_checked() {
        assert!(is_cart_name("halt"));
        assert!(is_cart_name("dma-modes-2"));
        assert!(!is_cart_name("Halt"));
        assert!(!is_cart_name("-x"));
        assert!(!is_cart_name("a_b"));
    }
}
