//! `cargo xtask provenance commits`: sign-off and AI-review rules for a commit range
//! (PROV-06, PROV-07).
//!
//! For every non-merge commit in `<base>..HEAD`:
//! - a `Signed-off-by:` trailer must match the commit author (DCO);
//! - a commit with an `Assisted-by:` trailer that touches a chip crate needs the PR's
//!   "AI-review complete" box ticked.
//!
//! Bot commits (`[bot]` authors) are exempt from sign-off when they touch only manifests,
//! lockfiles, changelogs or workflow pins. The PR body comes from `--pr-body-file` or, in GitHub
//! Actions, from the pull-request event payload.

use std::{env, fs, path::Path, process::Command};

use anyhow::{Context, Result, bail};
use regex::Regex;

use crate::finding::Finding;

const REVIEW_BOX: &str = r"(?im)^\s*-\s*\[[xX]\]\s*AI-review complete";

/// Command-line options.
pub(crate) struct CommitOptions {
    base: String,
    pr_body_file: Option<String>,
}

impl CommitOptions {
    pub(crate) fn parse(arguments: &[&str]) -> Result<Self> {
        let mut options = Self {
            base: String::from("origin/main"),
            pr_body_file: None,
        };
        let mut remaining = arguments.iter();
        while let Some(argument) = remaining.next() {
            let value = remaining
                .next()
                .with_context(|| format!("{argument} needs a value"));
            match *argument {
                "--base" => options.base = String::from(*value?),
                "--pr-body-file" => options.pr_body_file = Some(String::from(*value?)),
                other => bail!("unknown option {other}"),
            }
        }
        Ok(options)
    }
}

struct Commit {
    sha: String,
    author_name: String,
    author_email: String,
    message: String,
    files: Vec<String>,
}

pub(crate) fn run(root: &Path, options: &CommitOptions) -> Result<Vec<Finding>> {
    let range = format!("{}..HEAD", options.base);
    let shas = git(root, &["rev-list", "--no-merges", "--reverse", &range])?;
    let pr_body = pr_body(options)?;
    let mut findings = Vec::new();
    let mut count = 0_usize;
    for sha in shas.lines().filter(|line| !line.is_empty()) {
        let commit = load_commit(root, sha)?;
        findings.extend(check_commit(&commit, pr_body.as_deref())?);
        count += 1;
    }
    if count == 0 {
        println!("provenance commits: no commits in {range}");
    }
    Ok(findings)
}

fn git(root: &Path, arguments: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .current_dir(root)
        .args(arguments)
        .output()
        .context("running git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            arguments.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    String::from_utf8(output.stdout).context("git output is not UTF-8")
}

fn load_commit(root: &Path, sha: &str) -> Result<Commit> {
    let header = git(root, &["log", "-1", "--format=%an%x00%ae%x00%B", sha])?;
    let mut parts = header.splitn(3, '\0');
    let author_name = String::from(parts.next().unwrap_or_default());
    let author_email = String::from(parts.next().unwrap_or_default());
    let message = String::from(parts.next().unwrap_or_default());
    let files = git(
        root,
        &["diff-tree", "--no-commit-id", "--name-only", "-r", sha],
    )?
    .lines()
    .map(String::from)
    .collect::<Vec<_>>();
    Ok(Commit {
        sha: String::from(sha),
        author_name,
        author_email,
        message,
        files,
    })
}

fn pr_body(options: &CommitOptions) -> Result<Option<String>> {
    if let Some(path) = &options.pr_body_file {
        return fs::read_to_string(path)
            .map(Some)
            .with_context(|| format!("reading {path}"));
    }
    let Ok(event_path) = env::var("GITHUB_EVENT_PATH") else {
        return Ok(None);
    };
    let event = fs::read_to_string(&event_path).with_context(|| format!("reading {event_path}"))?;
    let payload =
        serde_json::from_str::<serde_json::Value>(&event).context("parsing the GitHub event")?;
    Ok(payload
        .pointer("/pull_request/body")
        .and_then(serde_json::Value::as_str)
        .map(String::from))
}

fn trailers<'message>(message: &'message str, key: &str) -> Vec<&'message str> {
    message
        .lines()
        .filter_map(|line| line.strip_prefix(key))
        .filter_map(|rest| rest.strip_prefix(':'))
        .map(str::trim)
        .collect::<Vec<_>>()
}

fn is_bot_only_change(commit: &Commit) -> bool {
    let is_bot = commit.author_name.ends_with("[bot]") || commit.author_email.contains("[bot]@");
    let is_routine = |file: &String| {
        let name = file.rsplit('/').next().unwrap_or_default();
        matches!(
            name,
            "Cargo.toml" | "Cargo.lock" | "CHANGELOG.md" | "pyproject.toml" | "uv.lock"
        ) || file.starts_with(".github/workflows/")
    };
    is_bot && commit.files.iter().all(is_routine)
}

