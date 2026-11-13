//! `cargo xtask provenance assets`: every test asset is licensed and recorded (PROV-09).
//!
//! Every file under `tests/assets/` (other than the manifest) needs an entry in
//! `tests/assets/MANIFEST.toml`, either for its exact path (with a matching SHA-256) or for a
//! directory entry ending in `/` that covers it. Entries with `fetch = true` describe files
//! downloaded at test time and may be absent; when present they must match their hash.
//!
//! A directory entry with `fetch = true` is a vector set downloaded by `cargo xtask vectors
//! fetch` from `source_url` at `commit`. Its files' hashes live in `tests/assets/vectors.lock`:
//! every file present must be in the lock with a matching hash.

use std::{
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::{
    files,
    finding::Finding,
    provenance::{ALLOWED_LICENSES, license_permitted},
    vectors::{self, Lock},
};

/// The asset directory, relative to the workspace root.
pub(crate) const ASSETS: &str = "tests/assets";
const MANIFEST: &str = "tests/assets/MANIFEST.toml";

/// Files in the asset directory that describe the assets and are not assets themselves.
const INDEX_FILES: &[&str] = &["MANIFEST.toml", "vectors.lock"];

/// File names the operating system scatters into directories. They are git-ignored, so they
/// can never be committed as assets.
const OS_FILES: &[&str] = &[".DS_Store", "Thumbs.db", "desktop.ini"];

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Manifest {
    #[serde(rename = "asset", default)]
    pub(crate) assets: Vec<Asset>,
}

impl Manifest {
    /// Reads and parses `tests/assets/MANIFEST.toml`.
    pub(crate) fn load(root: &Path) -> Result<Self> {
        let text = fs::read_to_string(root.join(MANIFEST))
            .with_context(|| format!("reading {MANIFEST}"))?;
        toml::from_str::<Self>(&text).with_context(|| format!("parsing {MANIFEST}"))
    }
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Asset {
    pub(crate) path: String,
    pub(crate) license: String,
    pub(crate) source_url: Option<String>,
    /// The pinned git commit of a fetched directory entry.
    pub(crate) commit: Option<String>,
    pub(crate) sha256: Option<String>,
    #[serde(default)]
    pub(crate) fetch: bool,
    #[expect(
        dead_code,
        reason = "schema field: parsed so unknown or mistyped fields fail, not read"
    )]
    pub(crate) notes: Option<String>,
}

pub(crate) fn run(root: &Path) -> Result<Vec<Finding>> {
    let manifest = Manifest::load(root)?;
    let lock = Lock::load(root)?;
    let assets_directory = root.join(ASSETS);
    let present = hash_tree(&assets_directory, &assets_directory)?;
    Ok(check(&manifest, &lock, &present))
}

/// Every asset file under `directory`, as `(path relative to assets_directory, SHA-256 hex)`,
/// sorted by path. A missing directory yields no files.
pub(crate) fn hash_tree(
    assets_directory: &Path,
    directory: &Path,
) -> Result<Vec<(String, String)>> {
    let mut present = Vec::new();
    for path in files::walk(directory, &|_| false)? {
        let relative = files::display(assets_directory, &path);
        let name = relative.rsplit('/').next().unwrap_or(&relative);
        if !INDEX_FILES.contains(&relative.as_str()) && !OS_FILES.contains(&name) {
            present.push((relative, hash_file(&path)?));
        }
    }
    Ok(present)
}

fn hash_file(path: &PathBuf) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(sha256_hex(&bytes))
}

/// Checks the manifest and the lock against the files present, given as `(path relative to
/// tests/assets, SHA-256 hex)`.
fn check(manifest: &Manifest, lock: &Lock, present: &[(String, String)]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for asset in &manifest.assets {
        findings.extend(check_entry(asset, present));
    }
    let sets = vectors::sets(manifest);
    for set in &sets {
        findings.extend(vectors::verify_set(set, lock, present, false));
    }
    findings.extend(vectors::stale_records(&sets, lock));
    for (path, hash) in present {
        let exact = manifest.assets.iter().find(|asset| asset.path == *path);
        let covered = manifest
            .assets
            .iter()
            .any(|asset| asset.path.ends_with('/') && path.starts_with(&asset.path));
        match exact {
            Some(Asset {
                sha256: Some(expected),
                ..
            }) if expected != hash => findings.push(Finding::new(
                format!("{ASSETS}/{path}"),
                0,
                format!("SHA-256 is {hash}, manifest says {expected}"),
            )),
            Some(_) => {}
            None if covered => {}
            None => findings.push(Finding::new(
                format!("{ASSETS}/{path}"),
                0,
                "no entry in tests/assets/MANIFEST.toml (PROV-09)",
            )),
        }
    }
    findings
}

