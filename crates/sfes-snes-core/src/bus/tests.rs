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
        0x00_4202, 0x00_4203, 0x00_4204, 0x00_4205, 0x00_4206, // the math operands
        0x00_2000, 0x00_2184, 0x00_21FF, 0x00_2200, 0x00_4400, 0x00_5FFF, // unused
        0x00_2100, 0x00_2140, // PPU and APU, until those chips exist
        0x00_4016, 0x00_4218, // internal registers not yet modelled
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

// @source: anomie-regs#4210
// @source: anomie-regs#4211
// @source: anomie-regs#4212
// @source: anomie-timing#s-cpu/registers
// @source: fullsnes#snesppuinterrupts
// @source: adr-0021
#[test]
fn rdnmi_timeup_and_hvbjoy_read_their_flags_over_open_bus() {
    let mut bus = bus();
    // $4210: version 2 in bits 3-0, open bus in bits 6-4.
    assert_eq!(read(&mut bus, 0x00_4210), 0x02);
    write(&mut bus, 0x00_0000, 0xF5);
    assert_eq!(read(&mut bus, 0x00_4210), 0x72);
    bus.interrupts.set_nmi_flag();
    assert_eq!(read(&mut bus, 0x00_4210), 0xF2);
    assert_eq!(read(&mut bus, 0x00_4210), 0x72, "the read cleared the flag");
    // $4211: open bus in bits 6-0.
    assert_eq!(read(&mut bus, 0x00_4211), 0x72);
    bus.interrupts.set_irq_flag();
    assert_eq!(read(&mut bus, 0x00_4211), 0xF2);
    assert_eq!(read(&mut bus, 0x00_4211), 0x72, "the read cleared the flag");
    // A write to $4211 changes nothing.
    bus.interrupts.set_irq_flag();
    write(&mut bus, 0x00_4211, 0x00);
    assert!(bus.interrupts.irq_flag());
    // $4212 at dot 125 and dot 274 of line 0: open bus in bits 5-1, joypad busy 0.
    write(&mut bus, 0x00_0000, 0xFF);
    bus.now = Cycles(500);
    assert_eq!(read(&mut bus, 0x00_4212), 0x3E);
    bus.now = Cycles(274 * 4);
    assert_eq!(read(&mut bus, 0x00_4212), 0x7E);
    // Vertical blank from line 225.
    while !bus.timing.in_vertical_blank() {
        idle(&mut bus);
    }
    bus.now = Cycles(225 * 1364 + 100 * 4);
    assert_eq!(read(&mut bus, 0x00_4212) & 0xC0, 0x80);
}

// @source: anomie-timing#s-cpu/interrupts
// @source: wdc-w65c816s#2.18
// @source: adr-0004
// @source: adr-0021
#[test]
fn the_cpu_sees_the_lines_as_they_were_when_its_last_cycle_started() {
    let mut bus = bus();
    bus.interrupts.set_irq_flag();
    assert!(!bus.pending().irq, "not recorded until a cycle starts");
    idle(&mut bus);
    assert!(bus.pending().irq);
    // A read that clears the flag started while it was set.
    read(&mut bus, 0x00_4211);
    assert!(bus.pending().irq);
    idle(&mut bus);
    assert!(!bus.pending().irq);
}

// @source: anomie-timing#s-cpu/interrupts
// @source: anomie-regs#4200
// @source: anomie-regs#4207
// @source: anomie-regs#4208
// @source: adr-0021
#[test]
fn a_trigger_inside_a_cycle_is_seen_from_the_next_one() {
    let mut bus = bus();
    // H-IRQ at HTIME 10: 14 + 4 × 10 = 54 cycles into each line.
    write(&mut bus, 0x00_4207, 10);
    write(&mut bus, 0x00_4208, 0);
    write(&mut bus, 0x00_4200, 0x10);
    assert_eq!(bus.now, Cycles(18));
    while bus.now < Cycles(54) {
        idle(&mut bus);
    }
    // The cycle 48-54 met the trigger point at its end; it started before it.
    assert_eq!(bus.now, Cycles(54));
    assert!(bus.interrupts.irq_flag());
    assert!(!bus.pending().irq);
    idle(&mut bus);
    assert!(bus.pending().irq);
    // Disabling the IRQ acknowledges it.
    write(&mut bus, 0x00_4200, 0x00);
    assert!(!bus.interrupts.irq_flag());
}

