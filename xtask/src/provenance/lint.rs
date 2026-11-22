//! `cargo xtask provenance lint`: source tags in chip crates (PROV-03).
//!
//! In every `crates/sfes-snes-*` Rust file:
//! - the file's leading comment block carries a tag (the module tag);
//! - every function carries a tag in its doc, comment or attribute block: free functions, impl
//!   methods, trait methods and `#[test]` functions, private ones included. A pure helper cites
//!   the source of the behaviour it serves. Methods in an impl of a standard-library trait
//!   ([`STD_TRAITS`]: conversions, formatting, comparison, operators, …) are exempt;
//! - every tag names a bibliography id or an existing ADR.
//!
//! In every `crates/` Rust file and every cartridge source, a run of consecutive tag lines lists
//! documents first, in any order, then ADRs in ascending order, each tag once (AGENTS.md
//! section 1).
//!
//! Tags anywhere else under `crates/` must also name known ids.
//!
//! In every test-cartridge source (`.s` and `.inc` under `tests/carts/`, ADR 0013):
//! - the file's leading `;` comment block carries a tag;
//! - every `.proc` and `.macro` carries a tag on its line or in the `;` comment lines directly
//!   above it;
//! - every tag names a bibliography id or an existing ADR.
//!
//! A tag is `@source: <id>[#anchor]`, with several separated by commas. `syn` finds the items
//! and their lines; the raw text is searched for tags, because `syn` drops `//` comments.

use std::{fs, path::Path};

use anyhow::{Context, Result};
use syn::{spanned::Spanned, visit::Visit};

use crate::{
    carts::CARTS,
    files,
    finding::Finding,
    provenance::bibliography::{self, KnownIds},
};

const MARKER: &str = "@source:";

/// Standard-library traits whose impls need no tag on their methods, matched by the last
/// segment of the trait path.
const STD_TRAITS: &[&str] = &[
    "Add",
    "AddAssign",
    "AsMut",
    "AsRef",
    "BitAnd",
    "BitAndAssign",
    "BitOr",
    "BitOrAssign",
    "BitXor",
    "BitXorAssign",
    "Borrow",
    "BorrowMut",
    "Clone",
    "Debug",
    "Default",
    "Deref",
    "DerefMut",
    "Display",
    "Div",
    "DivAssign",
    "DoubleEndedIterator",
    "Drop",
    "Eq",
    "Error",
    "ExactSizeIterator",
    "Extend",
    "From",
    "FromIterator",
    "FromStr",
    "Hash",
    "Index",
    "IndexMut",
    "IntoIterator",
    "Iterator",
    "LowerHex",
    "Mul",
    "MulAssign",
    "Neg",
    "Not",
    "Ord",
    "PartialEq",
    "PartialOrd",
    "Rem",
    "RemAssign",
    "Shl",
    "ShlAssign",
    "Shr",
    "ShrAssign",
    "Sub",
    "SubAssign",
    "TryFrom",
    "UpperHex",
];

pub(crate) fn run(root: &Path) -> Result<Vec<Finding>> {
    let (known, mut findings) = bibliography::load(root)?;
    for path in files::walk(&root.join("crates"), &|_| false)? {
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let display = files::display(root, &path);
        let text = fs::read_to_string(&path).with_context(|| format!("reading {display}"))?;
        findings.extend(check_tag_ids(&display, &text, &known));
        findings.extend(check_tag_order(&display, &text, "//"));
        if is_chip_crate_file(&display) {
            findings.extend(check_required_tags(&display, &text));
        }
    }
    for path in files::walk(&root.join(CARTS), &|_| false)? {
        if !path.extension().is_some_and(|extension| {
            CART_SOURCE_EXTENSIONS
                .iter()
                .any(|known| extension == *known)
        }) {
            continue;
        }
        let display = files::display(root, &path);
        let text = fs::read_to_string(&path).with_context(|| format!("reading {display}"))?;
        findings.extend(check_tag_ids(&display, &text, &known));
        findings.extend(check_tag_order(&display, &text, ";"));
        findings.extend(check_cart_tags(&display, &text));
    }
    Ok(findings)
}

/// Assembly source extensions in `tests/carts/`: sources and includes.
const CART_SOURCE_EXTENSIONS: &[&str] = &["s", "inc"];

