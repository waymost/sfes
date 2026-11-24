//! The bibliography registry, `provenance/bibliography.toml` (PROV-02), and the set of ids a
//! source tag may name.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

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
    /// A regular expression the whole of an anchor must match (ADR 0038, point 1). Without
    /// it the source takes no anchor.
    anchor: Option<String>,
    /// The file name of the project owner's reading copy, for `provenance lint --documents`
    /// (ADR 0038, point 3).
    file: Option<String>,
}

/// Sources whose anchors name pages archived under `provenance/archive/<id>/<anchor>.html`
/// (ADR 0038, point 2).
const ARCHIVED_PAGES: &[&str] = &["snesdev-wiki"];

/// What an anchor of one source may be.
#[derive(Debug, Default)]
pub(crate) struct AnchorRule {
    /// The whole-anchor pattern.
    pattern: Option<Regex>,
    /// The archived pages, for a source in [`ARCHIVED_PAGES`].
    archived: Option<BTreeSet<String>>,
    /// The reading copy's file name.
    pub(crate) file: Option<String>,
}

/// The ids a source tag may name: bibliography entries and existing ADRs (`adr-NNNN`), with
/// each entry's anchor rule.
pub(crate) struct KnownIds {
    ids: BTreeSet<String>,
    rules: BTreeMap<String, AnchorRule>,
}

impl KnownIds {
    pub(crate) fn contains(&self, id: &str) -> bool {
        self.ids.contains(id)
    }

    /// The anchor rule of bibliography entry `id`, or `None` for an ADR or an unknown id.
    pub(crate) fn rule(&self, id: &str) -> Option<&AnchorRule> {
        self.rules.get(id)
    }

    /// Why `anchor` is not an anchor of `id`, or `None` when it is one in form (ADR 0038,
    /// point 2). `id` must be known.
    pub(crate) fn anchor_problem(&self, id: &str, anchor: &str) -> Option<String> {
        let Some(rule) = self.rule(id) else {
            return Some(format!("`{id}#{anchor}`: an ADR takes no anchor"));
        };
        let Some(pattern) = &rule.pattern else {
            return Some(format!(
                "`{id}#{anchor}`: `{id}` takes no anchor (no `anchor` pattern in {PATH})"
            ));
        };
        if !pattern.is_match(anchor) {
            return Some(format!(
                "`{id}#{anchor}`: not in the form of `{id}`'s anchors, `{}`; see its notes in {PATH}",
                pattern.as_str()
            ));
        }
        if let Some(archived) = &rule.archived
            && !archived.contains(anchor)
        {
            return Some(format!(
                "`{id}#{anchor}`: no archived page provenance/archive/{id}/{anchor}.html"
            ));
        }
        None
    }

    /// Known `ids`, with whole-anchor `patterns` for some of them.
    #[cfg(test)]
    pub(crate) fn with_anchors(ids: &[&str], patterns: &[(&str, &str)]) -> Self {
        let mut rules = ids
            .iter()
            .filter(|id| !id.starts_with("adr-"))
            .map(|id| (String::from(*id), AnchorRule::default()))
            .collect::<BTreeMap<_, _>>();
        for &(id, pattern) in patterns {
            if let Some(rule) = rules.get_mut(id) {
                rule.pattern = Some(Regex::new(&format!("^(?:{pattern})$")).unwrap());
            }
        }
        Self {
            ids: ids
                .iter()
                .map(|id| String::from(*id))
                .collect::<BTreeSet<_>>(),
            rules,
        }
    }
}

/// Loads and validates the bibliography and collects ADR ids. Problems in the bibliography
/// itself are returned as findings.
pub(crate) fn load(root: &Path) -> Result<(KnownIds, Vec<Finding>)> {
    let text = fs::read_to_string(root.join(PATH)).with_context(|| format!("reading {PATH}"))?;
    let bibliography =
        toml::from_str::<Bibliography>(&text).with_context(|| format!("parsing {PATH}"))?;
    let (mut ids, mut rules, findings) = validate(&bibliography)?;
    for (id, rule) in &mut rules {
        if ARCHIVED_PAGES.contains(&id.as_str()) {
            rule.archived = Some(archived_pages(&root.join("provenance/archive").join(id))?);
        }
    }
    ids.extend(adr_ids(root)?);
    Ok((KnownIds { ids, rules }, findings))
}

/// The stems of the `.html` files in `directory`, none if it does not exist.
fn archived_pages(directory: &Path) -> Result<BTreeSet<String>> {
    let mut pages = BTreeSet::new();
    if !directory.is_dir() {
        return Ok(pages);
    }
    for entry in fs::read_dir(directory)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if let Some(stem) = name.strip_suffix(".html") {
            pages.insert(String::from(stem));
        }
    }
    Ok(pages)
}

type Validated = (BTreeSet<String>, BTreeMap<String, AnchorRule>, Vec<Finding>);

fn validate(bibliography: &Bibliography) -> Result<Validated> {
    let id_format = Regex::new(r"^[a-z0-9][a-z0-9-]*$")?;
    let date_format = Regex::new(r"^\d{4}-\d{2}-\d{2}$")?;
    let hash_format = Regex::new(r"^[0-9a-f]{64}$")?;
    let mut ids = BTreeSet::new();
    let mut rules = BTreeMap::new();
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
            anchor,
            file,
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
        let pattern = match anchor {
            Some(anchor) => match Regex::new(&format!("^(?:{anchor})$")) {
                Ok(pattern) => Some(pattern),
                Err(error) => {
                    problem(format!("anchor is not a regular expression: {error}"));
                    None
                }
            },
            None => None,
        };
        let rule = AnchorRule {
            pattern,
            archived: None,
            file: file.clone(),
        };
        rules.insert(id.clone(), rule);
    }
    Ok((ids, rules, findings))
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
        let (ids, _, findings) = validate(&bibliography).unwrap();
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
        let (_, _, findings) = validate(&bibliography).unwrap();
        assert_eq!(findings.len(), 5, "{findings:?}");
    }

    #[test]
    fn an_anchor_pattern_must_compile_and_match_whole() {
        let bibliography = parse(
            r#"
            [[source]]
            id = "good"
            title = "x"
            url = "https://example.org"
            license = "MIT"
            redistributable = true
            anchor = "[0-9a-f]{4}"

            [[source]]
            id = "bad"
            title = "x"
            url = "https://example.org"
            license = "MIT"
            redistributable = true
            anchor = "("
            "#,
        );
        let (ids, rules, findings) = validate(&bibliography).unwrap();
        assert_eq!(findings.len(), 1, "{findings:?}");
        let known = KnownIds { ids, rules };
        assert_eq!(known.anchor_problem("good", "2100"), None);
        assert!(known.anchor_problem("good", "21000").is_some());
        assert!(known.anchor_problem("bad", "x").is_some());
        assert!(known.anchor_problem("adr-0001", "x").is_some());
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
