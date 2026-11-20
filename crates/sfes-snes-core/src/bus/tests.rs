// @source: adr-0019
//! Unit tests of the bus: decode, the WRAM port, open bus, access speed and refresh.

use alloc::vec;

use sfes_api::{Addr24, Cycles, NoopObserver};
use sfes_snes_cart::{Cartridge, Options, Region};
use sfes_snes_cpu::{Bus65816, Pins};

use super::*;

/// A bus over a 32 KiB LoROM image whose byte at ROM offset `n` is `n & $FF`.
// @source: fullsnes#snescartridgeromheader
// @source: adr-0017
fn bus() -> Bus {
    let mut image = vec![0; 0x8000];
    for (index, byte) in image.iter_mut().enumerate() {
        *byte = index.to_le_bytes()[0];
    }
    image[0x7FC0..0x7FD5].fill(b' ');
    image[0x7FD5..].fill(0);
    image[0x7FD5] = 0x20;
    image[0x7FD7] = 0x08;
    image[0x7FD9] = 0x01;
    image[0x7FFC..0x7FFE].copy_from_slice(&[0x00, 0x80]);
    let cartridge = Cartridge::load(&image, Options::default()).expect("loads");
    Bus::power_on(cartridge, Timing::new(Region::Ntsc, Cycles(0)), 0)
}

// @source: adr-0019
fn read(bus: &mut Bus, address: u32) -> u8 {
    BusView::new(bus, &mut NoopObserver).read(Addr24::new(address), Pins::empty())
}

// @source: adr-0019
fn write(bus: &mut Bus, address: u32, value: u8) {
    BusView::new(bus, &mut NoopObserver).write(Addr24::new(address), value, Pins::empty());
}

// @source: adr-0019
fn idle(bus: &mut Bus) {
    BusView::new(bus, &mut NoopObserver).idle(Addr24::new(0), Pins::empty());
}

/// The master cycles one read of `address` takes, away from any refresh.
// @source: adr-0019
fn read_cost(bus: &mut Bus, address: u32) -> u64 {
    let before = bus.now.0;
    read(bus, address);
    bus.now.0 - before
}

// @source: anomie-regs#2180
// @source: anomie-regs#2181
// @source: fullsnes#snesmemoryworkramaccess
#[test]
fn the_wram_port_reads_and_writes_wram_and_advances() {
    let mut bus = bus();
    write(&mut bus, 0x00_2181, 0x34);
    write(&mut bus, 0x00_2182, 0x12);
    write(&mut bus, 0x00_2183, 0xFF);
    // Only bit 0 of WMADDH is kept: a 17-bit address.
    assert_eq!(bus.wram_port_address, 0x1_1234);
    write(&mut bus, 0x00_2180, 0x5A);
    write(&mut bus, 0x80_2180, 0xC3);
    assert_eq!(bus.wram[0x1_1234], 0x5A);
    assert_eq!(bus.wram[0x1_1235], 0xC3);
    assert_eq!(bus.wram_port_address, 0x1_1236);
    write(&mut bus, 0x00_2181, 0x34);
    assert_eq!(read(&mut bus, 0x00_2180), 0x5A);
    assert_eq!(read(&mut bus, 0x00_2180), 0xC3);
    assert_eq!(bus.wram_port_address, 0x1_1236);
    // The same bytes through Address Bus A, in bank $7F.
    assert_eq!(read(&mut bus, 0x7F_1234), 0x5A);
}

// @source: anomie-regs#2181
// @source: adr-0019
#[test]
fn the_wram_port_address_wraps_at_17_bits() {
    let mut bus = bus();
    bus.wram_port_address = 0x1_FFFF;
    bus.wram[0] = 0x77;
    write(&mut bus, 0x00_2180, 0x11);
    assert_eq!(bus.wram[0x1_FFFF], 0x11);
    assert_eq!(bus.wram_port_address, 0);
    assert_eq!(read(&mut bus, 0x00_2180), 0x77);
    assert_eq!(bus.wram_port_address, 1);
}