// @source: anomie-timing#s-cpu/interrupts
// @source: fullsnes#snesppuinterrupts
// @source: fullsnes#snestiminghvevents
// @source: adr-0021
#[test]
fn an_enabled_nmi_is_latched_half_a_dot_into_vertical_blank() {
    let mut bus = bus();
    write(&mut bus, 0x00_4200, 0x80);
    let mut before = bus.now;
    while !bus.interrupts.nmi_flag() {
        before = bus.now;
        idle(&mut bus);
    }
    // The flag falls 2 cycles after line 225 starts, in the cycle that ended last.
    assert!(before < Cycles(225 * 1364 + 2) && bus.now >= Cycles(225 * 1364 + 2));
    idle(&mut bus);
    assert!(bus.pending().nmi);
    // Reading $4210 clears the flag; the latch stays until the vector is read.
    assert_eq!(read(&mut bus, 0x00_4210) & 0x80, 0x80);
    idle(&mut bus);
    assert!(bus.pending().nmi);
    read(&mut bus, 0x00_FFEA);
    assert!(bus.pending().nmi, "a read without VPB keeps the latch");
    BusView::new(&mut bus, &mut NoopObserver).read(Addr24::new(0x00_FFEA), Pins::VPB);
    idle(&mut bus);
    assert!(!bus.pending().nmi);
}

// @source: anomie-regs#2137
// @source: anomie-regs#213c
// @source: anomie-regs#213d
// @source: anomie-regs#4201
// @source: fullsnes#snespputimersandstatus
// @source: anomie-timing
// @source: adr-0021
#[test]
fn slhv_and_wrio_latch_the_counters() {
    let mut bus = bus();
    // A $2137 read latches the dot where its access starts: 400 / 4 = dot 100, line 0.
    bus.now = Cycles(400);
    read(&mut bus, 0x00_2137);
    assert_eq!(read(&mut bus, 0x00_213C), 100);
    // Bit 8 is 0; bits 7-1 are the last value read from PPU2, 100 ($64).
    assert_eq!(read(&mut bus, 0x00_213C), 0x64);
    assert_eq!(read(&mut bus, 0x00_213D), 0x00);
    assert_eq!(read(&mut bus, 0x00_213D), 0x00);
    // Clearing $4201 bit 7 latches one dot after the write starts: dot 201.
    bus.now = Cycles(800);
    write(&mut bus, 0x00_4201, 0x7F);
    assert_eq!(read(&mut bus, 0x00_213C), 201);
    // With bit 7 clear a $2137 read latches nothing.
    bus.now = Cycles(1200);
    read(&mut bus, 0x00_2137);
    assert_eq!(read(&mut bus, 0x00_213C), 0xC9 & 0xFE); // 201 = $C9
    assert_eq!(read(&mut bus, 0x00_213C), 201);
    // Nor does writing 0 to a bit already 0.
    write(&mut bus, 0x00_4201, 0x00);
    write(&mut bus, 0x00_4201, 0x00);
    assert_eq!(read(&mut bus, 0x00_213C), 0xC9 & 0xFE); // 201 = $C9
}

// @source: anomie-regs#2137
// @source: anomie-regs#213c
// @source: anomie-regs#4210
// @source: adr-0021
#[test]
fn peek_reports_the_timing_registers_without_their_side_effects() {
    let mut bus = bus();
    bus.interrupts.set_nmi_flag();
    bus.interrupts.set_irq_flag();
    assert_eq!(bus.peek(0x00_4210), (0x82, true));
    assert_eq!(bus.peek(0x00_4211), (0x80, true));
    assert_eq!(bus.peek(0x00_4212), (0x40, false));
    assert_eq!(bus.peek(0x00_213C), (0xFF, true));
    assert_eq!(bus.peek(0x00_213D), (0xFF, true));
    assert_eq!(bus.peek(0x00_2137), (0x00, true));
    assert!(bus.interrupts.nmi_flag() && bus.interrupts.irq_flag());
    assert_eq!(read(&mut bus, 0x00_213C), 0xFF);
    assert_eq!(bus.peek(0x00_213C), (0xFF, true));
}

