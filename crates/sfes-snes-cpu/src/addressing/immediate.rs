// @source: wdc-w65c816s#3.5.18
//! Immediate addressing: the operand is in the instruction itself.

use crate::{bus::Bus65816, registers::Cpu65816, width::Width};

impl Cpu65816 {
    /// Row 18: cycle 2 fetches "IDL" at PBR,PC+1, and cycle 2a fetches "IDH" at PBR,PC+2
    /// when the operand is 16 bits wide (note 1). Both are program-address cycles.
    // @source: wdc-w65c816s#table-5-7-18
    // @source: wdc-w65c816s#table-5-7-note-1
    pub(super) fn immediate<W: Width>(&mut self, bus: &mut impl Bus65816) -> W {
        let low = self.fetch(bus);
        W::from_bytes(low, || self.fetch(bus))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        registers::Flags,
        testing::{Cycle, native, run},
    };

    // @source: wdc-w65c816s#table-5-7-18
    #[test]
    fn an_eight_bit_operand_is_one_program_byte() {
        let mut cpu = native(Flags::M);
        let bus = run(&mut cpu, &[0xA9, 0x34, 0x12], &[]);
        assert_eq!(
            bus.cycles,
            [Cycle::Opcode(0x12_8000), Cycle::Program(0x12_8001)]
        );
        assert_eq!((cpu.a, cpu.pc), (0x0034, 0x8002));
    }

    // @source: wdc-w65c816s#table-5-7-18
    // @source: wdc-w65c816s#table-5-7-note-1
    #[test]
    fn a_sixteen_bit_operand_adds_a_byte_and_a_cycle() {
        let mut cpu = native(Flags::empty());
        let bus = run(&mut cpu, &[0xA9, 0x34, 0x12], &[]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Program(0x12_8002),
            ]
        );
        assert_eq!((cpu.a, cpu.pc), (0x1234, 0x8003));
    }

    // @source: wdc-w65c816s#2.10
    // @source: 6502org-65c816-opcodes#5.1.2
    #[test]
    fn the_operand_wraps_within_the_program_bank() {
        let mut cpu = native(Flags::empty());
        cpu.pc = 0xFFFE;
        let bus = run(&mut cpu, &[0xA9, 0x34], &[(0x12_0000, &[0x12])]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_FFFE),
                Cycle::Program(0x12_FFFF),
                Cycle::Program(0x12_0000),
            ]
        );
        assert_eq!((cpu.a, cpu.pbr, cpu.pc), (0x1234, 0x12, 0x0001));
    }
}
