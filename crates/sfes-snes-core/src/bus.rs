// @source: anomie-memmap#memory-map
// @source: anomie-memmap#hardware/data-bus
// @source: adr-0004
// @source: adr-0018
//! The bus state and the CPU's view of it, provisional until #80 (ADR 0018, point 6).
//!
//! [`Bus`] owns WRAM, the cartridge, the data bus value, the NMI latch, the clock and the
//! line timing. [`BusView`] implements `Bus65816` over it: it finds each address in the
//! cartridge's memory map, charges the access time, performs the access and tells the
//! observer.

use alloc::{boxed::Box, vec};
use core::fmt;

use sfes_api::{Addr24, BusId, Buttons, Cycles, Observer, SplitMix64};
use sfes_snes_cart::{Cartridge, Page};
use sfes_snes_cpu::{Bus65816, Pending, Pins};

use crate::timing::Timing;

/// The size of WRAM, banks `$7E–$7F`.
pub(crate) const WRAM_SIZE: usize = 0x2_0000;
/// The number of controller ports.
pub(crate) const PORTS: usize = 2;

/// An internal-operation cycle, and a Fast access.
const FAST: u64 = 6;
/// A Slow access.
const SLOW: u64 = 8;
/// An `XSlow` access.
const XSLOW: u64 = 12;

/// The NMI vector's low byte in native mode, read with VPB.
const NMI_VECTOR_NATIVE: u32 = 0x00_FFEA;
/// The NMI vector's low byte in emulation mode, read with VPB.
const NMI_VECTOR_EMULATION: u32 = 0x00_FFFA;

/// Everything on the CPU's side of the machine except the CPU itself.
#[derive(Clone)]
pub(crate) struct Bus {
    /// Work RAM, 128 KiB, allocated on the heap without passing through the stack.
    pub(crate) wram: Box<[u8]>,
    /// The cartridge, with its ROM, RAM and memory map.
    pub(crate) cartridge: Cartridge,
    /// The last value placed on the data bus, which a read nothing answers returns.
    pub(crate) open_bus_value: u8,
    /// The NMI edge latch (ADR 0004). Nothing sets it until #82.
    pub(crate) nmi_latched: bool,
    /// The IRQ line, asserted when set. Nothing sets it until #82.
    pub(crate) irq_line: bool,
    /// Master cycles since power-on.
    pub(crate) now: Cycles,
    /// Line and frame position.
    pub(crate) timing: Timing,
    /// The buttons held on each port, latched by the controller ports in #85.
    pub(crate) input: [Buttons; PORTS],
}

impl fmt::Debug for Bus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Bus")
            .field("cartridge", &self.cartridge.header())
            .field("open_bus_value", &self.open_bus_value)
            .field("nmi_latched", &self.nmi_latched)
            .field("irq_line", &self.irq_line)
            .field("now", &self.now)
            .field("timing", &self.timing)
            .field("input", &self.input)
            .finish_non_exhaustive()
    }
}

impl Bus {
    /// The bus at power-on: WRAM filled from `seed` (ADR 0018), the clock at 0.
    // @source: vigna-splitmix
    // @source: adr-0018
    pub(crate) fn power_on(cartridge: Cartridge, timing: Timing, seed: u64) -> Self {
        let mut wram = vec![0; WRAM_SIZE].into_boxed_slice();
        SplitMix64::new(seed).fill(&mut wram);
        Self {
            wram,
            cartridge,
            open_bus_value: 0,
            nmi_latched: false,
            irq_line: false,
            now: Cycles(0),
            timing,
            input: [Buttons::empty(); PORTS],
        }
    }

    /// The interrupt requests as the CPU sees them at an instruction boundary (ADR 0004).
    // @source: adr-0004
    pub(crate) const fn pending(&self) -> Pending {
        Pending {
            nmi: self.nmi_latched,
            irq: self.irq_line,
        }
    }

    /// The byte at `address` as a read would see it, without the read's side effects or
    /// cost. Nothing on this bus has read side effects yet.
    // @source: anomie-memmap#memory-map
    // @source: anomie-memmap#hardware/data-bus
    pub(crate) fn peek(&self, address: u32) -> u8 {
        let open = self.open_bus_value;
        match self.cartridge.map().resolve(address) {
            Page::Wram(offset) => self.wram.get(offset as usize).copied().unwrap_or(open),
            Page::Rom(offset) => self
                .cartridge
                .rom()
                .get(offset as usize)
                .copied()
                .unwrap_or(open),
            Page::CartRam(offset) => self
                .cartridge
                .ram()
                .get(offset as usize)
                .copied()
                .unwrap_or(open),
            Page::Io | Page::Coproc(_) | Page::Open => open,
        }
    }

