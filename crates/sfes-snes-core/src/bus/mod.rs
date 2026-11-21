// @source: anomie-memmap#memory-map
// @source: anomie-memmap#hardware/data-bus
// @source: adr-0004
// @source: adr-0018
// @source: adr-0019
// @source: adr-0020
// @source: adr-0021
// @source: adr-0023
// @source: adr-0024
//! The bus state and the main CPU's view of it (ADR 0019).
//!
//! [`Bus`] owns WRAM, the cartridge, the data bus value, the WRAM port, `$420D`, the
//! interrupt state, `$4201`, the counter latch, the multiply and divide unit (ADR 0023), the
//! controller ports (ADR 0024), the clock and the scheduler. [`BusView`]
//! implements `Bus65816` over it: it records the interrupt lines (ADR 0021), decodes each
//! address, syncs the register's owner, performs the access, charges its time, services the
//! deadlines that fell due (ADR 0020) and tells the observer.

mod counters;
mod decode;
mod interrupts;
mod math;
mod ports;

use alloc::{boxed::Box, vec};
use core::fmt;

use sfes_api::{Addr24, BusId, Cycles, Observer, SplitMix64};
use sfes_snes_cart::Cartridge;
use sfes_snes_cpu::{Bus65816, Pending, Pins};

use crate::{
    bus::{
        counters::CounterLatch,
        decode::{BBus, FAST, Target, access_cycles, decode},
        interrupts::Interrupts,
        math::Math,
        ports::Ports,
    },
    timing::{Event, REFRESH_LENGTH, Timing, WRIO_LATCH_DELAY},
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
/// `$2137` SLHV on the B bus.
const SOFTWARE_LATCH: u8 = 0x37;
/// `$213C` OPHCT on the B bus.
const HORIZONTAL_COUNTER: u8 = 0x3C;
/// `$213D` OPVCT on the B bus.
const VERTICAL_COUNTER: u8 = 0x3D;
/// `$4201` WRIO bit 7, the PPU latch line.
const PPU_LATCH: u8 = 0x80;
/// `$4201` at power-on and reset (ADR 0021, point 8).
const IO_PORT_RESET: u8 = 0xFF;
/// `$4210` bits 3-0: the 5A22 version (ADR 0021, point 4).
const CPU_VERSION: u8 = 0x02;
/// Bit 7 of `$4210`, `$4211` and `$4212`: a flag.
const FLAG: u8 = 0x80;
/// `$4212` bit 6: horizontal blank.
const HBLANK: u8 = 0x40;
/// The open-bus bits of `$4210`.
const NMI_FLAG_OPEN_BUS: u8 = 0x70;
/// The open-bus bits of `$4211`.
const IRQ_FLAG_OPEN_BUS: u8 = 0x7F;
/// The open-bus bits of `$4212`.
const STATUS_OPEN_BUS: u8 = 0x3E;
/// `$4212` bit 0: auto-joypad read busy.
const AUTO_READ_BUSY: u8 = 0x01;
/// The open-bus bits of `$4016`.
const PORT_1_OPEN_BUS: u8 = 0xFC;
/// The open-bus bits of `$4017`.
const PORT_2_OPEN_BUS: u8 = 0xE0;
/// `$4017` bits 4–2, tied to ground and read as 1.
const PORT_2_GROUNDED: u8 = 0x1C;

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
    /// `$4200` bit 7, the NMI and IRQ flags, the NMI edge latch and the recorded lines
    /// (ADR 0004, ADR 0021).
    pub(crate) interrupts: Interrupts,
    /// $4201 WRIO: the programmable I/O port, whose bit 7 drives the PPU latch line.
    pub(crate) io_port: u8,
    /// $213C–$213D OPHCT, OPVCT: the latched H/V counters (ADR 0021, point 7).
    pub(crate) counters: CounterLatch,
    /// $4202–$4206 and $4214–$4217: the multiply and divide unit (ADR 0023).
    pub(crate) math: Math,
    /// The pads, $4016, the auto-joypad read into $4218–$421F and the polled flag
    /// (ADR 0024).
    pub(crate) ports: Ports,
    /// Master cycles since power-on.
    pub(crate) now: Cycles,
    /// Line and frame position, the refresh schedule and the deadline table.
    pub(crate) timing: Timing,
}

