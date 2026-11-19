//! `cargo xtask golden`: rewrite golden frame hashes and write frames as PNG (ADR 0015).
//!
//! - `golden --update [<name>…]` reruns the named goldens in `tests/assets/golden/`, or all of
//!   them, and rewrites the checkpoints that changed, resetting each to `unverified`.
//! - `golden --png <name> <frame>` runs a golden's ROM and input script to `frame` and writes
//!   `target/golden/<name>-<frame>.png`, for comparison with the oracle's output.
//!
//! The format, runner and update rules live in `sfes_test_harness::golden`, so these commands
//! run the same code as the tests.

use std::{
    fs,
    io::BufWriter,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use sfes_test_harness::{
    golden::{self, Frame, Golden},
    roms::Rom,
};

use crate::finding::Finding;

/// The golden directory, relative to the workspace root.
const GOLDEN: &str = "tests/assets/golden";
/// Where `--png` writes, relative to the workspace root.
const PNG_DIRECTORY: &str = "target/golden";

/// `cargo xtask golden --update [<name>…]`.
pub(crate) fn update(root: &Path, names: &[&str]) -> Result<Vec<Finding>> {
    let paths = if names.is_empty() {
        golden::files(&root.join(GOLDEN)).context("listing the golden files")?
    } else {
        names
            .iter()
            .map(|name| golden_path(root, name))
            .collect::<Result<Vec<_>>>()?
    };
    let mut findings = Vec::new();
    let mut changed_files = 0_usize;
    for path in &paths {
        let mut golden = golden::load(path)?;
        let Some(rom) = rom_bytes(root, path, &golden, &mut findings)? else {
            continue;
        };
        let mut machine = golden::boot(&rom)?;
        let observed = golden::observe(&golden, machine.as_mut());
        let changed = golden::update(&mut golden, &observed);
        if changed > 0 {
            let text = golden.to_toml().context("writing the golden as TOML")?;
            fs::write(path, text).with_context(|| format!("writing {}", path.display()))?;
            println!("{}: {changed} checkpoint(s) changed", display(root, path));
            changed_files += 1;
        }
    }
    println!(
        "{} golden file(s) checked, {changed_files} rewritten",
        paths.len()
    );
    Ok(findings)
}

/// `cargo xtask golden --png <name> <frame>`.
pub(crate) fn png(root: &Path, name: &str, frame: &str) -> Result<Vec<Finding>> {
    let frame = frame
        .parse::<u32>()
        .ok()
        .filter(|frame| *frame > 0)
        .with_context(|| format!("the frame number is counted from 1, not {frame:?}"))?;
    let path = golden_path(root, name)?;
    let golden = golden::load(&path)?;
    let mut findings = Vec::new();
    let Some(rom) = rom_bytes(root, &path, &golden, &mut findings)? else {
        return Ok(findings);
    };
    let mut machine = golden::boot(&rom)?;
    let image = golden::capture(&golden, machine.as_mut(), frame)
        .with_context(|| format!("frame {frame} was not rendered"))?;
    let bytes = encode(&image)?;
    let directory = root.join(PNG_DIRECTORY);
    fs::create_dir_all(&directory).with_context(|| format!("creating {}", directory.display()))?;
    let target = directory.join(format!("{name}-{frame}.png"));
    fs::write(&target, bytes).with_context(|| format!("writing {}", target.display()))?;
    println!("wrote {}", display(root, &target));
    Ok(findings)
}

/// The golden file named `name`, which must exist.
fn golden_path(root: &Path, name: &str) -> Result<PathBuf> {
    let plain = !name.is_empty()
        && name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        && !name.starts_with('.');
    if !plain {
        bail!("{name:?} is not a golden name: use the file name in {GOLDEN}/ without .toml");
    }
    let path = root.join(GOLDEN).join(format!("{name}.toml"));
    if !path.is_file() {
        bail!("{GOLDEN}/{name}.toml does not exist");
    }
    Ok(path)
}

/// The golden's ROM bytes, or `None` with a finding when the ROM is absent and not required.
fn rom_bytes(
    root: &Path,
    path: &Path,
    golden: &Golden,
    findings: &mut Vec<Finding>,
) -> Result<Option<Vec<u8>>> {
    match golden::rom(golden)? {
        Rom::Present(bytes) => Ok(Some(bytes)),
        Rom::Skipped(message) => {
            findings.push(Finding::new(display(root, path), 0, message));
            Ok(None)
        }
    }
}

/// `frame` as an 8-bit RGB PNG.
fn encode(frame: &Frame) -> Result<Vec<u8>> {
    let Frame {
        width,
        height,
        pixels,
    } = frame;
    if pixels.len() != usize::from(*width) * usize::from(*height) {
        bail!(
            "a {width}x{height} frame has {} pixels, not {}",
            usize::from(*width) * usize::from(*height),
            pixels.len()
        );
    }
    let data = pixels
        .iter()
        .flat_map(|pixel| rgb8(*pixel))
        .collect::<Vec<_>>();
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(
            BufWriter::new(&mut bytes),
            u32::from(*width),
            u32::from(*height),
        );
        encoder.set_color(png::ColorType::Rgb);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().context("writing the PNG header")?;
        writer
            .write_image_data(&data)
            .context("writing the PNG image")?;
        writer.finish().context("finishing the PNG")?;
    }
    Ok(bytes)
}

