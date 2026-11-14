// @source: adr-0004
//! A recording bus over sparse memory, and helpers that run one instruction on it, for this
//! crate's unit tests.

use std::{collections::BTreeMap, vec::Vec};

use sfes_api::Addr24;

use crate::{
    bus::{Bus65816, Pending, Pins},
    registers::{Cpu65816, Flags},
};

/// One bus cycle as a test states it: its kind, from the VDA and VPA signals, and its
/// address.
// @source: wdc-w65c816s#2.26
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Cycle {
    /// VDA and VPA active: an opcode fetch.
    Opcode(u32),
    /// VPA active: a program byte.
    Program(u32),
    /// VDA active: a data byte.
    Data(u32),
    /// Neither active: an internal operation.
    Internal(u32),
}

/// Memory that reads as zero where nothing was stored, with a record of every cycle.
#[derive(Default)]
pub(crate) struct TestBus {
    memory: BTreeMap<u32, u8>,
    /// The cycles so far, in order.
    pub(crate) cycles: Vec<Cycle>,
    /// The full signals of each cycle, in order.
    pub(crate) pins: Vec<Pins>,
}

impl TestBus {
    /// Stores `bytes` at consecutive 24-bit addresses from `address`.
    // @source: adr-0004
    pub(crate) fn store(&mut self, address: u32, bytes: &[u8]) {
        for (address, byte) in (address..).zip(bytes) {
            self.memory.insert(address & Addr24::MASK, *byte);
        }
    }
}

impl Bus65816 for TestBus {
    // @source: adr-0004
    fn read(&mut self, address: Addr24, pins: Pins) -> u8 {
        assert!(pins.contains(Pins::RWB), "a read has RWB high");
        let cycle = match (pins.contains(Pins::VDA), pins.contains(Pins::VPA)) {
            (true, true) => Cycle::Opcode(address.get()),
            (false, true) => Cycle::Program(address.get()),
            (true, false) => Cycle::Data(address.get()),
            (false, false) => panic!("a read with neither VDA nor VPA must be an idle cycle"),
        };
        self.cycles.push(cycle);
        self.pins.push(pins);
        self.memory.get(&address.get()).copied().unwrap_or(0)
    }

    // @source: adr-0004
    fn write(&mut self, address: Addr24, value: u8, pins: Pins) {
        panic!("unexpected write of {value:#04X} to {address:?} with {pins:?}");
    }

    // @source: adr-0004
    fn idle(&mut self, address: Addr24, pins: Pins) {
        assert!(
            pins.contains(Pins::RWB) && !pins.intersects(Pins::VDA | Pins::VPA),
            "an idle cycle has RWB high and VDA and VPA inactive"
        );
        self.cycles.push(Cycle::Internal(address.get()));
        self.pins.push(pins);
    }

    // @source: adr-0004
    fn interrupts(&self) -> Pending {
        Pending::default()
    }
}

/// A native-mode processor with the given status flags, at program address `12:8000`.
// @source: wdc-w65c816s#table-2-1
pub(crate) fn native(status: Flags) -> Cpu65816 {
    let mut cpu = Cpu65816::new();
    cpu.set_emulation(false);
    cpu.set_status(status);
    cpu.pbr = 0x12;
    cpu.pc = 0x8000;
    cpu
}

/// An emulation-mode processor at program address `12:8000`.
// @source: wdc-w65c816s#table-2-1
pub(crate) fn emulation() -> Cpu65816 {
    let mut cpu = Cpu65816::new();
    cpu.pbr = 0x12;
    cpu.pc = 0x8000;
    cpu
}

/// Puts `program` at the processor's program address and `memory` at its addresses, runs
/// one step, and returns the bus with its record.
// @source: adr-0004
pub(crate) fn run(cpu: &mut Cpu65816, program: &[u8], memory: &[(u32, &[u8])]) -> TestBus {
    let mut bus = TestBus::default();
    bus.store(cpu.program_address().get(), program);
    for (address, bytes) in memory {
        bus.store(*address, bytes);
    }
    cpu.step(&mut bus);
    bus
}