fn check_entry(asset: &Asset, present: &[(String, String)]) -> Vec<Finding> {
    let Asset {
        path,
        license,
        source_url,
        commit,
        sha256,
        fetch,
        notes: _,
    } = asset;
    let mut findings = Vec::new();
    let mut problem = |message: &str| {
        findings.push(Finding::new(
            MANIFEST,
            0,
            format!("asset `{path}`: {message}"),
        ));
    };
    let allowed = |term: &str| term == "own" || ALLOWED_LICENSES.contains(&term);
    if !license_permitted(license, &allowed) {
        problem(
            "license is not on the PROV-08 list (use an SPDX id, or \"own\" for project-made assets)",
        );
    }
    let fetched_directory = path.ends_with('/') && *fetch;
    if commit.is_some() && !fetched_directory {
        problem("`commit` applies only to directory entries with `fetch = true`");
    }
    if path.ends_with('/') {
        if sha256.is_some() {
            problem("directory entries take no sha256");
        }
        if fetched_directory {
            if !source_url.as_deref().is_some_and(vectors::is_source_url) {
                problem("fetched directories need an https:// `source_url`");
            }
            if !commit.as_deref().is_some_and(vectors::is_commit) {
                problem("fetched directories need a `commit` of 40 lower-case hex digits");
            }
        }
    } else {
        if sha256.is_none() {
            problem("file entries need a sha256");
        }
        let exists = present.iter().any(|(present_path, _)| present_path == path);
        if !exists && !fetch {
            problem("file is missing; commit it or mark the entry `fetch = true`");
        }
    }
    findings
}

fn sha256_hex(bytes: &[u8]) -> String {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(text: &str) -> Manifest {
        toml::from_str::<Manifest>(text).unwrap()
    }

    fn present(entries: &[(&str, &[u8])]) -> Vec<(String, String)> {
        entries
            .iter()
            .map(|(path, bytes)| (String::from(*path), sha256_hex(bytes)))
            .collect::<Vec<_>>()
    }

    #[test]
    fn empty_manifest_and_tree_pass() {
        assert_eq!(
            check(&Manifest::default(), &Lock::default(), &[]),
            Vec::<Finding>::new()
        );
    }

    #[test]
    fn recorded_files_pass() {
        let hash = sha256_hex(b"vector");
        let text = format!(
            "[[asset]]\npath = \"cpu/00.json\"\nlicense = \"MIT\"\nsha256 = \"{hash}\"\n\n\
             [[asset]]\npath = \"cpu/01.json\"\nlicense = \"MIT\"\nsha256 = \"{hash}\"\nfetch = true\n\n\
             [[asset]]\npath = \"golden/\"\nlicense = \"own\"\n"
        );
        let files = present(&[("cpu/00.json", b"vector"), ("golden/frame.txt", b"x")]);
        assert_eq!(
            check(&manifest(&text), &Lock::default(), &files),
            Vec::<Finding>::new()
        );
    }

    #[test]
    fn fetched_directories_need_a_pin_and_a_lock() {
        let commit = "307c63f5673c8c6b861c52f71ddb89212a227480";
        let pinned = format!(
            "[[asset]]\npath = \"vectors/65816/\"\nlicense = \"MIT\"\nfetch = true\n\
             source_url = \"https://example.org/65816\"\ncommit = \"{commit}\"\n"
        );
        // Well-formed, but nothing is locked yet.
        let unlocked = check(&manifest(&pinned), &Lock::default(), &[]);
        assert_eq!(unlocked.len(), 1, "{unlocked:?}");
        assert!(unlocked[0].to_string().contains("vectors lock"));

        let unpinned = "[[asset]]\npath = \"vectors/65816/\"\nlicense = \"MIT\"\nfetch = true\n\n\
                        [[asset]]\npath = \"golden/\"\nlicense = \"own\"\ncommit = \"00\"\n";
        let messages = check(&manifest(unpinned), &Lock::default(), &[])
            .into_iter()
            .map(|finding| finding.to_string())
            .collect::<Vec<_>>();
        assert_eq!(messages.len(), 3, "{messages:?}");
        assert!(messages[0].contains("source_url"));
        assert!(messages[1].contains("commit"));
        assert!(messages[2].contains("applies only"));
    }

    #[test]
    fn problems_are_reported() {
        let text = "[[asset]]\npath = \"rom.sfc\"\nlicense = \"GPL-3.0\"\nsha256 = \"00\"\n\n\
                    [[asset]]\npath = \"gone.json\"\nlicense = \"MIT\"\nsha256 = \"00\"\n";
        let files = present(&[("rom.sfc", b"rom"), ("stray.bin", b"?")]);
        let messages = check(&manifest(text), &Lock::default(), &files)
            .into_iter()
            .map(|finding| finding.to_string())
            .collect::<Vec<_>>();
        assert_eq!(messages.len(), 4, "{messages:?}");
        assert!(messages[0].contains("license"));
        assert!(messages[1].contains("missing"));
        assert!(messages[2].contains("SHA-256"));
        assert!(messages[3].contains("no entry"));
    }
}