impl fmt::Debug for Bus {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Bus")
            .field("cartridge", &self.cartridge.header())
            .field("open_bus_value", &self.open_bus_value)
            .field("wram_port_address", &self.wram_port_address)
            .field("fast_rom", &self.fast_rom)
            .field("interrupts", &self.interrupts)
            .field("io_port", &self.io_port)
            .field("counters", &self.counters)
            .field("math", &self.math)
            .field("ports", &self.ports)
            .field("now", &self.now)
            .field("timing", &self.timing)
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
    // @source: adr-0021
    // @source: adr-0023
    // @source: adr-0024
    pub(crate) fn power_on(cartridge: Cartridge, timing: Timing, seed: u64) -> Self {
        let mut wram = vec![0; WRAM_SIZE].into_boxed_slice();
        SplitMix64::new(seed).fill(&mut wram);
        Self {
            wram,
            cartridge,
            open_bus_value: 0,
            wram_port_address: 0,
            fast_rom: false,
            interrupts: Interrupts::power_on(),
            io_port: IO_PORT_RESET,
            counters: CounterLatch::power_on(),
            math: Math::power_on(),
            ports: Ports::power_on(),
            now: Cycles(0),
            timing,
        }
    }

    /// Sets the registers reset changes to their reset values: `$2181–$2183` and `$420D`
    /// are 0 (`fullsnes#snesiomap`, ADR 0019), `$4200` and the interrupt flags are clear and
    /// `$4201` is `$FF` (ADR 0021, point 8), and `$4016` bit 0 and the auto-read are off with
    /// a read in progress abandoned (ADR 0024, points 3, 5 and 7). WRAM, the data bus, the
    /// counter latch, the multiply and divide unit and `$4218–$421F` are kept (ADR 0018,
    /// ADR 0021, ADR 0023, ADR 0024).
    // @source: fullsnes#snesiomap
    // @source: fullsnes#snesmemorycontrol
    // @source: anomie-regs#4202
    // @source: anomie-regs#4204
    // @source: adr-0019
    // @source: adr-0021
    // @source: adr-0023
    // @source: adr-0024
    pub(crate) fn reset(&mut self) {
        self.wram_port_address = 0;
        self.fast_rom = false;
        self.interrupts = Interrupts::power_on();
        self.io_port = IO_PORT_RESET;
        self.ports.reset();
    }

    /// The interrupt requests as the CPU sees them at an instruction boundary: those
    /// recorded at the start of the step's last cycle (ADR 0004; ADR 0021, point 1).
    // @source: adr-0004
    // @source: adr-0021
    pub(crate) const fn pending(&self) -> Pending {
        self.interrupts.recorded()
    }

    /// The byte at `address` as a read would see it, without the read's side effects or
    /// cost, and whether the read would have side effects.
    // @source: anomie-memmap#memory-map
    // @source: anomie-memmap#hardware/data-bus
    // @source: anomie-regs#2180
    // @source: anomie-regs#2137
    // @source: anomie-regs#213c
    // @source: anomie-regs#4210
    // @source: anomie-regs#4211
    // @source: anomie-regs#4212
    // @source: anomie-regs#4213
    // @source: anomie-regs#4214
    // @source: anomie-regs#4216
    // @source: anomie-regs#4016
    // @source: anomie-regs#4017
    // @source: anomie-regs#4218
    // @source: adr-0019
    // @source: adr-0021
    // @source: adr-0023
    // @source: adr-0024
    pub(crate) fn peek(&self, address: u32) -> (u8, bool) {
        match decode(self.cartridge.map(), address) {
            Target::BBus(BBus::WramData) => (self.wram_port_byte(), true),
            Target::BBus(BBus::Ppu(SOFTWARE_LATCH)) => (self.open_bus_value, true),
            Target::BBus(BBus::Ppu(HORIZONTAL_COUNTER)) => (self.counters.peek_horizontal(), true),
            Target::BBus(BBus::Ppu(VERTICAL_COUNTER)) => (self.counters.peek_vertical(), true),
            Target::Cpu(0x4210) => (self.nmi_flag_byte(self.interrupts.nmi_flag()), true),
            Target::Cpu(0x4211) => (self.irq_flag_byte(self.interrupts.irq_flag()), true),
            Target::Cpu(0x4212) => (self.status_byte(), false),
            Target::Cpu(register @ 0x4213..=0x4217) => (self.port_or_result(register), false),
            Target::Cpu(0x4016) => (self.port_1_byte(self.ports.peek_data(0)), true),
            Target::Cpu(0x4017) => (self.port_2_byte(self.ports.peek_data(1)), true),
            Target::Cpu(register @ 0x4218..=0x421F) => {
                (self.ports.joypad(register - 0x4218), false)
            }
            target => (self.memory(target).unwrap_or(self.open_bus_value), false),
        }
    }

