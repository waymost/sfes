// @source: anomie-memmap#hardware/data-bus
// @source: anomie-memmap#memory-map
// @source: adr-0004
// @source: adr-0018
// @source: adr-0019
// @source: adr-0020
// @source: adr-0021
// @source: adr-0023
// @source: adr-0024
// @source: adr-0025
// @source: adr-0026
// @source: adr-0027
// @source: adr-0028
//! The bus state and the main CPU's view of it (ADR 0019).
//!
//! [`Bus`] owns WRAM, the cartridge, the data bus value, the WRAM port, `$420D`, the
//! interrupt state, `$4201`, the PPU and its video outputs (ADR 0028), the multiply and
//! divide unit (ADR 0023), the controller ports (ADR 0024), the DMA channels and HDMA
//! (ADR 0025, ADR 0026), the APU ports' placeholder (ADR 0027), the clock and the
//! scheduler. [`BusView`]
//! implements `Bus65816` over it: it records the interrupt lines (ADR 0021), decodes each
//! address, syncs the register's owner, performs the access, charges its time, services the
//! deadlines that fell due (ADR 0020) and tells the observer.

mod apu;
mod decode;
mod dma;
mod interrupts;
mod math;
mod ports;
mod ppu;

use alloc::{boxed::Box, vec};
use core::fmt;

use sfes_api::{Addr24, BusId, Cycles, Field, Observer, SplitMix64};
use sfes_snes_cart::{Cartridge, WRAM_SIZE};
use sfes_snes_cpu::{Bus65816, Pending, Pins};
use sfes_snes_ppu::{Ppu, Register};

use crate::{
    bus::{
        apu::Apu,
        decode::{BBus, CpuRegister, Speed, Target, access_speed, decode},
        dma::Dma,
        interrupts::Interrupts,
        math::Math,
        ports::{Port, PortRegister, Ports},
    },
    timing::{Event, REFRESH_LENGTH, Timing, WRIO_LATCH_DELAY},
    video::Video,
};

/// The WRAM port's address is 17 bits (ADR 0019, point 5).
const WRAM_PORT_MASK: u32 = 0x1_FFFF;
/// $420D MEMSEL's one bit, FastROM select.
const FAST_ROM: u8 = 0x01;
/// $4201 WRIO bit 7, the PPU latch line.
const PPU_LATCH: u8 = 0x80;
/// `$4201` at power-on and reset (ADR 0021, point 8).
const IO_PORT_RESET: u8 = 0xFF;
/// `$4210` bits 3–0: the 5A22 version (ADR 0021, point 4).
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

/// A unit's registers as the bus reaches them: the bus decodes an address to one of the
/// unit's registers and forwards the access (ADR 0019, point 1).
pub(crate) trait Registers {
    /// Which of the unit's registers an access names.
    type Register: Copy;

    /// What a read of `register` returns, without its side effects; `None` where open bus
    /// answers, as for a write-only register.
    // @source: anomie-memmap#hardware/data-bus
    // @source: adr-0019
    fn peek(&self, register: Self::Register) -> Option<u8>;

    /// The side effects of a read of `register`, once [`Registers::peek`] has given its
    /// value. Most registers have none.
    // @source: adr-0019
    fn after_read(&mut self, register: Self::Register) {
        let _ = register;
    }

    /// A read of `register`: its value, then its side effects. The bus itself peeks and
    /// carries out the side effects in two steps, since some registers' bytes take bits from
    /// outside the unit (see `Bus::read_target`); tests read a unit directly.
    // @source: adr-0019
    #[cfg(test)]
    fn read(&mut self, register: Self::Register) -> Option<u8> {
        let value = self.peek(register);
        self.after_read(register);
        value
    }

    /// A write of `value` to `register`; a write to a read-only register is lost.
    // @source: adr-0019
    fn write(&mut self, register: Self::Register, value: u8);
}

