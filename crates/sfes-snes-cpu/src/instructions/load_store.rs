// @source: wdc-w65c816s#table-5-5
//! Loads (LDA, LDX, LDY) and stores (STA, STX, STY, STZ). The stores change no flag.

use crate::{
    addressing::{Address, Mode},
    bus::Bus65816,
    registers::{Cpu65816, Flags},
    width::{Width, at_width},
};

impl Cpu65816 {
    /// LDA: loads the accumulator from memory, 8 bits wide when the M flag is set and 16
    /// bits otherwise.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.4
    pub(crate) fn lda(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        at_width!(self.p, Flags::M, W => self.load_accumulator::<W>(bus, mode));
    }

    /// LDX: loads X from memory, 8 bits wide when the X flag is set and 16 bits otherwise.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.7
    pub(crate) fn ldx(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        self.x = at_width!(self.p, Flags::X, W => self.load_index::<W>(bus, mode));
    }

    /// LDY: loads Y from memory, 8 bits wide when the X flag is set and 16 bits otherwise.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.7
    pub(crate) fn ldy(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        self.y = at_width!(self.p, Flags::X, W => self.load_index::<W>(bus, mode));
    }

    /// STA: stores the accumulator, A when the M flag is set and C otherwise.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.4
    pub(crate) fn sta(&mut self, bus: &mut impl Bus65816, address: Address) {
        at_width!(self.p, Flags::M, W => {
            self.write_operand(bus, address, self.accumulator::<W>());
        });
    }

    /// STX: stores X, 8 bits wide when the X flag is set and 16 bits otherwise.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.7
    pub(crate) fn stx(&mut self, bus: &mut impl Bus65816, address: Address) {
        self.store_index(bus, address, self.x);
    }

    /// STY: stores Y, 8 bits wide when the X flag is set and 16 bits otherwise.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.7
    pub(crate) fn sty(&mut self, bus: &mut impl Bus65816, address: Address) {
        self.store_index(bus, address, self.y);
    }

    /// STZ: stores zero, 8 bits wide when the M flag is set and 16 bits otherwise.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.4
    pub(crate) fn stz(&mut self, bus: &mut impl Bus65816, address: Address) {
        at_width!(self.p, Flags::M, W => self.write_operand(bus, address, W::from_register(0)));
    }

    /// "M → A" at width `W`, setting N and Z from the value. An 8-bit load leaves the B
    /// accumulator alone.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.4
    fn load_accumulator<W: Width>(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        let value = self.read_operand::<W>(bus, mode);
        self.set_accumulator(value);
        self.p.set_negative_zero(value);
    }

    /// "M → X" or "M → Y" at width `W`: reads the operand, sets N and Z from it and returns
    /// the new register value. The high byte of an 8-bit index register is zero.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.7
    fn load_index<W: Width>(&mut self, bus: &mut impl Bus65816, mode: Mode) -> u16 {
        let value = self.read_operand::<W>(bus, mode);
        self.p.set_negative_zero(value);
        value.to_u16()
    }

