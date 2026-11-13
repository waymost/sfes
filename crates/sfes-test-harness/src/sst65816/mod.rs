//! The `SingleStepTests` 65816 vectors, run against [`sfes_snes_cpu::Cpu65816`].
//!
//! Each file `v1/XX.n.json` or `v1/XX.e.json` holds the tests of opcode `XX` in native or
//! emulation mode. A test gives the registers and memory before one instruction, the same
//! after it, and every bus cycle in between. The runner loads the state, steps the core on a
//! [`RecordingBus`] and compares all three (ADR 0004).
//!
//! The vectors are evidence, not a specification: a disagreement between a vector and the
//! datasheet is settled in an ADR, not here.

mod bus;
mod model;
mod runner;

use std::{
    panic,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
    thread,
};

pub use crate::sst65816::{
    bus::RecordingBus,
    model::{BusCycle, Cycle, CycleError, State, Test, signal_string},
    runner::{Failure, FileReport, Mismatch, RunError, check, run_file, run_test, steps},
};
use crate::vectors::Config;

/// The opcodes the core implements so far. Only their files are run. Each instruction-family
/// issue adds its opcodes; the list is deleted with the bring-up scaffolding (#43), after
/// which every file runs.
pub const IMPLEMENTED: &[u8] = &[];

/// The set's files, relative to the vector directory.
const SET_DIRECTORY: &str = "65816/v1";

/// What a run over the set did.
#[derive(Debug)]
pub enum SetReport {
    /// The set has not been fetched, and the run did not require it.
    Skipped {
        /// Where the set was looked for.
        directory: PathBuf,
    },
    /// The files of the listed opcodes were run.
    Ran {
        /// How many opcodes were listed.
        opcodes: usize,
        /// How many tests ran.
        tests: usize,
        /// The first failing test of each file that has one, in file order.
        failures: Vec<Failure>,
    },
}

impl SetReport {
    /// Whether no test failed. A skipped run passes.
    #[must_use]
    pub fn passed(&self) -> bool {
        match self {
            Self::Skipped { .. } => true,
            Self::Ran { failures, .. } => failures.is_empty(),
        }
    }

    /// The report as lines of text for a test log.
    #[must_use]
    pub fn summary(&self) -> String {
        match self {
            Self::Skipped { directory } => format!(
                "SKIPPED: no 65816 vectors in {}. Run `cargo xtask vectors fetch`, or set {} to \
                 make this an error.",
                directory.display(),
                crate::vectors::REQUIRE_VARIABLE,
            ),
            Self::Ran {
                opcodes: 0,
                tests: _,
                failures: _,
            } => "0 of 256 opcodes are implemented: no 65816 vector file was run.".to_owned(),
            Self::Ran {
                opcodes,
                tests,
                failures,
            } => {
                let mut summary = format!(
                    "{opcodes} of 256 opcodes, {tests} tests, {} failing files.",
                    failures.len()
                );
                for failure in failures {
                    summary.push('\n');
                    summary.push_str(&failure.to_string());
                }
                summary
            }
        }
    }
}

