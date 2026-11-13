// @source: wdc-w65c816s
//! 65C816 CPU core, generic over its bus (main CPU and SA-1).
//!
//! Skeleton crate (milestone M0); see the technical design for its planned contents.

#![no_std]
#![forbid(unsafe_code)]

use sfes_api::Addr24;

/// The kind of a CPU bus read, as signalled on the 65C816's VPA/VDA pins.
// @source: wdc-w65c816s
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Access {
    /// Opcode fetch (VPA and VDA high).
    Opcode,
    /// Operand fetch (VPA high).
    Operand,
    /// Data read (VDA high).
    Data,
    /// Internal operation with neither VPA nor VDA high.
    Internal,
}

/// Interrupt lines as sampled by the CPU.
// @source: wdc-w65c816s
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Pending {
    /// NMI edge latched.
    pub nmi: bool,
    /// IRQ line asserted.
    pub irq: bool,
}

/// How a 65C816 core talks to its machine. The main CPU's bus view and the SA-1's bus both
/// implement it, so one CPU type serves both (MS-01).
// @source: wdc-w65c816s
pub trait Bus65816 {
    /// Reads a byte, advancing the clock by the access time of the address region.
    // @source: wdc-w65c816s
    fn read(&mut self, address: Addr24, kind: Access) -> u8;

    /// Writes a byte, advancing the clock by the access time of the address region.
    // @source: wdc-w65c816s
    fn write(&mut self, address: Addr24, value: u8);

    /// An internal-operation cycle.
    // @source: wdc-w65c816s
    fn idle(&mut self);

    /// Interrupt lines, sampled per the 65C816 rules.
    // @source: wdc-w65c816s
    fn interrupts(&self) -> Pending;
}
