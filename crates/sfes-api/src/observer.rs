//! Observer hooks that compile away when unused (FND-05).
//!
//! Observers receive `&mut self` and event values, never `&mut` machine state, so they cannot
//! change emulation. Addresses are `u32` so one trait covers the 24-bit CPU buses and the 16-bit
//! SPC700 bus; [`BusId`] says which.

use crate::units::Cycles;

/// Which bus an access happened on.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum BusId {
    /// The main CPU's 24-bit bus.
    Cpu,
    /// The SA-1's 24-bit bus.
    Sa1,
    /// The SPC700's 16-bit bus.
    Spc,
}

/// An interrupt delivered to a CPU.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Interrupt {
    /// Non-maskable interrupt.
    Nmi,
    /// Maskable interrupt request.
    Irq,
}

/// An executed instruction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InstrEvent {
    /// The CPU that executed it.
    pub bus: BusId,
    /// Address of the opcode.
    pub pc: u32,
    /// The opcode byte.
    pub opcode: u8,
    /// Master-clock time at the opcode fetch.
    pub at: Cycles,
}

/// One byte moved by a DMA or HDMA channel.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DmaEvent {
    /// Channel number, 0–7.
    pub channel: u8,
    /// Whether the transfer is HDMA.
    pub hdma: bool,
    /// Source address.
    pub source: u32,
    /// Destination address.
    pub destination: u32,
    /// The byte transferred.
    pub value: u8,
    /// Master-clock time of the transfer.
    pub at: Cycles,
}

/// Hooks called by a machine as it runs. Every method has an empty default.
pub trait Observer: Clone + Send {
    /// `false` lets call sites skip building event data entirely.
    const ACTIVE: bool = true;

    /// An instruction was executed.
    #[inline(always)]
    fn on_instruction(&mut self, _event: &InstrEvent) {}

    /// A byte was read from a bus.
    #[inline(always)]
    fn on_read(&mut self, _bus: BusId, _address: u32, _value: u8, _at: Cycles) {}

    /// A byte was written to a bus.
    #[inline(always)]
    fn on_write(&mut self, _bus: BusId, _address: u32, _value: u8, _at: Cycles) {}

    /// A DMA or HDMA byte was transferred.
    #[inline(always)]
    fn on_dma(&mut self, _event: &DmaEvent) {}

    /// A scanline started.
    #[inline(always)]
    fn on_scanline(&mut self, _line: u16, _at: Cycles) {}

    /// An interrupt was taken.
    #[inline(always)]
    fn on_interrupt(&mut self, _kind: Interrupt, _at: Cycles) {}

    /// A frame ended (FND-04 frame boundary).
    #[inline(always)]
    fn on_frame_end(&mut self, _frame: u64) {}
}

/// The default observer: does nothing and compiles away.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct NoopObserver;

impl Observer for NoopObserver {
    const ACTIVE: bool = false;
}

/// Two observers composed without boxing; each hook calls the first, then the second.
impl<First: Observer, Second: Observer> Observer for (First, Second) {
    const ACTIVE: bool = First::ACTIVE || Second::ACTIVE;

    fn on_instruction(&mut self, event: &InstrEvent) {
        self.0.on_instruction(event);
        self.1.on_instruction(event);
    }

    fn on_read(&mut self, bus: BusId, address: u32, value: u8, at: Cycles) {
        self.0.on_read(bus, address, value, at);
        self.1.on_read(bus, address, value, at);
    }

    fn on_write(&mut self, bus: BusId, address: u32, value: u8, at: Cycles) {
        self.0.on_write(bus, address, value, at);
        self.1.on_write(bus, address, value, at);
    }

    fn on_dma(&mut self, event: &DmaEvent) {
        self.0.on_dma(event);
        self.1.on_dma(event);
    }

    fn on_scanline(&mut self, line: u16, at: Cycles) {
        self.0.on_scanline(line, at);
        self.1.on_scanline(line, at);
    }

    fn on_interrupt(&mut self, kind: Interrupt, at: Cycles) {
        self.0.on_interrupt(kind, at);
        self.1.on_interrupt(kind, at);
    }

    fn on_frame_end(&mut self, frame: u64) {
        self.0.on_frame_end(frame);
        self.1.on_frame_end(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const fn assert_clone_send<T: Clone + Send>() {}

    #[test]
    fn observers_are_clone_and_send() {
        assert_clone_send::<NoopObserver>();
        assert_clone_send::<(NoopObserver, NoopObserver)>();
    }

    #[test]
    fn tuple_is_active_if_either_is() {
        #[derive(Clone)]
        struct Counting;
        impl Observer for Counting {}

        const { assert!(!<(NoopObserver, NoopObserver)>::ACTIVE) };
        const { assert!(<(NoopObserver, Counting)>::ACTIVE) };
    }
}
