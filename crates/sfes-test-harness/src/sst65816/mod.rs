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
pub const IMPLEMENTED: &[u8] = &[
    // ASL, LSR, ROL, ROR, INC, DEC, INX, INY, DEX and DEY (#37).
    0x06, 0x0A, 0x0E, 0x16, 0x1A, 0x1E, 0x26, 0x2A, 0x2E, 0x36, 0x3A, 0x3E, 0x46, 0x4A, 0x4E, 0x56,
    0x5E, 0x66, 0x6A, 0x6E, 0x76, 0x7E, 0x88, 0xC6, 0xC8, 0xCA, 0xCE, 0xD6, 0xDE, 0xE6, 0xE8, 0xEE,
    0xF6, 0xFE, // ADC, SBC, CMP, CPX and CPY in every addressing mode (#36).
    0x61, 0x63, 0x65, 0x67, 0x69, 0x6D, 0x6F, 0x71, 0x72, 0x73, 0x75, 0x77, 0x79, 0x7D, 0x7F, 0xC0,
    0xC1, 0xC3, 0xC4, 0xC5, 0xC7, 0xC9, 0xCC, 0xCD, 0xCF, 0xD1, 0xD2, 0xD3, 0xD5, 0xD7, 0xD9, 0xDD,
    0xDF, 0xE0, 0xE1, 0xE3, 0xE4, 0xE5, 0xE7, 0xE9, 0xEC, 0xED, 0xEF, 0xF1, 0xF2, 0xF3, 0xF5, 0xF7,
    0xF9, 0xFD, 0xFF, // ORA, AND, EOR, BIT, TSB and TRB in every addressing mode (#35).
    0x01, 0x03, 0x04, 0x05, 0x07, 0x09, 0x0C, 0x0D, 0x0F, 0x11, 0x12, 0x13, 0x14, 0x15, 0x17, 0x19,
    0x1C, 0x1D, 0x1F, 0x21, 0x23, 0x24, 0x25, 0x27, 0x29, 0x2C, 0x2D, 0x2F, 0x31, 0x32, 0x33, 0x34,
    0x35, 0x37, 0x39, 0x3C, 0x3D, 0x3F, 0x41, 0x43, 0x45, 0x47, 0x49, 0x4D, 0x4F, 0x51, 0x52, 0x53,
    0x55, 0x57, 0x59, 0x5D, 0x5F, 0x89,
    // STA, STX, STY and STZ in every addressing mode (#34).
    0x64, 0x74, 0x81, 0x83, 0x84, 0x85, 0x86, 0x87, 0x8C, 0x8D, 0x8E, 0x8F, 0x91, 0x92, 0x93, 0x94,
    0x95, 0x96, 0x97, 0x99, 0x9C, 0x9D, 0x9E, 0x9F,
    // LDA, LDX and LDY in every addressing mode (#33).
    0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xA6, 0xA7, 0xA9, 0xAC, 0xAD, 0xAE, 0xAF, 0xB1, 0xB2, 0xB3,
    0xB4, 0xB5, 0xB6, 0xB7, 0xB9, 0xBC, 0xBD, 0xBE, 0xBF,
];

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
