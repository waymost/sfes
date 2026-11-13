//! Project automation for sfes, run as `cargo xtask <command>`.
//!
//! Every check prints one line per finding and exits non-zero when there is any, so CI and
//! local runs behave the same.

mod deps;
mod files;
mod finding;
mod provenance;
mod vectors;

use std::{env, path::PathBuf, process::ExitCode};

use anyhow::{Context, Result};

use crate::{finding::Finding, provenance::commits::CommitOptions};

const USAGE: &str = "\
usage: cargo xtask <command>

commands:
  provenance lint      every chip-crate module and function has a known @source: tag (PROV-03)
  provenance scan      no emulator copyright headers or author names in code (PROV-05)
  provenance assets    every test asset has a licensed manifest entry (PROV-09)
  provenance commits   [--base <rev>] [--pr-body-file <path>]
                       sign-off on every commit; AI-assisted chip changes reviewed (PROV-06, PROV-07)
  vectors fetch [set]  download test-vector sets at their pinned commits; verify against the lock
  vectors lock [set]   download the sets and rewrite tests/assets/vectors.lock
  deps                 workspace dependency layering and player-only licenses (MS-02, PROV-08)";

fn main() -> ExitCode {
    match run() {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run() -> Result<bool> {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let words = arguments.iter().map(String::as_str).collect::<Vec<_>>();
    let root = workspace_root()?;
    let (name, findings) = match words.as_slice() {
        ["provenance", "lint"] => ("provenance lint", provenance::lint::run(&root)?),
        ["provenance", "scan"] => ("provenance scan", provenance::scan::run(&root)?),
        ["provenance", "assets"] => ("provenance assets", provenance::assets::run(&root)?),
        ["provenance", "commits", rest @ ..] => {
            let options = CommitOptions::parse(rest)?;
            (
                "provenance commits",
                provenance::commits::run(&root, &options)?,
            )
        }
        ["vectors", "fetch"] => ("vectors fetch", vectors::fetch(&root, None)?),
        ["vectors", "fetch", set] => ("vectors fetch", vectors::fetch(&root, Some(set))?),
        ["vectors", "lock"] => ("vectors lock", vectors::lock(&root, None)?),
        ["vectors", "lock", set] => ("vectors lock", vectors::lock(&root, Some(set))?),
        ["deps"] => ("deps", deps::run(&root)?),
        _ => {
            eprintln!("{USAGE}");
            return Ok(false);
        }
    };
    Ok(report(name, &findings))
}

fn report(name: &str, findings: &[Finding]) -> bool {
    for finding in findings {
        eprintln!("{finding}");
    }
    if findings.is_empty() {
        println!("{name}: ok");
        true
    } else {
        eprintln!("{name}: {} finding(s)", findings.len());
        false
    }
}

fn workspace_root() -> Result<PathBuf> {
    let manifest_directory = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_directory
        .parent()
        .map(PathBuf::from)
        .context("xtask has no parent directory")
}
