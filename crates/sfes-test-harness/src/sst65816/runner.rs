//! Runs tests against the core and finds the first difference.

use std::{
    fs, io,
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

use sfes_snes_cpu::{Cpu65816, RunState};

use crate::sst65816::{
    bus::RecordingBus,
    model::{BusCycle, Cycle, State, Test},
};

/// The opcodes of MVP and MVN (datasheet Table 5-5), whose tests take one step per byte.
const BLOCK_MOVES: [u8; 2] = [0x44, 0x54];

/// The first difference between what a test expects and what the core did.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum Mismatch {
    /// A bus cycle differs, or one side has more cycles than the other.
    #[error("cycle {number}: expected {}, got {}", cycle_text(*.expected), cycle_text(*.actual))]
    Cycle {
        /// The cycle's position, counted from 1.
        number: usize,
        /// What the vector records; `None` past its last cycle.
        expected: Option<BusCycle>,
        /// What the core did; `None` past its last cycle.
        actual: Option<BusCycle>,
    },
    /// A register's final value differs.
    #[error("register {name}: expected {expected:#06X}, got {actual:#06X}")]
    Register {
        /// The register's name as the vectors spell it.
        name: &'static str,
        /// The vector's final value.
        expected: u16,
        /// The core's final value.
        actual: u16,
    },
    /// A memory byte's final value differs.
    #[error("RAM {address:#08X}: expected {expected:#04X}, got {}", byte_text(*.actual))]
    Ram {
        /// The byte's address.
        address: u32,
        /// The vector's final value.
        expected: u8,
        /// The bus's final value; `None` when nothing is stored there.
        actual: Option<u8>,
    },
    /// The processor is halted when the vector says it runs, or the reverse.
    #[error("run state: expected {}, got {actual:?}", if *.halted { "Waiting or Stopped" } else { "Running" })]
    RunState {
        /// Whether the vector ends with the halt mark.
        halted: bool,
        /// The core's final state.
        actual: RunState,
    },
}

/// A cycle for a message, or the words for its absence.
fn cycle_text(cycle: Option<BusCycle>) -> String {
    cycle.map_or_else(|| "no cycle".to_owned(), |cycle| cycle.to_string())
}

/// A memory byte for a message, or the words for its absence.
fn byte_text(byte: Option<u8>) -> String {
    byte.map_or_else(
        || "nothing stored".to_owned(),
        |byte| format!("{byte:#04X}"),
    )
}

/// A failing test and its first difference.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{file}: test \"{test}\": {mismatch}")]
pub struct Failure {
    /// The vector file's name.
    pub file: String,
    /// The test's name.
    pub test: String,
    /// The first difference.
    pub mismatch: Mismatch,
}

/// What running one file did.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileReport {
    /// The vector file's name.
    pub file: String,
    /// How many tests ran, the failing one included.
    pub tests: usize,
    /// The first failing test, if any. The tests after it are not run.
    pub failure: Option<Failure>,
}

/// A run that could not be carried out.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    /// The set is required and has not been fetched.
    #[error("no 65816 vectors in {}: run `cargo xtask vectors fetch`", directory.display())]
    Missing {
        /// Where the set was looked for.
        directory: PathBuf,
    },
    /// A vector file cannot be read.
    #[error("reading {}", path.display())]
    Read {
        /// The file.
        path: PathBuf,
        /// The cause.
        source: io::Error,
    },
    /// A vector file is not a list of tests.
    #[error("parsing {}", path.display())]
    Parse {
        /// The file.
        path: PathBuf,
        /// The cause.
        source: serde_json::Error,
    },
}

/// How many times the runner steps the core for one test of `opcode`: once, except for the
/// block moves, where one step moves one byte and the test moves the initial C plus one
/// (ADR 0004).
#[must_use]
pub fn steps(opcode: u8, initial: &State) -> u32 {
    if BLOCK_MOVES.contains(&opcode) {
        u32::from(initial.a) + 1
    } else {
        1
    }
}

/// Loads the test's initial state, steps the core `steps` times and compares the result.
///
/// # Errors
///
/// Returns the first [`Mismatch`], in the order of [`check`].
pub fn run_test(test: &Test, steps: u32) -> Result<(), Mismatch> {
    let mut processor = test.initial.processor();
    let mut bus = RecordingBus::new(&test.initial.ram);
    for _ in 0..steps {
        processor.step(&mut bus);
    }
    check(test, &processor, &bus)
}

