// @source: anomie-memmap#memory-map
// @source: anomie-memmap#hardware/data-bus
// @source: adr-0004
// @source: adr-0018
// @source: adr-0019
// @source: adr-0020
//! The bus state and the main CPU's view of it (ADR 0019).
//!
//! [`Bus`] owns WRAM, the cartridge, the data bus value, the WRAM port, `$420D`, the NMI
//! latch, the clock and the scheduler. [`BusView`] implements `Bus65816` over it: it
//! decodes each address, syncs the register's owner, performs the access, charges its time,
//! services the deadlines that fell due (ADR 0020) and tells the observer.

mod decode;

use alloc::{boxed::Box, vec};
use core::fmt;

use sfes_api::{Addr24, BusId, Buttons, Cycles, Observer, SplitMix64};
use sfes_snes_cart::Cartridge;
use sfes_snes_cpu::{Bus65816, Pending, Pins};

use crate::{
    bus::decode::{BBus, FAST, Target, access_cycles, decode},
    timing::{Event, REFRESH_LENGTH, Timing},
};

/// The size of WRAM, banks `$7E–$7F`.
pub(crate) const WRAM_SIZE: usize = 0x2_0000;
/// The number of controller ports.
pub(crate) const PORTS: usize = 2;

/// The WRAM port's address is 17 bits (ADR 0019, point 5).
const WRAM_PORT_MASK: u32 = 0x1_FFFF;
/// `$2133` SETINI on the B bus, whose interlace and overscan bits the scheduler keeps.
const SCREEN_MODE: u8 = 0x33;
/// `$420D` MEMSEL's one bit, FastROM select.
const FAST_ROM: u8 = 0x01;

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
    /// The last value placed on the data bus, which a read nothing answers returns (MDR).
    pub(crate) open_bus_value: u8,
    /// $2181–$2183 WMADDL, WMADDM, WMADDH: the 17-bit WRAM address `$2180` accesses.
    pub(crate) wram_port_address: u32,
    /// $420D MEMSEL bit 0: Memory-2 is Fast when set.
    pub(crate) fast_rom: bool,
    /// The NMI edge latch (ADR 0004). Nothing sets it until #82.
    pub(crate) nmi_latched: bool,
    /// The IRQ line, asserted when set. Nothing sets it until #82.
    pub(crate) irq_line: bool,
    /// Master cycles since power-on.
    pub(crate) now: Cycles,
    /// Line and frame position, the refresh schedule and the deadline table.
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
            .field("wram_port_address", &self.wram_port_address)
            .field("fast_rom", &self.fast_rom)
            .field("nmi_latched", &self.nmi_latched)
            .field("irq_line", &self.irq_line)
            .field("now", &self.now)
            .field("timing", &self.timing)
            .field("input", &self.input)
            .finish_non_exhaustive()
    }
}

impl Bus {
    /// The bus at power-on: WRAM filled from `seed` (ADR 0018), the clock at 0 and the
    /// registers at their reset values (ADR 0019).
    // @source: vigna-splitmix
    // @source: fullsnes#snesiomap
    // @source: adr-0018
    // @source: adr-0019
    pub(crate) fn power_on(cartridge: Cartridge, timing: Timing, seed: u64) -> Self {
        let mut wram = vec![0; WRAM_SIZE].into_boxed_slice();
        SplitMix64::new(seed).fill(&mut wram);
        Self {
            wram,
            cartridge,
            open_bus_value: 0,
            wram_port_address: 0,
            fast_rom: false,
            nmi_latched: false,
            irq_line: false,
            now: Cycles(0),
            timing,
            input: [Buttons::empty(); PORTS],
        }
    }