/// Everything on the CPU's side of the machine except the CPU itself. Its clone copies the
/// state and starts with empty video outputs.
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
    /// $4200 NMITIMEN bit 7, the NMI and IRQ flags, the NMI edge latch and the recorded
    /// lines (ADR 0004, ADR 0021).
    pub(crate) interrupts: Interrupts,
    /// $4201 WRIO: the programmable I/O port, whose bit 7 drives the PPU latch line.
    pub(crate) io_port: u8,
    /// PPU1 and PPU2: VRAM, CGRAM, OAM, the registers at `$2100–$213F` and the H/V counter
    /// latch (ADR 0021, point 7; ADR 0028).
    pub(crate) ppu: Ppu,
    /// The frame being drawn and the last presented frame: outputs, not state, so a clone
    /// of the bus starts with `Video::default()` (ADR 0028, point 10).
    pub(crate) video: Video,
    /// $4202–$4206 and $4214–$4217: the multiply and divide unit (ADR 0023).
    pub(crate) math: Math,
    /// The pads, $4016 JOYSER0, the auto-joypad read into $4218–$421F and the polled flag
    /// (ADR 0024).
    pub(crate) ports: Ports,
    /// $420B MDMAEN, $420C HDMAEN and $4300–$437F: the DMA channels, a requested transfer
    /// and HDMA's state (ADR 0025, ADR 0026).
    pub(crate) dma: Dma,
    /// $2140–$2143 APUIO0–APUIO3: the APU ports, a placeholder until M3 (ADR 0027).
    pub(crate) apu: Apu,
    /// Master cycles since power-on.
    pub(crate) now: Cycles,
    /// Line and frame position, the refresh schedule and the deadline table.
    pub(crate) timing: Timing,
}

impl Clone for Bus {
    /// Copies every field but `video`, which starts empty: outputs are not state, and
    /// snapshots that never render never pay for frame buffers (technical design section 5;
    /// ADR 0028, point 10).
    fn clone(&self) -> Self {
        let Self {
            wram,
            cartridge,
            open_bus_value,
            wram_port_address,
            fast_rom,
            interrupts,
            io_port,
            ppu,
            video: _,
            math,
            ports,
            dma,
            apu,
            now,
            timing,
        } = self;
        Self {
            wram: wram.clone(),
            cartridge: cartridge.clone(),
            open_bus_value: *open_bus_value,
            wram_port_address: *wram_port_address,
            fast_rom: *fast_rom,
            interrupts: *interrupts,
            io_port: *io_port,
            ppu: ppu.clone(),
            video: Video::default(),
            math: *math,
            ports: *ports,
            dma: *dma,
            apu: *apu,
            now: *now,
            timing: *timing,
        }
    }
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
            .field("ppu", &self.ppu)
            .field("math", &self.math)
            .field("ports", &self.ports)
            .field("dma", &self.dma)
            .field("apu", &self.apu)
            .field("now", &self.now)
            .field("timing", &self.timing)
            .finish_non_exhaustive()
    }
}

impl Bus {
    /// The bus at power-on: WRAM, then VRAM, CGRAM and OAM, filled from one stream seeded
    /// with `seed` (ADR 0018; ADR 0028, point 1), the clock at 0 and the registers at their
    /// reset values (ADR 0019).
    // @source: fullsnes#snesiomap
    // @source: vigna-splitmix
    // @source: adr-0018
    // @source: adr-0019
    // @source: adr-0021
    // @source: adr-0023
    // @source: adr-0024
    // @source: adr-0025
    // @source: adr-0028
    pub(crate) fn power_on(cartridge: Cartridge, timing: Timing, seed: u64) -> Self {
        let mut wram = vec![0; WRAM_SIZE as usize].into_boxed_slice();
        let mut random = SplitMix64::new(seed);
        random.fill(&mut wram);
        let ppu = Ppu::power_on(&mut random);
        Self {
            wram,
            cartridge,
            open_bus_value: 0,
            wram_port_address: 0,
            fast_rom: false,
            interrupts: Interrupts::power_on(),
            io_port: IO_PORT_RESET,
            ppu,
            video: Video::default(),
            math: Math::power_on(),
            ports: Ports::power_on(),
            dma: Dma::power_on(),
            apu: Apu::power_on(),
            now: Cycles(0),
            timing,
        }
    }

