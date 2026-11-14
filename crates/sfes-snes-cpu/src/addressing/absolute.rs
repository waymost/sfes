// @source: wdc-w65c816s#3.5.1
// @source: wdc-w65c816s#3.5.3
// @source: wdc-w65c816s#3.5.4
// @source: wdc-w65c816s#3.5.6
// @source: wdc-w65c816s#3.5.7
//! Absolute addressing: the instruction holds a 16-bit or 24-bit address.

use sfes_api::Addr24;

use crate::{
    addressing::{Operand, indexed},
    bus::Bus65816,
    registers::Cpu65816,
};

impl Cpu65816 {
    /// Row 1a, cycles 2 and 3: fetches "AAL" and "AAH". The data is at DBR,AA.
    // @source: wdc-w65c816s#table-5-7-1a
    // @source: wdc-w65c816s#3.5.1
    pub(super) fn absolute(&mut self, bus: &mut impl Bus65816) -> Operand {
        let address = self.fetch_address(bus);
        Operand::spanning_banks(Addr24::from_bank_offset(self.dbr, address))
    }

    /// Rows 6a (`a,x`) and 7 (`a,y`), cycles 2 to 3a: fetches "AAL" and "AAH", then runs the
    /// optional internal cycle of note 4. The data is at DBR,AA plus the index register, a
    /// sum that carries into the bank.
    // @source: wdc-w65c816s#table-5-7-6a
    // @source: wdc-w65c816s#table-5-7-7
    // @source: wdc-w65c816s#7.3
    pub(super) fn absolute_indexed(&mut self, bus: &mut impl Bus65816, index: u16) -> Operand {
        let base = self.fetch_address(bus);
        Operand::spanning_banks(self.index_data_address(bus, base, index))
    }

    /// Row 4a, cycles 2 to 4: fetches "AAL", "AAH" and the bank "AAB". The data is at AAB,AA.
    // @source: wdc-w65c816s#table-5-7-4a
    // @source: wdc-w65c816s#3.5.7
    pub(super) fn absolute_long(&mut self, bus: &mut impl Bus65816) -> Operand {
        Operand::spanning_banks(self.fetch_long_address(bus))
    }

    /// Row 5, cycles 2 to 4: fetches "AAL", "AAH" and "AAB". The data is at the sum of that
    /// 24-bit address and X. The row has no optional cycle.
    // @source: wdc-w65c816s#table-5-7-5
    // @source: wdc-w65c816s#3.5.6
    pub(super) fn absolute_long_x(&mut self, bus: &mut impl Bus65816) -> Operand {
        let base = self.fetch_long_address(bus);
        Operand::spanning_banks(indexed(base, self.x))
    }

    /// Cycles 2 to 4 of the absolute long rows: three program bytes, low byte first.
    // @source: wdc-w65c816s#table-5-7-4a
    fn fetch_long_address(&mut self, bus: &mut impl Bus65816) -> Addr24 {
        let address = self.fetch_address(bus);
        let bank = self.fetch(bus);
        Addr24::from_bank_offset(bank, address)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        registers::Flags,
        testing::{Cycle, native, run},
    };

    // @source: wdc-w65c816s#table-5-7-1a
    #[test]
    fn absolute_reads_at_the_data_bank() {
        let mut cpu = native(Flags::empty());
        cpu.dbr = 0x34;
        let bus = run(&mut cpu, &[0xAD, 0x78, 0x56], &[(0x34_5678, &[0xCD, 0xAB])]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Program(0x12_8002),
                Cycle::Data(0x34_5678),
                Cycle::Data(0x34_5679),
            ]
        );
        assert_eq!((cpu.a, cpu.pc), (0xABCD, 0x8003));
    }

    // @source: 6502org-65c816-opcodes#5.2
    #[test]
    fn sixteen_bit_absolute_data_runs_into_the_next_bank() {
        // Example 2: with DBR $12, LDA $FFFF reads $12FFFF and $130000.
        let mut cpu = native(Flags::empty());
        cpu.dbr = 0x12;
        cpu.pbr = 0x00;
        let bus = run(&mut cpu, &[0xAD, 0xFF, 0xFF], &[]);
        assert_eq!(
            bus.cycles[3..],
            [Cycle::Data(0x12_FFFF), Cycle::Data(0x13_0000)]
        );
    }

    // @source: 6502org-65c816-opcodes#5.3
    // @source: wdc-w65c816s#7.3
    #[test]
    fn absolute_indexed_carries_into_the_bank() {
        // The example: DBR $12, X $000A, LDA $FFFE,X reads $130008 and $130009.
        let mut cpu = native(Flags::empty());
        cpu.dbr = 0x12;
        cpu.pbr = 0x00;
        cpu.x = 0x000A;
        let bus = run(&mut cpu, &[0xBD, 0xFE, 0xFF], &[]);
        assert_eq!(
            bus.cycles[3..],
            [
                Cycle::Internal(0x12_FF08),
                Cycle::Data(0x13_0008),
                Cycle::Data(0x13_0009),
            ]
        );
    }

    // @source: wdc-w65c816s#table-5-7-7
    #[test]
    fn absolute_y_indexes_with_y() {
        let mut cpu = native(Flags::M | Flags::X);
        cpu.dbr = 0x34;
        cpu.x = 0x01;
        cpu.y = 0x02;
        let bus = run(&mut cpu, &[0xB9, 0x00, 0x56], &[(0x34_5602, &[0x99])]);
        assert_eq!(bus.cycles[3..], [Cycle::Data(0x34_5602)]);
        assert_eq!(cpu.a, 0x0099);
    }

    // @source: wdc-w65c816s#table-5-7-4a
    // @source: 6502org-65c816-opcodes#5.16
    #[test]
    fn absolute_long_takes_its_bank_from_the_instruction() {
        // The example: LDA $12FFFF reads $12FFFF and $130000.
        let mut cpu = native(Flags::empty());
        cpu.dbr = 0x77;
        let bus = run(&mut cpu, &[0xAF, 0xFF, 0xFF, 0x12], &[]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Program(0x12_8002),
                Cycle::Program(0x12_8003),
                Cycle::Data(0x12_FFFF),
                Cycle::Data(0x13_0000),
            ]
        );
        assert_eq!(cpu.pc, 0x8004);
    }

    // @source: wdc-w65c816s#table-5-7-5
    // @source: 6502org-65c816-opcodes#5.17
    #[test]
    fn absolute_long_x_has_no_extra_cycle() {
        // The example: X $000A, LDA $12FFFE,X reads $130008 and $130009.
        let mut cpu = native(Flags::empty());
        cpu.x = 0x000A;
        let bus = run(&mut cpu, &[0xBF, 0xFE, 0xFF, 0x12], &[]);
        assert_eq!(
            bus.cycles[4..],
            [Cycle::Data(0x13_0008), Cycle::Data(0x13_0009)]
        );
    }
}
