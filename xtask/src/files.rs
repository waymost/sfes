//! Deterministic directory walking and path display.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

/// Directories never walked into.
const ALWAYS_SKIPPED: &[&str] = &[".git", "target", "node_modules", ".venv"];

/// All files under `directory`, sorted, skipping [`ALWAYS_SKIPPED`] and any directory for which
/// `skip` returns `true`. A missing directory yields no files.
pub(crate) fn walk(directory: &Path, skip: &dyn Fn(&Path) -> bool) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    if directory.is_dir() {
        walk_into(directory, skip, &mut files)?;
    }
    files.sort();
    Ok(files)
}

fn walk_into(
    directory: &Path,
    skip: &dyn Fn(&Path) -> bool,
    files: &mut Vec<PathBuf>,
) -> Result<()> {
    let entries = fs::read_dir(directory)
        .with_context(|| format!("reading directory {}", directory.display()))?;
    for entry in entries {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default();
        if path.is_dir() {
            if !ALWAYS_SKIPPED.contains(&name) && !skip(&path) {
                walk_into(&path, skip, files)?;
            }
        } else {
            files.push(path);
        }
    }
    Ok(())
}

/// `path` relative to `root`, with `/` separators on every platform.
pub(crate) fn display(root: &Path, path: &Path) -> String {
    let relative = path.strip_prefix(root).unwrap_or(path);
    relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// The SHA-256 of `bytes` as 64 lower-case hex digits.
pub(crate) fn sha256_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    Sha256::digest(bytes)
        .iter()
        .flat_map(|byte| {
            [
                DIGITS[usize::from(byte >> 4)],
                DIGITS[usize::from(byte & 0x0F)],
            ]
        })
        .map(char::from)
        .collect::<String>()
}