    /// Stores an index register's value at the width the X flag selects.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.7
    fn store_index(&mut self, bus: &mut impl Bus65816, address: Address, register: u16) {
        at_width!(self.p, Flags::X, W => {
            self.write_operand(bus, address, W::from_register(register));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Cycle, emulation, native, run};

    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.4
    #[test]
    fn an_eight_bit_lda_keeps_b_and_sets_n_and_z_from_a() {
        let mut cpu = native(Flags::M | Flags::Z);
        cpu.a = 0x1200;
        run(&mut cpu, &[0xA9, 0x80], &[]);
        assert_eq!(cpu.a, 0x1280);
        assert_eq!(cpu.p, Flags::M | Flags::N);

        run(&mut cpu, &[0xA9, 0x00], &[]);
        assert_eq!(cpu.a, 0x1200);
        assert_eq!(cpu.p, Flags::M | Flags::Z);
    }

    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.4
    #[test]
    fn a_sixteen_bit_lda_loads_c_and_sets_n_from_bit_15() {
        let mut cpu = native(Flags::C | Flags::V);
        run(&mut cpu, &[0xA9, 0x80, 0x00], &[]);
        assert_eq!(cpu.a, 0x0080);
        assert_eq!(cpu.p, Flags::C | Flags::V);

        run(&mut cpu, &[0xA9, 0x00, 0x80], &[]);
        assert_eq!(cpu.a, 0x8000);
        assert_eq!(cpu.p, Flags::C | Flags::V | Flags::N);
    }

    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.7
    #[test]
    fn ldx_and_ldy_take_their_width_from_the_x_flag() {
        // 8-bit accumulator, 16-bit index registers.
        let mut cpu = native(Flags::M);
        run(&mut cpu, &[0xA2, 0x34, 0x92], &[]);
        assert_eq!((cpu.x, cpu.pc), (0x9234, 0x8003));
        assert_eq!(cpu.p, Flags::M | Flags::N);
        run(&mut cpu, &[0xA0, 0x00, 0x00], &[]);
        assert_eq!((cpu.y, cpu.pc), (0x0000, 0x8006));
        assert_eq!(cpu.p, Flags::M | Flags::Z);

        // 16-bit accumulator, 8-bit index registers.
        let mut cpu = native(Flags::X);
        run(&mut cpu, &[0xA2, 0x92], &[]);
        assert_eq!((cpu.x, cpu.pc), (0x0092, 0x8002));
        assert_eq!(cpu.p, Flags::X | Flags::N);
        run(&mut cpu, &[0xA0, 0x01], &[]);
        assert_eq!((cpu.y, cpu.pc), (0x0001, 0x8004));
        assert_eq!(cpu.p, Flags::X);
    }

    // @source: wdc-w65c816s#table-5-7-1a
    // @source: wdc-w65c816s#2.4
    #[test]
    fn sta_writes_a_or_c_low_byte_first_and_changes_no_flag() {
        let mut cpu = native(Flags::M | Flags::Z);
        cpu.a = 0x1234;
        cpu.dbr = 0x7E;
        let bus = run(&mut cpu, &[0x8D, 0x00, 0x20], &[]);
        assert_eq!(bus.cycles[3..], [Cycle::Write(0x7E_2000, 0x34)]);
        assert_eq!(cpu.p, Flags::M | Flags::Z);

        let mut cpu = native(Flags::empty());
        cpu.a = 0x1234;
        cpu.dbr = 0x7E;
        let bus = run(&mut cpu, &[0x8D, 0xFF, 0xFF], &[]);
        assert_eq!(
            bus.cycles[3..],
            [Cycle::Write(0x7E_FFFF, 0x34), Cycle::Write(0x7F_0000, 0x12)]
        );
    }

    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.7
    #[test]
    fn stx_and_sty_take_their_width_from_the_x_flag() {
        let mut cpu = native(Flags::X);
        cpu.x = 0x0056;
        let bus = run(&mut cpu, &[0x86, 0x10], &[]);
        assert_eq!(bus.cycles[2..], [Cycle::Write(0x00_0010, 0x56)]);

        let mut cpu = native(Flags::M);
        cpu.y = 0xABCD;
        let bus = run(&mut cpu, &[0x84, 0x10], &[]);
        assert_eq!(
            bus.cycles[2..],
            [Cycle::Write(0x00_0010, 0xCD), Cycle::Write(0x00_0011, 0xAB)]
        );
    }

    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.4
    #[test]
    fn stz_writes_zero_at_the_accumulator_width() {
        let mut cpu = emulation();
        cpu.a = 0xFFFF;
        let bus = run(&mut cpu, &[0x64, 0x10], &[(0x00_0010, &[0xFF, 0xFF])]);
        assert_eq!(bus.cycles[2..], [Cycle::Write(0x00_0010, 0x00)]);
        assert_eq!(bus.byte(0x00_0011), 0xFF);
    }

    // @source: wdc-w65c816s#table-5-7-6a
    // @source: wdc-w65c816s#table-5-7-note-4
    #[test]
    fn an_indexed_store_always_spends_the_indexing_cycle() {
        // 8-bit index registers and no page crossing: a read would skip the cycle.
        let mut cpu = native(Flags::M | Flags::X);
        cpu.dbr = 0x34;
        cpu.x = 0x01;
        cpu.a = 0x99;
        let bus = run(&mut cpu, &[0x9D, 0x00, 0x56], &[]);
        assert_eq!(
            bus.cycles[3..],
            [Cycle::Internal(0x34_5601), Cycle::Write(0x34_5601, 0x99)]
        );

        let mut cpu = native(Flags::M | Flags::X);
        cpu.y = 0x01;
        cpu.a = 0x99;
        let bus = run(&mut cpu, &[0x91, 0x20], &[(0x00_0020, &[0x00, 0x56])]);
        assert_eq!(
            bus.cycles[4..],
            [Cycle::Internal(0x00_5601), Cycle::Write(0x00_5601, 0x99)]
        );
    }
}
