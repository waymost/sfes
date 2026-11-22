// @source: anomie-memmap#hardware/address-bus-b
// @source: anomie-memmap#memory-map
// @source: fullsnes#snesiomap
// @source: adr-0019
//! Address decode: which device and register answer a CPU address, and how long the access
//! takes (ADR 0019, points 1 and 3).

use sfes_api::Addr24;
use sfes_snes_cart::{MemoryMap, Page};

use crate::{
    bus::{
        apu::ApuPort,
        counters::CounterRegister,
        dma::{Dma, DmaRegister},
        math::MathRegister,
        ports::{JoypadByte, Port, PortRegister},
    },
    timing::TimerRegister,
};

/// How long a CPU cycle takes, in master cycles: anomie-memmap's speed column
/// (`anomie-timing#s-cpu/clocks-and-refresh`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Speed {
    /// A Fast access, and every internal-operation cycle.
    Fast = 6,
    /// A Slow access.
    Slow = 8,
    /// An `XSlow` access: the joypad ports at `$4000–$41FF`.
    XSlow = 12,
}

impl Speed {
    /// The master cycles a cycle at this speed takes.
    // @source: anomie-timing#s-cpu/clocks-and-refresh
    pub(crate) const fn cycles(self) -> u64 {
        self as u64
    }
}

/// What answers an address on the CPU's bus.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Target {
    /// Work RAM, at this offset, through Address Bus A and /WRAM.
    Wram(u32),
    /// Cartridge ROM, at this offset.
    Rom(u32),
    /// Cartridge RAM, at this offset.
    CartRam(u32),
    /// A device on Address Bus B, through `$2100–$21FF`.
    BBus(BBus),
    /// One of the CPU's internal registers at `$4000–$43FF`.
    Cpu(CpuRegister),
    /// Nothing on the console answers: the read sees open bus.
    Unanswered,
}

/// The devices on Address Bus B, by B-bus address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BBus {
    /// PPU1 and PPU2, `$00–$3F`.
    Ppu(PpuRegister),
    /// The APU's four ports, `$40–$7F`; the low two bits select the port.
    Apu(ApuPort),
    /// $2180 WMDATA: WRAM through the port's address.
    WramData,
    /// $2181 WMADDL: bits 0–7 of the WRAM port's address.
    WramAddressLow,
    /// $2182 WMADDM: bits 8–15 of the WRAM port's address.
    WramAddressMiddle,
    /// $2183 WMADDH: bit 16 of the WRAM port's address.
    WramAddressHigh,
    /// `$84–$FF`: nothing on the console answers.
    Unanswered,
}

/// The PPU registers the bus models before the PPU exists.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PpuRegister {
    /// $2133 SETINI: the interlace and overscan bits the scheduler keeps (ADR 0020, point 6).
    ScreenMode,
    /// $2137 SLHV: a read latches the H/V counters (ADR 0021, point 7).
    SoftwareLatch,
    /// $213C OPHCT and $213D OPVCT: the latched counters (ADR 0021, point 7).
    Counter(CounterRegister),
    /// Any other PPU register, by its B-bus address; open bus until the PPU exists.
    Other(u8),
}

/// The CPU's internal registers, `$4000–$43FF`, that answer.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CpuRegister {
    /// $4016 JOYSER0, $4017 JOYSER1 and $4218–$421F JOY1L–JOY4H (ADR 0024).
    Ports(PortRegister),
    /// $4200 NMITIMEN: NMI enable, IRQ mode and auto-joypad read enable.
    InterruptEnable,
    /// $4201 WRIO: the programmable I/O port.
    IoPort,
    /// $4202–$4206 WRMPYA–WRDIVB and $4214–$4217 RDDIVL–RDMPYH (ADR 0023).
    Math(MathRegister),
    /// $4207–$420A HTIMEL–VTIMEH (ADR 0021).
    Timer(TimerRegister),
    /// $420B MDMAEN, $420C HDMAEN and $4300–$437F, the channels (ADR 0025, ADR 0026).
    Dma(DmaRegister),
    /// $420D MEMSEL: FastROM select.
    MemorySelect,
    /// $4210 RDNMI: the NMI flag and the CPU version.
    NmiFlag,
    /// $4211 TIMEUP: the IRQ flag.
    IrqFlag,
    /// $4212 HVBJOY: blanking and auto-joypad busy.
    Status,
    /// $4213 RDIO: the programmable I/O port's lines.
    IoPortInput,
}

/// The device that answers `address`, from the cartridge's memory map and, inside the
/// console's I/O pages, the system-area table (ADR 0019, point 1).
// @source: anomie-memmap#memory-map
// @source: fullsnes#snesiomap
// @source: adr-0017
// @source: adr-0019
pub(crate) fn decode(map: &MemoryMap, address: u32) -> Target {
    match map.resolve(address) {
        Page::Wram(offset) => Target::Wram(offset),
        Page::Rom(offset) => Target::Rom(offset),
        Page::CartRam(offset) => Target::CartRam(offset),
        Page::Io => decode_system_area(Addr24::new(address).offset()),
        Page::Coproc(_) | Page::Open => Target::Unanswered,
    }
}