    /// `$4210` RDNMI with the NMI flag `flag`: version 2, bits 6–4 open bus (ADR 0021,
    /// point 4).
    // @source: anomie-regs#4210
    // @source: fullsnes#snesppuinterrupts
    // @source: adr-0021
    const fn nmi_flag_byte(&self, flag: bool) -> u8 {
        let flag = if flag { FLAG } else { 0 };
        flag | CPU_VERSION | (self.open_bus_value & NMI_FLAG_OPEN_BUS)
    }

    /// `$4211` TIMEUP with the IRQ flag `flag`: bits 6–0 open bus (ADR 0021, point 5).
    // @source: anomie-regs#4211
    // @source: adr-0021
    const fn irq_flag_byte(&self, flag: bool) -> u8 {
        let flag = if flag { FLAG } else { 0 };
        flag | (self.open_bus_value & IRQ_FLAG_OPEN_BUS)
    }

    /// `$4212` HVBJOY at the start of the access: vertical and horizontal blank, bits 5–1
    /// open bus (ADR 0021, point 6), and auto-joypad busy (ADR 0024, point 6).
    // @source: anomie-timing#s-cpu/registers
    // @source: anomie-timing#s-cpu/auto-joypad-read
    // @source: anomie-regs#4212
    // @source: fullsnes#snesppuinterrupts
    // @source: adr-0021
    // @source: adr-0024
    const fn status_byte(&self) -> u8 {
        let vertical = if self.timing.in_vertical_blank() {
            FLAG
        } else {
            0
        };
        let horizontal = if self.timing.in_horizontal_blank(self.now) {
            HBLANK
        } else {
            0
        };
        let busy = if self.ports.auto_read_busy() {
            AUTO_READ_BUSY
        } else {
            0
        };
        vertical | horizontal | busy | (self.open_bus_value & STATUS_OPEN_BUS)
    }

    /// `$4016` JOYSER0 with port 1's data bits `data`: bits 7–2 open bus (ADR 0024,
    /// point 4).
    // @source: anomie-regs
    // @source: anomie-regs#4016
    // @source: fullsnes#snescontrollersioportsmanualreading
    // @source: adr-0024
    const fn port_1_byte(&self, data: u8) -> u8 {
        (self.open_bus_value & PORT_1_OPEN_BUS) | data
    }

    /// `$4017` JOYSER1 with port 2's data bits `data`: bits 4–2 set and bits 7–5 open bus
    /// (ADR 0024, point 4).
    // @source: anomie-regs
    // @source: anomie-regs#4017
    // @source: fullsnes#snescontrollersioportsmanualreading
    // @source: adr-0024
    const fn port_2_byte(&self, data: u8) -> u8 {
        (self.open_bus_value & PORT_2_OPEN_BUS) | PORT_2_GROUNDED | data
    }