/// Compares what the core did with what the test expects: every bus cycle (address, value
/// and all eight signals), then the registers, then memory, then whether the processor is
/// halted.
///
/// # Errors
///
/// Returns the first [`Mismatch`] in that order.
pub fn check(test: &Test, processor: &Cpu65816, bus: &RecordingBus) -> Result<(), Mismatch> {
    let expected_cycles = test
        .cycles
        .iter()
        .filter_map(|cycle| match cycle {
            Cycle::Bus(cycle) => Some(*cycle),
            Cycle::Halted => None,
        })
        .collect::<Vec<_>>();
    let actual_cycles = bus.cycles();
    for index in 0..expected_cycles.len().max(actual_cycles.len()) {
        let expected = expected_cycles.get(index).copied();
        let actual = actual_cycles.get(index).copied();
        if expected != actual {
            return Err(Mismatch::Cycle {
                number: index + 1,
                expected,
                actual,
            });
        }
    }

    let State {
        pc,
        s,
        p,
        a,
        x,
        y,
        dbr,
        d,
        pbr,
        e,
        ram,
    } = &test.final_state;
    let registers = [
        ("pc", *pc, processor.pc),
        ("s", *s, processor.s),
        ("p", u16::from(*p), u16::from(processor.p.bits())),
        ("a", *a, processor.a),
        ("x", *x, processor.x),
        ("y", *y, processor.y),
        ("dbr", u16::from(*dbr), u16::from(processor.dbr)),
        ("d", *d, processor.d),
        ("pbr", u16::from(*pbr), u16::from(processor.pbr)),
        ("e", u16::from(*e), u16::from(processor.e)),
    ];
    for (name, expected, actual) in registers {
        if expected != actual {
            return Err(Mismatch::Register {
                name,
                expected,
                actual,
            });
        }
    }

    for &(address, expected) in ram {
        let actual = bus.byte(address);
        if actual != Some(expected) {
            return Err(Mismatch::Ram {
                address,
                expected,
                actual,
            });
        }
    }

    let halted = test.cycles.contains(&Cycle::Halted);
    if halted == (processor.run == RunState::Running) {
        return Err(Mismatch::RunState {
            halted,
            actual: processor.run,
        });
    }
    Ok(())
}

/// Runs the tests of one file of `opcode`, or its first `sample` tests, stopping at the
/// first failure.
///
/// # Errors
///
/// Returns a [`RunError`] when the file cannot be read or is not a list of tests.
pub fn run_file(
    path: &Path,
    opcode: u8,
    sample: Option<NonZeroUsize>,
) -> Result<FileReport, RunError> {
    let text = fs::read_to_string(path).map_err(|source| RunError::Read {
        path: path.to_owned(),
        source,
    })?;
    let tests = serde_json::from_str::<Vec<Test>>(&text).map_err(|source| RunError::Parse {
        path: path.to_owned(),
        source,
    })?;
    let file = path
        .file_name()
        .map_or_default(|name| name.to_string_lossy().into_owned());
    Ok(run_tests(file, &tests, sample, |test| {
        run_test(test, steps(opcode, &test.initial))
    }))
}