/// The device at `offset` in the I/O pages `$2000–$5FFF` of a system-area bank.
// @source: anomie-memmap#memory-map
// @source: fullsnes#snesiomap
// @source: adr-0019
#[expect(
    clippy::cast_possible_truncation,
    reason = "the B-bus address and the joypad register are the offset's low byte"
)]
const fn decode_system_area(offset: u16) -> Target {
    match offset {
        0x2100..=0x21FF => Target::BBus(decode_b_bus(offset as u8)),
        0x4000..=0x43FF => match decode_cpu_register(offset) {
            Some(register) => Target::Cpu(register),
            None => Target::Unanswered,
        },
        // $2000-$20FF, $2200-$3FFF and $4400-$5FFF are Address Bus A, which only a
        // cartridge could answer.
        _ => Target::Unanswered,
    }
}

/// The internal register at `offset` in `$4000–$43FF`, or `None` where nothing answers.
// @source: anomie-regs#4016
// @source: anomie-regs#4017
// @source: anomie-regs#4200
// @source: anomie-regs#4218
// @source: anomie-regs#43x0
// @source: fullsnes#snesiomap
// @source: adr-0019
// @source: adr-0023
// @source: adr-0024
// @source: adr-0025
#[expect(
    clippy::cast_possible_truncation,
    reason = "the B-bus address and the joypad register are the offset's low byte"
)]
const fn decode_cpu_register(offset: u16) -> Option<CpuRegister> {
    let register = match offset {
        0x4016 => CpuRegister::Ports(PortRegister::Serial(Port::One)),
        0x4017 => CpuRegister::Ports(PortRegister::Serial(Port::Two)),
        0x4200 => CpuRegister::InterruptEnable,
        0x4201 => CpuRegister::IoPort,
        0x4202 => CpuRegister::Math(MathRegister::Multiplicand),
        0x4203 => CpuRegister::Math(MathRegister::Multiplier),
        0x4204 => CpuRegister::Math(MathRegister::DividendLow),
        0x4205 => CpuRegister::Math(MathRegister::DividendHigh),
        0x4206 => CpuRegister::Math(MathRegister::Divisor),
        0x4207 => CpuRegister::Timer(TimerRegister::HorizontalLow),
        0x4208 => CpuRegister::Timer(TimerRegister::HorizontalHigh),
        0x4209 => CpuRegister::Timer(TimerRegister::VerticalLow),
        0x420A => CpuRegister::Timer(TimerRegister::VerticalHigh),
        0x420B => CpuRegister::Dma(DmaRegister::Enable),
        0x420C => CpuRegister::Dma(DmaRegister::HdmaEnable),
        0x420D => CpuRegister::MemorySelect,
        0x4210 => CpuRegister::NmiFlag,
        0x4211 => CpuRegister::IrqFlag,
        0x4212 => CpuRegister::Status,
        0x4213 => CpuRegister::IoPortInput,
        0x4214 => CpuRegister::Math(MathRegister::QuotientLow),
        0x4215 => CpuRegister::Math(MathRegister::QuotientHigh),
        0x4216 => CpuRegister::Math(MathRegister::ProductLow),
        0x4217 => CpuRegister::Math(MathRegister::ProductHigh),
        0x4218..=0x421F => CpuRegister::Ports(PortRegister::Joypad(JoypadByte::new(offset as u8))),
        0x4300..=0x43FF => match Dma::slot(offset) {
            Some((channel, register)) => CpuRegister::Dma(DmaRegister::Channel(channel, register)),
            None => return None,
        },
        _ => return None,
    };
    Some(register)
}

/// The device at B-bus address `address`.
// @source: anomie-memmap#hardware/address-bus-b
// @source: anomie-memmap#memory-map
// @source: anomie-regs#2133
// @source: anomie-regs#2137
// @source: anomie-regs#213c
// @source: anomie-regs#213d
// @source: anomie-regs#2140
// @source: adr-0019
pub(crate) const fn decode_b_bus(address: u8) -> BBus {
    match address {
        0x33 => BBus::Ppu(PpuRegister::ScreenMode),
        0x37 => BBus::Ppu(PpuRegister::SoftwareLatch),
        0x3C => BBus::Ppu(PpuRegister::Counter(CounterRegister::Horizontal)),
        0x3D => BBus::Ppu(PpuRegister::Counter(CounterRegister::Vertical)),
        0x00..=0x3F => BBus::Ppu(PpuRegister::Other(address)),
        0x40..=0x7F => BBus::Apu(ApuPort::from_b_bus(address)),
        0x80 => BBus::WramData,
        0x81 => BBus::WramAddressLow,
        0x82 => BBus::WramAddressMiddle,
        0x83 => BBus::WramAddressHigh,
        _ => BBus::Unanswered,
    }
}

