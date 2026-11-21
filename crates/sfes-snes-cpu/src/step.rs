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
    /// - waiting: one idle cycle; or, when an interrupt request is pending, the two internal
    ///   cycles that end the wait, then what a running processor's step would do (ADR 0022);
    /// - running with an NMI, or an IRQ and IRQs enabled: the interrupt sequence;
    /// - otherwise: one instruction, or one byte of a block move.
    ///
    /// The interrupt requests are read once, before the opcode fetch. Whether IRQs are
    /// enabled is [`irq_disabled_at_boundary`](Self::irq_disabled_at_boundary): the I flag
    /// from before the last cycle of the previous step (ADR 0022).
    // @source: adr-0004
    // @source: adr-0022
    // @source: anomie-timing#s-cpu/interrupts
    // @source: anomie-timing#s-cpu/instructions
    // @source: wdc-w65c816s#table-5-7-note-9
    pub fn step(&mut self, bus: &mut impl Bus65816) {
        let irq_disabled = self.irq_disabled_at_boundary();
        self.delayed_irq_disable = None;
        if self.run == RunState::Stopped {
            self.halted_cycle(bus);
            return;
        }
        let Pending { nmi, irq } = bus.interrupts();
        let interrupt = nmi || (irq && !irq_disabled);
        if self.run == RunState::Waiting {
            if !nmi && !irq {
                self.halted_cycle(bus);
                return;
            }
            // WAI takes two IO cycles to end (anomie-timing, note 9). An IRQ with the I flag
            // set then runs the instruction after WAI, not the handler (7.13).
            self.halted_cycle(bus);
            self.halted_cycle(bus);
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
    /// 0011). The two cycles that end a wait are the same cycle (ADR 0022).
    // @source: adr-0004
    // @source: adr-0011
    // @source: wdc-w65c816s#table-5-7-19c
    // @source: wdc-w65c816s#table-5-7-19d
    // @source: adr-0022
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
            0x01 => self.ora(bus, Mode::Memory(Address::DirectIndexedIndirect)),
            0x02 => self.cop(bus),
            0x03 => self.ora(bus, Mode::Memory(Address::StackRelative)),
            0x04 => self.tsb(bus, Address::Direct),
            0x05 => self.ora(bus, Mode::Memory(Address::Direct)),
            0x06 => self.modify_memory(bus, Address::Direct, Modify::ShiftLeft),
            0x07 => self.ora(bus, Mode::Memory(Address::DirectIndirectLong)),
            0x08 => self.php(bus),
            0x09 => self.ora(bus, Mode::Immediate),
            0x0A => self.modify_accumulator(bus, Modify::ShiftLeft),
            0x0B => self.phd(bus),
            0x0C => self.tsb(bus, Address::Absolute),
            0x0D => self.ora(bus, Mode::Memory(Address::Absolute)),
            0x0E => self.modify_memory(bus, Address::Absolute, Modify::ShiftLeft),
            0x0F => self.ora(bus, Mode::Memory(Address::AbsoluteLong)),
            0x10 => self.branch(bus, Condition::Plus),
            0x11 => self.ora(bus, Mode::Memory(Address::DirectIndirectIndexed)),
            0x12 => self.ora(bus, Mode::Memory(Address::DirectIndirect)),
            0x13 => self.ora(bus, Mode::Memory(Address::StackRelativeIndirectIndexed)),
            0x14 => self.trb(bus, Address::Direct),
            0x15 => self.ora(bus, Mode::Memory(Address::DirectX)),
            0x16 => self.modify_memory(bus, Address::DirectX, Modify::ShiftLeft),
            0x17 => self.ora(bus, Mode::Memory(Address::DirectIndirectLongIndexed)),
            0x18 => self.set_flag(bus, Flags::C, false),
            0x19 => self.ora(bus, Mode::Memory(Address::AbsoluteY)),
            0x1A => self.modify_accumulator(bus, Modify::Increment),
            0x1B => self.tcs(bus),
            0x1C => self.trb(bus, Address::Absolute),
            0x1D => self.ora(bus, Mode::Memory(Address::AbsoluteX)),
            0x1E => self.modify_memory(bus, Address::AbsoluteX, Modify::ShiftLeft),
            0x1F => self.ora(bus, Mode::Memory(Address::AbsoluteLongX)),
            0x20 => self.jsr_absolute(bus),
            0x21 => self.and(bus, Mode::Memory(Address::DirectIndexedIndirect)),
            0x22 => self.jsl(bus),
            0x23 => self.and(bus, Mode::Memory(Address::StackRelative)),
            0x24 => self.bit(bus, Mode::Memory(Address::Direct)),
            0x25 => self.and(bus, Mode::Memory(Address::Direct)),
            0x26 => self.modify_memory(bus, Address::Direct, Modify::RotateLeft),
            0x27 => self.and(bus, Mode::Memory(Address::DirectIndirectLong)),
            0x28 => self.plp(bus),
            0x29 => self.and(bus, Mode::Immediate),
            0x2A => self.modify_accumulator(bus, Modify::RotateLeft),
            0x2B => self.pld(bus),
            0x2C => self.bit(bus, Mode::Memory(Address::Absolute)),
            0x2D => self.and(bus, Mode::Memory(Address::Absolute)),
            0x2E => self.modify_memory(bus, Address::Absolute, Modify::RotateLeft),
            0x2F => self.and(bus, Mode::Memory(Address::AbsoluteLong)),
            0x30 => self.branch(bus, Condition::Minus),
            0x31 => self.and(bus, Mode::Memory(Address::DirectIndirectIndexed)),
            0x32 => self.and(bus, Mode::Memory(Address::DirectIndirect)),
            0x33 => self.and(bus, Mode::Memory(Address::StackRelativeIndirectIndexed)),
            0x34 => self.bit(bus, Mode::Memory(Address::DirectX)),
            0x35 => self.and(bus, Mode::Memory(Address::DirectX)),
            0x36 => self.modify_memory(bus, Address::DirectX, Modify::RotateLeft),
            0x37 => self.and(bus, Mode::Memory(Address::DirectIndirectLongIndexed)),
            0x38 => self.set_flag(bus, Flags::C, true),
            0x39 => self.and(bus, Mode::Memory(Address::AbsoluteY)),
            0x3A => self.modify_accumulator(bus, Modify::Decrement),
            0x3B => self.tsc(bus),
            0x3C => self.bit(bus, Mode::Memory(Address::AbsoluteX)),
            0x3D => self.and(bus, Mode::Memory(Address::AbsoluteX)),
            0x3E => self.modify_memory(bus, Address::AbsoluteX, Modify::RotateLeft),
            0x3F => self.and(bus, Mode::Memory(Address::AbsoluteLongX)),
            0x40 => self.rti(bus),
            0x41 => self.eor(bus, Mode::Memory(Address::DirectIndexedIndirect)),
            0x42 => self.wdm(bus),
            0x43 => self.eor(bus, Mode::Memory(Address::StackRelative)),
            0x44 => self.mvp(bus),
            0x45 => self.eor(bus, Mode::Memory(Address::Direct)),
            0x46 => self.modify_memory(bus, Address::Direct, Modify::ShiftRight),
            0x47 => self.eor(bus, Mode::Memory(Address::DirectIndirectLong)),
            0x48 => self.pha(bus),
            0x49 => self.eor(bus, Mode::Immediate),
            0x4A => self.modify_accumulator(bus, Modify::ShiftRight),
            0x4B => self.phk(bus),
            0x4C => self.jmp_absolute(bus),
            0x4D => self.eor(bus, Mode::Memory(Address::Absolute)),
            0x4E => self.modify_memory(bus, Address::Absolute, Modify::ShiftRight),
            0x4F => self.eor(bus, Mode::Memory(Address::AbsoluteLong)),
            0x50 => self.branch(bus, Condition::OverflowClear),
            0x51 => self.eor(bus, Mode::Memory(Address::DirectIndirectIndexed)),
            0x52 => self.eor(bus, Mode::Memory(Address::DirectIndirect)),
            0x53 => self.eor(bus, Mode::Memory(Address::StackRelativeIndirectIndexed)),
            0x54 => self.mvn(bus),
            0x55 => self.eor(bus, Mode::Memory(Address::DirectX)),
            0x56 => self.modify_memory(bus, Address::DirectX, Modify::ShiftRight),
            0x57 => self.eor(bus, Mode::Memory(Address::DirectIndirectLongIndexed)),
            0x58 => self.set_flag(bus, Flags::I, false),
            0x59 => self.eor(bus, Mode::Memory(Address::AbsoluteY)),
            0x5A => self.phy(bus),
            0x5B => self.tcd(bus),
            0x5C => self.jmp_long(bus),
            0x5D => self.eor(bus, Mode::Memory(Address::AbsoluteX)),
            0x5E => self.modify_memory(bus, Address::AbsoluteX, Modify::ShiftRight),
            0x5F => self.eor(bus, Mode::Memory(Address::AbsoluteLongX)),
            0x60 => self.rts(bus),
            0x61 => self.adc(bus, Mode::Memory(Address::DirectIndexedIndirect)),
            0x62 => self.per(bus),
            0x63 => self.adc(bus, Mode::Memory(Address::StackRelative)),
            0x64 => self.stz(bus, Address::Direct),
            0x65 => self.adc(bus, Mode::Memory(Address::Direct)),
            0x66 => self.modify_memory(bus, Address::Direct, Modify::RotateRight),
            0x67 => self.adc(bus, Mode::Memory(Address::DirectIndirectLong)),
            0x68 => self.pla(bus),
            0x69 => self.adc(bus, Mode::Immediate),
            0x6A => self.modify_accumulator(bus, Modify::RotateRight),
            0x6B => self.rtl(bus),
            0x6C => self.jmp_indirect(bus),
            0x6D => self.adc(bus, Mode::Memory(Address::Absolute)),
            0x6E => self.modify_memory(bus, Address::Absolute, Modify::RotateRight),
            0x6F => self.adc(bus, Mode::Memory(Address::AbsoluteLong)),
            0x70 => self.branch(bus, Condition::OverflowSet),
            0x71 => self.adc(bus, Mode::Memory(Address::DirectIndirectIndexed)),
            0x72 => self.adc(bus, Mode::Memory(Address::DirectIndirect)),
            0x73 => self.adc(bus, Mode::Memory(Address::StackRelativeIndirectIndexed)),
            0x74 => self.stz(bus, Address::DirectX),
            0x75 => self.adc(bus, Mode::Memory(Address::DirectX)),
            0x76 => self.modify_memory(bus, Address::DirectX, Modify::RotateRight),
            0x77 => self.adc(bus, Mode::Memory(Address::DirectIndirectLongIndexed)),
            0x78 => self.set_flag(bus, Flags::I, true),
            0x79 => self.adc(bus, Mode::Memory(Address::AbsoluteY)),
            0x7A => self.ply(bus),
            0x7B => self.tdc(bus),
            0x7C => self.jmp_indexed_indirect(bus),
            0x7D => self.adc(bus, Mode::Memory(Address::AbsoluteX)),
            0x7E => self.modify_memory(bus, Address::AbsoluteX, Modify::RotateRight),
            0x7F => self.adc(bus, Mode::Memory(Address::AbsoluteLongX)),
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
            0xA1 => self.lda(bus, Mode::Memory(Address::DirectIndexedIndirect)),
            0xA2 => self.ldx(bus, Mode::Immediate),
            0xA3 => self.lda(bus, Mode::Memory(Address::StackRelative)),
            0xA4 => self.ldy(bus, Mode::Memory(Address::Direct)),
            0xA5 => self.lda(bus, Mode::Memory(Address::Direct)),
            0xA6 => self.ldx(bus, Mode::Memory(Address::Direct)),
            0xA7 => self.lda(bus, Mode::Memory(Address::DirectIndirectLong)),
            0xA8 => self.tay(bus),
            0xA9 => self.lda(bus, Mode::Immediate),
            0xAA => self.tax(bus),
            0xAB => self.plb(bus),
            0xAC => self.ldy(bus, Mode::Memory(Address::Absolute)),
            0xAD => self.lda(bus, Mode::Memory(Address::Absolute)),
            0xAE => self.ldx(bus, Mode::Memory(Address::Absolute)),
            0xAF => self.lda(bus, Mode::Memory(Address::AbsoluteLong)),
            0xB0 => self.branch(bus, Condition::CarrySet),
            0xB1 => self.lda(bus, Mode::Memory(Address::DirectIndirectIndexed)),
            0xB2 => self.lda(bus, Mode::Memory(Address::DirectIndirect)),
            0xB3 => self.lda(bus, Mode::Memory(Address::StackRelativeIndirectIndexed)),
            0xB4 => self.ldy(bus, Mode::Memory(Address::DirectX)),
            0xB5 => self.lda(bus, Mode::Memory(Address::DirectX)),
            0xB6 => self.ldx(bus, Mode::Memory(Address::DirectY)),
            0xB7 => self.lda(bus, Mode::Memory(Address::DirectIndirectLongIndexed)),
            0xB8 => self.set_flag(bus, Flags::V, false),
            0xB9 => self.lda(bus, Mode::Memory(Address::AbsoluteY)),
            0xBA => self.tsx(bus),
            0xBB => self.tyx(bus),
            0xBC => self.ldy(bus, Mode::Memory(Address::AbsoluteX)),
            0xBD => self.lda(bus, Mode::Memory(Address::AbsoluteX)),
            0xBE => self.ldx(bus, Mode::Memory(Address::AbsoluteY)),
            0xBF => self.lda(bus, Mode::Memory(Address::AbsoluteLongX)),
            0xC0 => self.cpy(bus, Mode::Immediate),
            0xC1 => self.cmp(bus, Mode::Memory(Address::DirectIndexedIndirect)),
            0xC2 => self.rep(bus),
            0xC3 => self.cmp(bus, Mode::Memory(Address::StackRelative)),
            0xC4 => self.cpy(bus, Mode::Memory(Address::Direct)),
            0xC5 => self.cmp(bus, Mode::Memory(Address::Direct)),
            0xC6 => self.modify_memory(bus, Address::Direct, Modify::Decrement),
            0xC7 => self.cmp(bus, Mode::Memory(Address::DirectIndirectLong)),
            0xC8 => self.iny(bus),
            0xC9 => self.cmp(bus, Mode::Immediate),
            0xCA => self.dex(bus),
            0xCB => self.wai(bus),
            0xCC => self.cpy(bus, Mode::Memory(Address::Absolute)),
            0xCD => self.cmp(bus, Mode::Memory(Address::Absolute)),
            0xCE => self.modify_memory(bus, Address::Absolute, Modify::Decrement),
            0xCF => self.cmp(bus, Mode::Memory(Address::AbsoluteLong)),
            0xD0 => self.branch(bus, Condition::NotEqual),
            0xD1 => self.cmp(bus, Mode::Memory(Address::DirectIndirectIndexed)),
            0xD2 => self.cmp(bus, Mode::Memory(Address::DirectIndirect)),
            0xD3 => self.cmp(bus, Mode::Memory(Address::StackRelativeIndirectIndexed)),
            0xD4 => self.pei(bus),
            0xD5 => self.cmp(bus, Mode::Memory(Address::DirectX)),
            0xD6 => self.modify_memory(bus, Address::DirectX, Modify::Decrement),
            0xD7 => self.cmp(bus, Mode::Memory(Address::DirectIndirectLongIndexed)),
            0xD8 => self.set_flag(bus, Flags::D, false),
            0xD9 => self.cmp(bus, Mode::Memory(Address::AbsoluteY)),
            0xDA => self.phx(bus),
            0xDB => self.stp(bus),
            0xDC => self.jml_indirect(bus),
            0xDD => self.cmp(bus, Mode::Memory(Address::AbsoluteX)),
            0xDE => self.modify_memory(bus, Address::AbsoluteX, Modify::Decrement),
            0xDF => self.cmp(bus, Mode::Memory(Address::AbsoluteLongX)),
            0xE0 => self.cpx(bus, Mode::Immediate),
            0xE1 => self.sbc(bus, Mode::Memory(Address::DirectIndexedIndirect)),
            0xE2 => self.sep(bus),
            0xE3 => self.sbc(bus, Mode::Memory(Address::StackRelative)),
            0xE4 => self.cpx(bus, Mode::Memory(Address::Direct)),
            0xE5 => self.sbc(bus, Mode::Memory(Address::Direct)),
            0xE6 => self.modify_memory(bus, Address::Direct, Modify::Increment),
            0xE7 => self.sbc(bus, Mode::Memory(Address::DirectIndirectLong)),
            0xE8 => self.inx(bus),
            0xE9 => self.sbc(bus, Mode::Immediate),
            0xEA => self.nop(bus),
            0xEB => self.xba(bus),
            0xEC => self.cpx(bus, Mode::Memory(Address::Absolute)),
            0xED => self.sbc(bus, Mode::Memory(Address::Absolute)),
            0xEE => self.modify_memory(bus, Address::Absolute, Modify::Increment),
            0xEF => self.sbc(bus, Mode::Memory(Address::AbsoluteLong)),
            0xF0 => self.branch(bus, Condition::Equal),
            0xF1 => self.sbc(bus, Mode::Memory(Address::DirectIndirectIndexed)),
            0xF2 => self.sbc(bus, Mode::Memory(Address::DirectIndirect)),
            0xF3 => self.sbc(bus, Mode::Memory(Address::StackRelativeIndirectIndexed)),
            0xF4 => self.pea(bus),
            0xF5 => self.sbc(bus, Mode::Memory(Address::DirectX)),
            0xF6 => self.modify_memory(bus, Address::DirectX, Modify::Increment),
            0xF7 => self.sbc(bus, Mode::Memory(Address::DirectIndirectLongIndexed)),
            0xF8 => self.set_flag(bus, Flags::D, true),
            0xF9 => self.sbc(bus, Mode::Memory(Address::AbsoluteY)),
            0xFA => self.plx(bus),
            0xFB => self.xce(bus),
            0xFC => self.jsr_indexed_indirect(bus),
            0xFD => self.sbc(bus, Mode::Memory(Address::AbsoluteX)),
            0xFE => self.modify_memory(bus, Address::AbsoluteX, Modify::Increment),
            0xFF => self.sbc(bus, Mode::Memory(Address::AbsoluteLongX)),
        }
    }

    /// Cycle 1 of every instruction: reads the opcode at PBR,PC with VDA and VPA both
    /// active, and increments the program counter within the program bank.
    // @source: wdc-w65c816s#table-5-7
    // @source: wdc-w65c816s#2.10
    fn fetch_opcode(&mut self, bus: &mut impl Bus65816) -> u8 {
        let opcode = self.read_opcode(bus);
        self.pc = self.pc.wrapping_add(1);
        opcode
    }

    /// An opcode-fetch cycle at PBR,PC, with VDA and VPA both active, that leaves the
    /// program counter alone. A hardware interrupt and reset spend one and discard the byte
    /// (row 22a).
    // @source: wdc-w65c816s#table-5-7
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: wdc-w65c816s#2.26
    pub(crate) fn read_opcode(&self, bus: &mut impl Bus65816) -> u8 {
        let pins = Pins::VDA | Pins::VPA | Pins::RWB | self.status_pins();
        bus.read(self.program_address(), pins)
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
    use crate::testing::{Cycle, TestBus};

    /// STP: the opcode each test places at the program counter, so that a step that runs an
    /// instruction ends with the processor stopped after three cycles.
    const STP: u8 = 0xDB;

    /// A reset-state processor with its program counter at `pbr`,`pc`.
    // @source: wdc-w65c816s#2.9
    fn cpu_at(pbr: u8, pc: u16) -> Cpu65816 {
        let mut cpu = Cpu65816::new();
        cpu.pbr = pbr;
        cpu.pc = pc;
        cpu
    }

    /// A bus that reports `pending` and holds STP at `cpu`'s program address.
    // @source: adr-0004
    fn bus_for(cpu: &Cpu65816, pending: Pending) -> TestBus {
        let mut bus = TestBus::default();
        bus.pending = pending;
        bus.store(cpu.program_address().get(), &[STP]);
        bus
    }

    // @source: wdc-w65c816s#table-5-7
    // @source: wdc-w65c816s#2.26
    #[test]
    fn the_opcode_fetch_reads_pbr_pc_with_vda_and_vpa() {
        let mut cpu = cpu_at(0x12, 0x3456);
        let mut bus = bus_for(&cpu, Pending::default());
        cpu.step(&mut bus);
        let pins = Pins::VDA | Pins::VPA | Pins::RWB | Pins::E | Pins::M | Pins::X;
        assert_eq!(
            (bus.cycles[0], bus.pins[0]),
            (Cycle::Opcode(0x12_3456), pins)
        );
        assert_eq!((cpu.pbr, cpu.pc), (0x12, 0x3457));
    }

    // @source: wdc-w65c816s#2.10
    #[test]
    fn the_program_counter_wraps_within_the_program_bank() {
        let mut cpu = cpu_at(0x12, 0xFFFF);
        let mut bus = bus_for(&cpu, Pending::default());
        cpu.step(&mut bus);
        assert_eq!((cpu.pbr, cpu.pc), (0x12, 0x0000));
    }

    // @source: adr-0004
    // @source: wdc-w65c816s#7.14
    #[test]
    fn a_stopped_processor_idles_and_ignores_interrupts() {
        let mut cpu = cpu_at(0x12, 0x3456);
        cpu.run = RunState::Stopped;
        let pending = Pending {
            nmi: true,
            irq: true,
        };
        let mut bus = bus_for(&cpu, pending);
        let before = cpu.clone();
        cpu.step(&mut bus);
        assert_eq!(bus.cycles, [Cycle::Internal(0x12_3456)]);
        assert_eq!(bus.pins, [Pins::RWB | Pins::E | Pins::M | Pins::X]);
        assert_eq!(cpu, before);
    }

    // @source: adr-0004
    // @source: wdc-w65c816s#7.13
    #[test]
    fn a_waiting_processor_idles_until_a_request_arrives() {
        let mut cpu = cpu_at(0x12, 0x3456);
        cpu.run = RunState::Waiting;
        let mut bus = bus_for(&cpu, Pending::default());
        let before = cpu.clone();
        cpu.step(&mut bus);
        assert_eq!(bus.cycles, [Cycle::Internal(0x12_3456)]);
        assert_eq!(cpu, before);
    }

    // @source: wdc-w65c816s#7.13
    // @source: anomie-timing#s-cpu/instructions
    // @source: adr-0022
    #[test]
    fn an_irq_with_the_i_flag_set_resumes_after_wai_without_the_handler() {
        let mut cpu = cpu_at(0x12, 0x3456);
        assert!(cpu.p.contains(Flags::I));
        cpu.run = RunState::Waiting;
        let pending = Pending {
            nmi: false,
            irq: true,
        };
        let mut bus = bus_for(&cpu, pending);
        cpu.step(&mut bus);
        // Two internal cycles end the wait (ADR 0022), then the next instruction's opcode is
        // fetched: no interrupt sequence ran.
        assert_eq!(
            bus.cycles[..3],
            [
                Cycle::Internal(0x12_3456),
                Cycle::Internal(0x12_3456),
                Cycle::Opcode(0x12_3456),
            ]
        );
        assert_eq!(cpu.pc, 0x3457);
    }

    // @source: adr-0004
    // @source: wdc-w65c816s#2.18
    #[test]
    fn a_masked_irq_does_not_interrupt_a_running_processor() {
        let mut cpu = cpu_at(0, 0x8000);
        let pending = Pending {
            nmi: false,
            irq: true,
        };
        let mut bus = bus_for(&cpu, pending);
        cpu.step(&mut bus);
        assert_eq!(bus.cycles[0], Cycle::Opcode(0x00_8000));
    }

    // @source: adr-0004
    // @source: wdc-w65c816s#2.21
    // @source: wdc-w65c816s#7.19
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: wdc-w65c816s#table-5-3
    #[test]
    fn a_recognized_interrupt_replaces_the_instruction() {
        let both = Pending {
            nmi: true,
            irq: true,
        };
        let irq = Pending {
            nmi: false,
            irq: true,
        };
        for (pending, vector) in [(both, 0x00_FFEA), (irq, 0x00_FFEE)] {
            let mut cpu = cpu_at(0, 0x8000);
            cpu.set_emulation(false);
            cpu.set_status(Flags::empty());
            let mut bus = bus_for(&cpu, pending);
            bus.store(vector, &[STP, STP]);
            cpu.step(&mut bus);
            // The eight cycles of row 22a in native mode, ending with the vector, and no
            // instruction: the processor runs on at the vector's contents, `DBDB`.
            assert_eq!(bus.cycles.len(), 8);
            assert_eq!(bus.cycles[7], Cycle::Data(vector + 1));
            assert!(bus.pins[7].contains(Pins::VPB));
            assert_eq!((cpu.pbr, cpu.pc, cpu.run), (0, 0xDBDB, RunState::Running));
        }
    }

    /// Bytes from a fixed xorshift generator, so a test's memory is arbitrary but the same on
    /// every run.
    // @source: adr-0004
    fn arbitrary_bytes(seed: u64, count: usize) -> Vec<u8> {
        let mut state = seed;
        (0..count)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                state.to_le_bytes()[0]
            })
            .collect::<Vec<_>>()
    }

    /// Runs `steps` steps on `bus`, resetting the processor whenever it halts so that it
    /// keeps executing.
    // @source: adr-0004
    // @source: adr-0011
    fn drive(cpu: &mut Cpu65816, bus: &mut TestBus, steps: usize) {
        for _ in 0..steps {
            if cpu.run == RunState::Running {
                cpu.step(bus);
            } else {
                cpu.reset(bus);
            }
        }
    }

    /// A bus whose banks `00`, `12` and `7E` hold arbitrary bytes from `seed`.
    // @source: adr-0004
    fn arbitrary_bus(seed: u64) -> TestBus {
        let mut bus = TestBus::default();
        for bank in [0x00_u8, 0x12, 0x7E] {
            let bytes = arbitrary_bytes(seed ^ u64::from(bank), 0x1_0000);
            bus.store(u32::from(bank) << 16, &bytes);
        }
        bus
    }

    /// A clone is the snapshot (FND-01): a cloned processor stepped on an identical bus makes
    /// the same bus calls, with the same addresses, signals and written values, and ends in
    /// the same state as the original. A step's only inputs are the processor's fields and
    /// the bus (ADR 0004).
    // @source: adr-0004
    #[test]
    fn a_clone_runs_exactly_like_its_original() {
        // Compiles only if the processor is `Clone + Send`.
        // @source: adr-0004
        const fn clone_and_send<T: Clone + Send>() {}
        clone_and_send::<Cpu65816>();

        for (seed, emulation) in [(0x5EED_0001, true), (0x5EED_0002, false)] {
            // Run a while first, so the clone is taken from an arbitrary state.
            let mut original = cpu_at(0x12, 0x8000);
            original.set_emulation(emulation);
            drive(&mut original, &mut arbitrary_bus(seed), 1_000);

            let mut clone = original.clone();
            let mut original_bus = arbitrary_bus(seed.rotate_left(32));
            let mut clone_bus = arbitrary_bus(seed.rotate_left(32));
            drive(&mut original, &mut original_bus, 20_000);
            drive(&mut clone, &mut clone_bus, 20_000);
            assert_eq!(clone_bus.cycles, original_bus.cycles);
            assert_eq!(clone_bus.pins, original_bus.pins);
            assert_eq!(clone, original);
        }
    }

    /// The IRQ handler of the anomie-timing examples, entered through the native IRQ vector.
    const HANDLER: u32 = 0x00_9000;

    /// Whether the step just run entered the IRQ handler.
    // @source: wdc-w65c816s#table-5-3
    fn in_handler(cpu: &Cpu65816) -> bool {
        cpu.program_address().get() == HANDLER
    }

    /// Runs one of anomie-timing's IRQ examples: `program` at `12:8000` in native mode with
    /// 8-bit registers and I clear, `memory` stored, and the handler `STA $00; RTI` at
    /// [`HANDLER`]. The example's "set up IRQ" is an IRQ source whose line goes low once the
    /// processor waits and is acknowledged when its handler is entered, as reading `$4211`
    /// would. Runs until STP, and returns the bus and the number of IRQs taken.
    // @source: anomie-timing#s-cpu/interrupts
    // @source: wdc-w65c816s#table-5-3
    fn run_irq_example(
        program: &[u8],
        memory: &[(u32, &[u8])],
        stack_pointer: u16,
    ) -> (TestBus, usize) {
        let mut cpu = cpu_at(0x12, 0x8000);
        cpu.set_emulation(false);
        cpu.set_status(Flags::M | Flags::X);
        cpu.s = stack_pointer;
        let mut bus = TestBus::default();
        bus.store(0x12_8000, program);
        bus.store(0x00_FFEE, &[0x00, 0x90]);
        bus.store(HANDLER, &[0x85, 0x00, 0x40]);
        for (address, bytes) in memory {
            bus.store(*address, bytes);
        }
        let mut taken = 0;
        for _ in 0..100 {
            if cpu.run == RunState::Stopped {
                break;
            }
            if cpu.run == RunState::Waiting {
                bus.pending.irq = true;
            }
            cpu.step(&mut bus);
            if in_handler(&cpu) {
                bus.pending.irq = false;
                taken += 1;
            }
        }
        assert_eq!(cpu.run, RunState::Stopped, "the example ends at its STP");
        (bus, taken)
    }

    /// `SEI; WAI; STZ $00; LDA #$01; CLI; LDA #$42; STP`: the IRQ is taken after the
    /// `LDA #$42`, not after CLI, so `$00` ends as `42`.
    // @source: anomie-timing#s-cpu/interrupts
    // @source: adr-0022
    #[test]
    fn an_irq_waits_one_instruction_after_cli() {
        let program = [0x78, 0xCB, 0x64, 0x00, 0xA9, 0x01, 0x58, 0xA9, 0x42, STP];
        let (bus, taken) = run_irq_example(&program, &[], 0x01FF);
        assert_eq!((bus.byte(0x00_0000), taken), (0x42, 1));
    }

    /// `SEI; WAI; CLI; SEI`: the IRQ is taken after the second SEI, and the P it pushes has
    /// the I flag set.
    // @source: anomie-timing#s-cpu/interrupts
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: adr-0022
    #[test]
    fn an_irq_after_cli_and_sei_is_taken_after_the_sei() {
        let program = [0x78, 0xCB, 0x58, 0x78, STP];
        let (bus, taken) = run_irq_example(&program, &[], 0x01FF);
        assert_eq!(taken, 1);
        // Native mode pushes PBR, PCH, PCL and P from 01FF down: the return address is the
        // STP after the second SEI.
        assert_eq!(
            [bus.byte(0x01FF), bus.byte(0x01FE), bus.byte(0x01FD)],
            [0x12, 0x80, 0x04]
        );
        assert!(Flags::from_bits_retain(bus.byte(0x01FC)).contains(Flags::I));
    }

    /// `SEI; WAI; CLI; RTI`, with an RTI that sets I: no IRQ is taken at all.
    // @source: anomie-timing#s-cpu/interrupts
    // @source: wdc-w65c816s#table-5-7-22g
    // @source: adr-0022
    #[test]
    fn an_rti_that_sets_i_right_after_cli_allows_no_irq() {
        let program = [0x78, 0xCB, 0x58, 0x40];
        // RTI's frame: P with I set, then 12:8010, where an STP waits.
        let frame: &[u8] = &[0x34, 0x10, 0x80, 0x12];
        let (_, taken) = run_irq_example(&program, &[(0x01FC, frame), (0x12_8010, &[STP])], 0x01FB);
        assert_eq!(taken, 0);
    }

    /// `SEI; WAI; STZ $00; LDA #$01; RTI`, with an RTI that clears I and returns to
    /// `LDA #$42; STP`: RTI's new I flag is seen at once, so `$00` ends as `01`.
    // @source: anomie-timing#s-cpu/interrupts
    // @source: wdc-w65c816s#table-5-7-22g
    // @source: adr-0022
    #[test]
    fn an_rti_that_clears_i_lets_the_irq_in_at_once() {
        let program = [0x78, 0xCB, 0x64, 0x00, 0xA9, 0x01, 0x40, 0xA9, 0x42, STP];
        // RTI's frame: P with I clear, then 12:8007, the LDA #$42.
        let frame: &[u8] = &[0x30, 0x07, 0x80, 0x12];
        let (bus, taken) = run_irq_example(&program, &[(0x01FC, frame)], 0x01FB);
        assert_eq!((bus.byte(0x00_0000), taken), (0x01, 1));
    }

    /// A processor in native mode with 8-bit registers and `status` added, at `12:8000`, on a
    /// bus holding `instruction` there followed by NOPs, P byte `pulled` at `01FF` for PLP,
    /// and the IRQ vector pointing at [`HANDLER`].
    // @source: wdc-w65c816s#table-5-3
    fn flag_change(instruction: &[u8], status: Flags, pulled: u8) -> (Cpu65816, TestBus) {
        let mut cpu = cpu_at(0x12, 0x8000);
        cpu.set_emulation(false);
        cpu.set_status(Flags::M | Flags::X | status);
        cpu.s = 0x01FE;
        let mut bus = TestBus::default();
        bus.store(0x12_8000, instruction);
        bus.store(0x12_8002, &[0xEA, 0xEA]);
        if instruction.len() == 1 {
            bus.store(0x12_8001, &[0xEA]);
        }
        bus.store(0x00_01FF, &[pulled]);
        bus.store(0x00_FFEE, &[0x00, 0x90]);
        bus.store(HANDLER, &[0xEA]);
        (cpu, bus)
    }

    /// CLI, REP #$04 and PLP clear I in their final cycle: with an IRQ pending, one more
    /// instruction runs before the IRQ is taken.
    // @source: anomie-timing#s-cpu/interrupts
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-w65c816s#table-5-7-18
    // @source: wdc-w65c816s#table-5-7-22b
    // @source: adr-0022
    #[test]
    fn clearing_i_in_the_final_cycle_lets_one_more_instruction_run() {
        let cases: [(&[u8], u16); 3] = [
            (&[0x58], 0x8002),
            (&[0xC2, 0x04], 0x8003),
            (&[0x28], 0x8002),
        ];
        for (instruction, after_nop) in cases {
            let (mut cpu, mut bus) = flag_change(instruction, Flags::I, 0x30);
            bus.pending.irq = true;
            cpu.step(&mut bus);
            assert!(!cpu.p.contains(Flags::I), "{instruction:02X?} clears I");
            assert!(cpu.irq_disabled_at_boundary(), "{instruction:02X?}");
            cpu.step(&mut bus);
            assert_eq!(cpu.pc, after_nop, "{instruction:02X?}: the NOP ran");
            cpu.step(&mut bus);
            assert!(in_handler(&cpu), "{instruction:02X?}: then the IRQ");
        }
    }

    /// SEI, SEP #$04 and PLP set I in their final cycle: an IRQ that is pending at the next
    /// boundary is still taken, and the P it pushes has I set.
    // @source: anomie-timing#s-cpu/interrupts
    // @source: wdc-w65c816s#table-5-7-19a
    // @source: wdc-w65c816s#table-5-7-18
    // @source: wdc-w65c816s#table-5-7-22b
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: adr-0022
    #[test]
    fn setting_i_in_the_final_cycle_still_takes_a_pending_irq() {
        let cases: [&[u8]; 3] = [&[0x78], &[0xE2, 0x04], &[0x28]];
        for instruction in cases {
            let (mut cpu, mut bus) = flag_change(instruction, Flags::empty(), 0x34);
            cpu.step(&mut bus);
            assert!(cpu.p.contains(Flags::I), "{instruction:02X?} sets I");
            bus.pending.irq = true;
            bus.cycles.clear();
            cpu.step(&mut bus);
            assert!(in_handler(&cpu), "{instruction:02X?}: the IRQ is taken");
            let pushed = bus
                .cycles
                .iter()
                .filter_map(|cycle| match cycle {
                    Cycle::Write(_, value) => Some(*value),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(pushed.len(), 4, "{instruction:02X?}: PBR, PCH, PCL and P");
            assert!(
                Flags::from_bits_retain(pushed[3]).contains(Flags::I),
                "{instruction:02X?}: the pushed P has I set"
            );
        }
    }

    /// BRK and COP set I before their final cycle: an IRQ pending at the end of either is
    /// masked at once, and the handler's first instruction runs.
    // @source: anomie-timing#s-cpu/interrupts
    // @source: wdc-w65c816s#table-5-7-22j
    // @source: wdc-w65c816s#table-5-3
    // @source: adr-0022
    #[test]
    fn brk_and_cop_mask_an_irq_at_the_next_boundary() {
        for (opcode, vector) in [(0x00, 0x00_FFE6), (0x02, 0x00_FFE4)] {
            let (mut cpu, mut bus) = flag_change(&[opcode, 0x00], Flags::empty(), 0x30);
            bus.store(vector, &[0x00, 0xA0]);
            bus.store(0x00_A000, &[0xEA]);
            cpu.step(&mut bus);
            assert!(cpu.irq_disabled_at_boundary(), "{opcode:02X}");
            bus.pending.irq = true;
            bus.cycles.clear();
            cpu.step(&mut bus);
            assert_eq!(bus.cycles[0], Cycle::Opcode(0x00_A000), "{opcode:02X}");
            assert_eq!((cpu.pbr, cpu.pc), (0x00, 0xA001), "{opcode:02X}");
        }
    }

    /// An NMI ends a wait two internal cycles later, then enters its handler, whatever the I
    /// flag.
    // @source: anomie-timing#s-cpu/instructions
    // @source: wdc-w65c816s#table-5-7-note-9
    // @source: wdc-w65c816s#table-5-7-22a
    // @source: adr-0022
    #[test]
    fn an_nmi_ends_a_wait_two_internal_cycles_later() {
        let mut cpu = cpu_at(0x12, 0x3456);
        cpu.run = RunState::Waiting;
        let pending = Pending {
            nmi: true,
            irq: false,
        };
        let mut bus = bus_for(&cpu, pending);
        cpu.step(&mut bus);
        assert_eq!(
            bus.cycles[..3],
            [
                Cycle::Internal(0x12_3456),
                Cycle::Internal(0x12_3456),
                Cycle::Opcode(0x12_3456),
            ]
        );
        // Emulation mode: two cycles, then row 22a's seven.
        assert_eq!(bus.cycles.len(), 9);
        assert_eq!(bus.cycles[8], Cycle::Data(0x00_FFFB));
    }

    /// Every step clears the delayed I flag, so it applies to one boundary only, and reset
    /// clears it too.
    // @source: adr-0022
    // @source: wdc-w65c816s#2.25
    #[test]
    fn the_delayed_i_flag_lasts_one_boundary() {
        let (mut cpu, mut bus) = flag_change(&[0x58], Flags::I, 0x30);
        cpu.step(&mut bus);
        assert_eq!(cpu.delayed_irq_disable, Some(true));
        cpu.step(&mut bus);
        assert_eq!(cpu.delayed_irq_disable, None);
        cpu.delay_irq_disable();
        cpu.reset(&mut bus);
        assert_eq!(cpu.delayed_irq_disable, None);
    }
}