/// Runs `tests`, or the first `sample` of them, through `run`, stopping at the first failure.
fn run_tests(
    file: String,
    tests: &[Test],
    sample: Option<NonZeroUsize>,
    run: impl Fn(&Test) -> Result<(), Mismatch>,
) -> FileReport {
    let limit = sample.map_or(tests.len(), usize::from);
    let mut report = FileReport {
        file,
        tests: 0,
        failure: None,
    };
    for test in tests.iter().take(limit) {
        report.tests += 1;
        if let Err(mismatch) = run(test) {
            report.failure = Some(Failure {
                file: report.file.clone(),
                test: test.name.clone(),
                mismatch,
            });
            break;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use sfes_api::Addr24;
    use sfes_snes_cpu::{Bus65816, Pins};

    use super::*;

    /// The project-made fixture: three tests in the vectors' format.
    const FIXTURE: &str = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/assets/fixtures/sst-65816-format.json"
    );

    /// A change to one register.
    type Change = fn(&mut Cpu65816);

    /// The fixture's tests.
    // @source: sst-65816
    fn fixture() -> Vec<Test> {
        serde_json::from_str::<Vec<Test>>(&fs::read_to_string(FIXTURE).unwrap()).unwrap()
    }

    /// Plays a test's cycles onto a bus as a correct core would, and returns the processor
    /// in the test's final state with that bus.
    // @source: adr-0004
    fn replay(test: &Test) -> (Cpu65816, RecordingBus) {
        let mut bus = RecordingBus::new(&test.initial.ram);
        let mut processor = test.final_state.processor();
        for cycle in &test.cycles {
            match *cycle {
                Cycle::Halted => processor.run = RunState::Stopped,
                Cycle::Bus(BusCycle {
                    address,
                    value,
                    pins,
                }) => {
                    let address = Addr24::new(address);
                    match value {
                        Some(value) if pins.contains(Pins::RWB) => {
                            assert_eq!(bus.read(address, pins), value);
                        }
                        Some(value) => bus.write(address, value, pins),
                        None => bus.idle(address, pins),
                    }
                }
            }
        }
        (processor, bus)
    }

    // @source: sst-65816
    #[test]
    fn the_fixture_parses_into_three_tests() {
        let tests = fixture();
        let names = tests
            .iter()
            .map(|test| test.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, ["fixture 1", "fixture 2", "fixture 3"]);
        assert_eq!(tests[0].initial.ram.len(), 3);
        assert_eq!(tests[0].final_state.pc, 4098);
        assert_eq!(tests[0].cycles.len(), 5);
        assert_eq!(tests[1].cycles.last(), Some(&Cycle::Halted));
    }

    // @source: adr-0004
    #[test]
    fn a_core_that_does_what_the_test_records_passes() {
        for test in fixture() {
            let (processor, bus) = replay(&test);
            assert_eq!(check(&test, &processor, &bus), Ok(()), "{}", test.name);
        }
    }

    // @source: adr-0004
    #[test]
    fn a_different_signal_is_reported_as_the_first_differing_cycle() {
        let test = &fixture()[0];
        let (processor, _) = replay(test);
        let mut bus = RecordingBus::new(&test.initial.ram);
        bus.read(Addr24::new(331_776), Pins::VDA | Pins::VPA | Pins::RWB);
        bus.read(Addr24::new(331_777), Pins::VDA | Pins::RWB);
        let mismatch = check(test, &processor, &bus).unwrap_err();
        assert_eq!(
            mismatch,
            Mismatch::Cycle {
                number: 2,
                expected: Some(BusCycle {
                    address: 331_777,
                    value: Some(34),
                    pins: Pins::VPA | Pins::RWB,
                }),
                actual: Some(BusCycle {
                    address: 331_777,
                    value: Some(34),
                    pins: Pins::VDA | Pins::RWB,
                }),
            }
        );
        assert_eq!(
            mismatch.to_string(),
            r#"cycle 2: expected [0x051001, 0x22, "-p-r----"], got [0x051001, 0x22, "d--r----"]"#
        );
    }

    // @source: adr-0004
    #[test]
    fn a_read_where_the_test_idles_is_a_cycle_mismatch() {
        let test = &fixture()[0];
        let (processor, _) = replay(test);
        let mut bus = RecordingBus::new(&test.initial.ram);
        bus.read(Addr24::new(331_776), Pins::VDA | Pins::VPA | Pins::RWB);
        bus.read(Addr24::new(331_777), Pins::VPA | Pins::RWB);
        bus.read(Addr24::new(331_778), Pins::RWB);
        assert!(matches!(
            check(test, &processor, &bus),
            Err(Mismatch::Cycle {
                number: 3,
                expected: Some(BusCycle { value: None, .. }),
                actual: Some(BusCycle { value: Some(0), .. }),
            })
        ));
    }

    // @source: adr-0004
    #[test]
    fn missing_and_extra_cycles_are_cycle_mismatches() {
        let test = &fixture()[2];
        let (processor, mut bus) = replay(test);
        let none = RecordingBus::new(&test.initial.ram);
        assert!(matches!(
            check(test, &processor, &none),
            Err(Mismatch::Cycle {
                number: 1,
                expected: Some(_),
                actual: None,
            })
        ));
        bus.idle(Addr24::new(1), Pins::RWB);
        let mismatch = check(test, &processor, &bus).unwrap_err();
        assert!(matches!(
            mismatch,
            Mismatch::Cycle {
                number: 2,
                expected: None,
                actual: Some(_),
            }
        ));
        assert_eq!(
            mismatch.to_string(),
            r#"cycle 2: expected no cycle, got [0x000001, null, "---r----"]"#
        );
    }

    // @source: adr-0004
    #[test]
    fn each_register_is_compared() {
        let test = &fixture()[0];
        let (processor, bus) = replay(test);
        let changes: [(&str, Change); 10] = [
            ("pc", |processor| processor.pc ^= 1),
            ("s", |processor| processor.s ^= 1),
            ("p", |processor| processor.p.toggle(sfes_snes_cpu::Flags::C)),
            ("a", |processor| processor.a ^= 1),
            ("x", |processor| processor.x ^= 1),
            ("y", |processor| processor.y ^= 1),
            ("dbr", |processor| processor.dbr ^= 1),
            ("d", |processor| processor.d ^= 1),
            ("pbr", |processor| processor.pbr ^= 1),
            ("e", |processor| processor.e = !processor.e),
        ];
        for (register, change) in changes {
            let mut processor = processor.clone();
            change(&mut processor);
            let mismatch = check(test, &processor, &bus).unwrap_err();
            assert!(
                matches!(mismatch, Mismatch::Register { name, .. } if name == register),
                "{register}: {mismatch}"
            );
        }
        let mut wrong = processor.clone();
        wrong.pc = 0x1234;
        assert_eq!(
            check(test, &wrong, &bus).unwrap_err().to_string(),
            "register pc: expected 0x1002, got 0x1234"
        );
    }

    // @source: adr-0004
    #[test]
    fn final_memory_is_compared() {
        let mut test = fixture().swap_remove(0);
        let (processor, bus) = replay(&test);
        test.final_state.ram[2].1 = 69;
        assert_eq!(
            check(&test, &processor, &bus),
            Err(Mismatch::Ram {
                address: 196_864,
                expected: 69,
                actual: Some(68),
            })
        );
        test.final_state.ram[2] = (7, 1);
        let mismatch = check(&test, &processor, &bus).unwrap_err();
        assert_eq!(
            mismatch.to_string(),
            "RAM 0x000007: expected 0x01, got nothing stored"
        );
    }

    // @source: adr-0004
    #[test]
    fn the_halt_mark_requires_a_halted_processor_and_its_absence_a_running_one() {
        let tests = fixture();
        let (mut halted, bus) = replay(&tests[1]);
        halted.run = RunState::Waiting;
        assert_eq!(check(&tests[1], &halted, &bus), Ok(()));
        halted.run = RunState::Running;
        assert_eq!(
            check(&tests[1], &halted, &bus),
            Err(Mismatch::RunState {
                halted: true,
                actual: RunState::Running,
            })
        );
        let (mut running, bus) = replay(&tests[2]);
        running.run = RunState::Stopped;
        let mismatch = check(&tests[2], &running, &bus).unwrap_err();
        assert_eq!(
            mismatch.to_string(),
            "run state: expected Running, got Stopped"
        );
    }

    // @source: adr-0004
    #[test]
    fn a_block_move_takes_one_step_per_byte_and_everything_else_one() {
        let mut initial = fixture().swap_remove(0).initial;
        initial.a = 2;
        assert_eq!(steps(0x44, &initial), 3);
        assert_eq!(steps(0x54, &initial), 3);
        assert_eq!(steps(0xA9, &initial), 1);
        initial.a = 0xFFFF;
        assert_eq!(steps(0x54, &initial), 0x1_0000);
    }

    // @source: sst-65816
    #[test]
    fn a_file_runs_all_its_tests_or_the_sample() {
        let tests = fixture();
        let pass = |_: &Test| Ok(());
        let all = run_tests("fixture.json".to_owned(), &tests, None, pass);
        assert_eq!((all.tests, all.failure), (3, None));
        let sample = run_tests(
            "fixture.json".to_owned(),
            &tests,
            NonZeroUsize::new(2),
            pass,
        );
        assert_eq!((sample.tests, sample.failure), (2, None));
        let large = run_tests(
            "fixture.json".to_owned(),
            &tests,
            NonZeroUsize::new(50),
            pass,
        );
        assert_eq!(large.tests, 3);
    }

    // @source: sst-65816
    #[test]
    fn a_file_stops_at_its_first_failing_test_and_names_it() {
        let tests = fixture();
        let mismatch = Mismatch::RunState {
            halted: false,
            actual: RunState::Stopped,
        };
        let report = run_tests("fixture.json".to_owned(), &tests, None, |test| {
            if test.name == "fixture 2" {
                Err(mismatch.clone())
            } else {
                Ok(())
            }
        });
        assert_eq!(report.tests, 2);
        assert_eq!(
            report.failure.unwrap().to_string(),
            "fixture.json: test \"fixture 2\": run state: expected Running, got Stopped"
        );
    }

    // @source: sst-65816
    #[test]
    fn an_unreadable_or_malformed_file_is_an_error() {
        let missing = Path::new(FIXTURE).with_file_name("absent.json");
        assert!(matches!(
            run_file(&missing, 0xA9, None),
            Err(RunError::Read { .. })
        ));
        let manifest = Path::new(FIXTURE).with_file_name("../MANIFEST.toml");
        assert!(matches!(
            run_file(&manifest, 0xA9, None),
            Err(RunError::Parse { .. })
        ));
    }
}
