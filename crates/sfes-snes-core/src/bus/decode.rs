// @source: anomie-memmap#memory-map
// @source: anomie-memmap#hardware/address-bus-b
// @source: fullsnes#snesiomap
// @source: adr-0019
//! Address decode: which device answers a CPU address, and how long the access takes
//! (ADR 0019, points 1 and 3).

use sfes_api::Addr24;
use sfes_snes_cart::{MemoryMap, Page};

/// An internal-operation cycle, and a Fast access.
pub(crate) const FAST: u64 = 6;
/// A Slow access.
pub(crate) const SLOW: u64 = 8;
/// An `XSlow` access.
pub(crate) const XSLOW: u64 = 12;

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
    /// One of the CPU's internal registers at `$4000–$43FF`, by its 16-bit address.
    Cpu(u16),
    /// Nothing on the console answers: the read sees open bus.
    Unanswered,
}

/// The devices on Address Bus B, by B-bus address.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BBus {
    /// PPU1 and PPU2, `$00–$3F`; the low six bits select the register.
    Ppu(u8),
    /// The APU's four ports, `$40–$7F`; the low two bits select the port.
    Apu(u8),
    /// `$80` WMDATA: WRAM through the port's address.
    WramData,
    /// `$81` WMADDL: bits 0–7 of the WRAM port's address.
    WramAddressLow,
    /// `$82` WMADDM: bits 8–15 of the WRAM port's address.
    WramAddressMiddle,
    /// `$83` WMADDH: bit 16 of the WRAM port's address.
    WramAddressHigh,
    /// `$84–$FF`: nothing on the console answers.
    Unanswered,
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
const fn decode_system_area(offset: u16) -> Target {
    match offset {
        0x2100..=0x21FF => Target::BBus(decode_b_bus(offset.to_le_bytes()[0])),
        0x4000..=0x43FF => Target::Cpu(offset),
        // $2000-$20FF, $2200-$3FFF and $4400-$5FFF are Address Bus A, which only a
        // cartridge could answer.
        _ => Target::Unanswered,
    }
}

/// The device at B-bus address `address`.
// @source: anomie-memmap#memory-map
// @source: anomie-memmap#hardware/address-bus-b
// @source: anomie-regs#2140
// @source: adr-0019
const fn decode_b_bus(address: u8) -> BBus {
    match address {
        0x00..=0x3F => BBus::Ppu(address),
        0x40..=0x7F => BBus::Apu(address & 0x03),
        0x80 => BBus::WramData,
        0x81 => BBus::WramAddressLow,
        0x82 => BBus::WramAddressMiddle,
        0x83 => BBus::WramAddressHigh,
        _ => BBus::Unanswered,
    }
}

/// The master cycles an access to `address` takes: anomie-memmap's speed column, with
/// Memory-2 Fast while `$420D` bit 0 is set (ADR 0019, point 3).
// @source: anomie-memmap#memory-map
// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: fullsnes#snesmemorycontrol
// @source: adr-0019
pub(crate) const fn access_cycles(address: Addr24, fast_rom: bool) -> u64 {
    let memory_2 = if fast_rom { FAST } else { SLOW };
    match address.bank() {
        0x00..=0x3F | 0x80..=0xBF => match address.offset() {
            0x2000..=0x3FFF | 0x4200..=0x5FFF => FAST,
            0x4000..=0x41FF => XSLOW,
            0x8000..=0xFFFF if address.bank() >= 0x80 => memory_2,
            // $0000-$1FFF, $6000-$7FFF, and $8000-$FFFF in banks $00-$3F.
            _ => SLOW,
        },
        0x40..=0x7F => SLOW,
        _ => memory_2,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: anomie-memmap#memory-map
    // @source: fullsnes#snesmemorycontrol
    #[test]
    fn access_times_follow_the_speed_column_and_memsel() {
        // (address, Slow ROM, Fast ROM)
        let cases = [
            (0x00_0000, SLOW, SLOW),
            (0x00_1FFF, SLOW, SLOW),
            (0x00_2000, FAST, FAST),
            (0x00_2100, FAST, FAST),
            (0x00_3FFF, FAST, FAST),
            (0x00_4000, XSLOW, XSLOW),
            (0x00_41FF, XSLOW, XSLOW),
            (0x00_4200, FAST, FAST),
            (0x00_5FFF, FAST, FAST),
            (0x00_6000, SLOW, SLOW),
            (0x00_8000, SLOW, SLOW),
            (0x3F_FFFF, SLOW, SLOW),
            (0x40_0000, SLOW, SLOW),
            (0x7D_FFFF, SLOW, SLOW),
            (0x7E_0000, SLOW, SLOW),
            (0x7F_FFFF, SLOW, SLOW),
            (0x80_0000, SLOW, SLOW),
            (0x80_4016, XSLOW, XSLOW),
            (0x80_7FFF, SLOW, SLOW),
            (0x80_8000, SLOW, FAST),
            (0xBF_FFFF, SLOW, FAST),
            (0xC0_0000, SLOW, FAST),
            (0xFF_FFFF, SLOW, FAST),
        ];
        for (address, slow, fast) in cases {
            let address = Addr24::new(address);
            assert_eq!(access_cycles(address, false), slow, "{:06X}", address.get());
            assert_eq!(access_cycles(address, true), fast, "{:06X}", address.get());
        }
    }

    // @source: anomie-memmap#memory-map
    // @source: anomie-memmap#hardware/address-bus-b
    // @source: fullsnes#snesiomap
    #[test]
    fn the_system_area_decodes_to_the_b_bus_and_the_cpu_registers() {
        let cases = [
            (0x2000, Target::Unanswered),
            (0x2100, Target::BBus(BBus::Ppu(0x00))),
            (0x213F, Target::BBus(BBus::Ppu(0x3F))),
            (0x2140, Target::BBus(BBus::Apu(0))),
            (0x2147, Target::BBus(BBus::Apu(3))),
            (0x217F, Target::BBus(BBus::Apu(3))),
            (0x2180, Target::BBus(BBus::WramData)),
            (0x2181, Target::BBus(BBus::WramAddressLow)),
            (0x2182, Target::BBus(BBus::WramAddressMiddle)),
            (0x2183, Target::BBus(BBus::WramAddressHigh)),
            (0x2184, Target::BBus(BBus::Unanswered)),
            (0x21FF, Target::BBus(BBus::Unanswered)),
            (0x2200, Target::Unanswered),
            (0x4000, Target::Cpu(0x4000)),
            (0x420D, Target::Cpu(0x420D)),
            (0x43FF, Target::Cpu(0x43FF)),
            (0x4400, Target::Unanswered),
            (0x5FFF, Target::Unanswered),
        ];
        for (offset, target) in cases {
            assert_eq!(decode_system_area(offset), target, "{offset:04X}");
        }
    }
}