/// How long an access to `address` takes: anomie-memmap's speed column, with Memory-2 Fast
/// while `$420D` bit 0 is set (ADR 0019, point 3).
// @source: anomie-memmap#memory-map
// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: fullsnes#snesmemorycontrol
// @source: adr-0019
pub(crate) const fn access_speed(address: Addr24, fast_rom: bool) -> Speed {
    let memory_2 = if fast_rom { Speed::Fast } else { Speed::Slow };
    match address.bank() {
        0x00..=0x3F | 0x80..=0xBF => match address.offset() {
            0x2000..=0x3FFF | 0x4200..=0x5FFF => Speed::Fast,
            0x4000..=0x41FF => Speed::XSlow,
            0x8000..=0xFFFF if address.bank() >= 0x80 => memory_2,
            // $0000-$1FFF, $6000-$7FFF, and $8000-$FFFF in banks $00-$3F.
            _ => Speed::Slow,
        },
        0x40..=0x7F => Speed::Slow,
        _ => memory_2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bus::dma::{ChannelRegister, DmaChannel};

    // @source: anomie-memmap#memory-map
    // @source: fullsnes#snesmemorycontrol
    #[test]
    fn access_times_follow_the_speed_column_and_memsel() {
        use Speed::{Fast, Slow, XSlow};
        // (address, Slow ROM, Fast ROM)
        let cases = [
            (0x00_0000, Slow, Slow),
            (0x00_1FFF, Slow, Slow),
            (0x00_2000, Fast, Fast),
            (0x00_2100, Fast, Fast),
            (0x00_3FFF, Fast, Fast),
            (0x00_4000, XSlow, XSlow),
            (0x00_41FF, XSlow, XSlow),
            (0x00_4200, Fast, Fast),
            (0x00_5FFF, Fast, Fast),
            (0x00_6000, Slow, Slow),
            (0x00_8000, Slow, Slow),
            (0x3F_FFFF, Slow, Slow),
            (0x40_0000, Slow, Slow),
            (0x7D_FFFF, Slow, Slow),
            (0x7E_0000, Slow, Slow),
            (0x7F_FFFF, Slow, Slow),
            (0x80_0000, Slow, Slow),
            (0x80_4016, XSlow, XSlow),
            (0x80_7FFF, Slow, Slow),
            (0x80_8000, Slow, Fast),
            (0xBF_FFFF, Slow, Fast),
            (0xC0_0000, Slow, Fast),
            (0xFF_FFFF, Slow, Fast),
        ];
        for (address, slow, fast) in cases {
            let address = Addr24::new(address);
            assert_eq!(access_speed(address, false), slow, "{:06X}", address.get());
            assert_eq!(access_speed(address, true), fast, "{:06X}", address.get());
        }
        assert_eq!([Fast.cycles(), Slow.cycles(), XSlow.cycles()], [6, 8, 12]);
    }

    // @source: anomie-memmap#hardware/address-bus-b
    // @source: anomie-memmap#memory-map
    // @source: anomie-regs#2140
    // @source: fullsnes#snesiomap
    // @source: adr-0019
    #[test]
    fn the_system_area_decodes_to_the_b_bus_and_the_cpu_registers() {
        let cases = [
            (0x2000, Target::Unanswered),
            (0x2100, Target::BBus(BBus::Ppu(PpuRegister::Other(0x00)))),
            (0x2133, Target::BBus(BBus::Ppu(PpuRegister::ScreenMode))),
            (0x2137, Target::BBus(BBus::Ppu(PpuRegister::SoftwareLatch))),
            (
                0x213D,
                Target::BBus(BBus::Ppu(PpuRegister::Counter(CounterRegister::Vertical))),
            ),
            (0x213F, Target::BBus(BBus::Ppu(PpuRegister::Other(0x3F)))),
            (0x2140, Target::BBus(BBus::Apu(ApuPort::Port0))),
            (0x2147, Target::BBus(BBus::Apu(ApuPort::Port3))),
            (0x217F, Target::BBus(BBus::Apu(ApuPort::Port3))),
            (0x2180, Target::BBus(BBus::WramData)),
            (0x2181, Target::BBus(BBus::WramAddressLow)),
            (0x2182, Target::BBus(BBus::WramAddressMiddle)),
            (0x2183, Target::BBus(BBus::WramAddressHigh)),
            (0x2184, Target::BBus(BBus::Unanswered)),
            (0x21FF, Target::BBus(BBus::Unanswered)),
            (0x2200, Target::Unanswered),
            (0x4000, Target::Unanswered),
            (
                0x4017,
                Target::Cpu(CpuRegister::Ports(PortRegister::Serial(Port::Two))),
            ),
            (0x420D, Target::Cpu(CpuRegister::MemorySelect)),
            (
                0x421F,
                Target::Cpu(CpuRegister::Ports(PortRegister::Joypad(JoypadByte::new(7)))),
            ),
            (
                0x437F,
                Target::Cpu(CpuRegister::Dma(DmaRegister::Channel(
                    DmaChannel::new(7),
                    ChannelRegister::Unused,
                ))),
            ),
            (0x430C, Target::Unanswered),
            (0x43FF, Target::Unanswered),
            (0x4400, Target::Unanswered),
            (0x5FFF, Target::Unanswered),
        ];
        for (offset, target) in cases {
            assert_eq!(decode_system_area(offset), target, "{offset:04X}");
        }
    }
}
