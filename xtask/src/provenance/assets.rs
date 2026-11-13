//! `cargo xtask provenance assets`: every test asset is licensed and recorded (PROV-09).
//!
//! Every file under `tests/assets/` (other than the manifest) needs an entry in
//! `tests/assets/MANIFEST.toml`, either for its exact path (with a matching SHA-256) or for a
//! directory entry ending in `/` that covers it. Entries with `fetch = true` describe files
//! downloaded at test time and may be absent; when present they must match their hash.

use std::{fs, path::Path};

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};

use crate::{
    files,
    finding::Finding,
    provenance::{ALLOWED_LICENSES, license_permitted},
};

const ASSETS: &str = "tests/assets";
const MANIFEST: &str = "tests/assets/MANIFEST.toml";

#[derive(Debug, Default, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    #[serde(rename = "asset", default)]
    assets: Vec<Asset>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Asset {
    path: String,
    license: String,
    #[expect(
        dead_code,
        reason = "schema field: parsed so unknown or mistyped fields fail, not read"
    )]
    source_url: Option<String>,
    sha256: Option<String>,
    #[serde(default)]
    fetch: bool,
    #[expect(
        dead_code,
        reason = "schema field: parsed so unknown or mistyped fields fail, not read"
    )]
    notes: Option<String>,
}

pub(crate) fn run(root: &Path) -> Result<Vec<Finding>> {
    let manifest_text =
        fs::read_to_string(root.join(MANIFEST)).with_context(|| format!("reading {MANIFEST}"))?;
    let manifest = toml::from_str::<Manifest>(&manifest_text)
        .with_context(|| format!("parsing {MANIFEST}"))?;
    let assets_directory = root.join(ASSETS);
    let mut present = Vec::new();
    for path in files::walk(&assets_directory, &|_| false)? {
        let relative = files::display(&assets_directory, &path);
        if relative != "MANIFEST.toml" {
            let hash = sha256_hex(&fs::read(&path).with_context(|| format!("reading {relative}"))?);
            present.push((relative, hash));
        }
    }
    Ok(check(&manifest, &present))
}

/// Checks the manifest against the files present, given as `(path relative to tests/assets,
/// SHA-256 hex)`.
fn check(manifest: &Manifest, present: &[(String, String)]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for asset in &manifest.assets {
        findings.extend(check_entry(asset, present));
    }
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
        source_url: _,
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
    if path.ends_with('/') {
        if sha256.is_some() || *fetch {
            problem("directory entries take no sha256 and cannot be fetched");
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
        assert_eq!(check(&Manifest::default(), &[]), Vec::<Finding>::new());
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
        assert_eq!(check(&manifest(&text), &files), Vec::<Finding>::new());
    }

    #[test]
    fn problems_are_reported() {
        let text = "[[asset]]\npath = \"rom.sfc\"\nlicense = \"GPL-3.0\"\nsha256 = \"00\"\n\n\
                    [[asset]]\npath = \"gone.json\"\nlicense = \"MIT\"\nsha256 = \"00\"\n";
        let files = present(&[("rom.sfc", b"rom"), ("stray.bin", b"?")]);
        let messages = check(&manifest(text), &files)
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