/// Assembly directives that open a test case or a reusable block, each needing a tag.
const CART_BLOCKS: &[&str] = &[".proc", ".macro"];

/// The module tag and the per-block tags of one cartridge source.
fn check_cart_tags(display: &str, text: &str) -> Vec<Finding> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut findings = Vec::new();
    let leading_tag = lines
        .iter()
        .map(|line| line.trim())
        .take_while(|line| line.is_empty() || line.starts_with(';'))
        .any(has_tag);
    if !leading_tag {
        findings.push(Finding::new(
            display,
            1,
            "cartridge source has no `; @source:` tag in its leading comments",
        ));
    }
    for (index, line) in lines.iter().enumerate() {
        let mut words = line.split_whitespace();
        let directive = words.next().unwrap_or_default().to_ascii_lowercase();
        if !CART_BLOCKS.contains(&directive.as_str()) {
            continue;
        }
        let name = words
            .next()
            .unwrap_or_default()
            .trim_end_matches(',')
            .to_owned();
        let tagged = has_tag(line)
            || lines[..index]
                .iter()
                .rev()
                .take_while(|above| above.trim_start().starts_with(';'))
                .any(|above| has_tag(above));
        if !tagged {
            findings.push(Finding::new(
                display,
                index + 1,
                format!("`{directive} {name}` has no `; @source:` tag"),
            ));
        }
    }
    findings
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

/// The full tags (`id#anchor`) on a line that is nothing but a `prefix` comment holding tags,
/// or `None` for any other line.
fn tag_line(line: &str, prefix: &str) -> Option<Vec<String>> {
    let rest = line.trim().strip_prefix(prefix)?.trim_start();
    let list = rest.strip_prefix(MARKER)?;
    let tags = list
        .split(',')
        .filter_map(|segment| segment.split_whitespace().next())
        .map(String::from)
        .collect::<Vec<_>>();
    Some(tags)
}

/// The sort key of a tag: documents, all equal, before ADRs by number.
fn tag_order_key(tag: &str) -> Option<&str> {
    tag.strip_prefix("adr-")
}

