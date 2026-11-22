// @source: wdc-w65c816s#table-5-7-22a
// @source: wdc-w65c816s#table-5-7-22g
// @source: wdc-w65c816s#table-5-7-22j
// @source: wdc-w65c816s#table-5-7-19c
// @source: wdc-w65c816s#table-5-7-19d
// @source: wdc-w65c816s#table-5-2
// @source: wdc-w65c816s#table-5-3
// @source: wdc-w65c816s#2.25
// @source: wdc-w65c816s#7.11
// @source: wdc-w65c816s#7.12
// @source: wdc-w65c816s#7.13
// @source: wdc-w65c816s#7.14
// @source: wdc-65816-manual#chapter-18
// @source: 6502org-65c816-opcodes#6.3.1
// @source: 6502org-65c816-opcodes#6.3.1.1
// @source: 6502org-65c816-opcodes#6.3.2
// @source: 6502org-65c816-opcodes#6.9
// @source: adr-0004
// @source: adr-0011
//! Interrupts and the run states: BRK, COP, RTI, WAI and STP, the hardware NMI and IRQ
//! entry, and reset.
//!
//! Every interrupt ends the same way (rows 22a and 22j): in native mode it pushes PBR, then
//! the program counter, high byte first, then P; emulation mode leaves out PBR. It then sets
//! I, clears D, clears PBR, and loads the program counter from the vector in bank 0, whose two
//! bytes it reads with VPB active. In emulation mode the pushes stay in page 1, as for every
//! instruction that datasheet 7.1 does not list (ADR 0008). DBR is not changed.

use sfes_api::Addr24;

use crate::{
    addressing::{Direction, StackWrap},
    bus::{Bus65816, Pins},
    registers::{Cpu65816, Flags, RunState},
};

/// The interrupts that enter a handler through a vector. Reset has its own sequence
/// ([`Cpu65816::reset`]); ABORT is not modelled (ADR 0004).
// @source: wdc-w65c816s#table-5-2
// @source: wdc-w65c816s#table-5-3
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Interrupt {
    /// The BRK instruction.
    Break,
    /// The COP instruction.
    Coprocessor,
    /// A non-maskable interrupt request.
    Nmi,
    /// An interrupt request on IRQB.
    Irq,
}

impl Interrupt {
    /// The address of the vector's low byte: Table 5-3 in native mode, Table 5-2 in emulation
    /// mode, where BRK and IRQ share one vector.
    // @source: wdc-w65c816s#table-5-2
    // @source: wdc-w65c816s#table-5-3
    const fn vector(self, emulation: bool) -> u16 {
        match (self, emulation) {
            (Self::Coprocessor, false) => 0xFFE4,
            (Self::Break, false) => 0xFFE6,
            (Self::Nmi, false) => 0xFFEA,
            (Self::Irq, false) => 0xFFEE,
            (Self::Coprocessor, true) => 0xFFF4,
            (Self::Nmi, true) => 0xFFFA,
            (Self::Break | Self::Irq, true) => 0xFFFE,
        }
    }

    /// Whether this is a hardware interrupt. In emulation mode a hardware interrupt pushes P
    /// with bit 4, the break flag, clear (note 11); BRK and COP push it as it is, set.
    // @source: wdc-w65c816s#table-5-7-note-11
    // @source: wdc-65816-manual#chapter-18-brk
    const fn is_hardware(self) -> bool {
        matches!(self, Self::Nmi | Self::Irq)
    }
}

/// The reset vector's low byte, in bank 0 (Table 5-2).
// @source: wdc-w65c816s#table-5-2
// @source: wdc-w65c816s#2.25
const RESET_VECTOR: u16 = 0xFFFC;

impl Cpu65816 {
    /// BRK (row 22j): fetches the signature byte, which nothing uses (7.22), then enters the
    /// handler with the address after the signature as the return address.
    // @source: wdc-w65c816s#table-5-7-22j
    // @source: wdc-w65c816s#7.22
    // @source: wdc-65816-manual#chapter-18-brk
    // @source: 6502org-65c816-opcodes#6.3.1
    pub(crate) fn brk(&mut self, bus: &mut impl Bus65816) {
        self.fetch(bus);
        self.enter_handler(bus, Interrupt::Break);
    }