    /// The reset button. The timing position returns to the start of a frame (ADR 0018,
    /// ADR 0020), and the registers reset changes take their reset values:
    ///
    /// - `$2181–$2183` and `$420D` are 0 (`fullsnes#snesiomap`, ADR 0019);
    /// - `$4200` and the interrupt flags are clear and `$4201` is `$FF` (ADR 0021, point 8);
    /// - `$4016` bit 0 and the auto-read are off, and a read in progress is abandoned
    ///   (ADR 0024, points 3, 5 and 7);
    /// - `$420B` is 0 and a transfer not yet started is dropped (ADR 0025, point 2), and
    ///   `$420C` is 0 with HDMA's flags clear (ADR 0026, point 1);
    /// - the APU ports are `$00` (ADR 0027, point 6);
    /// - `$2133` is 0 and the frame being drawn is abandoned (ADR 0028, points 5 and 10).
    ///
    /// WRAM, the data bus, the PPU's memories and the counter latch, the multiply and divide
    /// unit, `$4218–$421F` and the DMA channels' registers are kept (ADR 0018, ADR 0021,
    /// ADR 0023, ADR 0024, ADR 0025, ADR 0028).
    // @source: anomie-regs#4202
    // @source: anomie-regs#4204
    // @source: anomie-regs#420b
    // @source: anomie-regs#420c
    // @source: anomie-regs#420d
    // @source: anomie-regs#43x0
    // @source: anomie-timing
    // @source: fullsnes#snesiomap
    // @source: fullsnes#snesmemorycontrol
    // @source: adr-0018
    // @source: adr-0019
    // @source: adr-0020
    // @source: adr-0021
    // @source: adr-0023
    // @source: adr-0024
    // @source: adr-0025
    // @source: adr-0026
    // @source: adr-0027
    // @source: adr-0028
    pub(crate) fn reset(&mut self) {
        self.timing.restart(self.now);
        self.wram_port_address = 0;
        self.fast_rom = false;
        self.interrupts = Interrupts::power_on();
        self.io_port = IO_PORT_RESET;
        self.ports.reset();
        self.dma.reset();
        self.apu.reset();
        self.ppu.reset();
        self.video.abandon();
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
    // @source: anomie-memmap#hardware/data-bus
    // @source: anomie-memmap#memory-map
    // @source: adr-0019
    pub(crate) fn peek(&self, address: u32) -> (u8, bool) {
        let (value, side_effects) = self.peek_target(decode(self.cartridge.map(), address));
        (value.unwrap_or(self.open_bus_value), side_effects)
    }

    /// What a read of `target` returns without its side effects, `None` where nothing
    /// answers or the register is write-only (ADR 0019, point 4), and whether the read has
    /// side effects, which [`Bus::after_read`] carries out.
    // @source: anomie-memmap#hardware/data-bus
    // @source: anomie-memmap#memory-map
    // @source: anomie-regs#2137
    // @source: anomie-regs#2140
    // @source: anomie-regs#2180
    // @source: anomie-regs#2181
    // @source: anomie-regs#4016
    // @source: anomie-regs#4017
    // @source: anomie-regs#4210
    // @source: anomie-regs#4211
    // @source: anomie-regs#4212
    // @source: anomie-regs#4213
    // @source: anomie-regs#4214
    // @source: anomie-regs#4216
    // @source: anomie-regs#4218
    // @source: anomie-regs#43x0
    // @source: anomie-regs#registers
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: fullsnes#snesiomap
    // @source: adr-0019
    // @source: adr-0021
    // @source: adr-0023
    // @source: adr-0024
    // @source: adr-0025
    // @source: adr-0027
    // @source: adr-0028
    fn peek_target(&self, target: Target) -> (Option<u8>, bool) {
        match target {
            Target::Wram(offset) => (self.wram.get(offset as usize).copied(), false),
            Target::Rom(offset) => (self.cartridge.rom().get(offset as usize).copied(), false),
            Target::CartRam(offset) => (self.cartridge.ram().get(offset as usize).copied(), false),
            Target::BBus(device) => match device {
                BBus::Ppu(Register::SoftwareLatch) => (None, true),
                BBus::Ppu(register) => (
                    self.ppu.peek(register),
                    Ppu::read_has_side_effects(register),
                ),
                BBus::Apu(port) => (self.apu.peek(port), false),
                BBus::WramData => (
                    self.wram.get(self.wram_port_address as usize).copied(),
                    true,
                ),
                BBus::WramAddressLow
                | BBus::WramAddressMiddle
                | BBus::WramAddressHigh
                | BBus::Unanswered => (None, false),
            },
            Target::Cpu(register) => match register {
                CpuRegister::NmiFlag => (Some(self.nmi_flag_byte()), true),
                CpuRegister::IrqFlag => (Some(self.irq_flag_byte()), true),
                CpuRegister::Status => (Some(self.status_byte()), false),
                CpuRegister::IoPortInput => (Some(self.io_port_input()), false),
                CpuRegister::Math(register) => (self.math.peek(register), false),
                CpuRegister::Ports(register @ PortRegister::Serial(port)) => {
                    let data = self.ports.peek(register).unwrap_or_default();
                    (Some(self.serial_byte(port, data)), true)
                }
                CpuRegister::Ports(register @ PortRegister::Joypad(_)) => {
                    (self.ports.peek(register), false)
                }
                CpuRegister::Dma(register) => (self.dma.peek(register), false),
                CpuRegister::InterruptEnable
                | CpuRegister::IoPort
                | CpuRegister::Timer(_)
                | CpuRegister::MemorySelect => (None, false),
            },
            Target::Unanswered => (None, false),
        }
    }

    /// The side effects of a read of `target`, after [`Bus::peek_target`] has given its
    /// value: `$2137` latches the counters while `$4201` bit 7 is set, the PPU's registers
    /// carry out theirs (ADR 0028, point 4), `$2180` moves the WRAM port's address on, `$4210` and `$4211` clear their
    /// flags, and the controller registers poll and clock the pads.
    // @source: anomie-regs#2137
    // @source: anomie-regs#213c
    // @source: anomie-regs#2180
    // @source: anomie-regs#4016
    // @source: anomie-regs#4210
    // @source: anomie-regs#4211
    // @source: anomie-regs#4218
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0019
    // @source: adr-0021
    // @source: adr-0024
    // @source: adr-0028
    fn after_read(&mut self, target: Target) {
        match target {
            Target::BBus(BBus::Ppu(Register::SoftwareLatch)) => {
                if self.io_port & PPU_LATCH != 0 {
                    let (line, dot) = self.timing.position(self.now);
                    self.ppu.latch_counters(line, dot);
                }
            }
            Target::BBus(BBus::Ppu(register)) => self.ppu.after_read(register),
            Target::BBus(BBus::Apu(port)) => self.apu.after_read(port),
            Target::BBus(BBus::WramData) => self.advance_wram_port(),
            Target::Cpu(CpuRegister::NmiFlag) => self.interrupts.clear_nmi_flag(),
            Target::Cpu(CpuRegister::IrqFlag) => self.interrupts.clear_irq_flag(),
            Target::Cpu(CpuRegister::Ports(register)) => self.ports.after_read(register),
            _ => {}
        }
    }

    /// $4210 RDNMI: the NMI flag, version 2, bits 6–4 open bus (ADR 0021, point 4).
    // @source: anomie-regs#4210
    // @source: fullsnes#snesppuinterrupts
    // @source: adr-0021
    const fn nmi_flag_byte(&self) -> u8 {
        let flag = if self.interrupts.nmi_flag() { FLAG } else { 0 };
        flag | CPU_VERSION | (self.open_bus_value & NMI_FLAG_OPEN_BUS)
    }

    /// $4211 TIMEUP: the IRQ flag, bits 6–0 open bus (ADR 0021, point 5).
    // @source: anomie-regs#4211
    // @source: adr-0021
    const fn irq_flag_byte(&self) -> u8 {
        let flag = if self.interrupts.irq_flag() { FLAG } else { 0 };
        flag | (self.open_bus_value & IRQ_FLAG_OPEN_BUS)
    }

    /// $4212 HVBJOY at the start of the access: vertical and horizontal blank, bits 5–1
    /// open bus (ADR 0021, point 6), and auto-joypad busy (ADR 0024, point 6).
    // @source: anomie-regs#4212
    // @source: anomie-timing#s-cpu/auto-joypad-read
    // @source: anomie-timing#s-cpu/registers
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

    /// $4016 JOYSER0 or $4017 JOYSER1 with `port`'s data bits `data`: at `$4016` bits 7–2
    /// open bus, at `$4017` bits 4–2 set and bits 7–5 open bus (ADR 0024, point 4).
    // @source: anomie-regs
    // @source: anomie-regs#4016
    // @source: anomie-regs#4017
    // @source: fullsnes#snescontrollersioportsmanualreading
    // @source: adr-0024
    const fn serial_byte(&self, port: Port, data: u8) -> u8 {
        match port {
            Port::One => (self.open_bus_value & PORT_1_OPEN_BUS) | data,
            Port::Two => (self.open_bus_value & PORT_2_OPEN_BUS) | PORT_2_GROUNDED | data,
        }
    }

    /// $4213 RDIO: each line of the I/O port, low where `$4201` drives it low or a device
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

    /// Moves the WRAM port's address on by one, wrapping at 17 bits (ADR 0019, point 5).
    // @source: anomie-regs#2180
    // @source: anomie-regs#2181
    // @source: adr-0019
    const fn advance_wram_port(&mut self) {
        self.wram_port_address = (self.wram_port_address + 1) & WRAM_PORT_MASK;
    }

    /// Reads `target` with its side effects: syncs the register's owner, takes the value
    /// [`Bus::peek_target`] gives, then carries out [`Bus::after_read`]. `None` is a read
    /// nothing answers, or one of a write-only register: the data bus keeps its value
    /// (ADR 0019, point 4).
    // @source: anomie-memmap#hardware/data-bus
    // @source: adr-0019
    pub(crate) fn read_target(&mut self, target: Target) -> Option<u8> {
        self.sync_owner(target);
        let (value, _) = self.peek_target(target);
        self.after_read(target);
        value
    }

    /// Writes `value` to `target` with its side effects. Writes nothing answers are lost.
    // @source: anomie-memmap#memory-map
    // @source: anomie-regs#4016
    // @source: anomie-regs#4200
    // @source: anomie-regs#4201
    // @source: anomie-regs#4202
    // @source: anomie-regs#4207
    // @source: anomie-regs#420b
    // @source: anomie-regs#420c
    // @source: anomie-regs#420d
    // @source: anomie-regs#43x0
    // @source: fullsnes#snesdmaandhdmachannel07registers
    // @source: fullsnes#snesmemorycontrol
    // @source: adr-0019
    // @source: adr-0021
    // @source: adr-0023
    // @source: adr-0024
    // @source: adr-0025
    // @source: adr-0026
    pub(crate) fn write_target(&mut self, target: Target, value: u8) {
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
            Target::Cpu(register) => match register {
                CpuRegister::Ports(register) => self.ports.write(register, value),
                CpuRegister::InterruptEnable => {
                    self.interrupts.write_enable(value);
                    self.timing.write_interrupt_enable(value, self.now);
                    self.ports.write_auto_read_enable(value);
                }
                CpuRegister::IoPort => self.write_io_port(value),
                CpuRegister::Math(register) => self.math.write(register, value),
                CpuRegister::Timer(register) => self.timing.write_timer(register, value, self.now),
                CpuRegister::Dma(register) => self.dma.write(register, value),
                CpuRegister::MemorySelect => self.fast_rom = value & FAST_ROM != 0,
                CpuRegister::NmiFlag
                | CpuRegister::IrqFlag
                | CpuRegister::Status
                | CpuRegister::IoPortInput => {}
            },
            Target::Rom(_) | Target::Unanswered => {}
        }
    }

