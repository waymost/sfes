//! `cargo xtask provenance lint`: source tags in chip crates (PROV-03).
//!
//! In every `crates/sfes-snes-*` Rust file:
//! - the file's leading comment block carries a tag (the module tag);
//! - every function with explicit visibility (`pub`, `pub(crate)`, …), every method of a `pub`
//!   trait, and every `#[test]` function carries a tag in its doc, comment or attribute block;
//! - every tag names a bibliography id or an existing ADR.
//!
//! Tags anywhere else under `crates/` must also name known ids.
//!
//! A tag is `@source: <id>[#anchor]`, with several separated by commas. `syn` finds the items
//! and their lines; the raw text is searched for tags, because `syn` drops `//` comments.

use std::{fs, path::Path};

use anyhow::{Context, Result};
use syn::{spanned::Spanned, visit::Visit};

use crate::{
    files,
    finding::Finding,
    provenance::bibliography::{self, KnownIds},
};

const MARKER: &str = "@source:";

pub(crate) fn run(root: &Path) -> Result<Vec<Finding>> {
    let (known, mut findings) = bibliography::load(root)?;
    for path in files::walk(&root.join("crates"), &|_| false)? {
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let display = files::display(root, &path);
        let text = fs::read_to_string(&path).with_context(|| format!("reading {display}"))?;
        findings.extend(check_tag_ids(&display, &text, &known));
        if is_chip_crate_file(&display) {
            findings.extend(check_required_tags(&display, &text));
        }
    }
    Ok(findings)
}

fn is_chip_crate_file(display: &str) -> bool {
    display
        .strip_prefix("crates/")
        .and_then(|rest| rest.split('/').next())
        .is_some_and(|crate_name| crate_name.starts_with("sfes-snes-"))
}

/// The ids named by tags on one line, with an error for each malformed tag.
fn tags_in_line(line: &str) -> Vec<Result<String, String>> {
    let mut tags = Vec::new();
    let mut rest = line;
    while let Some(position) = rest.find(MARKER) {
        rest = &rest[position + MARKER.len()..];
        let list_end = rest.find(MARKER).unwrap_or(rest.len());
        for segment in rest[..list_end].split(',') {
            let token = segment.split_whitespace().next().unwrap_or_default();
            let id = token.split('#').next().unwrap_or_default();
            let valid = !id.is_empty()
                && id.chars().all(|character| {
                    character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
                });
            tags.push(if valid {
                Ok(String::from(id))
            } else {
                Err(format!("malformed source tag `{}`", segment.trim()))
            });
        }
        rest = &rest[list_end..];
    }
    tags
}

fn has_tag(line: &str) -> bool {
    tags_in_line(line).iter().any(Result::is_ok)
}

fn check_tag_ids(display: &str, text: &str, known: &KnownIds) -> Vec<Finding> {
    let mut findings = Vec::new();
    for (index, line) in text.lines().enumerate() {
        for tag in tags_in_line(line) {
            match tag {
                Ok(id) if !known.contains(&id) => findings.push(Finding::new(
                    display,
                    index + 1,
                    format!("unknown source id `{id}`: add it to provenance/bibliography.toml or cite an existing ADR"),
                )),
                Ok(_) => {}
                Err(message) => findings.push(Finding::new(display, index + 1, message)),
            }
        }
    }
    findings
}

fn check_required_tags(display: &str, text: &str) -> Vec<Finding> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut findings = Vec::new();
    if !has_module_tag(&lines) {
        findings.push(Finding::new(
            display,
            1,
            "module has no `// @source:` tag in its leading comments",
        ));
    }
    let file = match syn::parse_file(text) {
        Ok(file) => file,
        Err(error) => {
            let line = error.span().start().line;
            findings.push(Finding::new(
                display,
                line,
                format!("could not parse: {error}"),
            ));
            return findings;
        }
    };
    let mut collector = Collector::default();
    collector.visit_file(&file);
    for function in collector.functions {
        if !function_has_tag(&lines, &function) {
            findings.push(Finding::new(
                display,
                function.fn_line,
                format!("`{}` has no `@source:` tag", function.name),
            ));
        }
    }
    findings
}

fn has_module_tag(lines: &[&str]) -> bool {
    for line in lines {
        let trimmed = line.trim();
        if trimmed.starts_with("//") {
            if has_tag(trimmed) {
                return true;
            }
        } else if !trimmed.is_empty() && !trimmed.starts_with("#!") {
            return false;
        }
    }
    false
}