    /// COP (row 22j): as BRK, through the COP vector. Its signature byte is for the handler
    /// (7.15).
    // @source: wdc-w65c816s#table-5-7-22j
    // @source: wdc-w65c816s#7.15
    // @source: wdc-65816-manual#chapter-18-cop
    // @source: 6502org-65c816-opcodes#6.3.1
    pub(crate) fn cop(&mut self, bus: &mut impl Bus65816) {
        self.fetch(bus);
        self.enter_handler(bus, Interrupt::Coprocessor);
    }

    /// A hardware interrupt (row 22a), taken in place of an instruction: cycle 1 reads the
    /// next opcode at PBR,PC with VDA and VPA active and discards it, cycle 2 is internal at
    /// the same address, and the handler is entered with PBR,PC as the return address.
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: wdc-w65c816s#2.18
    // @source: wdc-w65c816s#2.21
    // @source: 6502org-65c816-opcodes#6.3.1.1
    // @source: adr-0004
    pub(crate) fn hardware_interrupt(&mut self, bus: &mut impl Bus65816, interrupt: Interrupt) {
        self.discarded_opcode_cycles(bus);
        self.enter_handler(bus, interrupt);
    }

    /// RTI (row 22g): two internal cycles at PBR,PC+1, then pulls P, the program counter,
    /// low byte first, and in native mode PBR. The program counter is used as pulled. The
    /// pulled P takes effect after the last cycle, through [`Cpu65816::set_status`]: in
    /// emulation mode M and X stay set, and a set X flag clears the index high bytes. The
    /// pulls stay in page 1 in emulation mode (ADR 0008).
    // @source: wdc-w65c816s#table-5-7-22g
    // @source: wdc-w65c816s#7.11.3
    // @source: wdc-65816-manual#chapter-18-rti
    // @source: 6502org-65c816-opcodes#6.3.2
    // @source: adr-0008
    pub(crate) fn rti(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.internal_at_program_counter(bus);
        let status = Flags::from_bits_retain(self.pull(bus, StackWrap::PageOne));
        self.pc = self.pull_value::<u16>(bus, StackWrap::PageOne);
        if !self.e {
            self.pbr = self.pull(bus, StackWrap::PageOne);
        }
        self.set_status(status);
    }

    /// WAI (row 19d): two internal cycles at PBR,PC+1, then the processor waits for an
    /// interrupt with the program counter on the next instruction (7.13). [`Cpu65816::step`]
    /// ends the wait, two internal cycles later (ADR 0022).
    // @source: wdc-w65c816s#table-5-7-19d
    // @source: wdc-w65c816s#7.13
    // @source: wdc-w65c816s#2.24
    // @source: wdc-65816-manual#chapter-18-wai
    // @source: 6502org-65c816-opcodes#6.9
    pub(crate) fn wai(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.internal_at_program_counter(bus);
        self.run = RunState::Waiting;
    }

    /// STP (row 19c): two internal cycles at PBR,PC+1, then the processor stops until reset
    /// (7.14).
    // @source: wdc-w65c816s#table-5-7-19c
    // @source: wdc-w65c816s#7.14
    // @source: wdc-65816-manual#chapter-18-stp
    // @source: 6502org-65c816-opcodes#6.9
    pub(crate) fn stp(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.internal_at_program_counter(bus);
        self.run = RunState::Stopped;
    }

    /// Resets the processor from any run state and runs the reset sequence, seven bus
    /// cycles, ending with the program counter loaded from `00FFFC`.
    ///
    /// The registers first take the values of 2.25: D, DBR and PBR zero, emulation mode, so
    /// M and X set, S in page 1 and the index high bytes zero, D flag clear, I set. A, the
    /// other flags and the low bytes of X, Y and S keep their values. STP and WAI are
    /// cleared. Then the sequence of row 22a runs in emulation mode (note 7): cycle 1 reads
    /// PBR,PC and discards it, cycle 2 is internal, the three stack cycles read rather than
    /// write (note 10) and step S down in page 1, and the vector is read with VPB active
    /// (ADR 0011).
    // @source: wdc-w65c816s#2.25
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: wdc-w65c816s#table-5-7-note-7
    // @source: wdc-w65c816s#table-5-7-note-10
    // @source: wdc-65816-manual#chapter-18-stp
    // @source: 6502org-65c816-opcodes#6.3.1.1
    // @source: adr-0011
    pub fn reset(&mut self, bus: &mut impl Bus65816) {
        self.d = 0;
        self.dbr = 0;
        self.pbr = 0;
        self.set_emulation(true);
        self.p.remove(Flags::D);
        self.p.insert(Flags::I);
        self.run = RunState::Running;
        self.delayed_irq_disable = None;

        self.discarded_opcode_cycles(bus);
        for _ in 0..3 {
            self.read_byte(bus, self.stack_address());
            self.step_stack(StackWrap::PageOne, Direction::Down);
        }
        self.pc = self.read_vector(bus, RESET_VECTOR);
    }

