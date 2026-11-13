//! The bus the vectors run on: 16 MiB of sparse RAM that records every cycle.

use std::collections::BTreeMap;

use sfes_api::Addr24;
use sfes_snes_cpu::{Bus65816, Pending, Pins};

use crate::sst65816::model::BusCycle;

/// A [`Bus65816`] over sparse RAM that records `(address, value, pins)` for every call, in
/// order (ADR 0004). An idle cycle records no value, which is the vectors' `null`.
///
/// The vectors assume RAM at every address and list each byte an instruction reads, so a
/// read of a byte that was never stored returns zero; the recorded cycle then differs from
/// the vector's. No interrupt is ever pending.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecordingBus {
    memory: BTreeMap<u32, u8>,
    cycles: Vec<BusCycle>,
}

impl RecordingBus {
    /// A bus whose memory holds the given `(address, value)` pairs and nothing else.
    #[must_use]
    pub fn new(ram: &[(u32, u8)]) -> Self {
        let memory = ram
            .iter()
            .map(|&(address, value)| (address & Addr24::MASK, value))
            .collect::<BTreeMap<_, _>>();
        Self {
            memory,
            cycles: Vec::new(),
        }
    }

    /// The byte stored at `address`, if one was seeded or written.
    #[must_use]
    pub fn byte(&self, address: u32) -> Option<u8> {
        self.memory.get(&address).copied()
    }

    /// Every cycle so far, in order.
    #[must_use]
    pub fn cycles(&self) -> &[BusCycle] {
        &self.cycles
    }

    fn record(&mut self, address: Addr24, value: Option<u8>, pins: Pins) {
        let address = address.get();
        self.cycles.push(BusCycle {
            address,
            value,
            pins,
        });
    }
}

impl Bus65816 for RecordingBus {
    fn read(&mut self, address: Addr24, pins: Pins) -> u8 {
        let value = self.byte(address.get()).unwrap_or(0);
        self.record(address, Some(value), pins);
        value
    }

    fn write(&mut self, address: Addr24, value: u8, pins: Pins) {
        self.memory.insert(address.get(), value);
        self.record(address, Some(value), pins);
    }

    fn idle(&mut self, address: Addr24, pins: Pins) {
        self.record(address, None, pins);
    }

    fn interrupts(&self) -> Pending {
        Pending::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: adr-0004
    #[test]
    fn a_read_returns_the_seeded_byte_and_records_it() {
        let mut bus = RecordingBus::new(&[(0x12_3456, 0xAB)]);
        let pins = Pins::VDA | Pins::RWB;
        assert_eq!(bus.read(Addr24::new(0x12_3456), pins), 0xAB);
        assert_eq!(
            bus.cycles(),
            [BusCycle {
                address: 0x12_3456,
                value: Some(0xAB),
                pins,
            }]
        );
    }

    // @source: adr-0004
    #[test]
    fn a_read_of_an_unlisted_byte_returns_zero_and_stores_nothing() {
        let mut bus = RecordingBus::new(&[]);
        assert_eq!(bus.read(Addr24::new(0x40), Pins::VDA | Pins::RWB), 0);
        assert_eq!(bus.byte(0x40), None);
        assert_eq!(bus.cycles()[0].value, Some(0));
    }

    // @source: adr-0004
    #[test]
    fn a_write_stores_the_byte_and_records_it() {
        let mut bus = RecordingBus::new(&[(0x7E_0000, 1)]);
        bus.write(Addr24::new(0x7E_0000), 0x99, Pins::VDA);
        assert_eq!(bus.byte(0x7E_0000), Some(0x99));
        assert_eq!(
            bus.cycles(),
            [BusCycle {
                address: 0x7E_0000,
                value: Some(0x99),
                pins: Pins::VDA,
            }]
        );
    }

    // @source: adr-0004
    #[test]
    fn an_idle_cycle_records_its_address_and_no_value() {
        let mut bus = RecordingBus::new(&[(0x10, 0x55)]);
        bus.idle(Addr24::new(0x10), Pins::RWB);
        assert_eq!(
            bus.cycles(),
            [BusCycle {
                address: 0x10,
                value: None,
                pins: Pins::RWB,
            }]
        );
    }

    // @source: adr-0004
    #[test]
    fn cycles_are_recorded_in_call_order_and_nothing_is_pending() {
        let mut bus = RecordingBus::new(&[]);
        bus.idle(Addr24::new(1), Pins::RWB);
        bus.write(Addr24::new(2), 3, Pins::VDA);
        bus.read(Addr24::new(2), Pins::VDA | Pins::RWB);
        let addresses = bus
            .cycles()
            .iter()
            .map(|cycle| cycle.address)
            .collect::<Vec<_>>();
        assert_eq!(addresses, [1, 2, 2]);
        assert_eq!(bus.cycles()[2].value, Some(3));
        assert_eq!(bus.interrupts(), Pending::default());
    }
}