fn check_commit(commit: &Commit, pr_body: Option<&str>) -> Result<Vec<Finding>> {
    let Commit {
        sha,
        author_name,
        author_email,
        message,
        files,
    } = commit;
    let short = &sha[..sha.len().min(10)];
    let subject = message.lines().next().unwrap_or_default();
    let label = format!("commit {short} ({subject})");
    let mut findings = Vec::new();

    let expected = format!("{author_name} <{author_email}>");
    let signed_off = trailers(message, "Signed-off-by").contains(&expected.as_str());
    if !signed_off && !is_bot_only_change(commit) {
        findings.push(Finding::new(
            &label,
            0,
            format!("missing `Signed-off-by: {expected}` (commit with `git commit -s`; PROV-06)"),
        ));
    }

    let assisted = !trailers(message, "Assisted-by").is_empty();
    let touches_chip_crate = files
        .iter()
        .any(|file| file.starts_with("crates/sfes-snes-"));
    if assisted && touches_chip_crate {
        let review_box = Regex::new(REVIEW_BOX)?;
        match pr_body {
            Some(body) if review_box.is_match(body) => {}
            Some(_) => findings.push(Finding::new(
                &label,
                0,
                "AI-assisted chip-crate change: tick \"AI-review complete\" in the PR description (PROV-07)",
            )),
            None => println!("note: {label} is an AI-assisted chip-crate change; its PR needs the AI-review box ticked"),
        }
    }
    Ok(findings)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commit(author: &str, message: &str, files: &[&str]) -> Commit {
        Commit {
            sha: String::from("0123456789abcdef"),
            author_name: String::from(author),
            author_email: String::from("someone@example.org"),
            message: String::from(message),
            files: files
                .iter()
                .map(|file| String::from(*file))
                .collect::<Vec<_>>(),
        }
    }

    const SIGNED: &str = "feat(cpu): add\n\nSigned-off-by: Ada <someone@example.org>\n";
    const ASSISTED: &str =
        "feat(cpu): add\n\nAssisted-by: Tool\nSigned-off-by: Ada <someone@example.org>\n";

    #[test]
    fn signed_commit_passes() {
        let findings = check_commit(
            &commit("Ada", SIGNED, &["crates/sfes-snes-cpu/src/lib.rs"]),
            None,
        )
        .unwrap();
        assert_eq!(findings, Vec::<Finding>::new());
    }

    #[test]
    fn missing_or_mismatched_sign_off_fails() {
        let unsigned = check_commit(&commit("Ada", "fix: x\n", &["README.md"]), None).unwrap();
        assert_eq!(unsigned.len(), 1);
        let other_author = check_commit(&commit("Bob", SIGNED, &["README.md"]), None).unwrap();
        assert_eq!(other_author.len(), 1);
    }

    #[test]
    fn bots_are_exempt_for_routine_files_only() {
        let routine = commit(
            "dependabot[bot]",
            "chore(deps): bump\n",
            &["Cargo.lock", "crates/sfes-api/Cargo.toml"],
        );
        assert_eq!(check_commit(&routine, None).unwrap(), Vec::<Finding>::new());
        let code = commit(
            "dependabot[bot]",
            "chore: x\n",
            &["crates/sfes-api/src/lib.rs"],
        );
        assert_eq!(check_commit(&code, None).unwrap().len(), 1);
    }

    #[test]
    fn assisted_chip_change_needs_review_box() {
        let change = commit("Ada", ASSISTED, &["crates/sfes-snes-ppu/src/lib.rs"]);
        let unticked = "- [ ] AI-review complete: …";
        let ticked = "- [x] AI-review complete: every function was read against its source.";
        assert_eq!(check_commit(&change, Some(unticked)).unwrap().len(), 1);
        assert_eq!(
            check_commit(&change, Some(ticked)).unwrap(),
            Vec::<Finding>::new()
        );
        let elsewhere = commit("Ada", ASSISTED, &["crates/sfes-api/src/lib.rs"]);
        assert_eq!(
            check_commit(&elsewhere, Some(unticked)).unwrap(),
            Vec::<Finding>::new()
        );
    }

    #[test]
    fn options_parse() {
        let options =
            CommitOptions::parse(&["--base", "abc", "--pr-body-file", "body.md"]).unwrap();
        assert_eq!(options.base, "abc");
        assert_eq!(options.pr_body_file.as_deref(), Some("body.md"));
        assert!(CommitOptions::parse(&["--base"]).is_err());
        assert!(CommitOptions::parse(&["--bogus", "x"]).is_err());
    }
}
