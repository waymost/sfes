// @source: wdc-w65c816s#table-2-1
//! The programming model: registers, status flags and run state.

use crate::{bus::Pins, width::Width};

bitflags::bitflags! {
    /// The Processor Status Register (P).
    ///
    /// The emulation flag is not one of its eight bits; it is [`Cpu65816::e`]. In emulation
    /// mode bits 5 and 4 are always set here, and bit 4 reads as the Break flag only in the
    /// copy of P that an interrupt or `PHP` writes to the stack.
    // @source: wdc-w65c816s#table-2-1
    // @source: wdc-w65c816s#2.8
    #[derive(
        Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize,
    )]
    pub struct Flags: u8 {
        /// Carry.
        const C = 1 << 0;
        /// Zero: the result was zero.
        const Z = 1 << 1;
        /// IRQ disable.
        const I = 1 << 2;
        /// Decimal mode.
        const D = 1 << 3;
        /// Index register select: set for 8-bit X and Y.
        const X = 1 << 4;
        /// Memory select: set for an 8-bit accumulator and 8-bit memory operands.
        const M = 1 << 5;
        /// Overflow.
        const V = 1 << 6;
        /// Negative: the result's sign bit.
        const N = 1 << 7;
    }
}

impl Flags {
    /// Sets N from the sign bit of `value` and Z from whether it is zero.
    // @source: wdc-w65c816s#table-2-1
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "bring-up: first used by the load instructions (#33)"
        )
    )]
    pub(crate) fn set_negative_zero<W: Width>(&mut self, value: W) {
        let value = value.to_u16();
        self.set(Self::N, value & W::SIGN != 0);
        self.set(Self::Z, value == 0);
    }
}

/// Whether the processor is executing instructions.
// @source: adr-0004
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum RunState {
    /// Executing instructions.
    Running,
    /// Halted by `WAI` until an interrupt request arrives.
    // @source: wdc-w65c816s#7.13
    Waiting,
    /// Halted by `STP` until reset.
    // @source: wdc-w65c816s#7.14
    Stopped,
}

/// A 65C816: its registers and run state, and nothing else. It holds no reference to its
/// machine; every [`step`](Self::step) borrows a bus.
///
/// The fields are public so that tests, debuggers and save states can read and write raw
/// values. Code that changes the mode or the status register goes through
/// [`set_emulation`](Self::set_emulation) and [`set_status`](Self::set_status), which keep
/// the register widths consistent with the flags.
// @source: wdc-w65c816s#table-2-1
#[derive(Clone, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct Cpu65816 {
    /// The 16-bit accumulator C: the A accumulator in its low byte, B in its high byte.
    pub a: u16,
    /// Index register X. Its high byte is zero while the X flag is set.
    pub x: u16,
    /// Index register Y. Its high byte is zero while the X flag is set.
    pub y: u16,
    /// Stack pointer S. Its high byte is `01` in emulation mode.
    pub s: u16,
    /// Direct register D.
    pub d: u16,
    /// Data Bank Register.
    pub dbr: u8,
    /// Program Bank Register.
    pub pbr: u8,
    /// Program counter, an offset within the program bank.
    pub pc: u16,
    /// Processor Status Register.
    pub p: Flags,
    /// The emulation flag.
    pub e: bool,
    /// Whether the processor is running, waiting or stopped.
    pub run: RunState,
}

impl Cpu65816 {
    /// A processor with the register values that reset establishes: emulation mode, 8-bit
    /// accumulator and index registers, IRQ disabled, decimal mode off, stack page `01`, and
    /// the direct register and both bank registers zero.
    ///
    /// Reset leaves A, the low bytes of X, Y and S, and the N, V, Z and C flags
    /// uninitialized. They are zero here so that every machine starts the same (FND-02).
    /// The program counter is zero until the reset sequence loads it from the vector (#42).
    // @source: wdc-w65c816s#2.25
    #[must_use]
    pub const fn new() -> Self {
        Self {
            a: 0,
            x: 0,
            y: 0,
            s: 0x0100,
            d: 0,
            dbr: 0,
            pbr: 0,
            pc: 0,
            p: Flags::M.union(Flags::X).union(Flags::I),
            e: true,
            run: RunState::Running,
        }
    }

    /// Switches between emulation and native mode. Entering emulation mode sets the M and X
    /// flags, forces the high byte of S to `01` and clears the high bytes of X and Y; the
    /// accumulator and the low bytes are untouched. Entering native mode changes no register.
    // @source: wdc-w65c816s#7.10
    pub const fn set_emulation(&mut self, emulation: bool) {
        self.e = emulation;
        if emulation {
            self.s = 0x0100 | (self.s & 0x00FF);
            self.set_status(self.p);
        }
    }

    /// Replaces the status register. In emulation mode the M and X flags stay set, and
    /// whenever the X flag is set the high bytes of X and Y are forced to zero.
    // @source: wdc-w65c816s#2.8
    // @source: wdc-w65c816s#2.7
    pub const fn set_status(&mut self, status: Flags) {
        self.p = if self.e {
            status.union(Flags::M).union(Flags::X)
        } else {
            status
        };
        if self.p.contains(Flags::X) {
            self.x &= 0x00FF;
            self.y &= 0x00FF;
        }
    }