    /// `$4213` RDIO or one of the results `$4214–$4217`, none of which a read changes.
    // @source: anomie-regs#4213
    // @source: anomie-regs#4214
    // @source: anomie-regs#4215
    // @source: anomie-regs#4216
    // @source: anomie-regs#4217
    // @source: adr-0023
    const fn port_or_result(&self, register: u16) -> u8 {
        match register {
            0x4213 => self.io_port_input(),
            0x4214 => self.math.quotient_low(),
            0x4215 => self.math.quotient_high(),
            0x4216 => self.math.product_low(),
            _ => self.math.product_high(),
        }
    }

    /// `$4213` RDIO: each line of the I/O port, low where `$4201` drives it low or a device
    /// pulls it low. A standard pad leaves `IOBit` unconnected (ADR 0024, point 2), so it
    /// reads back `$4201` (ADR 0023, point 6).
    // @source: anomie-regs#4213
    // @source: anomie-regs#controllers/joypads
    // @source: fullsnes#snescontrollersioportsmanualreading
    // @source: adr-0023
    // @source: adr-0024
    const fn io_port_input(&self) -> u8 {
        self.io_port
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
    // @source: fullsnes#snespputimersandstatus
    // @source: anomie-regs#2137
    // @source: anomie-regs#213c
    // @source: anomie-regs#213d
    // @source: anomie-regs#4210
    // @source: anomie-regs#4211
    // @source: anomie-regs#4212
    // @source: anomie-regs#4213
    // @source: anomie-regs#4214
    // @source: anomie-regs#4216
    // @source: anomie-regs#4016
    // @source: anomie-regs#4017
    // @source: anomie-regs#4218
    // @source: adr-0019
    // @source: adr-0021
    // @source: adr-0023
    // @source: adr-0024
    fn read_target(&mut self, target: Target) -> Option<u8> {
        match target {
            Target::BBus(device) => match device {
                BBus::Ppu(register) => {
                    self.sync_ppu();
                    match register {
                        SOFTWARE_LATCH => {
                            if self.io_port & PPU_LATCH != 0 {
                                let (line, dot) = self.timing.position(self.now);
                                self.counters.latch(line, dot);
                            }
                            None
                        }
                        HORIZONTAL_COUNTER => Some(self.counters.read_horizontal()),
                        VERTICAL_COUNTER => Some(self.counters.read_vertical()),
                        _ => None,
                    }
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
            Target::Cpu(0x4210) => {
                let flag = self.interrupts.read_nmi_flag();
                Some(self.nmi_flag_byte(flag))
            }
            Target::Cpu(0x4211) => {
                let flag = self.interrupts.read_irq_flag();
                Some(self.irq_flag_byte(flag))
            }
            Target::Cpu(0x4212) => Some(self.status_byte()),
            Target::Cpu(register @ 0x4213..=0x4217) => Some(self.port_or_result(register)),
            Target::Cpu(0x4016) => {
                let data = self.ports.read_data(0);
                Some(self.port_1_byte(data))
            }
            Target::Cpu(0x4017) => {
                let data = self.ports.read_data(1);
                Some(self.port_2_byte(data))
            }
            Target::Cpu(register @ 0x4218..=0x421F) => {
                Some(self.ports.read_joypad(register - 0x4218))
            }
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
    // @source: anomie-regs#4200
    // @source: anomie-regs#4201
    // @source: anomie-regs#4202
    // @source: anomie-regs#4203
    // @source: anomie-regs#4204
    // @source: anomie-regs#4205
    // @source: anomie-regs#4206
    // @source: anomie-regs#4207
    // @source: anomie-regs#4208
    // @source: anomie-regs#4209
    // @source: anomie-regs#420a
    // @source: anomie-regs#4016
    // @source: adr-0019
    // @source: adr-0021
    // @source: adr-0023
    // @source: adr-0024
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
            Target::Cpu(0x4016) => self.ports.write_latch(value),
            Target::Cpu(0x4200) => {
                self.interrupts.write_enable(value);
                self.timing.write_interrupt_enable(value, self.now);
                self.ports.write_auto_read_enable(value);
            }
            Target::Cpu(0x4201) => self.write_io_port(value),
            Target::Cpu(0x4202) => self.math.write_multiplicand(value),
            Target::Cpu(0x4203) => self.math.write_multiplier(value),
            Target::Cpu(0x4204) => self.math.write_dividend_low(value),
            Target::Cpu(0x4205) => self.math.write_dividend_high(value),
            Target::Cpu(0x4206) => self.math.write_divisor(value),
            Target::Cpu(0x4207) => self.timing.write_horizontal_timer_low(value, self.now),
            Target::Cpu(0x4208) => self.timing.write_horizontal_timer_high(value, self.now),
            Target::Cpu(0x4209) => self.timing.write_vertical_timer_low(value, self.now),
            Target::Cpu(0x420A) => self.timing.write_vertical_timer_high(value, self.now),
            Target::Cpu(0x420D) => self.fast_rom = value & FAST_ROM != 0,
            Target::Rom(_) | Target::Cpu(_) | Target::Unanswered => {}
        }
    }

    /// A `$4201` WRIO write: a change of bit 7 from 1 to 0 latches the counters one dot
    /// after the access starts (ADR 0021, points 3 and 7).
    // @source: anomie-timing
    // @source: anomie-regs#4201
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0021
    fn write_io_port(&mut self, value: u8) {
        if self.io_port & PPU_LATCH != 0 && value & PPU_LATCH == 0 {
            self.timing
                .schedule_latch(Cycles(self.now.0 + WRIO_LATCH_DELAY));
        }
        self.io_port = value;
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
                    self.timing.write_screen_mode(value, self.now);
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
    /// `observer`, after the frame end it may bring (ADR 0020, point 8), and line 0 clears
    /// the NMI flag; the NMI flag, a trigger point and a `$4201` latch change the interrupt
    /// state and the counters (ADR 0021). A frame end closes the frame's polled flag and the
    /// auto-read takes its next step (ADR 0024, points 6, 8 and 9). The multiply and divide
    /// unit counts the cycle (ADR 0023, point 2).
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: anomie-timing#s-cpu/interrupts
    // @source: anomie-timing#s-cpu/registers
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: adr-0001
    // @source: adr-0019
    // @source: adr-0020
    // @source: adr-0021
    // @source: adr-0023
    // @source: adr-0024
    fn charge<O: Observer>(&mut self, cycles: u64, observer: &mut O) {
        self.math.clock();
        self.now = Cycles(self.now.0 + cycles);
        while self.now >= self.timing.next_deadline() {
            match self.timing.service(self.now) {
                Some(Event::Refresh) => self.now = Cycles(self.now.0 + REFRESH_LENGTH),
                Some(Event::Line {
                    line,
                    at,
                    frame_ended,
                }) => {
                    if line == 0 {
                        self.interrupts.end_vertical_blank();
                    }
                    if frame_ended {
                        self.ports.end_frame();
                    }
                    if O::ACTIVE {
                        if frame_ended {
                            observer.on_frame_end(self.timing.frames());
                        }
                        observer.on_scanline(line, at);
                    }
                }
                Some(Event::Nmi) => self.interrupts.set_nmi_flag(),
                Some(Event::Irq) => self.interrupts.set_irq_flag(),
                Some(Event::Latch { line, dot }) => self.counters.latch(line, dot),
                Some(Event::Joypad { at }) => {
                    let next = self.ports.auto_read_event(at);
                    self.timing.schedule_joypad(next);
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
    // @source: adr-0021
    fn read(&mut self, address: Addr24, pins: Pins) -> u8 {
        self.bus.interrupts.record();
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
            self.bus.interrupts.acknowledge_nmi();
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
    // @source: adr-0021
    fn write(&mut self, address: Addr24, value: u8, _pins: Pins) {
        self.bus.interrupts.record();
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
    // @source: adr-0021
    fn idle(&mut self, _address: Addr24, _pins: Pins) {
        self.bus.interrupts.record();
        self.bus.charge(FAST, self.observer);
    }

    // @source: adr-0004
    // @source: adr-0021
    fn interrupts(&self) -> Pending {
        self.bus.pending()
    }
}

#[cfg(test)]
mod tests;
