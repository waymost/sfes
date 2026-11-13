//! The bibliography registry, `provenance/bibliography.toml` (PROV-02), and the set of ids a
//! source tag may name.

use std::{collections::BTreeSet, fs, path::Path};

use anyhow::{Context, Result};
use regex::Regex;

use crate::finding::Finding;

const PATH: &str = "provenance/bibliography.toml";

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Bibliography {
    #[serde(rename = "source", default)]
    sources: Vec<Source>,
}

#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    id: String,
    title: String,
    url: String,
    license: String,
    #[expect(
        dead_code,
        reason = "schema field: parsed so unknown or mistyped fields fail, not read"
    )]
    redistributable: bool,
    retrieved: Option<String>,
    #[expect(
        dead_code,
        reason = "schema field: parsed so unknown or mistyped fields fail, not read"
    )]
    version: Option<String>,
    sha256: Option<String>,
    #[expect(
        dead_code,
        reason = "schema field: parsed so unknown or mistyped fields fail, not read"
    )]
    notes: Option<String>,
}

/// The ids a source tag may name: bibliography entries and existing ADRs (`adr-NNNN`).
pub(crate) struct KnownIds {
    ids: BTreeSet<String>,
}

impl KnownIds {
    pub(crate) fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }

    #[cfg(test)]
    pub(crate) fn from_ids(ids: &[&str]) -> Self {
        Self {
            ids: ids
                .iter()
                .map(|id| String::from(*id))
                .collect::<BTreeSet<_>>(),
        }
    }
}

/// Loads and validates the bibliography and collects ADR ids. Problems in the bibliography
/// itself are returned as findings.
pub(crate) fn load(root: &Path) -> Result<(KnownIds, Vec<Finding>)> {
    let text = fs::read_to_string(root.join(PATH)).with_context(|| format!("reading {PATH}"))?;
    let bibliography =
        toml::from_str::<Bibliography>(&text).with_context(|| format!("parsing {PATH}"))?;
    let (mut ids, findings) = validate(&bibliography)?;
    ids.extend(adr_ids(root)?);
    Ok((KnownIds { ids }, findings))
}

fn validate(bibliography: &Bibliography) -> Result<(BTreeSet<String>, Vec<Finding>)> {
    let id_format = Regex::new(r"^[a-z0-9][a-z0-9-]*$")?;
    let date_format = Regex::new(r"^\d{4}-\d{2}-\d{2}$")?;
    let hash_format = Regex::new(r"^[0-9a-f]{64}$")?;
    let mut ids = BTreeSet::new();
    let mut findings = Vec::new();
    for source in &bibliography.sources {
        let Source {
            id,
            title,
            url,
            license,
            redistributable: _,
            retrieved,
            version: _,
            sha256,
            notes: _,
        } = source;
        let mut problem = |message: String| {
            findings.push(Finding::new(PATH, 0, format!("source `{id}`: {message}")));
        };
        if !id_format.is_match(id) {
            problem(String::from(
                "id must be lowercase letters, digits and hyphens",
            ));
        }
        if id.starts_with("adr-") {
            problem(String::from(
                "ids starting with `adr-` are reserved for ADRs",
            ));
        }
        if !ids.insert(id.clone()) {
            problem(String::from("duplicate id"));
        }
        if title.trim().is_empty() || license.trim().is_empty() {
            problem(String::from("title and license must not be empty"));
        }
        if !url.starts_with("https://") && !url.starts_with("http://") {
            problem(String::from("url must be http(s)"));
        }
        if let Some(date) = retrieved
            && !date_format.is_match(date)
        {
            problem(String::from("retrieved must be YYYY-MM-DD"));
        }
        if let Some(hash) = sha256
            && !hash_format.is_match(hash)
        {
            problem(String::from("sha256 must be 64 lowercase hex digits"));
        }
    }
    Ok((ids, findings))
}

fn adr_ids(root: &Path) -> Result<BTreeSet<String>> {
    let adr_name = Regex::new(r"^(\d{4})-.+\.md$")?;
    let mut ids = BTreeSet::new();
    let directory = root.join("docs/adr");
    if !directory.is_dir() {
        return Ok(ids);
    }
    for entry in fs::read_dir(&directory)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if let Some(captures) = adr_name.captures(&name)
            && &captures[1] != "0000"
        {
            ids.insert(format!("adr-{}", &captures[1]));
        }
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Bibliography {
        toml::from_str::<Bibliography>(text).unwrap()
    }

    #[test]
    fn valid_entry_passes() {
        let bibliography = parse(
            r#"
            [[source]]
            id = "fullsnes"
            title = "fullsnes"
            url = "https://problemkaputt.de/fullsnes.htm"
            license = "unstated"
            redistributable = false
            retrieved = "2026-11-12"
            "#,
        );
        let (ids, findings) = validate(&bibliography).unwrap();
        assert!(findings.is_empty(), "{findings:?}");
        assert!(ids.contains("fullsnes"));
    }

    #[test]
    fn bad_entries_are_reported() {
        let bibliography = parse(
            r#"
            [[source]]
            id = "Bad_Id"
            title = "x"
            url = "ftp://example"
            license = "MIT"
            redistributable = true
            retrieved = "12/11/2026"
            sha256 = "abc"

            [[source]]
            id = "adr-0001"
            title = "x"
            url = "https://example.org"
            license = "MIT"
            redistributable = true
            "#,
        );
        let (_, findings) = validate(&bibliography).unwrap();
        assert_eq!(findings.len(), 5, "{findings:?}");
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let result = toml::from_str::<Bibliography>(
            r#"
            [[source]]
            id = "x"
            title = "x"
            url = "https://example.org"
            license = "MIT"
            redistributable = true
            licence = "typo"
            "#,
        );
        assert!(result.is_err());
    }
}
