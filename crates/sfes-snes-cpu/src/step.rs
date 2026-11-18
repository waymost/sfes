// @source: adr-0004
//! One step of the processor: an idle cycle while halted, an interrupt entry, or one
//! instruction (ADR 0004).

use sfes_api::Addr24;

use crate::{
    addressing::{Address, Mode},
    bus::{Bus65816, Pending, Pins},
    instructions::{Condition, Interrupt, Modify},
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
            self.interrupt_entry(bus, nmi);
        } else {
            self.instruction(bus);
        }
    }

    /// The cycle a waiting or stopped processor spends per step, so that the machine's time
    /// keeps passing (ADR 0004): an internal cycle at PBR,PC, which after WAI or STP is the
    /// "PBR,PC+1" of their last cycle, repeated with the same signals (rows 19c and 19d, ADR
    /// 0011).
    // @source: adr-0004
    // @source: adr-0011
    // @source: wdc-w65c816s#table-5-7-19c
    // @source: wdc-w65c816s#table-5-7-19d
    fn halted_cycle(&self, bus: &mut impl Bus65816) {
        bus.idle(self.program_address(), Pins::RWB | self.status_pins());
    }

    /// The hardware interrupt sequence (row 22a) for the request `step` recognized: an NMI
    /// before an IRQ (7.19).
    // @source: wdc-w65c816s#7.19
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: adr-0004
    fn interrupt_entry(&mut self, bus: &mut impl Bus65816, nmi: bool) {
        let interrupt = if nmi { Interrupt::Nmi } else { Interrupt::Irq };
        self.hardware_interrupt(bus, interrupt);
    }

    /// Fetches an opcode and executes it. The arms follow the opcode matrix, Table 5-4,
    /// checked against the opcode columns of Table 5-5.
    // @source: wdc-w65c816s#table-5-4
    // @source: wdc-w65c816s#table-5-5
    #[expect(
        clippy::too_many_lines,
        reason = "one arm per opcode, read against Table 5-4 row by row"
    )]
    fn instruction(&mut self, bus: &mut impl Bus65816) {
        let opcode = self.fetch_opcode(bus);
        match opcode {
            0x00 => self.brk(bus),
            0x01 => self.ora(bus, Mode::DirectIndexedIndirect),
            0x02 => self.cop(bus),
            0x03 => self.ora(bus, Mode::StackRelative),
            0x04 => self.tsb(bus, Address::Direct),
            0x05 => self.ora(bus, Mode::Direct),
            0x06 => self.modify_memory(bus, Address::Direct, Modify::ShiftLeft),
            0x07 => self.ora(bus, Mode::DirectIndirectLong),
            0x08 => self.php(bus),
            0x09 => self.ora(bus, Mode::Immediate),
            0x0A => self.modify_accumulator(bus, Modify::ShiftLeft),
            0x0B => self.phd(bus),
            0x0C => self.tsb(bus, Address::Absolute),
            0x0D => self.ora(bus, Mode::Absolute),
            0x0E => self.modify_memory(bus, Address::Absolute, Modify::ShiftLeft),
            0x0F => self.ora(bus, Mode::AbsoluteLong),
            0x10 => self.branch(bus, Condition::Plus),
            0x11 => self.ora(bus, Mode::DirectIndirectIndexed),
            0x12 => self.ora(bus, Mode::DirectIndirect),
            0x13 => self.ora(bus, Mode::StackRelativeIndirectIndexed),
            0x14 => self.trb(bus, Address::Direct),
            0x15 => self.ora(bus, Mode::DirectX),
            0x16 => self.modify_memory(bus, Address::DirectX, Modify::ShiftLeft),
            0x17 => self.ora(bus, Mode::DirectIndirectLongIndexed),
            0x18 => self.set_flag(bus, Flags::C, false),
            0x19 => self.ora(bus, Mode::AbsoluteY),
            0x1A => self.modify_accumulator(bus, Modify::Increment),
            0x1B => self.tcs(bus),
            0x1C => self.trb(bus, Address::Absolute),
            0x1D => self.ora(bus, Mode::AbsoluteX),
            0x1E => self.modify_memory(bus, Address::AbsoluteX, Modify::ShiftLeft),
            0x1F => self.ora(bus, Mode::AbsoluteLongX),
            0x20 => self.jsr_absolute(bus),
            0x21 => self.and(bus, Mode::DirectIndexedIndirect),
            0x22 => self.jsl(bus),
            0x23 => self.and(bus, Mode::StackRelative),
            0x24 => self.bit(bus, Mode::Direct),
            0x25 => self.and(bus, Mode::Direct),
            0x26 => self.modify_memory(bus, Address::Direct, Modify::RotateLeft),
            0x27 => self.and(bus, Mode::DirectIndirectLong),
            0x28 => self.plp(bus),
            0x29 => self.and(bus, Mode::Immediate),
            0x2A => self.modify_accumulator(bus, Modify::RotateLeft),
            0x2B => self.pld(bus),
            0x2C => self.bit(bus, Mode::Absolute),
            0x2D => self.and(bus, Mode::Absolute),
            0x2E => self.modify_memory(bus, Address::Absolute, Modify::RotateLeft),
            0x2F => self.and(bus, Mode::AbsoluteLong),
            0x30 => self.branch(bus, Condition::Minus),
            0x31 => self.and(bus, Mode::DirectIndirectIndexed),
            0x32 => self.and(bus, Mode::DirectIndirect),
            0x33 => self.and(bus, Mode::StackRelativeIndirectIndexed),
            0x34 => self.bit(bus, Mode::DirectX),
            0x35 => self.and(bus, Mode::DirectX),
            0x36 => self.modify_memory(bus, Address::DirectX, Modify::RotateLeft),
            0x37 => self.and(bus, Mode::DirectIndirectLongIndexed),
            0x38 => self.set_flag(bus, Flags::C, true),
            0x39 => self.and(bus, Mode::AbsoluteY),
            0x3A => self.modify_accumulator(bus, Modify::Decrement),
            0x3B => self.tsc(bus),
            0x3C => self.bit(bus, Mode::AbsoluteX),
            0x3D => self.and(bus, Mode::AbsoluteX),
            0x3E => self.modify_memory(bus, Address::AbsoluteX, Modify::RotateLeft),
            0x3F => self.and(bus, Mode::AbsoluteLongX),
            0x40 => self.rti(bus),
            0x41 => self.eor(bus, Mode::DirectIndexedIndirect),
            0x42 => self.wdm(bus),
            0x43 => self.eor(bus, Mode::StackRelative),
            0x44 => self.mvp(bus),
            0x45 => self.eor(bus, Mode::Direct),
            0x46 => self.modify_memory(bus, Address::Direct, Modify::ShiftRight),
            0x47 => self.eor(bus, Mode::DirectIndirectLong),
            0x48 => self.pha(bus),
            0x49 => self.eor(bus, Mode::Immediate),
            0x4A => self.modify_accumulator(bus, Modify::ShiftRight),
            0x4B => self.phk(bus),
            0x4C => self.jmp_absolute(bus),
            0x4D => self.eor(bus, Mode::Absolute),
            0x4E => self.modify_memory(bus, Address::Absolute, Modify::ShiftRight),
            0x4F => self.eor(bus, Mode::AbsoluteLong),
            0x50 => self.branch(bus, Condition::OverflowClear),
            0x51 => self.eor(bus, Mode::DirectIndirectIndexed),
            0x52 => self.eor(bus, Mode::DirectIndirect),
            0x53 => self.eor(bus, Mode::StackRelativeIndirectIndexed),
            0x54 => self.mvn(bus),
            0x55 => self.eor(bus, Mode::DirectX),
            0x56 => self.modify_memory(bus, Address::DirectX, Modify::ShiftRight),
            0x57 => self.eor(bus, Mode::DirectIndirectLongIndexed),
            0x58 => self.set_flag(bus, Flags::I, false),
            0x59 => self.eor(bus, Mode::AbsoluteY),
            0x5A => self.phy(bus),
            0x5B => self.tcd(bus),
            0x5C => self.jmp_long(bus),
            0x5D => self.eor(bus, Mode::AbsoluteX),
            0x5E => self.modify_memory(bus, Address::AbsoluteX, Modify::ShiftRight),
            0x5F => self.eor(bus, Mode::AbsoluteLongX),
            0x60 => self.rts(bus),
            0x61 => self.adc(bus, Mode::DirectIndexedIndirect),
            0x62 => self.per(bus),
            0x63 => self.adc(bus, Mode::StackRelative),
            0x64 => self.stz(bus, Address::Direct),
            0x65 => self.adc(bus, Mode::Direct),
            0x66 => self.modify_memory(bus, Address::Direct, Modify::RotateRight),
            0x67 => self.adc(bus, Mode::DirectIndirectLong),
            0x68 => self.pla(bus),
            0x69 => self.adc(bus, Mode::Immediate),
            0x6A => self.modify_accumulator(bus, Modify::RotateRight),
            0x6B => self.rtl(bus),
            0x6C => self.jmp_indirect(bus),
            0x6D => self.adc(bus, Mode::Absolute),
            0x6E => self.modify_memory(bus, Address::Absolute, Modify::RotateRight),
            0x6F => self.adc(bus, Mode::AbsoluteLong),
            0x70 => self.branch(bus, Condition::OverflowSet),
            0x71 => self.adc(bus, Mode::DirectIndirectIndexed),
            0x72 => self.adc(bus, Mode::DirectIndirect),
            0x73 => self.adc(bus, Mode::StackRelativeIndirectIndexed),
            0x74 => self.stz(bus, Address::DirectX),
            0x75 => self.adc(bus, Mode::DirectX),
            0x76 => self.modify_memory(bus, Address::DirectX, Modify::RotateRight),
            0x77 => self.adc(bus, Mode::DirectIndirectLongIndexed),
            0x78 => self.set_flag(bus, Flags::I, true),
            0x79 => self.adc(bus, Mode::AbsoluteY),
            0x7A => self.ply(bus),
            0x7B => self.tdc(bus),
            0x7C => self.jmp_indexed_indirect(bus),
            0x7D => self.adc(bus, Mode::AbsoluteX),
            0x7E => self.modify_memory(bus, Address::AbsoluteX, Modify::RotateRight),
            0x7F => self.adc(bus, Mode::AbsoluteLongX),
            0x80 => self.branch(bus, Condition::Always),
            0x81 => self.sta(bus, Address::DirectIndexedIndirect),
            0x82 => self.brl(bus),
            0x83 => self.sta(bus, Address::StackRelative),
            0x84 => self.sty(bus, Address::Direct),
            0x85 => self.sta(bus, Address::Direct),
            0x86 => self.stx(bus, Address::Direct),
            0x87 => self.sta(bus, Address::DirectIndirectLong),
            0x88 => self.dey(bus),
            0x89 => self.bit(bus, Mode::Immediate),
            0x8A => self.txa(bus),
            0x8B => self.phb(bus),
            0x8C => self.sty(bus, Address::Absolute),
            0x8D => self.sta(bus, Address::Absolute),
            0x8E => self.stx(bus, Address::Absolute),
            0x8F => self.sta(bus, Address::AbsoluteLong),
            0x90 => self.branch(bus, Condition::CarryClear),
            0x91 => self.sta(bus, Address::DirectIndirectIndexed),
            0x92 => self.sta(bus, Address::DirectIndirect),
            0x93 => self.sta(bus, Address::StackRelativeIndirectIndexed),
            0x94 => self.sty(bus, Address::DirectX),
            0x95 => self.sta(bus, Address::DirectX),
            0x96 => self.stx(bus, Address::DirectY),
            0x97 => self.sta(bus, Address::DirectIndirectLongIndexed),
            0x98 => self.tya(bus),
            0x99 => self.sta(bus, Address::AbsoluteY),
            0x9A => self.txs(bus),
            0x9B => self.txy(bus),
            0x9C => self.stz(bus, Address::Absolute),
            0x9D => self.sta(bus, Address::AbsoluteX),
            0x9E => self.stz(bus, Address::AbsoluteX),
            0x9F => self.sta(bus, Address::AbsoluteLongX),
            0xA0 => self.ldy(bus, Mode::Immediate),
            0xA1 => self.lda(bus, Mode::DirectIndexedIndirect),
            0xA2 => self.ldx(bus, Mode::Immediate),
            0xA3 => self.lda(bus, Mode::StackRelative),
            0xA4 => self.ldy(bus, Mode::Direct),
            0xA5 => self.lda(bus, Mode::Direct),
            0xA6 => self.ldx(bus, Mode::Direct),
            0xA7 => self.lda(bus, Mode::DirectIndirectLong),
            0xA8 => self.tay(bus),
            0xA9 => self.lda(bus, Mode::Immediate),
            0xAA => self.tax(bus),
            0xAB => self.plb(bus),
            0xAC => self.ldy(bus, Mode::Absolute),
            0xAD => self.lda(bus, Mode::Absolute),
            0xAE => self.ldx(bus, Mode::Absolute),
            0xAF => self.lda(bus, Mode::AbsoluteLong),
            0xB0 => self.branch(bus, Condition::CarrySet),
            0xB1 => self.lda(bus, Mode::DirectIndirectIndexed),
            0xB2 => self.lda(bus, Mode::DirectIndirect),
            0xB3 => self.lda(bus, Mode::StackRelativeIndirectIndexed),
            0xB4 => self.ldy(bus, Mode::DirectX),
            0xB5 => self.lda(bus, Mode::DirectX),
            0xB6 => self.ldx(bus, Mode::DirectY),
            0xB7 => self.lda(bus, Mode::DirectIndirectLongIndexed),
            0xB8 => self.set_flag(bus, Flags::V, false),
            0xB9 => self.lda(bus, Mode::AbsoluteY),
            0xBA => self.tsx(bus),
            0xBB => self.tyx(bus),
            0xBC => self.ldy(bus, Mode::AbsoluteX),
            0xBD => self.lda(bus, Mode::AbsoluteX),
            0xBE => self.ldx(bus, Mode::AbsoluteY),
            0xBF => self.lda(bus, Mode::AbsoluteLongX),
            0xC0 => self.cpy(bus, Mode::Immediate),
            0xC1 => self.cmp(bus, Mode::DirectIndexedIndirect),
            0xC2 => self.rep(bus),
            0xC3 => self.cmp(bus, Mode::StackRelative),
            0xC4 => self.cpy(bus, Mode::Direct),
            0xC5 => self.cmp(bus, Mode::Direct),
            0xC6 => self.modify_memory(bus, Address::Direct, Modify::Decrement),
            0xC7 => self.cmp(bus, Mode::DirectIndirectLong),
            0xC8 => self.iny(bus),
            0xC9 => self.cmp(bus, Mode::Immediate),
            0xCA => self.dex(bus),
            0xCB => self.wai(bus),
            0xCC => self.cpy(bus, Mode::Absolute),
            0xCD => self.cmp(bus, Mode::Absolute),
            0xCE => self.modify_memory(bus, Address::Absolute, Modify::Decrement),
            0xCF => self.cmp(bus, Mode::AbsoluteLong),
            0xD0 => self.branch(bus, Condition::NotEqual),
            0xD1 => self.cmp(bus, Mode::DirectIndirectIndexed),
            0xD2 => self.cmp(bus, Mode::DirectIndirect),
            0xD3 => self.cmp(bus, Mode::StackRelativeIndirectIndexed),
            0xD4 => self.pei(bus),
            0xD5 => self.cmp(bus, Mode::DirectX),
            0xD6 => self.modify_memory(bus, Address::DirectX, Modify::Decrement),
            0xD7 => self.cmp(bus, Mode::DirectIndirectLongIndexed),
            0xD8 => self.set_flag(bus, Flags::D, false),
            0xD9 => self.cmp(bus, Mode::AbsoluteY),
            0xDA => self.phx(bus),
            0xDB => self.stp(bus),
            0xDC => self.jml_indirect(bus),
            0xDD => self.cmp(bus, Mode::AbsoluteX),
            0xDE => self.modify_memory(bus, Address::AbsoluteX, Modify::Decrement),
            0xDF => self.cmp(bus, Mode::AbsoluteLongX),
            0xE0 => self.cpx(bus, Mode::Immediate),
            0xE1 => self.sbc(bus, Mode::DirectIndexedIndirect),
            0xE2 => self.sep(bus),
            0xE3 => self.sbc(bus, Mode::StackRelative),
            0xE4 => self.cpx(bus, Mode::Direct),
            0xE5 => self.sbc(bus, Mode::Direct),
            0xE6 => self.modify_memory(bus, Address::Direct, Modify::Increment),
            0xE7 => self.sbc(bus, Mode::DirectIndirectLong),
            0xE8 => self.inx(bus),
            0xE9 => self.sbc(bus, Mode::Immediate),
            0xEA => self.nop(bus),
            0xEB => self.xba(bus),
            0xEC => self.cpx(bus, Mode::Absolute),
            0xED => self.sbc(bus, Mode::Absolute),
            0xEE => self.modify_memory(bus, Address::Absolute, Modify::Increment),
            0xEF => self.sbc(bus, Mode::AbsoluteLong),
            0xF0 => self.branch(bus, Condition::Equal),
            0xF1 => self.sbc(bus, Mode::DirectIndirectIndexed),
            0xF2 => self.sbc(bus, Mode::DirectIndirect),
            0xF3 => self.sbc(bus, Mode::StackRelativeIndirectIndexed),
            0xF4 => self.pea(bus),
            0xF5 => self.sbc(bus, Mode::DirectX),
            0xF6 => self.modify_memory(bus, Address::DirectX, Modify::Increment),
            0xF7 => self.sbc(bus, Mode::DirectIndirectLongIndexed),
            0xF8 => self.set_flag(bus, Flags::D, true),
            0xF9 => self.sbc(bus, Mode::AbsoluteY),
            0xFA => self.plx(bus),
            0xFB => self.xce(bus),
            0xFC => self.jsr_indexed_indirect(bus),
            0xFD => self.sbc(bus, Mode::AbsoluteX),
            0xFE => self.modify_memory(bus, Address::AbsoluteX, Modify::Increment),
            0xFF => self.sbc(bus, Mode::AbsoluteLongX),
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
    pub(crate) fn program_address(&self) -> Addr24 {
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
        Write(u32, Pins),
        Idle(u32, Pins),
    }

    /// A bus that returns one fixed byte, reports fixed interrupt requests and records every
    /// cycle. The byte is `DB`, STP: three cycles, the opcode fetch first, after which the
    /// processor stops.
    #[derive(Default)]
    struct TestBus {
        pending: Pending,
        cycles: Vec<Cycle>,
    }

    impl Bus65816 for TestBus {
        // @source: adr-0004
        fn read(&mut self, address: Addr24, pins: Pins) -> u8 {
            self.cycles.push(Cycle::Read(address.get(), pins));
            0xDB
        }

        // @source: adr-0004
        fn write(&mut self, address: Addr24, _value: u8, pins: Pins) {
            self.cycles.push(Cycle::Write(address.get(), pins));
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
        assert_eq!(bus.cycles[0], Cycle::Read(0x12_3456, pins));
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
        assert!(matches!(bus.cycles[..], [Cycle::Read(0x12_3456, _), ..]));
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
        assert!(matches!(bus.cycles[..], [Cycle::Read(0x00_8000, _), ..]));
    }

    // @source: adr-0004
    // @source: wdc-w65c816s#2.21
    // @source: wdc-w65c816s#7.19
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: wdc-w65c816s#table-5-3
    #[test]
    fn a_recognized_interrupt_replaces_the_instruction() {
        for (pending, vector) in [
            (
                Pending {
                    nmi: true,
                    irq: true,
                },
                0xFFEA,
            ),
            (
                Pending {
                    nmi: false,
                    irq: true,
                },
                0xFFEE,
            ),
        ] {
            let mut cpu = cpu_at(0, 0x8000);
            cpu.set_emulation(false);
            cpu.set_status(Flags::empty());
            let mut bus = TestBus {
                pending,
                cycles: Vec::new(),
            };
            cpu.step(&mut bus);
            // The eight cycles of row 22a in native mode, ending with the vector, and no
            // instruction: the processor runs on at the vector's contents, `DBDB`.
            assert_eq!(bus.cycles.len(), 8);
            assert!(matches!(bus.cycles[7], Cycle::Read(address, pins)
                if address == vector + 1 && pins.contains(Pins::VPB)));
            assert_eq!((cpu.pbr, cpu.pc, cpu.run), (0, 0xDBDB, RunState::Running));
        }
    }
}
