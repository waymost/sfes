// @source: adr-0004
//! One step of the processor: an idle cycle while halted, an interrupt entry, or one
//! instruction (ADR 0004).

use sfes_api::Addr24;

use crate::{
    bus::{Bus65816, Pending, Pins},
    registers::{Cpu65816, Flags, RunState},
};

impl Cpu65816 {
    /// Runs one step, making one bus call per clock cycle (ADR 0004):
    ///
    /// - stopped: one idle cycle;
    /// - waiting: one idle cycle, or the wake-up when an interrupt request is pending;
    /// - running with an NMI, or an IRQ and the I flag clear: the interrupt sequence;
    /// - otherwise: one instruction, or one byte of a block move.
    ///
    /// The interrupt requests are read once, before the opcode fetch.
    // @source: adr-0004
    pub fn step(&mut self, bus: &mut impl Bus65816) {
        if self.run == RunState::Stopped {
            self.halted_cycle(bus);
            return;
        }
        let Pending { nmi, irq } = bus.interrupts();
        let interrupt = nmi || (irq && !self.p.contains(Flags::I));
        if self.run == RunState::Waiting {
            if !nmi && !irq {
                self.halted_cycle(bus);
                return;
            }
            // An IRQ with the I flag set ends the wait without entering the handler: the
            // instruction after WAI runs next.
            self.run = RunState::Running;
        }
        if interrupt {
            self.interrupt_entry();
        } else {
            self.instruction(bus);
        }
    }

    /// The cycle a waiting or stopped processor spends per step, so that the machine's time
    /// keeps passing. A modelling choice of ADR 0004, not a claim about the pins: the address
    /// and signals here are provisional until the WAI and STP rows are implemented (#42).
    // @source: adr-0004
    fn halted_cycle(&self, bus: &mut impl Bus65816) {
        bus.idle(self.program_address(), Pins::RWB | self.status_pins());
    }

    /// Bring-up placeholder for the hardware interrupt sequence: it stops the processor. The
    /// sequence replaces it in #42. (`todo!` and `panic!` are denied in library code.)
    // @source: adr-0004
    fn interrupt_entry(&mut self) {
        self.run = RunState::Stopped;
    }