    /// The accumulator at width `W`: A for `u8`, C for `u16`.
    // @source: wdc-w65c816s#2.4
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "bring-up: first used by the load instructions (#33)"
        )
    )]
    pub(crate) fn accumulator<W: Width>(&self) -> W {
        W::from_register(self.a)
    }

    /// Stores `value` in the accumulator. An 8-bit store leaves B alone.
    // @source: wdc-w65c816s#2.4
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "bring-up: first used by the load instructions (#33)"
        )
    )]
    pub(crate) fn set_accumulator<W: Width>(&mut self, value: W) {
        self.a = value.store_in(self.a);
    }

    /// The E, M and X signals for the current mode and flags, as every cycle reports them.
    // @source: wdc-w65c816s#2.17
    // @source: wdc-w65c816s#2.20
    pub(crate) fn status_pins(&self) -> Pins {
        let mut pins = Pins::empty();
        pins.set(Pins::E, self.e);
        pins.set(Pins::M, self.p.contains(Flags::M));
        pins.set(Pins::X, self.p.contains(Flags::X));
        pins
    }
}

impl Default for Cpu65816 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: wdc-w65c816s#table-2-1
    // @source: wdc-w65c816s#2.20
    #[test]
    fn status_bits_are_nvmxdizc_from_bit_7_down() {
        let order = [
            Flags::C,
            Flags::Z,
            Flags::I,
            Flags::D,
            Flags::X,
            Flags::M,
            Flags::V,
            Flags::N,
        ];
        for (bit, flag) in order.into_iter().enumerate() {
            assert_eq!(flag.bits(), 1 << bit);
        }
    }

    // @source: wdc-w65c816s#2.25
    #[test]
    fn new_holds_the_reset_values() {
        let cpu = Cpu65816::new();
        assert!(cpu.e);
        assert_eq!(cpu.p, Flags::M | Flags::X | Flags::I);
        assert_eq!(cpu.s.to_be_bytes()[0], 0x01);
        assert_eq!((cpu.d, cpu.dbr, cpu.pbr), (0, 0, 0));
        assert_eq!((cpu.x.to_be_bytes()[0], cpu.y.to_be_bytes()[0]), (0, 0));
        assert_eq!(cpu.run, RunState::Running);
        assert_eq!(cpu, Cpu65816::default());
    }

    /// A native-mode processor with the given status and distinct bytes in every register.
    // @source: wdc-w65c816s#table-2-1
    fn native(status: Flags) -> Cpu65816 {
        let mut cpu = Cpu65816::new();
        cpu.set_emulation(false);
        cpu.set_status(status);
        cpu.a = 0x1234;
        cpu.x = 0x5678;
        cpu.y = 0x9ABC;
        cpu.s = 0xDEF0;
        cpu
    }

    // @source: wdc-w65c816s#7.10
    #[test]
    fn entering_emulation_narrows_the_registers() {
        let mut cpu = native(Flags::C);
        cpu.set_emulation(true);
        assert!(cpu.p.contains(Flags::M | Flags::X | Flags::C));
        assert_eq!(cpu.s, 0x01F0);
        assert_eq!((cpu.x, cpu.y), (0x0078, 0x00BC));
        assert_eq!(cpu.a, 0x1234);
    }

    // @source: wdc-w65c816s#7.10
    #[test]
    fn entering_native_mode_changes_no_register() {
        let mut cpu = Cpu65816::new();
        let before = cpu.clone();
        cpu.set_emulation(false);
        assert!(!cpu.e);
        cpu.e = true;
        assert_eq!(cpu, before);
    }

    // @source: wdc-w65c816s#2.8
    #[test]
    fn emulation_mode_keeps_m_and_x_set() {
        let mut cpu = Cpu65816::new();
        cpu.set_status(Flags::empty());
        assert_eq!(cpu.p, Flags::M | Flags::X);
    }

    // @source: wdc-w65c816s#2.7
    #[test]
    fn setting_the_x_flag_clears_the_index_high_bytes() {
        let mut cpu = native(Flags::M);
        assert_eq!((cpu.x, cpu.y), (0x5678, 0x9ABC));
        cpu.set_status(Flags::X);
        assert_eq!((cpu.x, cpu.y), (0x0078, 0x00BC));
        assert_eq!(cpu.a, 0x1234);
        // Widening again does not bring the old high bytes back.
        cpu.set_status(Flags::empty());
        assert_eq!((cpu.x, cpu.y), (0x0078, 0x00BC));
    }

    // @source: wdc-w65c816s#2.4
    #[test]
    fn the_accumulator_is_read_and_written_at_either_width() {
        let mut cpu = native(Flags::empty());
        assert_eq!(cpu.accumulator::<u8>(), 0x34);
        assert_eq!(cpu.accumulator::<u16>(), 0x1234);
        cpu.set_accumulator(0xAB_u8);
        assert_eq!(cpu.a, 0x12AB);
        cpu.set_accumulator(0xBEEF_u16);
        assert_eq!(cpu.a, 0xBEEF);
    }

    // @source: wdc-w65c816s#table-2-1
    #[test]
    fn negative_and_zero_follow_the_width() {
        let mut flags = Flags::empty();
        flags.set_negative_zero(0x80_u8);
        assert_eq!(flags, Flags::N);
        flags.set_negative_zero(0x0080_u16);
        assert_eq!(flags, Flags::empty());
        flags.set_negative_zero(0x8000_u16);
        assert_eq!(flags, Flags::N);
        flags.set_negative_zero(0_u8);
        assert_eq!(flags, Flags::Z);
    }

    // @source: wdc-w65c816s#2.17
    // @source: wdc-w65c816s#2.20
    #[test]
    fn status_pins_report_e_m_and_x() {
        assert_eq!(Cpu65816::new().status_pins(), Pins::E | Pins::M | Pins::X);
        assert_eq!(native(Flags::M).status_pins(), Pins::M);
        assert_eq!(native(Flags::X | Flags::C).status_pins(), Pins::X);
    }
}