/// An RGB555 pixel, red in the low bits as CGRAM stores it, as 8-bit red, green and blue.
/// Each 5-bit channel is widened by repeating its top bits (ADR 0015).
fn rgb8(pixel: u16) -> [u8; 3] {
    let widen = |shift: u16| {
        let channel = u8::try_from((pixel >> shift) & 0x1F).unwrap_or_default();
        (channel << 3) | (channel >> 2)
    };
    [widen(0), widen(5), widen(10)]
}

/// `path` relative to `root`, for messages.
fn display(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .display()
        .to_string()
        .replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_widen_to_eight_bits() {
        assert_eq!(rgb8(0x0000), [0, 0, 0]);
        assert_eq!(rgb8(0x7FFF), [255, 255, 255]);
        assert_eq!(rgb8(0x001F), [255, 0, 0]);
        assert_eq!(rgb8(0x03E0), [0, 255, 0]);
        assert_eq!(rgb8(0x7C00), [0, 0, 255]);
        assert_eq!(rgb8(0x0010), [0x84, 0, 0]);
    }

    #[test]
    fn a_frame_encodes_as_a_png_that_decodes_back() {
        let frame = Frame {
            width: 3,
            height: 2,
            pixels: vec![0x001F, 0x03E0, 0x7C00, 0x0000, 0x7FFF, 0x0010],
        };
        let bytes = encode(&frame).unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut data = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut data).unwrap();
        assert_eq!((info.width, info.height), (3, 2));
        assert_eq!(info.color_type, png::ColorType::Rgb);
        let expected = frame
            .pixels
            .iter()
            .flat_map(|pixel| rgb8(*pixel))
            .collect::<Vec<_>>();
        assert_eq!(&data[..info.buffer_size()], expected.as_slice());
    }

    #[test]
    fn a_frame_with_the_wrong_pixel_count_is_refused() {
        let frame = Frame {
            width: 2,
            height: 2,
            pixels: vec![0; 3],
        };
        assert!(encode(&frame).is_err());
    }

    #[test]
    fn golden_names_are_plain_file_names() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for name in ["", "../x", "a/b", ".hidden", "a b"] {
            let error = golden_path(root, name).unwrap_err().to_string();
            assert!(error.contains("not a golden name"), "{name:?}: {error}");
        }
        let error = golden_path(root, "no-such-golden").unwrap_err().to_string();
        assert!(error.contains("does not exist"), "{error}");
    }

    #[test]
    fn update_with_no_goldens_succeeds() {
        let root = std::env::temp_dir().join(format!("sfes-golden-{}", std::process::id()));
        assert_eq!(update(&root, &[]).unwrap(), Vec::new());
    }
}
