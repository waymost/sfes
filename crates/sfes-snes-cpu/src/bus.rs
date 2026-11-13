// @source: adr-0004
// @source: wdc-w65c816s#2.12
//! The bus a 65C816 core runs on: one call per clock cycle (ADR 0004).

use sfes_api::Addr24;

bitflags::bitflags! {
    /// The processor's output signals during one cycle.
    ///
    /// A flag is set when the signal is active, not by pin voltage: [`Pins::VPB`] and
    /// [`Pins::MLB`] are set when their active-low pins are low (ADR 0004). The flags are the
    /// eight signals the `SingleStepTests` vectors record for every cycle.
    // @source: adr-0004
    #[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
    pub struct Pins: u8 {
        /// Valid Data Address: the address is a data address. Together with [`Pins::VPA`]
        /// this marks an opcode fetch; with neither, an internal operation.
        // @source: wdc-w65c816s#2.26
        const VDA = 1 << 0;
        /// Valid Program Address: the address is a program address.
        // @source: wdc-w65c816s#2.26
        const VPA = 1 << 1;
        /// Vector Pull: an interrupt vector is being read.
        // @source: wdc-w65c816s#2.28
        const VPB = 1 << 2;
        /// Read/Write: set when the processor reads, clear when it writes.
        // @source: wdc-w65c816s#2.23
        const RWB = 1 << 3;
        /// Emulation Status: the E flag.
        // @source: wdc-w65c816s#2.17
        const E = 1 << 4;
        /// The M flag, as carried on the Memory/Index Select Status pin.
        // @source: wdc-w65c816s#2.20
        const M = 1 << 5;
        /// The X flag, as carried on the Memory/Index Select Status pin.
        // @source: wdc-w65c816s#2.20
        const X = 1 << 6;
        /// Memory Lock: a read-modify-write instruction holds the bus.
        // @source: wdc-w65c816s#2.19
        const MLB = 1 << 7;
    }
}

/// The interrupt requests the processor has recognized at an instruction boundary.
// @source: adr-0004
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct Pending {
    /// An NMI edge is latched. The bus owns the latch: it sets it on a high-to-low transition
    /// of the line and clears it when it sees a read with [`Pins::VPB`] at the NMI vector.
    // @source: wdc-w65c816s#2.21
    pub nmi: bool,
    /// The level-sensitive IRQ line is asserted.
    // @source: wdc-w65c816s#2.18
    pub irq: bool,
}

/// How a 65C816 core talks to its machine. The main CPU's bus view and the SA-1's bus both
/// implement it, so one CPU type serves both (MS-01).
///
/// The core makes exactly one call to [`read`](Self::read), [`write`](Self::write) or
/// [`idle`](Self::idle) for every clock cycle, in order. The bus is the only clock: each call
/// advances time by what that cycle costs on the machine.
// @source: adr-0004
pub trait Bus65816 {
    /// A cycle with RWB high and VDA or VPA active: returns the byte at `address`.
    // @source: adr-0004
    fn read(&mut self, address: Addr24, pins: Pins) -> u8;

    /// A cycle with RWB low: stores `value` at `address`.
    // @source: adr-0004
    fn write(&mut self, address: Addr24, value: u8, pins: Pins);

    /// An internal-operation cycle (RWB high, VDA and VPA inactive). The processor drives
    /// `address`, but the bus must not perform a memory access.
    // @source: adr-0004
    fn idle(&mut self, address: Addr24, pins: Pins);

    /// The recognized interrupt requests. The core calls this once per step, before the
    /// opcode fetch.
    // @source: adr-0004
    fn interrupts(&self) -> Pending;
}