    /// $4201 WRIO: a change of bit 7 from 1 to 0 latches the counters one dot after the
    /// access starts (ADR 0021, points 3 and 7).
    // @source: anomie-regs#4201
    // @source: anomie-timing
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0021
    fn write_io_port(&mut self, value: u8) {
        if self.io_port & PPU_LATCH != 0 && value & PPU_LATCH == 0 {
            self.timing.schedule_latch(self.now + WRIO_LATCH_DELAY);
        }
        self.io_port = value;
    }

    /// Writes `value` to a B-bus device, after syncing its owner.
    // @source: anomie-memmap#memory-map
    // @source: anomie-regs#2133
    // @source: anomie-regs#2140
    // @source: anomie-regs#2180
    // @source: anomie-regs#2181
    // @source: fullsnes#snesmemoryworkramaccess
    // @source: adr-0019
    // @source: adr-0020
    // @source: adr-0027
    // @source: adr-0028
    pub(crate) fn write_b_bus(&mut self, device: BBus, value: u8) {
        self.sync_owner(Target::BBus(device));
        let [low, middle, high, _] = self.wram_port_address.to_le_bytes();
        match device {
            BBus::Ppu(register) => {
                if register == Register::ScreenMode {
                    self.timing.write_screen_mode(value, self.now);
                }
                self.ppu.write(register, value);
            }
            BBus::Apu(port) => self.apu.write(port, value),
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

    /// Runs the chip that owns `target`'s register up to `now` before an access: the PPU
    /// for `$2100–$213F` and the APU for `$2140–$217F` (technical design section 4,
    /// catch-up rule 1; ADR 0019, point 2).
    // @source: adr-0019
    // @source: adr-0027
    // @source: adr-0028
    fn sync_owner(&mut self, target: Target) {
        match target {
            Target::BBus(BBus::Ppu(_)) => self.sync_ppu(),
            Target::BBus(BBus::Apu(_)) => self.apu.run_until(self.now),
            _ => {}
        }
    }

    /// Runs the PPU up to `now` before an access to its registers: renders every output
    /// line that has started and not been drawn (technical design section 4, catch-up rule
    /// 2; ADR 0028, point 6). Vertical blank outputs nothing.
    // @source: anomie-timing#s-ppu/rendering
    // @source: adr-0019
    // @source: adr-0028
    fn sync_ppu(&mut self) {
        if !self.timing.in_vertical_blank() {
            self.render_through(self.timing.line());
        }
    }

    /// Renders the lines from the PPU's next line through `last` (ADR 0028, point 6).
    // @source: anomie-timing#s-ppu/rendering
    // @source: adr-0028
    fn render_through(&mut self, last: u16) {
        while self.ppu.next_line() <= last {
            let line = self.ppu.next_line();
            self.video.render(&mut self.ppu, line);
        }
    }

    /// Vertical blank has begun on `line`: the PPU renders the lines left before it, the
    /// frame is presented with the field it shows and the next frame starts from line 1
    /// (ADR 0028, points 6 and 8).
    // @source: anomie-regs#213f
    // @source: anomie-timing#s-ppu/blanking-periods
    // @source: anomie-timing#s-ppu/rendering
    // @source: adr-0028
    fn end_frame_video(&mut self, line: u16) {
        let last = line.saturating_sub(1);
        self.render_through(last);
        let field = if self.timing.field(self.now) {
            Field::Second
        } else {
            Field::First
        };
        self.video.present(last, field);
        self.ppu.start_frame();
    }

    /// Ends a CPU cycle at `speed`: the multiply and divide unit counts it (ADR 0023,
    /// point 2), and so does a requested DMA transfer's countdown to its pause (ADR 0025,
    /// point 3); then the clock advances past it, with [`Bus::advance`].
    // @source: anomie-timing#s-cpu/dma
    // @source: anomie-timing#s-cpu/registers
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: adr-0023
    // @source: adr-0025
    fn charge<O: Observer>(&mut self, speed: Speed, observer: &mut O) {
        self.math.clock();
        self.dma.cycle_ended();
        self.advance(speed.cycles(), observer);
    }

    /// Advances the clock by `cycles` master cycles of a CPU cycle or a DMA transfer, then
    /// services in time order every deadline that fell due during them or at their end
    /// (ADR 0020, point 9; ADR 0025, point 9):
    ///
    /// - a refresh pauses the CPU (ADR 0019, point 6);
    /// - a line start is reported to `observer`, after the frame end it may bring
    ///   (ADR 0020, point 8); line 0 clears the NMI flag, and a frame end presents the
    ///   frame (ADR 0028, point 8) and closes its polled flag (ADR 0024, point 8);
    /// - the NMI flag, a trigger point and a `$4201` latch change the interrupt state and
    ///   the counters (ADR 0021);
    /// - the auto-read takes its next step (ADR 0024, points 6 and 9);
    /// - an HDMA step is marked due, to run at the next point the bus can pause (ADR 0026,
    ///   point 3).
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: anomie-timing#s-cpu/interrupts
    // @source: adr-0001
    // @source: adr-0019
    // @source: adr-0020
    // @source: adr-0021
    // @source: adr-0024
    // @source: adr-0025
    // @source: adr-0026
    // @source: adr-0028
    fn advance<O: Observer>(&mut self, cycles: u64, observer: &mut O) {
        self.now += cycles;
        while self.now >= self.timing.next_deadline() {
            match self.timing.service(self.now) {
                Some(Event::Refresh) => self.now += REFRESH_LENGTH,
                Some(Event::Line {
                    line,
                    at,
                    frame_ended,
                }) => {
                    if line == 0 {
                        self.interrupts.end_vertical_blank();
                    }
                    if frame_ended {
                        self.end_frame_video(line);
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
                Some(Event::Latch { line, dot }) => self.ppu.latch_counters(line, dot),
                Some(Event::Joypad { at }) => {
                    let next = self.ports.auto_read_event(at);
                    self.timing.schedule_joypad(next);
                }
                Some(Event::Hdma(step)) => self.dma.hdma_fell_due(step),
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
    // @source: anomie-memmap#hardware/data-bus
    // @source: anomie-memmap#memory-map
    // @source: anomie-timing#s-cpu/dma
    // @source: anomie-timing#s-cpu/hdma
    // @source: anomie-timing#s-cpu/instructions
    // @source: adr-0004
    // @source: adr-0019
    // @source: adr-0020
    // @source: adr-0021
    // @source: adr-0025
    // @source: adr-0026
    fn read(&mut self, address: Addr24, pins: Pins) -> u8 {
        let speed = access_speed(address, self.bus.fast_rom);
        self.bus.start_due_pause(speed, self.observer);
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
        self.bus.charge(speed, self.observer);
        value
    }

    // @source: anomie-memmap#hardware/data-bus
    // @source: anomie-memmap#memory-map
    // @source: anomie-timing#s-cpu/dma
    // @source: anomie-timing#s-cpu/hdma
    // @source: anomie-timing#s-cpu/instructions
    // @source: adr-0004
    // @source: adr-0006
    // @source: adr-0019
    // @source: adr-0020
    // @source: adr-0021
    // @source: adr-0025
    // @source: adr-0026
    fn write(&mut self, address: Addr24, value: u8, _pins: Pins) {
        // The access's speed is the one in force when it starts, so a write to $420D takes
        // effect from the next access.
        let speed = access_speed(address, self.bus.fast_rom);
        self.bus.start_due_pause(speed, self.observer);
        self.bus.interrupts.record();
        let at = self.bus.now;
        let raw = address.get();
        let target = decode(self.bus.cartridge.map(), raw);
        self.bus.write_target(target, value);
        self.bus.open_bus_value = value;
        if O::ACTIVE {
            self.observer.on_write(BusId::Cpu, raw, value, at);
        }
        self.bus.charge(speed, self.observer);
    }

    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: anomie-timing#s-cpu/dma
    // @source: anomie-timing#s-cpu/hdma
    // @source: adr-0004
    // @source: adr-0019
    // @source: adr-0020
    // @source: adr-0021
    // @source: adr-0025
    // @source: adr-0026
    fn idle(&mut self, _address: Addr24, _pins: Pins) {
        self.bus.start_due_pause(Speed::Fast, self.observer);
        self.bus.interrupts.record();
        self.bus.charge(Speed::Fast, self.observer);
    }

    // @source: adr-0004
    // @source: adr-0021
    fn interrupts(&self) -> Pending {
        self.bus.pending()
    }
}

#[cfg(test)]
mod tests;