/// Each run of consecutive tag lines lists documents, in any order, then ADRs in ascending
/// order, with no tag twice.
fn check_tag_order(display: &str, text: &str, prefix: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut previous: Option<String> = None;
    let mut seen = Vec::<String>::new();
    for (index, line) in text.lines().enumerate() {
        let Some(tags) = tag_line(line, prefix) else {
            previous = None;
            seen.clear();
            continue;
        };
        for tag in tags {
            if seen.contains(&tag) {
                findings.push(Finding::new(
                    display,
                    index + 1,
                    format!("source tag `{tag}` is listed twice"),
                ));
                continue;
            }
            if let Some(before) = &previous
                && tag_order_key(&tag) < tag_order_key(before)
            {
                findings.push(Finding::new(
                    display,
                    index + 1,
                    format!(
                        "source tag `{tag}` comes before `{before}`: documents first, then ADRs in ascending order"
                    ),
                ));
            }
            seen.push(tag.clone());
            previous = Some(tag);
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
    in_std_trait_impl: bool,
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

impl<'ast> Visit<'ast> for Collector {
    fn visit_item_fn(&mut self, item: &'ast syn::ItemFn) {
        self.push(item, &item.sig);
        syn::visit::visit_item_fn(self, item);
    }

    fn visit_item_impl(&mut self, item: &'ast syn::ItemImpl) {
        let outer = self.in_std_trait_impl;
        self.in_std_trait_impl = item
            .trait_
            .as_ref()
            .and_then(|(path, _)| path.segments.last())
            .is_some_and(|segment| STD_TRAITS.contains(&segment.ident.to_string().as_str()));
        syn::visit::visit_item_impl(self, item);
        self.in_std_trait_impl = outer;
    }

    fn visit_impl_item_fn(&mut self, item: &'ast syn::ImplItemFn) {
        if !self.in_std_trait_impl {
            self.push(item, &item.sig);
        }
        syn::visit::visit_impl_item_fn(self, item);
    }

    fn visit_trait_item_fn(&mut self, item: &'ast syn::TraitItemFn) {
        self.push(item, &item.sig);
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

// @source: fullsnes#dma
fn private_helper() {}

struct Channel;

impl Channel {
    // @source: fullsnes#dma
    fn step(&mut self) {}
}

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
    fn private_functions_need_tags() {
        let text = "\
// @source: fullsnes

fn helper() {}

struct Channel;

impl Channel {
    fn step(&mut self) {
        fn nested() {}
    }
}

trait Private {
    fn idle(&mut self);
}

impl Private for Channel {
    fn idle(&mut self) {}
}
";
        assert_eq!(
            messages(text),
            [
                "3: `helper` has no `@source:` tag",
                "8: `step` has no `@source:` tag",
                "9: `nested` has no `@source:` tag",
                "14: `idle` has no `@source:` tag",
                "18: `idle` has no `@source:` tag",
            ]
        );
    }

    #[test]
    fn std_trait_impls_are_exempt() {
        let text = "\
// @source: fullsnes

struct Channel;

impl From<u8> for Channel {
    fn from(_: u8) -> Self {
        Channel
    }
}

impl core::fmt::Display for Channel {
    fn fmt(&self, _: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        Ok(())
    }
}

impl Channel {
    fn step(&mut self) {}
}
";
        assert_eq!(messages(text), ["18: `step` has no `@source:` tag"]);
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

    fn cart_messages(text: &str) -> Vec<String> {
        let known = KnownIds::from_ids(&["fullsnes", "adr-0013"]);
        let mut findings = check_tag_ids("tests/carts/x/x.s", text, &known);
        findings.extend(check_cart_tags("tests/carts/x/x.s", text));
        findings
            .into_iter()
            .map(|finding| format!("{}: {}", finding.line, finding.message))
            .collect::<Vec<_>>()
    }

    #[test]
    fn tagged_cart_source_passes() {
        let text = "\
; A cartridge.
; @source: adr-0013

.p816
.include \"header.inc\"

; Boots.
; @source: fullsnes#snescartridgeromheader
.proc reset
    stp
.endproc

.PROC halt ; @source: fullsnes
    stp
.endproc

; @source: adr-0013
.macro twice value
    .byte value, value
.endmacro
";
        assert_eq!(cart_messages(text), Vec::<String>::new());
    }

    #[test]
    fn missing_cart_tags_are_reported() {
        let text = "\
; A cartridge without a tag.
.p816

; @source: fullsnes

.proc reset
    stp
.endproc

; Halts.
.proc halt
.endproc

.macro twice value
.endmacro
";
        assert_eq!(
            cart_messages(text),
            [
                "1: cartridge source has no `; @source:` tag in its leading comments",
                "6: `.proc reset` has no `; @source:` tag",
                "11: `.proc halt` has no `; @source:` tag",
                "14: `.macro twice` has no `; @source:` tag",
            ]
        );
    }

    #[test]
    fn cart_tag_ids_are_checked() {
        let text = "; @source: fulsnes#x\n; @source: Adr-0013\n";
        assert_eq!(
            cart_messages(text),
            [
                "1: unknown source id `fulsnes`: add it to provenance/bibliography.toml or cite an existing ADR",
                "2: malformed source tag `Adr-0013`",
            ]
        );
    }

    #[test]
    fn tags_are_listed_documents_then_adrs_once() {
        let text = "\
// @source: anomie-regs#2140
// @source: fullsnes#dma
// @source: adr-0001
// @source: adr-0019

// @source: fullsnes
// @source: adr-0019
// @source: anomie-regs
// @source: adr-0019, adr-0001
// @source: fullsnes
let code = 0; // @source: zzz
";
        let messages = check_tag_order("crates/x.rs", text, "//")
            .into_iter()
            .map(|finding| format!("{}: {}", finding.line, finding.message))
            .collect::<Vec<_>>();
        assert_eq!(
            messages,
            [
                "8: source tag `anomie-regs` comes before `adr-0019`: documents first, then ADRs in ascending order",
                "9: source tag `adr-0019` is listed twice",
                "10: source tag `fullsnes` is listed twice",
            ]
        );
    }

    #[test]
    fn only_chip_crates_need_tags() {
        assert!(is_chip_crate_file("crates/sfes-snes-cpu/src/lib.rs"));
        assert!(is_chip_crate_file("crates/sfes-snes-cpu/tests/vectors.rs"));
        assert!(!is_chip_crate_file("crates/sfes-api/src/lib.rs"));
        assert!(!is_chip_crate_file("xtask/src/main.rs"));
    }
}
