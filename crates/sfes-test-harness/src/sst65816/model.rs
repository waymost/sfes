//! The JSON form of a `SingleStepTests` 65816 test, as the set's README defines it.

use std::fmt::{self, Display, Formatter};

use sfes_snes_cpu::{Cpu65816, Flags, Pins, RunState};

/// The signal letters of a cycle's third field, in order, with the flag each one stands for.
/// A signal that is not active is written `-`, except RWB, which is `w`.
const SIGNALS: [(char, Pins); 8] = [
    ('d', Pins::VDA),
    ('p', Pins::VPA),
    ('v', Pins::VPB),
    ('r', Pins::RWB),
    ('e', Pins::E),
    ('m', Pins::M),
    ('x', Pins::X),
    ('l', Pins::MLB),
];

/// One test: the state before an instruction, the state after it and its bus cycles.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Test {
    /// A label for people, such as `a9 n 1`.
    pub name: String,
    /// The processor and memory before the instruction.
    pub initial: State,
    /// The processor and memory after the instruction.
    #[serde(rename = "final")]
    pub final_state: State,
    /// What the bus saw, cycle by cycle.
    pub cycles: Vec<Cycle>,
}

/// The registers and the memory bytes that matter to one test.
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct State {
    /// Program counter.
    pub pc: u16,
    /// Stack pointer.
    pub s: u16,
    /// Processor Status Register.
    pub p: u8,
    /// The 16-bit accumulator.
    pub a: u16,
    /// Index register X.
    pub x: u16,
    /// Index register Y.
    pub y: u16,
    /// Data Bank Register.
    pub dbr: u8,
    /// Direct register.
    pub d: u16,
    /// Program Bank Register.
    pub pbr: u8,
    /// The emulation flag, 0 or 1.
    pub e: u8,
    /// Memory contents as `[address, value]` pairs.
    pub ram: Vec<(u32, u8)>,
}

impl State {
    /// A running processor holding these register values. One value is adjusted: in
    /// emulation mode the high byte of S becomes `01`. The emulation-mode vectors start with
    /// stack pointers outside page 1, which the processor cannot hold, and expect the
    /// instruction to behave as if S were in page 1 (ADR 0005). Everything else is loaded
    /// unchanged.
    #[must_use]
    pub fn processor(&self) -> Cpu65816 {
        let emulation = self.e != 0;
        let s = if emulation {
            0x0100 | (self.s & 0x00FF)
        } else {
            self.s
        };
        Cpu65816 {
            a: self.a,
            x: self.x,
            y: self.y,
            s,
            d: self.d,
            dbr: self.dbr,
            pbr: self.pbr,
            pc: self.pc,
            p: Flags::from_bits_retain(self.p),
            e: emulation,
            run: RunState::Running,
        }
    }
}

/// One record of a test's `cycles` list.
#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize)]
#[serde(try_from = "(Option<u32>, Option<u8>, String)")]
pub enum Cycle {
    /// A clock cycle on the bus.
    Bus(BusCycle),
    /// A record with no address: not a bus cycle, but the mark that the processor has
    /// halted (ADR 0004). The WAI and STP tests end with one.
    Halted,
}

/// One clock cycle: what the bus trait is called with, and what the vectors record.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BusCycle {
    /// The 24-bit address.
    pub address: u32,
    /// The byte read or written; `None` for a cycle that transfers no data.
    pub value: Option<u8>,
    /// The output signals.
    pub pins: Pins,
}

impl Display for BusCycle {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        let Self {
            address,
            value,
            pins,
        } = self;
        write!(formatter, "[{address:#08X}, ")?;
        match value {
            Some(value) => write!(formatter, "{value:#04X}")?,
            None => formatter.write_str("null")?,
        }
        write!(formatter, ", \"{}\"]", signal_string(*pins))
    }
}

/// A cycle record that does not follow the README's format.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum CycleError {
    /// The signal string is not eight characters of the form `dpvremxl`.
    #[error("signal string {0:?} is not of the form \"dpvremxl\"")]
    Signals(String),
    /// A record with no address carries a value.
    #[error("a cycle with no address has the value {0:#04X}")]
    ValueWithoutAddress(u8),
}

impl TryFrom<(Option<u32>, Option<u8>, String)> for Cycle {
    type Error = CycleError;

    fn try_from(record: (Option<u32>, Option<u8>, String)) -> Result<Self, Self::Error> {
        let (address, value, signals) = record;
        match (address, value) {
            (Some(address), value) => Ok(Self::Bus(BusCycle {
                address,
                value,
                pins: parse_signals(&signals)?,
            })),
            (None, None) => Ok(Self::Halted),
            (None, Some(value)) => Err(CycleError::ValueWithoutAddress(value)),
        }
    }
}

