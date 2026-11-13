//! `cargo xtask provenance scan`: signs of emulator source code in the tree (PROV-05).
//!
//! Matches the patterns in `provenance/scan-patterns.toml` against code files, excluding
//! `provenance/`, `docs/` and `tests/assets/` (covered by the asset manifest).

use std::{fs, path::Path};

use anyhow::{Context, Result};
use regex::Regex;

use crate::{files, finding::Finding};

const PATTERNS_PATH: &str = "provenance/scan-patterns.toml";
const CODE_EXTENSIONS: &[&str] = &["rs", "c", "h", "cc", "cpp", "hpp", "py", "pyi", "js", "ts"];
const SKIPPED_DIRECTORIES: &[&str] = &["provenance", "docs", "tests/assets"];

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PatternFile {
    #[serde(rename = "pattern")]
    patterns: Vec<PatternEntry>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct PatternEntry {
    regex: String,
    reason: String,
}

struct Pattern {
    regex: Regex,
    reason: String,
}

pub(crate) fn run(root: &Path) -> Result<Vec<Finding>> {
    let patterns = load_patterns(root)?;
    let skip = |path: &Path| {
        let display = files::display(root, path);
        SKIPPED_DIRECTORIES.contains(&display.as_str())
    };
    let mut findings = Vec::new();
    for path in files::walk(root, &skip)? {
        let is_code = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| CODE_EXTENSIONS.contains(&extension));
        if !is_code {
            continue;
        }
        let display = files::display(root, &path);
        let bytes = fs::read(&path).with_context(|| format!("reading {display}"))?;
        let text = String::from_utf8_lossy(&bytes);
        findings.extend(scan_text(&display, &text, &patterns));
    }
    Ok(findings)
}

fn load_patterns(root: &Path) -> Result<Vec<Pattern>> {
    let text = fs::read_to_string(root.join(PATTERNS_PATH))
        .with_context(|| format!("reading {PATTERNS_PATH}"))?;
    let file =
        toml::from_str::<PatternFile>(&text).with_context(|| format!("parsing {PATTERNS_PATH}"))?;
    file.patterns
        .into_iter()
        .map(|entry| {
            let PatternEntry { regex, reason } = entry;
            let compiled =
                Regex::new(&regex).with_context(|| format!("compiling pattern `{regex}`"))?;
            Ok(Pattern {
                regex: compiled,
                reason,
            })
        })
        .collect::<Result<Vec<_>>>()
}

fn scan_text(display: &str, text: &str, patterns: &[Pattern]) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for pattern in patterns {
            if let Some(found) = pattern.regex.find(line) {
                findings.push(Finding::new(
                    display,
                    index + 1,
                    format!(
                        "{}: `{}` (see PROVENANCE.md, section 3)",
                        pattern.reason,
                        found.as_str()
                    ),
                ));
            }
        }
    }
    findings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_are_reported_per_line() {
        let patterns = vec![Pattern {
            regex: Regex::new(r"(?i)copyright[^\n]{0,80}\bacme-emu\b").unwrap(),
            reason: String::from("emulator copyright header"),
        }];
        let text = "// fine\n// Copyright (c) 2004 ACME-EMU team\n";
        let findings = scan_text("a.rs", text, &patterns);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].line, 2);
    }

    #[test]
    fn shipped_patterns_compile() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        assert!(load_patterns(root).unwrap().len() >= 3);
    }
}