    /// Fetches an opcode and executes it.
    // @source: wdc-w65c816s#table-5-4
    fn instruction(&mut self, bus: &mut impl Bus65816) {
        let opcode = self.fetch_opcode(bus);
        #[expect(
            clippy::match_single_binding,
            reason = "bring-up: each instruction family adds its arms (#33 to #42)"
        )]
        match opcode {
            // Bring-up placeholder: an opcode with no arm yet stops the processor. The vector
            // harness only runs opcodes on its implemented list. This arm is deleted once all
            // 256 opcodes have one (#43), so that the compiler proves the match is complete.
            _ => self.run = RunState::Stopped,
        }
    }

    /// Cycle 1 of every instruction: reads the opcode at PBR,PC with VDA and VPA both
    /// active, and increments the program counter within the program bank.
    // @source: wdc-w65c816s#table-5-7
    // @source: wdc-w65c816s#2.10
    fn fetch_opcode(&mut self, bus: &mut impl Bus65816) -> u8 {
        let pins = Pins::VDA | Pins::VPA | Pins::RWB | self.status_pins();
        let opcode = bus.read(self.program_address(), pins);
        self.pc = self.pc.wrapping_add(1);
        opcode
    }

    /// The 24-bit address of the program counter: the Program Bank Register over PC.
    // @source: wdc-w65c816s#2.9
    fn program_address(&self) -> Addr24 {
        Addr24::from_bank_offset(self.pbr, self.pc)
    }
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use super::*;

    /// What a bus call looked like: the method, the address and the signals.
    #[derive(Debug, Eq, PartialEq)]
    enum Cycle {
        Read(u32, Pins),
        Idle(u32, Pins),
    }

    /// A bus that returns one fixed byte, reports fixed interrupt requests and records every
    /// cycle.
    #[derive(Default)]
    struct TestBus {
        pending: Pending,
        cycles: Vec<Cycle>,
    }

    impl Bus65816 for TestBus {
        // @source: adr-0004
        fn read(&mut self, address: Addr24, pins: Pins) -> u8 {
            self.cycles.push(Cycle::Read(address.get(), pins));
            0x42
        }

        // @source: adr-0004
        fn write(&mut self, address: Addr24, value: u8, pins: Pins) {
            panic!("unexpected write of {value:#04X} to {address:?} with {pins:?}");
        }

        // @source: adr-0004
        fn idle(&mut self, address: Addr24, pins: Pins) {
            self.cycles.push(Cycle::Idle(address.get(), pins));
        }

        // @source: adr-0004
        fn interrupts(&self) -> Pending {
            self.pending
        }
    }

    /// A reset-state processor with its program counter at `pbr`,`pc`.
    // @source: wdc-w65c816s#2.9
    fn cpu_at(pbr: u8, pc: u16) -> Cpu65816 {
        let mut cpu = Cpu65816::new();
        cpu.pbr = pbr;
        cpu.pc = pc;
        cpu
    }

    // @source: wdc-w65c816s#table-5-7
    // @source: wdc-w65c816s#2.26
    #[test]
    fn the_opcode_fetch_reads_pbr_pc_with_vda_and_vpa() {
        let mut cpu = cpu_at(0x12, 0x3456);
        let mut bus = TestBus::default();
        cpu.step(&mut bus);
        let pins = Pins::VDA | Pins::VPA | Pins::RWB | Pins::E | Pins::M | Pins::X;
        assert_eq!(bus.cycles, [Cycle::Read(0x12_3456, pins)]);
        assert_eq!((cpu.pbr, cpu.pc), (0x12, 0x3457));
    }

    // @source: wdc-w65c816s#2.10
    #[test]
    fn the_program_counter_wraps_within_the_program_bank() {
        let mut cpu = cpu_at(0x12, 0xFFFF);
        cpu.step(&mut TestBus::default());
        assert_eq!((cpu.pbr, cpu.pc), (0x12, 0x0000));
    }

    // @source: adr-0004
    #[test]
    fn an_opcode_with_no_arm_stops_the_processor_during_bring_up() {
        let mut cpu = cpu_at(0, 0x8000);
        cpu.step(&mut TestBus::default());
        assert_eq!(cpu.run, RunState::Stopped);
    }

    // @source: adr-0004
    // @source: wdc-w65c816s#7.14
    #[test]
    fn a_stopped_processor_idles_and_ignores_interrupts() {
        let mut cpu = cpu_at(0x12, 0x3456);
        cpu.run = RunState::Stopped;
        let mut bus = TestBus {
            pending: Pending {
                nmi: true,
                irq: true,
            },
            cycles: Vec::new(),
        };
        let before = cpu.clone();
        cpu.step(&mut bus);
        let pins = Pins::RWB | Pins::E | Pins::M | Pins::X;
        assert_eq!(bus.cycles, [Cycle::Idle(0x12_3456, pins)]);
        assert_eq!(cpu, before);
    }

    // @source: adr-0004
    // @source: wdc-w65c816s#7.13
    #[test]
    fn a_waiting_processor_idles_until_a_request_arrives() {
        let mut cpu = cpu_at(0x12, 0x3456);
        cpu.run = RunState::Waiting;
        let mut bus = TestBus::default();
        let before = cpu.clone();
        cpu.step(&mut bus);
        assert!(matches!(bus.cycles[..], [Cycle::Idle(0x12_3456, _)]));
        assert_eq!(cpu, before);
    }

    // @source: wdc-w65c816s#7.13
    #[test]
    fn an_irq_with_the_i_flag_set_resumes_after_wai_without_the_handler() {
        let mut cpu = cpu_at(0x12, 0x3456);
        assert!(cpu.p.contains(Flags::I));
        cpu.run = RunState::Waiting;
        let mut bus = TestBus {
            pending: Pending {
                nmi: false,
                irq: true,
            },
            cycles: Vec::new(),
        };
        cpu.step(&mut bus);
        // The next instruction's opcode was fetched: no interrupt sequence ran.
        assert!(matches!(bus.cycles[..], [Cycle::Read(0x12_3456, _)]));
        assert_eq!(cpu.pc, 0x3457);
    }

    // @source: adr-0004
    // @source: wdc-w65c816s#2.18
    #[test]
    fn a_masked_irq_does_not_interrupt_a_running_processor() {
        let mut cpu = cpu_at(0, 0x8000);
        let mut bus = TestBus {
            pending: Pending {
                nmi: false,
                irq: true,
            },
            cycles: Vec::new(),
        };
        cpu.step(&mut bus);
        assert!(matches!(bus.cycles[..], [Cycle::Read(0x00_8000, _)]));
    }

    // @source: adr-0004
    // @source: wdc-w65c816s#2.21
    #[test]
    fn a_recognized_interrupt_replaces_the_instruction() {
        for pending in [
            Pending {
                nmi: true,
                irq: false,
            },
            Pending {
                nmi: false,
                irq: true,
            },
        ] {
            let mut cpu = cpu_at(0, 0x8000);
            cpu.set_emulation(false);
            cpu.set_status(Flags::empty());
            let mut bus = TestBus {
                pending,
                cycles: Vec::new(),
            };
            cpu.step(&mut bus);
            // No opcode fetch. (The sequence itself is a bring-up placeholder until #42.)
            assert_eq!(bus.cycles, []);
            assert_eq!(cpu.pc, 0x8000);
        }
    }
}