    /// Stores `value` at `address` where something writable answers.
    // @source: anomie-memmap#memory-map
    fn store(&mut self, address: u32, value: u8) {
        match self.cartridge.map().resolve(address) {
            Page::Wram(offset) => {
                if let Some(byte) = self.wram.get_mut(offset as usize) {
                    *byte = value;
                }
            }
            Page::CartRam(offset) => {
                if let Some(byte) = self.cartridge.ram_mut().get_mut(offset as usize) {
                    *byte = value;
                }
            }
            Page::Rom(_) | Page::Io | Page::Coproc(_) | Page::Open => {}
        }
    }

    /// Advances the clock by `cycles`.
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    const fn charge(&mut self, cycles: u64) {
        self.now = Cycles(self.now.0 + cycles);
    }
}

/// The master cycles an access to `address` takes, from anomie-memmap's speed column, with
/// `$420D` bit 0 at its reset value 0 until #80 (ADR 0018, point 6).
// @source: anomie-memmap#memory-map
// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: fullsnes#snesiomap
// @source: adr-0018
pub(crate) const fn access_cycles(address: Addr24) -> u64 {
    match address.bank() {
        0x00..=0x3F | 0x80..=0xBF => match address.offset() {
            0x2000..=0x3FFF | 0x4200..=0x5FFF => FAST,
            0x4000..=0x41FF => XSLOW,
            // $0000-$1FFF and $6000-$7FFF are Slow. $8000-$FFFF is Slow in $00-$3F and, in
            // $80-$BF, Slow while $420D bit 0 is clear.
            _ => SLOW,
        },
        // $40-$7F are Slow; $C0-$FF are Slow while $420D bit 0 is clear.
        _ => SLOW,
    }
}

/// The CPU's view of the bus for one step.
pub(crate) struct BusView<'view, O: Observer> {
    bus: &'view mut Bus,
    observer: &'view mut O,
    /// The first byte read in this step: the opcode of an instruction step.
    first_read: Option<u8>,
}

impl<'view, O: Observer> BusView<'view, O> {
    /// A view of `bus` reporting to `observer`.
    // @source: adr-0004
    pub(crate) const fn new(bus: &'view mut Bus, observer: &'view mut O) -> Self {
        Self {
            bus,
            observer,
            first_read: None,
        }
    }

    /// The first byte read since the view was made.
    // @source: adr-0004
    pub(crate) const fn first_read(&self) -> Option<u8> {
        self.first_read
    }
}

impl<O: Observer> Bus65816 for BusView<'_, O> {
    // @source: anomie-memmap#memory-map
    // @source: anomie-memmap#hardware/data-bus
    // @source: adr-0004
    // @source: adr-0018
    fn read(&mut self, address: Addr24, pins: Pins) -> u8 {
        let at = self.bus.now;
        let raw = address.get();
        let value = self.bus.peek(raw);
        self.bus.open_bus_value = value;
        self.bus.charge(access_cycles(address));
        if pins.contains(Pins::VPB) && (raw == NMI_VECTOR_NATIVE || raw == NMI_VECTOR_EMULATION) {
            self.bus.nmi_latched = false;
        }
        if self.first_read.is_none() {
            self.first_read = Some(value);
        }
        if O::ACTIVE {
            self.observer.on_read(BusId::Cpu, raw, value, at);
        }
        value
    }

    // @source: anomie-memmap#memory-map
    // @source: anomie-memmap#hardware/data-bus
    // @source: adr-0004
    fn write(&mut self, address: Addr24, value: u8, _pins: Pins) {
        let at = self.bus.now;
        let raw = address.get();
        self.bus.store(raw, value);
        self.bus.open_bus_value = value;
        self.bus.charge(access_cycles(address));
        if O::ACTIVE {
            self.observer.on_write(BusId::Cpu, raw, value, at);
        }
    }

    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: adr-0004
    fn idle(&mut self, _address: Addr24, _pins: Pins) {
        self.bus.charge(FAST);
    }

    // @source: adr-0004
    fn interrupts(&self) -> Pending {
        self.bus.pending()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: anomie-memmap#memory-map
    #[test]
    fn access_times_follow_the_speed_column() {
        let cases = [
            (0x00_0000, SLOW),
            (0x00_1FFF, SLOW),
            (0x00_2000, FAST),
            (0x00_2100, FAST),
            (0x00_3FFF, FAST),
            (0x00_4000, XSLOW),
            (0x00_41FF, XSLOW),
            (0x00_4200, FAST),
            (0x00_5FFF, FAST),
            (0x00_6000, SLOW),
            (0x00_8000, SLOW),
            (0x3F_FFFF, SLOW),
            (0x40_0000, SLOW),
            (0x7E_0000, SLOW),
            (0x7F_FFFF, SLOW),
            (0x80_4016, XSLOW),
            (0x80_8000, SLOW),
            (0xC0_0000, SLOW),
        ];
        for (address, cycles) in cases {
            assert_eq!(access_cycles(Addr24::new(address)), cycles, "{address:06X}");
        }
    }
}