    /// Cycles 1 and 2 of row 22a, which hardware interrupts and reset share: the opcode at
    /// PBR,PC is read with VDA and VPA active and discarded, then an internal cycle runs at the
    /// same address.
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: adr-0011
    fn discarded_opcode_cycles(&self, bus: &mut impl Bus65816) {
        self.read_opcode(bus);
        self.internal_at_program_counter(bus);
    }

    /// The end of every interrupt sequence: the pushes, the flag changes and the vector.
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: wdc-w65c816s#table-5-7-22j
    // @source: wdc-w65c816s#table-5-7-note-7
    // @source: wdc-w65c816s#table-5-7-note-11
    // @source: wdc-w65c816s#7.11
    // @source: wdc-w65c816s#7.12
    // @source: wdc-w65c816s#table-5-3
    // @source: 6502org-65c816-opcodes#6.3.1
    // @source: 6502org-65c816-opcodes#6.3.1.1
    // @source: adr-0008
    fn enter_handler(&mut self, bus: &mut impl Bus65816, interrupt: Interrupt) {
        if !self.e {
            self.push(bus, self.pbr, StackWrap::PageOne);
        }
        self.push_value(bus, self.pc, StackWrap::PageOne);
        let status = if self.e && interrupt.is_hardware() {
            self.p.difference(Flags::X)
        } else {
            self.p
        };
        self.push(bus, status.bits(), StackWrap::PageOne);
        self.p.insert(Flags::I);
        self.p.remove(Flags::D);
        self.pbr = 0;
        self.pc = self.read_vector(bus, interrupt.vector(self.e));
    }

    /// Cycles 7 and 8 of rows 22a and 22j: "AAVL" at 0,VA and "AAVH" at 0,VA+1, data reads
    /// with VPB active (2.28).
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: wdc-w65c816s#2.28
    fn read_vector(&self, bus: &mut impl Bus65816, vector: u16) -> u16 {
        let pins = Pins::VDA | Pins::VPB | Pins::RWB | self.status_pins();
        let low = bus.read(Addr24::from_bank_offset(0, vector), pins);
        let high = bus.read(Addr24::from_bank_offset(0, vector.wrapping_add(1)), pins);
        u16::from_le_bytes([low, high])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        bus::Pending,
        testing::{Cycle, TestBus, emulation, native, run},
    };

    /// A bus holding `program` at the processor's program address and `memory`, reporting
    /// `pending`.
    // @source: adr-0004
    fn bus_with(
        cpu: &Cpu65816,
        program: &[u8],
        memory: &[(u32, &[u8])],
        pending: Pending,
    ) -> TestBus {
        let mut bus = TestBus::default();
        bus.pending = pending;
        bus.store(cpu.program_address().get(), program);
        for (address, bytes) in memory {
            bus.store(*address, bytes);
        }
        bus
    }

    /// Whether only the last two cycles recorded have VPB active, as 2.28 requires of every
    /// interrupt sequence.
    // @source: wdc-w65c816s#2.28
    fn vpb_only_on_the_last_two(bus: &TestBus) -> bool {
        let count = bus.pins.len();
        bus.pins
            .iter()
            .enumerate()
            .all(|(index, pins)| pins.contains(Pins::VPB) == (index + 2 >= count))
    }

    const NMI: Pending = Pending {
        nmi: true,
        irq: false,
    };
    const IRQ: Pending = Pending {
        nmi: false,
        irq: true,
    };

