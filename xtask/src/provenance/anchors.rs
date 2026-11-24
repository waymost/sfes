//! `provenance lint --documents <dir>`: every cited anchor is found in the project owner's
//! reading copy of its document (ADR 0038, point 3).
//!
//! Each document's anchors are read from the copy named by its bibliography entry's `file`,
//! in memory only: nothing derived from a document is written (ADR 0038, point 4). Sources
//! with no parser here (`sst-65816`, `snesdev-wiki`) are settled by the form check alone.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use anyhow::Result;
use regex::Regex;

use crate::{
    finding::Finding,
    provenance::{bibliography::KnownIds, lint::Cited},
};

/// How one document's anchors are found.
enum Document {
    /// A set of anchors, complete for the document.
    Exact(BTreeSet<String>),
    /// The datasheet: its anchor set, and a note is found when its table is.
    Datasheet(BTreeSet<String>),
    /// The manual: its chapter numbers and the kebab case of every line.
    Manual {
        chapters: BTreeSet<String>,
        lines: BTreeSet<String>,
    },
}

impl Document {
    /// Whether `anchor` names a place in the document.
    fn contains(&self, anchor: &str) -> bool {
        match self {
            Self::Exact(anchors) => anchors.contains(anchor),
            Self::Datasheet(anchors) => {
                anchors.contains(anchor)
                    || anchor
                        .split_once("-note-")
                        .is_some_and(|(table, _)| anchors.contains(table))
            }
            Self::Manual { chapters, lines } => {
                let Some(rest) = anchor.strip_prefix("chapter-") else {
                    return false;
                };
                let (number, heading) = rest.split_once('-').unwrap_or((rest, ""));
                chapters.contains(number)
                    && (heading.is_empty()
                        || lines.iter().any(|line| {
                            line == heading
                                || line
                                    .strip_prefix(heading)
                                    .is_some_and(|after| after.starts_with('-'))
                        }))
            }
        }
    }
}

/// Reports every anchor in `cited` that its document's copy in `documents` does not have, and
/// each document whose copy is missing.
pub(crate) fn check(documents: &Path, known: &KnownIds, cited: &[Cited]) -> Result<Vec<Finding>> {
    let mut by_id = BTreeMap::<&str, Vec<&Cited>>::new();
    for tag in cited {
        by_id.entry(tag.id.as_str()).or_default().push(tag);
    }
    let mut findings = Vec::new();
    for (id, tags) in by_id {
        let Some(file) = known.rule(id).and_then(|rule| rule.file.as_deref()) else {
            continue;
        };
        let Some(parse) = parser(id) else {
            continue;
        };
        let path = documents.join(file);
        let text = match fs::read(&path) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(error) => {
                findings.push(Finding::new(
                    path.display(),
                    0,
                    format!("cannot read `{id}`'s reading copy: {error}"),
                ));
                continue;
            }
        };
        let document = parse(&text)?;
        for tag in tags {
            if !document.contains(&tag.anchor) {
                findings.push(Finding::new(
                    &tag.display,
                    tag.line,
                    format!("`{id}#{}` is not in {}", tag.anchor, path.display()),
                ));
            }
        }
    }
    Ok(findings)
}

/// The parser for bibliography entry `id`, if there is one.
fn parser(id: &str) -> Option<fn(&str) -> Result<Document>> {
    match id {
        "fullsnes" => Some(html_anchors),
        "anomie-regs" => Some(anomie_regs),
        "anomie-timing" | "anomie-memmap" => {
            Some(|text| Ok(Document::Exact(anomie_headings(text))))
        }
        "wdc-w65c816s" => Some(datasheet),
        "wdc-65816-manual" => Some(manual),
        "6502org-65c816-opcodes" => Some(numbered_page),
        _ => None,
    }
}