/// Tag search window: the item's own attribute and doc lines up to the `fn` line, plus the
/// contiguous `//` comment lines directly above it.
fn function_has_tag(lines: &[&str], function: &Function) -> bool {
    let first = function.start_line.saturating_sub(1);
    let last = function.fn_line.min(lines.len());
    if lines[first.min(last)..last]
        .iter()
        .any(|line| has_tag(line))
    {
        return true;
    }
    lines[..first]
        .iter()
        .rev()
        .take_while(|line| line.trim_start().starts_with("//"))
        .any(|line| has_tag(line))
}

struct Function {
    name: String,
    /// First line of the item, including its attributes and doc comments.
    start_line: usize,
    /// Line of the `fn` keyword.
    fn_line: usize,
}

#[derive(Default)]
struct Collector {
    functions: Vec<Function>,
    in_public_trait: bool,
}

impl Collector {
    fn push(&mut self, item: &dyn Spanned, signature: &syn::Signature) {
        self.functions.push(Function {
            name: signature.ident.to_string(),
            start_line: item.span().start().line,
            fn_line: signature.fn_token.span.start().line,
        });
    }
}

fn is_explicitly_visible(visibility: &syn::Visibility) -> bool {
    !matches!(visibility, syn::Visibility::Inherited)
}

fn is_test(attributes: &[syn::Attribute]) -> bool {
    attributes.iter().any(|attribute| {
        attribute
            .path()
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "test")
    })
}

impl<'ast> Visit<'ast> for Collector {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        if is_explicitly_visible(&item.vis) || is_test(&item.attrs) {
            self.push(item, &item.sig);
        }
        syn::visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if is_explicitly_visible(&item.vis) || is_test(&item.attrs) {
            self.push(item, &item.sig);
        }
        syn::visit::visit_impl_item_fn(self, item);
    }

    fn visit_item_trait(&mut self, item: &'ast syn::ItemTrait) {
        let outer = self.in_public_trait;
        self.in_public_trait = matches!(item.vis, syn::Visibility::Public(_));
        syn::visit::visit_item_trait(self, item);
        self.in_public_trait = outer;
    }

    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        if self.in_public_trait {
            self.push(item, &item.sig);
        }
        syn::visit::visit_trait_item_fn(self, item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn messages(text: &str) -> Vec<String> {
        let known = KnownIds::from_ids(&["fullsnes", "adr-0001"]);
        let mut findings = check_tag_ids("crates/sfes-snes-x/src/lib.rs", text, &known);
        findings.extend(check_required_tags("crates/sfes-snes-x/src/lib.rs", text));
        findings
            .into_iter()
            .map(|finding| format!("{}: {}", finding.line, finding.message))
            .collect::<Vec<_>>()
    }

    #[test]
    fn tagged_file_passes() {
        let text = "\
// @source: fullsnes#dma
//! Docs.
#![no_std]

/// Reads.
// @source: fullsnes#dma, adr-0001
pub fn read() {}

// @source: fullsnes
#[inline]
pub(crate) fn write() {}

fn private_helper() {}

/// A trait.
pub trait Bus {
    /// Idle.
    // @source: fullsnes
    fn idle(&mut self);
}

#[cfg(test)]
mod tests {
    #[test]
    // @source: adr-0001
    fn works() {}
}
";
        assert_eq!(messages(text), Vec::<String>::new());
    }

    #[test]
    fn missing_tags_are_reported() {
        let text = "\
//! Docs without a tag.

pub fn read() {}

pub trait Bus {
    fn idle(&mut self);
}

#[test]
fn works() {}
";
        assert_eq!(
            messages(text),
            [
                "1: module has no `// @source:` tag in its leading comments",
                "3: `read` has no `@source:` tag",
                "6: `idle` has no `@source:` tag",
                "10: `works` has no `@source:` tag",
            ]
        );
    }

    #[test]
    fn unknown_and_malformed_ids_are_reported() {
        let text = "// @source: nocash-typo#dma, Fullsnes\n";
        assert_eq!(
            messages(text),
            [
                "1: unknown source id `nocash-typo`: add it to provenance/bibliography.toml or cite an existing ADR",
                "1: malformed source tag `Fullsnes`",
            ]
        );
    }

    #[test]
    fn tag_must_be_adjacent() {
        let text = "\
// @source: fullsnes

// @source: fullsnes

const GAP: u8 = 0;
pub fn read() {}
";
        assert_eq!(messages(text), ["6: `read` has no `@source:` tag"]);
    }

    #[test]
    fn only_chip_crates_need_tags() {
        assert!(is_chip_crate_file("crates/sfes-snes-cpu/src/lib.rs"));
        assert!(is_chip_crate_file("crates/sfes-snes-cpu/tests/vectors.rs"));
        assert!(!is_chip_crate_file("crates/sfes-api/src/lib.rs"));
        assert!(!is_chip_crate_file("xtask/src/main.rs"));
    }
}