    // @source: 6502org-65c816-opcodes#6.3.1
    // @source: wdc-w65c816s#table-5-7-22j
    #[test]
    fn brk_follows_the_tutorial_example() {
        // S $01FF, e = 0, P $08, $00FFE6 holds $AB, $CD: a BRK at $123456 stores $12, $34,
        // $58 and $08 at $0001FF down to $0001FC and jumps to $00CDAB; S is $01FB, d = 0,
        // i = 1.
        let mut cpu = native(Flags::D);
        cpu.pc = 0x3456;
        cpu.s = 0x01FF;
        let bus = run(&mut cpu, &[0x00, 0xEA], &[(0x00_FFE6, &[0xAB, 0xCD])]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_3456),
                Cycle::Program(0x12_3457),
                Cycle::Write(0x00_01FF, 0x12),
                Cycle::Write(0x00_01FE, 0x34),
                Cycle::Write(0x00_01FD, 0x58),
                Cycle::Write(0x00_01FC, 0x08),
                Cycle::Data(0x00_FFE6),
                Cycle::Data(0x00_FFE7),
            ]
        );
        assert!(vpb_only_on_the_last_two(&bus));
        assert_eq!((cpu.pbr, cpu.pc, cpu.s), (0x00, 0xCDAB, 0x01FB));
        assert_eq!(cpu.p, Flags::I);
    }

    // @source: wdc-w65c816s#table-5-7-22j
    // @source: wdc-w65c816s#table-5-2
    // @source: wdc-w65c816s#7.11.2
    // @source: wdc-65816-manual#chapter-18-cop
    #[test]
    fn cop_in_emulation_mode_pushes_three_bytes_and_uses_its_own_vector() {
        let mut cpu = emulation();
        cpu.s = 0x01F0;
        cpu.dbr = 0x7E;
        let bus = run(&mut cpu, &[0x02, 0x12], &[(0x00_FFF4, &[0x00, 0x90])]);
        assert_eq!(bus.cycles.len(), 7);
        assert_eq!(
            [bus.byte(0x01F0), bus.byte(0x01EF), bus.byte(0x01EE)],
            [0x80, 0x02, 0x34]
        );
        assert_eq!(
            (cpu.pbr, cpu.pc, cpu.s, cpu.dbr),
            (0x00, 0x9000, 0x01ED, 0x7E)
        );
    }

    // @source: 6502org-65c816-opcodes#6.3.1.1
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: wdc-w65c816s#table-5-3
    // @source: wdc-w65c816s#2.21
    #[test]
    fn nmi_follows_the_tutorial_example() {
        // S $01FF, e = 0, P $08: an NMI during LDA #$1122 at $345678 lets the LDA finish,
        // then stores $34, $56, $7B and $08 at $0001FF down to $0001FC and jumps to $00CDAB;
        // S is $01FB, d = 0, i = 1. The example puts the vector at $00FFE6, BRK's; the
        // tutorial's own vector list and Table 5-3 put the native NMI vector at $00FFEA.
        let mut cpu = native(Flags::D);
        cpu.pbr = 0x34;
        cpu.pc = 0x5678;
        cpu.s = 0x01FF;
        let program = [0xA9, 0x22, 0x11];
        let mut bus = bus_with(&cpu, &program, &[(0x00_FFEA, &[0xAB, 0xCD])], NMI);
        bus.pending = Pending::default();
        cpu.step(&mut bus);
        assert_eq!((cpu.a, cpu.pc), (0x1122, 0x567B));

        bus.cycles.clear();
        bus.pins.clear();
        bus.pending = NMI;
        cpu.step(&mut bus);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x34_567B),
                Cycle::Internal(0x34_567B),
                Cycle::Write(0x00_01FF, 0x34),
                Cycle::Write(0x00_01FE, 0x56),
                Cycle::Write(0x00_01FD, 0x7B),
                Cycle::Write(0x00_01FC, 0x08),
                Cycle::Data(0x00_FFEA),
                Cycle::Data(0x00_FFEB),
            ]
        );
        assert!(vpb_only_on_the_last_two(&bus));
        assert_eq!((cpu.pbr, cpu.pc, cpu.s), (0x00, 0xCDAB, 0x01FB));
        assert_eq!(cpu.p, Flags::I);
    }

    // @source: wdc-w65c816s#table-5-7-22a
    // @source: wdc-w65c816s#table-5-7-note-7
    // @source: wdc-w65c816s#table-5-7-note-11
    // @source: wdc-w65c816s#table-5-2
    // @source: wdc-w65c816s#2.18
    // @source: wdc-65816-manual#chapter-18-brk
    #[test]
    fn an_irq_in_emulation_mode_pushes_p_with_the_break_bit_clear() {
        let mut cpu = emulation();
        cpu.p.remove(Flags::I);
        cpu.s = 0x0101;
        let mut bus = bus_with(&cpu, &[0xEA], &[(0x00_FFFE, &[0x00, 0xC0])], IRQ);
        cpu.step(&mut bus);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Internal(0x12_8000),
                Cycle::Write(0x00_0101, 0x80),
                Cycle::Write(0x00_0100, 0x00),
                Cycle::Write(0x00_01FF, 0x20),
                Cycle::Data(0x00_FFFE),
                Cycle::Data(0x00_FFFF),
            ]
        );
        assert!(vpb_only_on_the_last_two(&bus));
        assert_eq!((cpu.pbr, cpu.pc, cpu.s), (0x00, 0xC000, 0x01FE));
        assert!(cpu.p.contains(Flags::I));
    }

    // @source: wdc-w65c816s#2.18
    // @source: wdc-w65c816s#7.19
    #[test]
    fn an_nmi_is_taken_with_the_i_flag_set_and_before_an_irq() {
        let mut cpu = native(Flags::I);
        let both = Pending {
            nmi: true,
            irq: true,
        };
        let mut bus = bus_with(&cpu, &[0xEA], &[(0x00_FFEA, &[0x00, 0xB0])], both);
        cpu.step(&mut bus);
        assert_eq!(cpu.pc, 0xB000);
    }

    // @source: 6502org-65c816-opcodes#6.3.2
    // @source: wdc-w65c816s#table-5-7-22g
    #[test]
    fn rti_follows_the_tutorial_example() {
        // S $01FB, e = 0, $0001FC to $0001FF hold $08, $12, $34, $56: RTI jumps to $563412,
        // S is $01FF and P is $08.
        let mut cpu = native(Flags::empty());
        cpu.s = 0x01FB;
        let bus = run(&mut cpu, &[0x40], &[(0x00_01FC, &[0x08, 0x12, 0x34, 0x56])]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Internal(0x12_8001),
                Cycle::Internal(0x12_8001),
                Cycle::Data(0x00_01FC),
                Cycle::Data(0x00_01FD),
                Cycle::Data(0x00_01FE),
                Cycle::Data(0x00_01FF),
            ]
        );
        assert_eq!(
            (cpu.pbr, cpu.pc, cpu.s, cpu.p),
            (0x56, 0x3412, 0x01FF, Flags::D)
        );
    }

    // @source: wdc-w65c816s#table-5-7-22g
    // @source: wdc-w65c816s#table-5-7-note-7
    // @source: wdc-65816-manual#chapter-18-rti
    #[test]
    fn rti_in_emulation_mode_pulls_three_bytes_and_keeps_m_and_x() {
        let mut cpu = emulation();
        cpu.s = 0x01FC;
        let bus = run(&mut cpu, &[0x40], &[(0x00_01FD, &[0x01, 0x34, 0x12])]);
        assert_eq!(bus.cycles.len(), 6);
        assert_eq!((cpu.pbr, cpu.pc, cpu.s), (0x12, 0x1234, 0x01FF));
        assert_eq!(cpu.p, Flags::M | Flags::X | Flags::C);
    }

    // @source: wdc-w65c816s#table-5-7-19d
    // @source: wdc-w65c816s#7.13
    // @source: wdc-w65c816s#2.24
    // @source: 6502org-65c816-opcodes#6.9
    // @source: anomie-timing#s-cpu/instructions
    // @source: adr-0011
    // @source: adr-0022
    #[test]
    fn wai_waits_in_internal_cycles_and_an_irq_wakes_it_into_the_handler() {
        let mut cpu = native(Flags::empty());
        cpu.s = 0x01FF;
        let mut bus = bus_with(
            &cpu,
            &[0xCB],
            &[(0x00_FFEE, &[0x00, 0xA0])],
            Pending::default(),
        );
        cpu.step(&mut bus);
        cpu.step(&mut bus);
        cpu.step(&mut bus);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Internal(0x12_8001),
                Cycle::Internal(0x12_8001),
                Cycle::Internal(0x12_8001),
                Cycle::Internal(0x12_8001),
            ]
        );
        assert_eq!(cpu.run, RunState::Waiting);

        bus.cycles.clear();
        bus.pending = IRQ;
        cpu.step(&mut bus);
        // Two internal cycles end the wait (ADR 0022), then row 22a begins with its opcode
        // fetch at PBR,PC+1.
        assert_eq!(
            bus.cycles[..4],
            [
                Cycle::Internal(0x12_8001),
                Cycle::Internal(0x12_8001),
                Cycle::Opcode(0x12_8001),
                Cycle::Internal(0x12_8001),
            ]
        );
        assert_eq!(bus.cycles.len(), 10);
        assert_eq!([bus.byte(0x01FE), bus.byte(0x01FD)], [0x80, 0x01]);
        assert_eq!(
            (cpu.run, cpu.pbr, cpu.pc),
            (RunState::Running, 0x00, 0xA000)
        );
    }

    // @source: wdc-w65c816s#7.13
    // @source: wdc-w65c816s#2.24
    // @source: wdc-65816-manual#chapter-18-wai
    // @source: 6502org-65c816-opcodes#6.9
    // @source: anomie-timing#s-cpu/instructions
    // @source: adr-0011
    // @source: adr-0022
    #[test]
    fn wai_with_irqs_disabled_resumes_at_the_next_instruction() {
        let mut cpu = native(Flags::I);
        let mut bus = bus_with(&cpu, &[0xCB, 0xE8], &[], Pending::default());
        cpu.step(&mut bus);
        bus.cycles.clear();
        bus.pending = IRQ;
        cpu.step(&mut bus);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Internal(0x12_8001),
                Cycle::Internal(0x12_8001),
                Cycle::Opcode(0x12_8001),
                Cycle::Internal(0x12_8002),
            ]
        );
        assert_eq!((cpu.run, cpu.x), (RunState::Running, 0x0001));
    }

    // @source: wdc-w65c816s#table-5-7-19c
    // @source: wdc-w65c816s#7.14
    // @source: wdc-65816-manual#chapter-18-stp
    // @source: 6502org-65c816-opcodes#6.9
    // @source: adr-0011
    #[test]
    fn stp_stops_until_reset_and_ignores_interrupts() {
        let mut cpu = native(Flags::empty());
        let mut bus = bus_with(&cpu, &[0xDB], &[], Pending::default());
        cpu.step(&mut bus);
        bus.pending = Pending {
            nmi: true,
            irq: true,
        };
        cpu.step(&mut bus);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Internal(0x12_8001),
                Cycle::Internal(0x12_8001),
                Cycle::Internal(0x12_8001),
            ]
        );
        assert_eq!((cpu.run, cpu.pc), (RunState::Stopped, 0x8001));
    }

    // @source: wdc-w65c816s#2.25
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: wdc-w65c816s#table-5-7-note-10
    // @source: wdc-w65c816s#7.14
    // @source: wdc-65816-manual#chapter-18-stp
    // @source: adr-0011
    #[test]
    fn reset_restarts_a_stopped_processor_in_emulation_mode() {
        let mut cpu = native(Flags::D | Flags::C);
        cpu.a = 0x1234;
        cpu.x = 0x5678;
        cpu.y = 0x9ABC;
        cpu.s = 0x1F80;
        cpu.d = 0x2000;
        cpu.dbr = 0x7E;
        cpu.run = RunState::Stopped;
        let mut bus = bus_with(&cpu, &[], &[(0x00_FFFC, &[0x00, 0x80])], Pending::default());
        cpu.reset(&mut bus);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x00_8000),
                Cycle::Internal(0x00_8000),
                Cycle::Data(0x00_0180),
                Cycle::Data(0x00_017F),
                Cycle::Data(0x00_017E),
                Cycle::Data(0x00_FFFC),
                Cycle::Data(0x00_FFFD),
            ]
        );
        assert!(
            bus.pins
                .iter()
                .all(|pins| pins.contains(Pins::RWB | Pins::E))
        );
        assert!(vpb_only_on_the_last_two(&bus));
        assert_eq!(cpu.run, RunState::Running);
        assert!(cpu.e);
        assert_eq!(
            (cpu.a, cpu.x, cpu.y, cpu.s, cpu.d, cpu.dbr, cpu.pbr, cpu.pc),
            (0x1234, 0x0078, 0x00BC, 0x017D, 0x0000, 0x00, 0x00, 0x8000)
        );
        assert_eq!(cpu.p, Flags::M | Flags::X | Flags::I | Flags::C);
    }

    // @source: wdc-w65c816s#2.25
    // @source: wdc-w65c816s#7.13
    // @source: adr-0011
    #[test]
    fn reset_ends_a_wait() {
        let mut cpu = emulation();
        cpu.run = RunState::Waiting;
        let mut bus = bus_with(&cpu, &[], &[(0x00_FFFC, &[0x34, 0x12])], Pending::default());
        cpu.reset(&mut bus);
        assert_eq!((cpu.run, cpu.pc), (RunState::Running, 0x1234));
    }
}