/// Runs the native-mode and emulation-mode files of each listed opcode, spread over the
/// machine's cores. Each file stops at its first failing test.
///
/// # Errors
///
/// Returns a [`RunError`] when the set is missing and `config.required` is set, or when a
/// file of a listed opcode cannot be read or parsed.
pub fn run_set(config: &Config, opcodes: &[u8]) -> Result<SetReport, RunError> {
    let directory = config.directory.join(SET_DIRECTORY);
    if !directory.is_dir() {
        return if config.required {
            Err(RunError::Missing { directory })
        } else {
            Ok(SetReport::Skipped { directory })
        };
    }
    let files = opcodes
        .iter()
        .flat_map(|&opcode| {
            ["e", "n"].map(|mode| (opcode, directory.join(format!("{opcode:02x}.{mode}.json"))))
        })
        .collect::<Vec<_>>();
    let next = AtomicUsize::new(0);
    let workers = thread::available_parallelism().map_or(1, usize::from);
    let work = || {
        let mut reports = Vec::new();
        while let Some((opcode, path)) = files.get(next.fetch_add(1, Ordering::Relaxed)) {
            reports.push(run_file(path, *opcode, config.sample));
        }
        reports
    };
    let mut reports = thread::scope(|scope| {
        let handles = (0..workers.min(files.len()))
            .map(|_| scope.spawn(work))
            .collect::<Vec<_>>();
        handles
            .into_iter()
            .flat_map(|handle| match handle.join() {
                Ok(reports) => reports,
                Err(payload) => panic::resume_unwind(payload),
            })
            .collect::<Result<Vec<_>, _>>()
    })?;
    reports.sort_by(|left, right| left.file.cmp(&right.file));
    let tests = reports.iter().map(|report| report.tests).sum::<usize>();
    let failures = reports
        .into_iter()
        .filter_map(|report| report.failure)
        .collect::<Vec<_>>();
    Ok(SetReport::Ran {
        opcodes: opcodes.len(),
        tests,
        failures,
    })
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process};

    use super::*;

    /// A fresh temporary vector directory that holds an empty 65816 set.
    // @source: sst-65816
    fn empty_set(name: &str) -> PathBuf {
        let directory = env::temp_dir().join(format!("sfes-harness-{}-{name}", process::id()));
        fs::create_dir_all(directory.join(SET_DIRECTORY)).unwrap();
        directory
    }

    /// The settings for a run over `directory`.
    // @source: sst-65816
    fn config(directory: PathBuf, required: bool) -> Config {
        Config {
            directory,
            required,
            sample: None,
        }
    }

    // @source: sst-65816
    #[test]
    fn a_missing_set_is_skipped_with_a_message() {
        let directory = env::temp_dir().join("sfes-harness-no-such-directory");
        let report = run_set(&config(directory, false), &[0xA9]).unwrap();
        assert!(matches!(report, SetReport::Skipped { .. }));
        assert!(report.passed());
        let summary = report.summary();
        assert!(
            summary.starts_with("SKIPPED: no 65816 vectors in "),
            "{summary}"
        );
        assert!(summary.contains("cargo xtask vectors fetch"), "{summary}");
    }

    // @source: sst-65816
    #[test]
    fn a_missing_set_is_an_error_when_required() {
        let directory = env::temp_dir().join("sfes-harness-no-such-directory");
        let error = run_set(&config(directory, true), &[]).unwrap_err();
        assert!(matches!(error, RunError::Missing { .. }), "{error}");
    }

    // @source: sst-65816
    #[test]
    fn an_empty_opcode_list_runs_nothing_and_says_so() {
        let directory = empty_set("empty-list");
        let report = run_set(&config(directory.clone(), true), &[]).unwrap();
        fs::remove_dir_all(directory).unwrap();
        assert!(report.passed());
        assert_eq!(
            report.summary(),
            "0 of 256 opcodes are implemented: no 65816 vector file was run."
        );
    }

    // @source: sst-65816
    #[test]
    fn a_listed_opcode_with_no_file_is_an_error() {
        let directory = empty_set("no-file");
        let result = run_set(&config(directory.clone(), false), &[0xA9]);
        fs::remove_dir_all(directory).unwrap();
        let Err(RunError::Read { path, .. }) = result else {
            panic!("expected a read error, got {result:?}");
        };
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name == "a9.e.json" || name == "a9.n.json", "{name}");
    }

    // @source: sst-65816
    #[test]
    fn failures_are_listed_under_the_counts() {
        let failure = Failure {
            file: "a9.n.json".to_owned(),
            test: "a9 n 1".to_owned(),
            mismatch: Mismatch::Register {
                name: "a",
                expected: 1,
                actual: 2,
            },
        };
        let report = SetReport::Ran {
            opcodes: 1,
            tests: 10_001,
            failures: vec![failure],
        };
        assert!(!report.passed());
        assert_eq!(
            report.summary(),
            "1 of 256 opcodes, 10001 tests, 1 failing files.\n\
             a9.n.json: test \"a9 n 1\": register a: expected 0x0001, got 0x0002"
        );
    }

    // @source: sst-65816
    #[test]
    fn the_implemented_list_has_no_duplicates() {
        let mut opcodes = IMPLEMENTED.to_vec();
        opcodes.sort_unstable();
        opcodes.dedup();
        assert_eq!(opcodes.len(), IMPLEMENTED.len());
    }
}