/// Kebab case as the bibliography notes define it: lower case, `&` as `and`, parenthesised
/// text and other punctuation dropped, words joined with `-`.
fn kebab(heading: &str) -> String {
    let mut cleaned = String::new();
    let mut depth = 0_usize;
    for character in heading.chars() {
        match character {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            _ if depth > 0 => {}
            '&' => cleaned.push_str(" and "),
            '-' | '/' => cleaned.push(' '),
            _ if character.is_ascii_alphanumeric() || character.is_whitespace() => {
                cleaned.push(character.to_ascii_lowercase());
            }
            _ => {}
        }
    }
    cleaned.split_whitespace().collect::<Vec<_>>().join("-")
}

/// fullsnes: the `<a name="…">` anchors of the HTML.
fn html_anchors(text: &str) -> Result<Document> {
    let name = Regex::new(r#"(?i)<a\s+name="([^"]+)""#)?;
    Ok(Document::Exact(
        name.captures_iter(text)
            .map(|captures| captures[1].to_ascii_lowercase())
            .collect(),
    ))
}

/// The heading paths of one of anomie's files: a line underlined with `=` is a top-level
/// heading, one underlined with `-` a heading below the last top-level one.
fn anomie_headings(text: &str) -> BTreeSet<String> {
    let mut anchors = BTreeSet::new();
    let mut top = String::new();
    let lines = text.lines().map(str::trim_end).collect::<Vec<_>>();
    for pair in lines.windows(2) {
        let [heading, underline] = pair else {
            continue;
        };
        let level = if underline.len() > 1 && underline.len() == heading.len() {
            if underline.chars().all(|character| character == '=') {
                Some(true)
            } else if underline.chars().all(|character| character == '-') {
                Some(false)
            } else {
                None
            }
        } else {
            None
        };
        match level {
            Some(true) => {
                top = kebab(heading);
                anchors.insert(top.clone());
            }
            Some(false) => {
                anchors.insert(format!("{top}/{}", kebab(heading)));
            }
            None => {}
        }
    }
    anchors
}

/// anomie-regs: its heading paths, and the register entries of its REGISTERS section, each an
/// address at the start of a line.
fn anomie_regs(text: &str) -> Result<Document> {
    let mut anchors = anomie_headings(text);
    let entry = Regex::new(r"^([0-9a-f]{2}[0-9a-fx][0-9a-f]) ")?;
    let mut in_registers = false;
    let mut previous = "";
    for line in text.lines() {
        if line.starts_with("==") && line.chars().all(|character| character == '=') {
            in_registers = previous.trim() == "REGISTERS";
        }
        if in_registers && let Some(captures) = entry.captures(line) {
            anchors.insert(String::from(&captures[1]));
        }
        previous = line;
    }
    Ok(Document::Exact(anchors))
}

/// The WDC datasheet's text: numbered sections, tables, figures and Table 5-7's rows. A row
/// is its label at the start of a line, with or without a dot, then its address mode.
fn datasheet(text: &str) -> Result<Document> {
    let section = Regex::new(r"(?m)^\s*(\d+(?:\.\d+)*)\.?\s+[A-Z]")?;
    let numbered = Regex::new(r"(Table|Figure)\s+(\d+-\d+)")?;
    let row = Regex::new(
        r"(?m)^\s*(\d+[a-z]?)\.?\s*(Absolute|Accumulator|Block|Direct|Immediate|Implied|Relative|Stack|Stop|Wait)",
    )?;
    let mut anchors = BTreeSet::new();
    for captures in section.captures_iter(text) {
        anchors.insert(String::from(&captures[1]));
    }
    for captures in numbered.captures_iter(text) {
        anchors.insert(format!(
            "{}-{}",
            captures[1].to_ascii_lowercase(),
            &captures[2]
        ));
    }
    for captures in row.captures_iter(text) {
        anchors.insert(format!("table-5-7-{}", &captures[1]));
    }
    Ok(Document::Datasheet(anchors))
}

/// The programming manual's text: the chapter numbers, printed as `Chapter N` or as `N)` on a
/// line of their own, and every line in kebab case.
fn manual(text: &str) -> Result<Document> {
    let chapter = Regex::new(r"(?im)\bchapter\s+(\d+)\b|^\s*(\d+)\)\s*$")?;
    Ok(Document::Manual {
        chapters: chapter
            .captures_iter(text)
            .filter_map(|captures| captures.get(1).or_else(|| captures.get(2)))
            .map(|number| String::from(number.as_str()))
            .collect(),
        lines: text
            .lines()
            .map(kebab)
            .filter(|line| !line.is_empty())
            .collect(),
    })
}

/// The 6502.org tutorial: the section numbers printed at the start of a line of its text, and
/// `appendix` when it has one.
fn numbered_page(text: &str) -> Result<Document> {
    let tag = Regex::new(r"<[^>]*>")?;
    let plain = tag.replace_all(text, "\n").replace("&nbsp;", " ");
    let number = Regex::new(r"(?m)^\s*(\d+(?:\.\d+)*)\.?\s+\S")?;
    let mut anchors = number
        .captures_iter(&plain)
        .map(|captures| String::from(&captures[1]))
        .collect::<BTreeSet<_>>();
    if plain.to_ascii_lowercase().contains("appendix") {
        anchors.insert(String::from("appendix"));
    }
    Ok(Document::Exact(anchors))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kebab_case_follows_the_notes() {
        assert_eq!(kebab("S-CPU (5A22)"), "s-cpu");
        assert_eq!(kebab("CLOCKS & REFRESH"), "clocks-and-refresh");
        assert_eq!(
            kebab("Tile Maps and Character Maps"),
            "tile-maps-and-character-maps"
        );
        assert_eq!(kebab("\"Open Port\""), "open-port");
    }

    #[test]
    fn anomie_headings_and_registers_are_paths_and_addresses() {
        let text = "\
REGISTERS
=========

2100  wb++++ INIDISP - Screen Display
43x0  rwb++++ DMAPx - DMA Control
  2101 is not an entry

BACKGROUNDS
===========

Mode 7
------
2105 in prose is not an entry
";
        let Document::Exact(anchors) = anomie_regs(text).unwrap() else {
            panic!("anomie-regs is an exact set");
        };
        let expected = [
            "2100",
            "43x0",
            "backgrounds",
            "backgrounds/mode-7",
            "registers",
        ];
        assert_eq!(anchors, expected.map(String::from).into_iter().collect());
    }

    #[test]
    fn datasheet_notes_need_their_table() {
        let text = "2.20 Memory Select\nTable 5-7 Instruction Operation\n1a. Absolute a\n6a Absolute, X\n18.Immediate #\n19 OpCodes\n";
        let document = datasheet(text).unwrap();
        for anchor in [
            "2.20",
            "table-5-7",
            "table-5-7-1a",
            "table-5-7-6a",
            "table-5-7-18",
            "table-5-7-note-4",
        ] {
            assert!(document.contains(anchor), "{anchor}");
        }
        assert!(!document.contains("table-5-8-note-1"));
        assert!(!document.contains("table-5-7-2b"));
        assert!(!document.contains("table-5-7-19"));
    }

    #[test]
    fn manual_headings_need_their_chapter_and_a_line() {
        let text =
            "17)\nChapter Seventeen\nBlock Move Addressing\nin Chapter 18.\nADC Add With Carry\n";
        let document = manual(text).unwrap();
        assert!(document.contains("chapter-18"));
        assert!(document.contains("chapter-18-adc"));
        assert!(document.contains("chapter-17-block-move-addressing"));
        assert!(!document.contains("chapter-19"));
        assert!(!document.contains("chapter-18-ad"));
    }

    #[test]
    fn html_anchors_and_page_numbers() {
        let Document::Exact(names) =
            html_anchors(r#"<A NAME="snesiomap"></A><a name="x">"#).unwrap()
        else {
            panic!("fullsnes is an exact set");
        };
        assert!(names.contains("snesiomap") && names.contains("x"));
        let Document::Exact(numbers) = numbered_page("<h3>5.1.1 Wrapping</h3><p>Appendix").unwrap()
        else {
            panic!("the page is an exact set");
        };
        assert!(numbers.contains("5.1.1") && numbers.contains("appendix"));
    }
}
