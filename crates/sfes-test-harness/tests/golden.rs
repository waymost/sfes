//! Runs every golden file in `tests/assets/golden/` against the machine (QA-01 layer 3, ADR 0015).

use std::path::Path;

use sfes_test_harness::{
    golden::{self, DIRECTORY},
    roms::Rom,
};

// @source: adr-0015
#[test]
fn every_golden_reaches_its_frame_hashes() {
    let mut failures = Vec::new();
    let mut ran = 0_usize;
    for path in golden::files(Path::new(DIRECTORY)).unwrap() {
        let golden = golden::load(&path).unwrap();
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let bytes = match golden::rom(&golden).unwrap() {
            Rom::Present(bytes) => bytes,
            Rom::Skipped(message) => {
                eprintln!("{name}: {message}");
                continue;
            }
        };
        let mut machine = golden::boot(&bytes).unwrap();
        let observed = golden::observe(&golden, machine.as_mut());
        for mismatch in golden::mismatches(&golden, &observed) {
            failures.push(format!("{name}: {mismatch}"));
        }
        ran += 1;
    }
    eprintln!("{ran} golden file(s) run");
    assert!(
        failures.is_empty(),
        "golden mismatches:\n{}",
        failures.join("\n")
    );
}
