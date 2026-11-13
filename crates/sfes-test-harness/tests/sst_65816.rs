//! Runs the `SingleStepTests` 65816 vectors for every implemented opcode (EMU-02).
//!
//! See `sfes_test_harness::vectors` for the environment variables that say where the vectors
//! are, whether they are required and how many tests of each file to run.

use sfes_test_harness::{
    sst65816::{IMPLEMENTED, run_set},
    vectors::Config,
};

// @source: sst-65816
#[test]
fn implemented_opcodes_match_the_vectors() {
    let config = Config::from_env().unwrap_or_else(|error| panic!("{error}"));
    let report = run_set(&config, IMPLEMENTED).unwrap_or_else(|error| panic!("{error}"));
    println!("{}", report.summary());
    assert!(report.passed(), "{}", report.summary());
}
