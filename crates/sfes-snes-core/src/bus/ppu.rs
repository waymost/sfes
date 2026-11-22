// @source: anomie-regs#registers
// @source: adr-0019
// @source: adr-0028
// @source: adr-0029
//! The PPU as the bus reaches it: [`Registers`] over `sfes_snes_ppu::Ppu` (ADR 0028,
//! point 2).

use sfes_snes_ppu::{Ppu, Register};

use crate::bus::Registers;

impl Registers for Ppu {
    type Register = Register;

    /// A PPU register's value, or `None` where CPU open bus answers (ADR 0028, point 3;
    /// ADR 0029).
    // @source: anomie-regs#registers
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0028
    // @source: adr-0029
    fn peek(&self, register: Register) -> Option<u8> {
        Ppu::peek(self, register)
    }

    /// A read's side effects: the open-bus latches, the memory ports' addresses, and the
    /// counters' flip-flops and latch flag (ADR 0028, point 4; ADR 0029, point 11).
    // @source: anomie-regs#213c
    // @source: adr-0028
    // @source: adr-0029
    fn after_read(&mut self, register: Register) {
        Ppu::after_read(self, register);
    }

    /// A CPU write (ADR 0028, point 5; ADR 0029).
    // @source: anomie-regs#2133
    // @source: adr-0028
    // @source: adr-0029
    fn write(&mut self, register: Register, value: u8) {
        Ppu::write(self, register, value);
    }
}
