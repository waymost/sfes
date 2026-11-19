//! `cargo xtask roms fetch`: third-party test ROMs downloaded by URL and verified by hash.
//!
//! A ROM whose license does not allow committing it (ADR 0014) is a file entry in
//! `tests/assets/MANIFEST.toml` with `fetch = true`, an `https://` `source_url` and its
//! `sha256`, under `tests/assets/fetched/`, which git ignores. `fetch` downloads each such file
//! that is missing or differs, with `curl`, and keeps it only when its SHA-256 matches the
//! manifest.

use std::{fs, path::Path, process::Command};

use anyhow::{Context, Result, bail};

use crate::{
    files,
    finding::Finding,
    provenance::assets::{ASSETS, Asset, FETCHED, Manifest},
    vectors,
};

/// A file downloaded at test time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct FetchedFile {
    /// Path relative to `tests/assets/`, under `fetched/`.
    pub(crate) path: String,
    /// The `https://` URL the file is downloaded from.
    pub(crate) source_url: String,
    /// The expected SHA-256, 64 lower-case hex digits.
    pub(crate) sha256: String,
}

/// The manifest's well-formed fetched file entries. Malformed ones are reported by
/// `cargo xtask provenance assets` and skipped here.
pub(crate) fn files(manifest: &Manifest) -> Vec<FetchedFile> {
    manifest
        .assets
        .iter()
        .filter_map(|asset| {
            let Asset {
                path,
                license: _,
                source_url,
                commit: _,
                sha256,
                fetch,
                notes: _,
            } = asset;
            let source_url = source_url
                .as_deref()
                .filter(|url| vectors::is_source_url(url))?;
            let sha256 = sha256
                .as_deref()
                .filter(|hash| vectors::is_lower_hex(hash, 64))?;
            let fetched = *fetch && !path.ends_with('/') && path.starts_with(FETCHED);
            fetched.then(|| FetchedFile {
                path: path.clone(),
                source_url: String::from(source_url),
                sha256: String::from(sha256),
            })
        })
        .collect::<Vec<_>>()
}

/// `cargo xtask roms fetch`.
pub(crate) fn fetch(root: &Path) -> Result<Vec<Finding>> {
    let manifest = Manifest::load(root)?;
    let assets_directory = root.join(ASSETS);
    let mut findings = Vec::new();
    let mut fresh = 0_usize;
    for file in files(&manifest) {
        let target = assets_directory.join(&file.path);
        if hash_of(&target)?.as_deref() == Some(file.sha256.as_str()) {
            fresh += 1;
            continue;
        }
        println!("{}: fetching {}", file.path, file.source_url);
        let partial = target.with_extension("part");
        download(&file.source_url, &partial)?;
        let hash = hash_of(&partial)?.unwrap_or_default();
        if let Some(finding) = mismatch(&file, &hash) {
            fs::remove_file(&partial).with_context(|| format!("removing {}", partial.display()))?;
            findings.push(finding);
        } else {
            fs::rename(&partial, &target)
                .with_context(|| format!("moving the download to {}", target.display()))?;
        }
    }
    println!("{fresh} file(s) already up to date");
    Ok(findings)
}

/// A finding when a downloaded file's `hash` differs from the manifest.
fn mismatch(file: &FetchedFile, hash: &str) -> Option<Finding> {
    (hash != file.sha256).then(|| {
        Finding::new(
            format!("{ASSETS}/{}", file.path),
            0,
            format!(
                "download from {} has SHA-256 {hash}, the manifest says {}; it was discarded",
                file.source_url, file.sha256
            ),
        )
    })
}

/// The SHA-256 of the file at `path`, or `None` when there is no file.
fn hash_of(path: &Path) -> Result<Option<String>> {
    if !path.is_file() {
        return Ok(None);
    }
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    Ok(Some(files::sha256_hex(&bytes)))
}

/// Downloads `url` to `target` with `curl`, HTTPS only, failing on an HTTP error.
fn download(url: &str, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let status = Command::new("curl")
        .args([
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            "=https",
            "--proto-redir",
            "=https",
            "--retry",
            "3",
            "--output",
        ])
        .arg(target)
        .args(["--url", url])
        .status()
        .context("running curl (is it installed?)")?;
    if !status.success() {
        bail!("downloading {url} failed: {status}");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hash(digit: char) -> String {
        String::from(digit).repeat(64)
    }

    #[test]
    fn fetched_files_come_from_well_formed_entries() {
        let good = hash('a');
        let text = format!(
            "[[asset]]\npath = \"fetched/krom/RotZoom.sfc\"\nlicense = \"NOASSERTION\"\n\
             fetch = true\nsource_url = \"https://example.org/RotZoom.sfc\"\nsha256 = \"{good}\"\n\n\
             [[asset]]\npath = \"fetched/krom/plain.sfc\"\nlicense = \"NOASSERTION\"\nfetch = true\n\
             source_url = \"http://example.org/plain.sfc\"\nsha256 = \"{good}\"\n\n\
             [[asset]]\npath = \"fetched/krom/short.sfc\"\nlicense = \"NOASSERTION\"\nfetch = true\n\
             source_url = \"https://example.org/short.sfc\"\nsha256 = \"00\"\n\n\
             [[asset]]\npath = \"third-party/x.sfc\"\nlicense = \"MIT\"\n\
             source_url = \"https://example.org/x.sfc\"\nsha256 = \"{good}\"\n"
        );
        let manifest = toml::from_str::<Manifest>(&text).unwrap();
        let expected = FetchedFile {
            path: String::from("fetched/krom/RotZoom.sfc"),
            source_url: String::from("https://example.org/RotZoom.sfc"),
            sha256: good,
        };
        assert_eq!(files(&manifest), vec![expected]);
    }

    #[test]
    fn a_download_with_another_hash_is_reported() {
        let file = FetchedFile {
            path: String::from("fetched/a.sfc"),
            source_url: String::from("https://example.org/a.sfc"),
            sha256: hash('a'),
        };
        assert_eq!(mismatch(&file, &hash('a')), None);
        let finding = mismatch(&file, &hash('b')).unwrap().to_string();
        assert!(
            finding.starts_with("tests/assets/fetched/a.sfc:"),
            "{finding}"
        );
        assert!(finding.contains("discarded"), "{finding}");
    }
}