/// Reads a signal string such as `dp-r-mx-`.
fn parse_signals(signals: &str) -> Result<Pins, CycleError> {
    let error = || CycleError::Signals(signals.to_owned());
    if signals.chars().count() != SIGNALS.len() {
        return Err(error());
    }
    let mut pins = Pins::empty();
    for (character, (letter, flag)) in signals.chars().zip(SIGNALS) {
        let inactive = if flag == Pins::RWB { 'w' } else { '-' };
        if character == letter {
            pins |= flag;
        } else if character != inactive {
            return Err(error());
        }
    }
    Ok(pins)
}

/// Writes signals the way the vectors do, such as `dp-r-mx-`.
#[must_use]
pub fn signal_string(pins: Pins) -> String {
    SIGNALS
        .iter()
        .map(|&(letter, flag)| {
            if pins.contains(flag) {
                letter
            } else if flag == Pins::RWB {
                'w'
            } else {
                '-'
            }
        })
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: sst-65816
    #[test]
    fn a_signal_string_sets_the_flags_of_its_letters() {
        assert_eq!(
            parse_signals("dp-r-mx-"),
            Ok(Pins::VDA | Pins::VPA | Pins::RWB | Pins::M | Pins::X)
        );
        assert_eq!(parse_signals("dpvremxl"), Ok(Pins::all()));
    }

    // @source: sst-65816
    #[test]
    fn a_write_is_the_letter_w_and_clears_rwb() {
        assert_eq!(parse_signals("d--w----"), Ok(Pins::VDA));
        assert_eq!(parse_signals("---w----"), Ok(Pins::empty()));
    }

    // @source: sst-65816
    #[test]
    fn a_malformed_signal_string_is_an_error() {
        for signals in [
            "",
            "dp-r-mx",
            "dp-r-mx--",
            "pd-r-mx-",
            "d-------",
            "dp-R-mx-",
        ] {
            assert_eq!(
                parse_signals(signals),
                Err(CycleError::Signals(signals.to_owned())),
                "{signals:?}"
            );
        }
    }

    // @source: sst-65816
    #[test]
    fn every_signal_combination_survives_a_round_trip() {
        for bits in 0..=u8::MAX {
            let pins = Pins::from_bits_retain(bits);
            assert_eq!(parse_signals(&signal_string(pins)), Ok(pins));
        }
    }

    // @source: sst-65816
    #[test]
    fn a_cycle_record_is_a_bus_cycle_or_the_halt_mark() {
        let cycles = serde_json::from_str::<Vec<Cycle>>(
            r#"[[4660, 86, "dp-remx-"], [4661, null, "---remx-"], [null, null, "--------"]]"#,
        )
        .unwrap();
        let status = Pins::RWB | Pins::E | Pins::M | Pins::X;
        assert_eq!(
            cycles,
            [
                Cycle::Bus(BusCycle {
                    address: 0x1234,
                    value: Some(0x56),
                    pins: Pins::VDA | Pins::VPA | status,
                }),
                Cycle::Bus(BusCycle {
                    address: 0x1235,
                    value: None,
                    pins: status,
                }),
                Cycle::Halted,
            ]
        );
    }

    // @source: sst-65816
    #[test]
    fn a_value_with_no_address_is_an_error() {
        let error = serde_json::from_str::<Cycle>(r#"[null, 5, "--------"]"#).unwrap_err();
        assert!(error.to_string().contains("no address"), "{error}");
    }

    // @source: sst-65816
    #[test]
    fn a_bus_cycle_prints_like_a_vector_record() {
        let read = BusCycle {
            address: 0x62_FD19,
            value: Some(0xA9),
            pins: Pins::VDA | Pins::VPA | Pins::RWB,
        };
        assert_eq!(read.to_string(), r#"[0x62FD19, 0xA9, "dp-r----"]"#);
        let internal = BusCycle {
            address: 0x12,
            value: None,
            pins: Pins::RWB,
        };
        assert_eq!(internal.to_string(), r#"[0x000012, null, "---r----"]"#);
    }

    // @source: sst-65816
    // @source: adr-0005
    #[test]
    fn the_processor_takes_the_register_values_with_s_in_page_1_in_emulation_mode() {
        let state = State {
            pc: 0x1234,
            s: 0x91C8,
            p: 0x77,
            a: 0x6485,
            x: 0x00BA,
            y: 0x0093,
            dbr: 0x65,
            d: 0x451E,
            pbr: 0x87,
            e: 1,
            ram: Vec::new(),
        };
        let processor = state.processor();
        assert_eq!(processor.s, 0x01C8);
        let native = State { e: 0, ..state };
        assert_eq!(native.processor().s, 0x91C8);
        assert_eq!(processor.p.bits(), 0x77);
        assert!(processor.e);
        assert_eq!(processor.run, RunState::Running);
        assert_eq!(
            (processor.pbr, processor.pc, processor.dbr, processor.d),
            (0x87, 0x1234, 0x65, 0x451E)
        );
        assert_eq!(
            (processor.a, processor.x, processor.y),
            (0x6485, 0x00BA, 0x0093)
        );
    }
}