    /// Sets the registers reset changes to their reset values: `$2181–$2183` and `$420D`
    /// are 0 (`fullsnes#snesiomap`, ADR 0019). WRAM and the data bus are kept (ADR 0018).
    // @source: fullsnes#snesiomap
    // @source: fullsnes#snesmemorycontrol
    // @source: adr-0019
    pub(crate) const fn reset(&mut self) {
        self.wram_port_address = 0;
        self.fast_rom = false;
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
    /// cost, and whether the read would have side effects.
    // @source: anomie-memmap#memory-map
    // @source: anomie-memmap#hardware/data-bus
    // @source: anomie-regs#2180
    // @source: adr-0019
    pub(crate) fn peek(&self, address: u32) -> (u8, bool) {
        match decode(self.cartridge.map(), address) {
            Target::BBus(BBus::WramData) => (self.wram_port_byte(), true),
            target => (self.memory(target).unwrap_or(self.open_bus_value), false),
        }
    }

    /// The byte a memory target holds, or `None` for a register or an unanswered address.
    // @source: anomie-memmap#memory-map
    fn memory(&self, target: Target) -> Option<u8> {
        match target {
            Target::Wram(offset) => self.wram.get(offset as usize).copied(),
            Target::Rom(offset) => self.cartridge.rom().get(offset as usize).copied(),
            Target::CartRam(offset) => self.cartridge.ram().get(offset as usize).copied(),
            Target::BBus(_) | Target::Cpu(_) | Target::Unanswered => None,
        }
    }

    /// The WRAM byte at the WRAM port's address.
    // @source: anomie-regs#2180
    // @source: fullsnes#snesmemoryworkramaccess
    fn wram_port_byte(&self) -> u8 {
        self.wram
            .get(self.wram_port_address as usize)
            .copied()
            .unwrap_or(self.open_bus_value)
    }

    /// Moves the WRAM port's address on by one, wrapping at 17 bits (ADR 0019, point 5).
    // @source: anomie-regs#2180
    // @source: anomie-regs#2181
    // @source: adr-0019
    const fn advance_wram_port(&mut self) {
        self.wram_port_address = (self.wram_port_address + 1) & WRAM_PORT_MASK;
    }

    /// Reads `target` with its side effects. `None` is a read nothing answers, or one of a
    /// write-only register: the data bus keeps its value (ADR 0019, point 4).
    // @source: anomie-memmap#memory-map
    // @source: anomie-memmap#hardware/data-bus
    // @source: anomie-regs#2180
    // @source: anomie-regs#2181
    // @source: fullsnes#snesiomap
    // @source: adr-0019
    fn read_target(&mut self, target: Target) -> Option<u8> {
        match target {
            Target::BBus(device) => match device {
                BBus::Ppu(_) => {
                    self.sync_ppu();
                    None
                }
                BBus::Apu(_) => {
                    self.sync_apu();
                    None
                }
                BBus::WramData => {
                    let value = self.wram_port_byte();
                    self.advance_wram_port();
                    Some(value)
                }
                BBus::WramAddressLow
                | BBus::WramAddressMiddle
                | BBus::WramAddressHigh
                | BBus::Unanswered => None,
            },
            // $420D and every other internal register is write-only or not yet modelled.
            Target::Cpu(_) => None,
            memory => self.memory(memory),
        }
    }

    /// Writes `value` to `target` with its side effects. Writes nothing answers are lost.
    // @source: anomie-memmap#memory-map
    // @source: anomie-regs#2180
    // @source: anomie-regs#2181
    // @source: anomie-regs#420d
    // @source: fullsnes#snesmemorycontrol
    // @source: adr-0019
    fn write_target(&mut self, target: Target, value: u8) {
        match target {
            Target::Wram(offset) => {
                if let Some(byte) = self.wram.get_mut(offset as usize) {
                    *byte = value;
                }
            }
            Target::CartRam(offset) => {
                if let Some(byte) = self.cartridge.ram_mut().get_mut(offset as usize) {
                    *byte = value;
                }
            }
            Target::BBus(device) => self.write_b_bus(device, value),
            Target::Cpu(0x420D) => self.fast_rom = value & FAST_ROM != 0,
            Target::Rom(_) | Target::Cpu(_) | Target::Unanswered => {}
        }
    }

    /// Writes `value` to a B-bus device.
    // @source: anomie-memmap#memory-map
    // @source: anomie-regs#2180
    // @source: anomie-regs#2181
    // @source: fullsnes#snesmemoryworkramaccess
    // @source: anomie-regs#2133
    // @source: adr-0019
    // @source: adr-0020
    fn write_b_bus(&mut self, device: BBus, value: u8) {
        let [low, middle, high, _] = self.wram_port_address.to_le_bytes();
        match device {
            BBus::Ppu(register) => {
                self.sync_ppu();
                if register == SCREEN_MODE {
                    self.timing.write_screen_mode(value);
                }
            }
            BBus::Apu(_) => self.sync_apu(),
            BBus::WramData => {
                if let Some(byte) = self.wram.get_mut(self.wram_port_address as usize) {
                    *byte = value;
                }
                self.advance_wram_port();
            }
            BBus::WramAddressLow => {
                self.wram_port_address = u32::from_le_bytes([value, middle, high, 0]);
            }
            BBus::WramAddressMiddle => {
                self.wram_port_address = u32::from_le_bytes([low, value, high, 0]);
            }
            BBus::WramAddressHigh => {
                self.wram_port_address = u32::from_le_bytes([low, middle, value & 0x01, 0]);
            }
            BBus::Unanswered => {}
        }
    }

    /// Runs the PPU up to `now` before an access to its registers (technical design section
    /// 4, catch-up rule 1). There is no PPU to run until the PPU issues land.
    // @source: adr-0019
    #[expect(
        clippy::unused_self,
        reason = "the hook the PPU's catch-up fills in (ADR 0019, point 2)"
    )]
    const fn sync_ppu(&mut self) {}

    /// Runs the APU up to `now` before an access to its ports (technical design section 4,
    /// catch-up rule 1). There is no APU to run until the APU issues land.
    // @source: adr-0019
    #[expect(
        clippy::unused_self,
        reason = "the hook the APU's catch-up fills in (ADR 0019, point 2)"
    )]
    const fn sync_apu(&mut self) {}

    /// Ends a CPU cycle of `cycles` master cycles: advances the clock, then services in time
    /// order every deadline that fell due during the cycle or at its end (ADR 0020,
    /// point 9). A refresh pauses the CPU (ADR 0019, point 6); a line start is reported to
    /// `observer`, after the frame end it may bring (ADR 0020, point 8).
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: adr-0001
    // @source: adr-0019
    // @source: adr-0020
    fn charge<O: Observer>(&mut self, cycles: u64, observer: &mut O) {
        self.now = Cycles(self.now.0 + cycles);
        while self.now >= self.timing.next_deadline() {
            match self.timing.service(self.now) {
                Some(Event::Refresh) => self.now = Cycles(self.now.0 + REFRESH_LENGTH),
                Some(Event::Line {
                    line,
                    at,
                    frame_ended,
                }) => {
                    if O::ACTIVE {
                        if frame_ended {
                            observer.on_frame_end(self.timing.frames());
                        }
                        observer.on_scanline(line, at);
                    }
                }
                None => break,
            }
        }
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
    // @source: anomie-timing#s-cpu/instructions
    // @source: adr-0004
    // @source: adr-0019
    // @source: adr-0020
    fn read(&mut self, address: Addr24, pins: Pins) -> u8 {
        let at = self.bus.now;
        let raw = address.get();
        let target = decode(self.bus.cartridge.map(), raw);
        let value = match self.bus.read_target(target) {
            Some(value) => {
                self.bus.open_bus_value = value;
                value
            }
            None => self.bus.open_bus_value,
        };
        if pins.contains(Pins::VPB) && (raw == NMI_VECTOR_NATIVE || raw == NMI_VECTOR_EMULATION) {
            self.bus.nmi_latched = false;
        }
        if self.first_read.is_none() {
            self.first_read = Some(value);
        }
        // The observer hears of the access before the deadlines it carries the clock past.
        if O::ACTIVE {
            self.observer.on_read(BusId::Cpu, raw, value, at);
        }
        self.bus
            .charge(access_cycles(address, self.bus.fast_rom), self.observer);
        value
    }

    // @source: anomie-memmap#memory-map
    // @source: anomie-memmap#hardware/data-bus
    // @source: anomie-timing#s-cpu/instructions
    // @source: adr-0004
    // @source: adr-0006
    // @source: adr-0019
    // @source: adr-0020
    fn write(&mut self, address: Addr24, value: u8, _pins: Pins) {
        let at = self.bus.now;
        let raw = address.get();
        let target = decode(self.bus.cartridge.map(), raw);
        // The access's speed is the one in force when it starts, so a write to $420D takes
        // effect from the next access.
        let cycles = access_cycles(address, self.bus.fast_rom);
        self.bus.write_target(target, value);
        self.bus.open_bus_value = value;
        if O::ACTIVE {
            self.observer.on_write(BusId::Cpu, raw, value, at);
        }
        self.bus.charge(cycles, self.observer);
    }

    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: adr-0004
    // @source: adr-0019
    // @source: adr-0020
    fn idle(&mut self, _address: Addr24, _pins: Pins) {
        self.bus.charge(FAST, self.observer);
    }

    // @source: adr-0004
    fn interrupts(&self) -> Pending {
        self.bus.pending()
    }
}

#[cfg(test)]
mod tests;