// @source: anomie-regs#2180
// @source: adr-0019
#[test]
fn peeking_wmdata_reads_without_advancing() {
    let mut bus = bus();
    bus.wram_port_address = 0x40;
    bus.wram[0x40] = 0x99;
    assert_eq!(bus.peek(0x00_2180), (0x99, true));
    assert_eq!(bus.wram_port_address, 0x40);
    assert_eq!(bus.peek(0x7E_0040), (0x99, false));
}

// @source: anomie-memmap#hardware/data-bus
// @source: anomie-regs#2181
// @source: anomie-regs#420d
// @source: fullsnes#snesiomap
// @source: adr-0019
#[test]
fn write_only_and_unanswered_addresses_read_open_bus() {
    let mut bus = bus();
    // A write places its value on the data bus, even to an internal CPU register.
    write(&mut bus, 0x00_4200, 0x5C);
    assert_eq!(bus.open_bus_value, 0x5C);
    for address in [
        0x00_2181, 0x00_2182, 0x00_2183, // WMADD*, write-only
        0x00_420D, // MEMSEL, write-only
        0x00_2000, 0x00_2184, 0x00_21FF, 0x00_2200, 0x00_4400, 0x00_5FFF, // unused
        0x00_2100, 0x00_2140, // PPU and APU, until those chips exist
        0x00_4016, 0x00_4210, // internal registers not yet modelled
    ] {
        assert_eq!(read(&mut bus, address), 0x5C, "{address:06X}");
    }
    // A read that something answers places its value on the bus.
    assert_eq!(read(&mut bus, 0x00_8012), 0x12);
    assert_eq!(read(&mut bus, 0x00_2181), 0x12);
}

// @source: anomie-memmap#memory-map
// @source: anomie-regs#420d
// @source: fullsnes#snesmemorycontrol
#[test]
fn memsel_makes_memory_2_fast_until_reset() {
    let mut bus = bus();
    assert_eq!(read_cost(&mut bus, 0x80_8000), 8);
    assert_eq!(read_cost(&mut bus, 0xC0_0000), 8);
    // The write to $420D is itself a Fast access.
    let before = bus.now.0;
    write(&mut bus, 0x00_420D, 0xFF);
    assert_eq!(bus.now.0 - before, 6);
    assert!(bus.fast_rom);
    assert_eq!(read_cost(&mut bus, 0x80_8000), 6);
    assert_eq!(read_cost(&mut bus, 0xC0_0000), 6);
    assert_eq!(read_cost(&mut bus, 0x00_8000), 8);
    assert_eq!(read_cost(&mut bus, 0x80_0000), 8);
    write(&mut bus, 0x80_420D, 0xFE);
    assert_eq!(read_cost(&mut bus, 0x80_8000), 8);
    write(&mut bus, 0x00_420D, 0x01);
    write(&mut bus, 0x00_2181, 0x01);
    bus.reset();
    assert!(!bus.fast_rom);
    assert_eq!(bus.wram_port_address, 0);
}

// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: fullsnes#snestimingoscillators
// @source: adr-0019
#[test]
fn a_refresh_pauses_the_cpu_for_40_cycles_after_the_cycle_it_falls_in() {
    // A cycle that ends exactly when the refresh is due.
    let mut bus = bus();
    bus.now = Cycles(530);
    read(&mut bus, 0x00_8000);
    assert_eq!(bus.now, Cycles(538 + 40));
    // A cycle the refresh falls inside completes first.
    let mut bus = self::bus();
    bus.now = Cycles(535);
    idle(&mut bus);
    assert_eq!(bus.now, Cycles(541 + 40));
    // A cycle that ends before it is due is not paused.
    let mut bus = self::bus();
    bus.now = Cycles(531);
    idle(&mut bus);
    assert_eq!(bus.now, Cycles(537));
    idle(&mut bus);
    assert_eq!(bus.now, Cycles(543 + 40));
    // The next one is due 534 cycles into the next line.
    assert_eq!(bus.timing.next_refresh(), Cycles(1364 + 534));
}