// @source: fullsnes#snesiomap
// @source: anomie-regs#4200
// @source: adr-0021
#[test]
fn reset_clears_the_interrupts_and_sets_wrio() {
    let mut bus = bus();
    write(&mut bus, 0x00_4200, 0x80);
    bus.interrupts.set_nmi_flag();
    bus.interrupts.set_irq_flag();
    write(&mut bus, 0x00_4201, 0x00);
    bus.reset();
    assert_eq!(bus.interrupts, Interrupts::power_on());
    assert_eq!(bus.io_port, 0xFF);
    // The latch from the $4201 write is kept.
    assert_ne!(bus.counters, CounterLatch::power_on());
}

// @source: anomie-regs#4202
// @source: anomie-regs#4203
// @source: anomie-regs#4216
// @source: anomie-regs#4217
// @source: fullsnes#snesmathsmultiplydivide
// @source: adr-0023
#[test]
fn the_product_is_read_from_the_ninth_cpu_cycle_after_the_write() {
    let mut bus = bus();
    write(&mut bus, 0x00_4202, 200);
    write(&mut bus, 0x00_4203, 250);
    // Seven cycles of any kind, then a read in the eighth: too early.
    for _ in 0..6 {
        idle(&mut bus);
    }
    read(&mut bus, 0x00_8000);
    assert_eq!(read(&mut bus, 0x00_4216), 0x00);
    // The ninth cycle sees 50000 = $C350, and the read drives the data bus.
    assert_eq!(read(&mut bus, 0x00_4216), 0x50);
    assert_eq!(bus.open_bus_value, 0x50);
    assert_eq!(read(&mut bus, 0x00_4217), 0xC3);
    assert_eq!(read(&mut bus, 0x00_4214), 250);
    assert_eq!(read(&mut bus, 0x00_4215), 0x00);
}

// @source: anomie-regs#4204
// @source: anomie-regs#4205
// @source: anomie-regs#4206
// @source: anomie-regs#4214
// @source: anomie-regs#4216
// @source: fullsnes#snesmathsmultiplydivide
// @source: adr-0023
#[test]
fn the_quotient_is_read_from_the_seventeenth_cpu_cycle_after_the_write() {
    let mut bus = bus();
    write(&mut bus, 0x00_4204, 0x39);
    write(&mut bus, 0x00_4205, 0x30);
    write(&mut bus, 0x00_4206, 100);
    for _ in 0..15 {
        idle(&mut bus);
    }
    assert_eq!(read(&mut bus, 0x00_4214), 0);
    // 12345 = 123 × 100 + 45.
    assert_eq!(read(&mut bus, 0x00_4214), 123);
    assert_eq!(read(&mut bus, 0x00_4216), 45);
}

// @source: anomie-regs#4201
// @source: anomie-regs#4213
// @source: fullsnes#snescontrollersioportsmanualreading
// @source: adr-0023
#[test]
fn rdio_reads_back_wrio() {
    let mut bus = bus();
    assert_eq!(read(&mut bus, 0x00_4213), 0xFF);
    write(&mut bus, 0x00_4201, 0x40);
    assert_eq!(read(&mut bus, 0x00_4213), 0x40);
    assert_eq!(bus.open_bus_value, 0x40);
}

// @source: anomie-regs#4213
// @source: anomie-regs#4214
// @source: anomie-regs#4216
// @source: anomie-regs#4202
// @source: anomie-regs#4204
// @source: adr-0023
#[test]
fn the_math_unit_is_peeked_without_side_effects_and_kept_on_reset() {
    let mut bus = bus();
    write(&mut bus, 0x00_4202, 2);
    write(&mut bus, 0x00_4203, 3);
    for _ in 0..8 {
        idle(&mut bus);
    }
    bus.reset();
    assert_eq!(bus.peek(0x00_4213), (0xFF, false));
    assert_eq!(bus.peek(0x00_4214), (3, false));
    assert_eq!(bus.peek(0x00_4215), (0, false));
    assert_eq!(bus.peek(0x00_4216), (6, false));
    assert_eq!(bus.peek(0x00_4217), (0, false));
    // The multiplicand is kept too.
    write(&mut bus, 0x00_4203, 5);
    for _ in 0..8 {
        idle(&mut bus);
    }
    assert_eq!(bus.peek(0x00_4216), (10, false));
}
