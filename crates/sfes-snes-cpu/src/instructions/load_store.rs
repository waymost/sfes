// @source: wdc-w65c816s#table-5-5
//! Loads: LDA, LDX and LDY. (The stores arrive with #34.)

use crate::{
    addressing::Mode,
    bus::Bus65816,
    registers::{Cpu65816, Flags},
    width::Width,
};

impl Cpu65816 {
    /// LDA: loads the accumulator from memory, 8 bits wide when the M flag is set and 16
    /// bits otherwise.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.4
    pub(crate) fn lda(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        if self.p.contains(Flags::M) {
            self.load_accumulator::<u8>(bus, mode);
        } else {
            self.load_accumulator::<u16>(bus, mode);
        }
    }

    /// LDX: loads X from memory, 8 bits wide when the X flag is set and 16 bits otherwise.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.7
    pub(crate) fn ldx(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        self.x = if self.p.contains(Flags::X) {
            self.load_index::<u8>(bus, mode)
        } else {
            self.load_index::<u16>(bus, mode)
        };
    }

    /// LDY: loads Y from memory, 8 bits wide when the X flag is set and 16 bits otherwise.
    // @source: wdc-w65c816s#table-5-5
    // @source: wdc-w65c816s#2.7
    pub(crate) fn ldy(&mut self, bus: &mut impl Bus65816, mode: Mode) {
        self.y = if self.p.contains(Flags::X) {
            self.load_index::<u8>(bus, mode)
        } else {
            self.load_index::<u16>(bus, mode)
        };
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{native, run};

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
}
