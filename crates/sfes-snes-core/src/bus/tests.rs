// @source: adr-0019
//! Unit tests of the bus: decode, the WRAM port, open bus, access speed, refresh, the
//! interrupt and timer registers, the math unit, the controller ports, DMA and the APU
//! ports' placeholder.

use alloc::vec::Vec;

use sfes_api::{Addr24, Buttons, Cycles, DmaEvent, NoopObserver, Rgb555};
use sfes_snes_cart::{
    Cartridge, Mapping, Options, Region,
    test_support::{TestHeader, write_header},
};
use sfes_snes_cpu::{Bus65816, Pins};

use super::*;
use crate::bus::apu::ApuPort;

/// A bus over a 32 KiB LoROM image whose byte at ROM offset `n` is `n & $FF`.
// @source: fullsnes#snescartridgeromheader
// @source: adr-0017
fn bus() -> Bus {
    let mut image = (0..0x8000)
        .map(|index: usize| index.to_le_bytes()[0])
        .collect::<Vec<_>>();
    write_header(&mut image, Mapping::LoRom, TestHeader::default());
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
    let before = bus.now;
    read(bus, address);
    bus.now - before
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
// @source: adr-0024
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
        0x00_2100, 0x00_2137, 0x00_213F, // INIDISP, SLHV, STAT78 until #90 (ADR 0028)
        0x00_4000, 0x00_4015, 0x00_4018, 0x00_41FF, 0x00_4220, // unused CPU registers
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
    let before = bus.now;
    write(&mut bus, 0x00_420D, 0xFF);
    assert_eq!(bus.now - before, 6);
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

// @source: anomie-regs#4200
// @source: anomie-regs#4207
// @source: anomie-regs#4208
// @source: anomie-timing#s-cpu/interrupts
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
// @source: anomie-timing
// @source: fullsnes#snespputimersandstatus
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

// @source: anomie-regs#4200
// @source: fullsnes#snesiomap
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
    // The latch from the $4201 write is kept: V is no longer $1FF.
    assert_eq!(bus.ppu.peek(Register::VerticalCounter), Some(0x00));
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

// @source: anomie-regs#4202
// @source: anomie-regs#4204
// @source: anomie-regs#4213
// @source: anomie-regs#4214
// @source: anomie-regs#4216
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

// @source: anomie-regs
// @source: anomie-regs#4016
// @source: anomie-regs#4017
// @source: fullsnes#snescontrollersioportsmanualreading
// @source: adr-0024
#[test]
fn joyser0_and_joyser1_read_data1_over_open_bus_and_clock_their_pad() {
    let mut bus = bus();
    bus.ports.input = [Buttons::B, Buttons::Y];
    write(&mut bus, 0x00_4016, 0x01);
    write(&mut bus, 0x00_4016, 0x00);
    // $4016: bits 7-2 open bus ($00 from the write), Data2 0, Data1 the B button.
    assert_eq!(read(&mut bus, 0x00_4016), 0x01);
    assert_eq!(read(&mut bus, 0x00_4016), 0x00);
    // $4017: bits 4-2 set, bits 7-5 open bus. Port 2's pad has not moved: B, then Y.
    write(&mut bus, 0x00_0000, 0xFF);
    assert_eq!(read(&mut bus, 0x00_4017), 0xFC);
    write(&mut bus, 0x00_0000, 0x00);
    assert_eq!(read(&mut bus, 0x00_4017), 0x1D);
    // A peek reports a side effect and does not clock the pad. The data bus holds $1D.
    assert_eq!(bus.peek(0x00_4016), (0x1C, true));
    assert_eq!(bus.peek(0x00_4016), (0x1C, true));
    assert_eq!(bus.peek(0x00_4017), (0x1C, true));
}

// @source: anomie-regs#4200
// @source: anomie-regs#4212
// @source: anomie-regs#4218
// @source: anomie-timing#s-cpu/auto-joypad-read
// @source: adr-0024
#[test]
fn the_auto_read_sets_hvbjoy_bit_0_and_fills_joy1_at_dot_74_5_of_line_225() {
    let mut bus = bus();
    bus.ports.input[0] = Buttons::A | Buttons::START;
    write(&mut bus, 0x00_4200, 0x01);
    let start = 225 * 1364 + 298;
    // Each idle is 6 master cycles: stop in the cycle before the start.
    while bus.now + 6 <= Cycles(start) {
        idle(&mut bus);
    }
    assert_eq!(bus.peek(0x00_4212).0 & 0x01, 0);
    idle(&mut bus);
    assert_eq!(bus.peek(0x00_4212).0 & 0x01, 0x01);
    while bus.now.0 < start + 4224 {
        assert_eq!(bus.peek(0x00_4212).0 & 0x01, 0x01, "{}", bus.now.0);
        idle(&mut bus);
    }
    assert_eq!(bus.peek(0x00_4212).0 & 0x01, 0);
    // byetUDLR axlr0000: Start and A are $1080.
    assert_eq!(read(&mut bus, 0x00_4218), 0x80);
    assert_eq!(read(&mut bus, 0x00_4219), 0x10);
    assert_eq!(bus.peek(0x00_421A), (0, false));
    assert_eq!(bus.peek(0x00_421F), (0, false));
}

// @source: anomie-regs#4200
// @source: anomie-regs#4218
// @source: adr-0024
#[test]
fn with_4200_bit_0_clear_no_auto_read_runs() {
    let mut bus = bus();
    bus.ports.input[0] = Buttons::A;
    write(&mut bus, 0x00_4200, 0x80);
    while bus.now.0 < 226 * 1364 {
        assert_eq!(bus.peek(0x00_4212).0 & 0x01, 0);
        idle(&mut bus);
    }
    assert_eq!(read(&mut bus, 0x00_4218), 0);
}

// @source: adr-0020
// @source: adr-0024
#[test]
fn the_polled_flag_is_kept_at_each_frame_end() {
    let mut bus = bus();
    assert_eq!(bus.peek(0x00_4218), (0, false));
    assert!(bus.peek(0x00_4016).1);
    // Neither peek polled: the first frame ends unpolled.
    while bus.timing.frames() < 1 {
        idle(&mut bus);
    }
    assert!(!bus.ports.last_frame_polled());
    read(&mut bus, 0x00_421B);
    while bus.timing.frames() < 2 {
        idle(&mut bus);
    }
    assert!(bus.ports.last_frame_polled());
    read(&mut bus, 0x00_4017);
    while bus.timing.frames() < 3 {
        idle(&mut bus);
    }
    assert!(bus.ports.last_frame_polled());
    while bus.timing.frames() < 4 {
        idle(&mut bus);
    }
    assert!(!bus.ports.last_frame_polled());
}

// @source: anomie-regs#4200
// @source: fullsnes#snesiomap
// @source: adr-0024
#[test]
fn reset_clears_the_latch_and_the_enable_and_keeps_joy1() {
    let mut bus = bus();
    bus.ports.input[0] = Buttons::B;
    write(&mut bus, 0x00_4200, 0x01);
    // Past the end of the first auto-read, at dot 74.5 of line 225.
    while bus.now.0 < 225 * 1364 + 298 + 4224 {
        idle(&mut bus);
    }
    write(&mut bus, 0x00_4016, 0x01);
    bus.reset();
    // The latch is low: the pad moves on, B then Y.
    assert_eq!(read(&mut bus, 0x00_4016) & 0x03, 1);
    assert_eq!(read(&mut bus, 0x00_4016) & 0x03, 0);
    assert_eq!(read(&mut bus, 0x00_4219), 0x80);
}

/// An observer that keeps every byte a DMA moves.
#[derive(Clone, Debug, Default)]
struct DmaLog(Vec<DmaEvent>);

impl Observer for DmaLog {
    // @source: adr-0025
    fn on_dma(&mut self, event: &DmaEvent) {
        self.0.push(*event);
    }
}

/// Sets channel `channel`'s `$43x0–$43x6` through CPU writes.
// @source: anomie-regs#43x0
// @source: anomie-regs#43x1
// @source: anomie-regs#43x2
// @source: anomie-regs#43x5
fn set_channel(
    bus: &mut Bus,
    channel: u32,
    control: u8,
    b_address: u8,
    a_address: u32,
    count: u16,
) {
    let base = 0x00_4300 | (channel << 4);
    let [low, high, bank, _] = a_address.to_le_bytes();
    let [count_low, count_high] = count.to_le_bytes();
    for (register, value) in [control, b_address, low, high, bank, count_low, count_high]
        .into_iter()
        .enumerate()
    {
        write(bus, base + u32::try_from(register).expect("small"), value);
    }
}

/// "STA $420B : NOP": the `$420B` write, the next opcode fetched from `next`, then the
/// NOP's internal cycle, before which the pause falls. Returns the master cycles from the
/// start of that internal cycle to its end, and the bytes moved.
// @source: anomie-timing#s-cpu/dma
// @source: adr-0025
fn start_dma(bus: &mut Bus, channels: u8, next: u32) -> (u64, Vec<DmaEvent>) {
    let mut log = DmaLog::default();
    BusView::new(bus, &mut log).write(Addr24::new(0x00_420B), channels, Pins::empty());
    BusView::new(bus, &mut log).read(Addr24::new(next), Pins::empty());
    let before = bus.now;
    BusView::new(bus, &mut log).idle(Addr24::new(0), Pins::empty());
    (bus.now - before, log.0)
}

// @source: anomie-regs#420b
// @source: anomie-regs#43x0
// @source: fullsnes#snesdmaandhdmachannel07registers
// @source: adr-0025
#[test]
fn the_cpu_reads_and_writes_the_channel_registers_and_not_mdmaen() {
    let mut bus = bus();
    assert_eq!(bus.peek(0x00_4302), (0xFF, false));
    write(&mut bus, 0x80_4362, 0x9A);
    assert_eq!(read(&mut bus, 0x00_4362), 0x9A);
    assert_eq!(bus.peek(0x00_4362), (0x9A, false));
    // $420B is write-only and $43xC and $4380 are open bus.
    read(&mut bus, 0x00_8077);
    assert_eq!(read(&mut bus, 0x00_420B), 0x77);
    assert_eq!(read(&mut bus, 0x00_436C), 0x77);
    assert_eq!(read(&mut bus, 0x00_4380), 0x77);
}

// @source: anomie-regs#2180
// @source: anomie-regs#dma-and-hdma/dma
// @source: anomie-timing#s-cpu/dma
// @source: adr-0025
#[test]
fn the_pause_reproduces_anomies_four_worked_examples() {
    // The pause starts 14 cycles after `start`: 6 for the $420B write, 8 for the opcode
    // fetch from Slow ROM. Pauses 6, 4, 2 and 0 cycles past a multiple of 8 take 48, 48,
    // 48 and 54 cycles (anomie-timing), then the NOP's 6-cycle internal cycle runs.
    // (start, wait to a multiple of 8, pause)
    for (start, wait, pause) in [(104, 2, 48), (102, 4, 48), (100, 6, 48), (106, 8, 54)] {
        let mut bus = bus();
        set_channel(&mut bus, 0, 0x00, 0x80, 0x00_8000, 3);
        bus.now = Cycles(start);
        let (cost, log) = start_dma(&mut bus, 0x01, 0x00_8000);
        assert_eq!(cost, pause + 6, "pause at {}", start + 14);
        // The three ROM bytes 0, 1 and 2 reach WRAM through $2180.
        assert_eq!(&bus.wram[0..3], &[0, 1, 2]);
        assert_eq!(bus.wram_port_address, 3);
        assert_eq!(log.len(), 3);
        // The first byte's slot follows the wait to 8, the transfer's 8 and the channel's 8.
        let first = start + 14 + wait + 16;
        for (index, event) in (0..).zip(&log) {
            assert_eq!(
                *event,
                DmaEvent {
                    channel: 0,
                    hdma: false,
                    source: 0x00_8000 + index,
                    destination: 0x2180,
                    value: u8::try_from(index).expect("small"),
                    at: Cycles(first + 8 * u64::from(index)),
                }
            );
        }
        // The count ends at 0 and the address has moved on; the bank has not.
        assert_eq!(bus.peek(0x00_4305), (0x00, false));
        assert_eq!(bus.peek(0x00_4306), (0x00, false));
        assert_eq!(bus.peek(0x00_4302), (0x03, false));
        assert_eq!(bus.peek(0x00_4304), (0x00, false));
    }
}

// @source: anomie-regs#43x0
// @source: anomie-regs#43x1
// @source: anomie-regs#43x5
// @source: adr-0025
#[test]
fn a_four_register_mode_wraps_within_21xx_and_stops_at_the_count() {
    let mut bus = bus();
    set_channel(&mut bus, 3, 0x04, 0xFE, 0x00_8010, 5);
    let (_, log) = start_dma(&mut bus, 0x08, 0x00_8000);
    let destinations = log
        .iter()
        .map(|event| event.destination)
        .collect::<Vec<_>>();
    assert_eq!(destinations, [0x21FE, 0x21FF, 0x2100, 0x2101, 0x21FE]);
    assert!(log.iter().all(|event| event.channel == 3));
}

// @source: anomie-regs#43x0
// @source: anomie-regs#43x5
// @source: anomie-regs#dma-and-hdma/dma
// @source: adr-0020
// @source: adr-0025
#[test]
fn a_count_of_0_moves_65536_bytes_while_lines_go_on() {
    let mut bus = bus();
    // Fixed address, to $21FF where nothing answers.
    set_channel(&mut bus, 0, 0x08, 0xFF, 0x00_8000, 0);
    let (_, log) = start_dma(&mut bus, 0x01, 0x00_8000);
    assert_eq!(log.len(), 0x1_0000);
    assert_eq!(bus.peek(0x00_4305), (0x00, false));
    assert_eq!(bus.peek(0x00_4302), (0x00, false));
    assert_eq!(bus.peek(0x00_4303), (0x80, false));
    // 524288 cycles of bytes span more than 384 lines.
    assert!(bus.timing.lines() >= 384);
}

// @source: anomie-regs#420b
// @source: anomie-timing#s-cpu/dma
// @source: fullsnes#snesdmaandhdmastartenableregisters
// @source: adr-0025
#[test]
fn several_channels_run_in_order_each_with_its_overhead() {
    let mut bus = bus();
    set_channel(&mut bus, 2, 0x00, 0xFF, 0x00_8020, 1);
    set_channel(&mut bus, 0, 0x00, 0xFF, 0x00_8000, 2);
    bus.now = Cycles(100);
    let (cost, log) = start_dma(&mut bus, 0x05, 0x00_8000);
    let channels = log.iter().map(|event| event.channel).collect::<Vec<_>>();
    assert_eq!(channels, [0, 0, 2]);
    // From the pause at 114: 6 to 120, 8, then 8 + 16 and 8 + 8, 54 in all; a whole
    // number of 6-cycle clocks needs 6 more, then the internal cycle's 6.
    assert_eq!(cost, 6 + 8 + 24 + 16 + 6 + 6);
    // Channel 1 was not set and keeps its count.
    assert_eq!(bus.peek(0x00_4315), (0xFF, false));
}

// @source: anomie-regs#4214
// @source: anomie-regs#43x2
// @source: anomie-regs#dma-and-hdma/dma
// @source: fullsnes#snesunpredictablethings
// @source: adr-0023
// @source: adr-0025
#[test]
fn the_a_bus_does_not_reach_b_bus_dma_or_slow_registers() {
    // The opcode fetched before the pause, $42, is the open-bus value.
    for (address, expected) in [
        (0x00_2118, 0x42), // a B-bus register through the A bus
        (0x00_4016, 0x42), // $4000-$41FF
        (0x00_4017, 0x5E), // with bits 4-2 set
        (0x00_4300, 0x42), // the DMA registers
        (0x00_420B, 0x42),
        (0x00_4214, 0x00), // $4210-$421F work: the power-on quotient
        (0x00_8055, 0x55), // ROM
    ] {
        let mut bus = bus();
        set_channel(&mut bus, 0, 0x00, 0x80, address, 1);
        start_dma(&mut bus, 0x01, 0x00_8042);
        assert_eq!(bus.wram[0], expected, "{address:#08x}");
    }
    // Writes to them are lost: channel 1's control is untouched.
    let mut bus = bus();
    set_channel(&mut bus, 0, 0x80, 0x80, 0x00_4310, 1);
    start_dma(&mut bus, 0x01, 0x00_8000);
    assert_eq!(bus.peek(0x00_4310), (0xFF, false));
}

// @source: anomie-regs#2180
// @source: anomie-regs#2181
// @source: anomie-regs#dma-and-hdma/dma
// @source: fullsnes#snesmemoryworkramaccess
// @source: adr-0025
#[test]
fn wram_through_both_buses_leaves_the_port_alone() {
    // From WRAM to $2180: WRAM is not written and the port does not move.
    let mut bus = bus();
    let before = bus.wram[0];
    set_channel(&mut bus, 0, 0x00, 0x80, 0x7E_0100, 2);
    start_dma(&mut bus, 0x01, 0x00_8000);
    assert_eq!(bus.wram[0], before);
    assert_eq!(bus.wram_port_address, 0);
    // From $2180 to WRAM: WRAM takes the open-bus value.
    let mut bus = self::bus();
    set_channel(&mut bus, 0, 0x80, 0x80, 0x7E_0100, 1);
    start_dma(&mut bus, 0x01, 0x00_8042);
    assert_eq!(bus.wram[0x100], 0x42);
    assert_eq!(bus.wram_port_address, 0);
    // From WRAM to $2181: no effect.
    let mut bus = self::bus();
    set_channel(&mut bus, 0, 0x00, 0x81, 0x7E_0100, 1);
    start_dma(&mut bus, 0x01, 0x00_8000);
    assert_eq!(bus.wram_port_address, 0);
}

// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: anomie-timing#s-cpu/dma
// @source: adr-0019
// @source: adr-0025
#[test]
fn a_refresh_pauses_a_transfer_after_the_slot_it_falls_in() {
    let mut bus = bus();
    set_channel(&mut bus, 0, 0x08, 0xFF, 0x00_8000, 60);
    bus.now = Cycles(100);
    let (cost, log) = start_dma(&mut bus, 0x01, 0x00_8000);
    // Slots from 136; the refresh due at 538 falls in the slot at 536 and follows it.
    assert_eq!(log[50].at, Cycles(536));
    assert_eq!(log[51].at, Cycles(536 + 8 + 40));
    // The bytes end at 656, 542 cycles after the pause; 4 more reach a whole clock.
    assert_eq!(cost, 542 + 4 + 6);
}

// @source: anomie-regs#4203
// @source: anomie-regs#4216
// @source: fullsnes#snesmathsmultiplydivide
// @source: adr-0023
// @source: adr-0025
#[test]
fn the_math_unit_does_not_count_during_a_transfer() {
    let mut bus = bus();
    set_channel(&mut bus, 0, 0x08, 0xFF, 0x00_8000, 100);
    write(&mut bus, 0x00_4202, 200);
    write(&mut bus, 0x00_4203, 250);
    // Cycles 1 to 3, the transfer before the third; then 4 to 7, and a read in the eighth.
    start_dma(&mut bus, 0x01, 0x00_8000);
    for _ in 0..4 {
        idle(&mut bus);
    }
    assert_eq!(read(&mut bus, 0x00_4216), 0x00);
    assert_eq!(read(&mut bus, 0x00_4216), 0x50);
}

// @source: anomie-regs#420b
// @source: adr-0025
#[test]
fn reset_drops_a_transfer_not_yet_started() {
    let mut bus = bus();
    set_channel(&mut bus, 0, 0x00, 0xFF, 0x00_8000, 4);
    write(&mut bus, 0x00_420B, 0x01);
    bus.reset();
    read(&mut bus, 0x00_8000);
    let before = bus.now;
    idle(&mut bus);
    assert_eq!(bus.now - before, 6);
    assert_eq!(bus.peek(0x00_4305), (0x04, false));
}

/// Sets HDMA channel `channel`'s `$43x0–$43x4` directly: control, B-bus register and the
/// table's start.
// @source: anomie-regs#43x0
// @source: anomie-regs#43x1
// @source: anomie-regs#43x2
// @source: adr-0026
fn set_hdma(bus: &mut Bus, channel: u16, control: u8, b_address: u8, table: u32) {
    let base = 0x4300 | (channel << 4);
    let [low, high, bank, _] = table.to_le_bytes();
    for (register, value) in (0..).zip([control, b_address, low, high, bank]) {
        bus.dma.write_at(base + register, value);
    }
}

/// Idles until the clock reaches `end`. Returns each idle that took other than 6 cycles,
/// as its start and end, and the bytes DMA and HDMA moved.
// @source: adr-0019
// @source: adr-0026
fn idle_until(bus: &mut Bus, end: u64) -> (Vec<(u64, u64)>, Vec<DmaEvent>) {
    let mut log = DmaLog::default();
    let mut pauses = Vec::new();
    while bus.now.0 < end {
        let start = bus.now;
        BusView::new(bus, &mut log).idle(Addr24::new(0), Pins::empty());
        if bus.now - start != 6 {
            pauses.push((start.0, bus.now.0));
        }
    }
    (pauses, log.0)
}

/// The line, value, source and destination of each byte, in the first frame.
// @source: adr-0026
fn by_line(log: &[DmaEvent]) -> Vec<(u64, u8, u32, u32)> {
    log.iter()
        .map(|event| {
            (
                event.at.0 / 1364,
                event.value,
                event.source,
                event.destination,
            )
        })
        .collect::<Vec<_>>()
}

// @source: anomie-regs#420c
// @source: fullsnes#snesdmaandhdmastartenableregisters
// @source: adr-0026
#[test]
fn hdmaen_is_write_only_and_cleared_by_reset() {
    let mut bus = bus();
    write(&mut bus, 0x00_420C, 0xA5);
    assert_eq!(bus.dma.hdma_enabled(), 0xA5);
    read(&mut bus, 0x00_8033);
    assert_eq!(read(&mut bus, 0x00_420C), 0x33);
    bus.reset();
    assert_eq!(bus.dma.hdma_enabled(), 0);
}

// @source: anomie-regs#43x8
// @source: anomie-regs#43xa
// @source: anomie-regs#dma-and-hdma/hdma
// @source: fullsnes#snesdmaandhdmachannel07registers
// @source: adr-0026
#[test]
fn a_direct_table_moves_one_unit_per_line_repeats_and_terminates() {
    let mut bus = bus();
    // Two lines with one unit, then three lines of repeat, then the end.
    bus.wram[0x100..0x107].copy_from_slice(&[0x02, 0xAA, 0x83, 0x11, 0x22, 0x33, 0x00]);
    set_hdma(&mut bus, 0, 0x00, 0xFF, 0x7E_0100);
    bus.dma.write_at(0x420C, 0x01);
    let (_, log) = idle_until(&mut bus, 10 * 1364);
    assert_eq!(
        by_line(&log),
        [
            (0, 0xAA, 0x7E_0101, 0x21FF),
            (2, 0x11, 0x7E_0103, 0x21FF),
            (3, 0x22, 0x7E_0104, 0x21FF),
            (4, 0x33, 0x7E_0105, 0x21FF),
        ]
    );
    assert!(log.iter().all(|event| event.hdma && event.channel == 0));
    // The table address has passed the $00, which is left in $430A; $4302/3 are untouched.
    assert_eq!(bus.peek(0x00_4308), (0x07, false));
    assert_eq!(bus.peek(0x00_4309), (0x01, false));
    assert_eq!(bus.peek(0x00_430A), (0x00, false));
    assert_eq!(bus.peek(0x00_4302), (0x00, false));
}

// @source: anomie-regs#43x5
// @source: anomie-regs#43xa
// @source: anomie-regs#dma-and-hdma/hdma
// @source: fullsnes#snesdmaandhdmachannel07registers
// @source: adr-0026
#[test]
fn an_indirect_table_reads_pointers_and_the_last_channel_loads_one_byte_at_the_end() {
    // Two lines of repeat from $0310, one line from $0320, then the end with $55 $66
    // after it.
    let table = [0x82, 0x10, 0x03, 0x01, 0x20, 0x03, 0x00, 0x55, 0x66];
    let setup = |bus: &mut Bus| {
        bus.wram[0x200..0x209].copy_from_slice(&table);
        bus.wram[0x310..0x314].copy_from_slice(&[0xA1, 0xA2, 0xA3, 0xA4]);
        bus.wram[0x320..0x322].copy_from_slice(&[0xB1, 0xB2]);
        // Indirect, mode 1, to $21FE and $21FF, with data in bank $7E.
        set_hdma(bus, 1, 0x41, 0xFE, 0x7E_0200);
        bus.dma.write_at(0x4317, 0x7E);
    };
    let mut bus = bus();
    setup(&mut bus);
    bus.dma.write_at(0x420C, 0x02);
    let (_, log) = idle_until(&mut bus, 10 * 1364);
    let expected = [
        (0, 0xA1, 0x7E_0310, 0x21FE),
        (0, 0xA2, 0x7E_0311, 0x21FF),
        (1, 0xA3, 0x7E_0312, 0x21FE),
        (1, 0xA4, 0x7E_0313, 0x21FF),
        (2, 0xB1, 0x7E_0320, 0x21FE),
        (2, 0xB2, 0x7E_0321, 0x21FF),
    ];
    assert_eq!(by_line(&log), expected);
    // The last active channel reads only $55, into $4316, with $4315 0.
    assert_eq!(bus.peek(0x00_4315), (0x00, false));
    assert_eq!(bus.peek(0x00_4316), (0x55, false));
    assert_eq!(bus.peek(0x00_4318), (0x08, false));
    assert_eq!(bus.peek(0x00_431A), (0x00, false));

    // With channel 2 still running after it, channel 1 reads both bytes.
    let mut bus = self::bus();
    setup(&mut bus);
    bus.wram[0x400..0x402].copy_from_slice(&[0x7F, 0x00]);
    set_hdma(&mut bus, 2, 0x00, 0xFF, 0x7E_0400);
    bus.dma.write_at(0x420C, 0x06);
    let (_, log) = idle_until(&mut bus, 10 * 1364);
    let channel_1 = log
        .iter()
        .filter(|event| event.channel == 1)
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(by_line(&channel_1), expected);
    assert_eq!(bus.peek(0x00_4315), (0x55, false));
    assert_eq!(bus.peek(0x00_4316), (0x66, false));
    assert_eq!(bus.peek(0x00_4318), (0x09, false));
}

// @source: anomie-regs#43xa
// @source: fullsnes#snesdmaandhdmachannel07registers
// @source: adr-0026
#[test]
fn a_line_count_of_80_is_128_lines_without_repeat() {
    let mut bus = bus();
    bus.wram[0x100..0x105].copy_from_slice(&[0x80, 0x11, 0x01, 0x22, 0x00]);
    set_hdma(&mut bus, 0, 0x00, 0xFF, 0x7E_0100);
    bus.dma.write_at(0x420C, 0x01);
    let (_, log) = idle_until(&mut bus, 200 * 1364);
    let lines = by_line(&log)
        .into_iter()
        .map(|(line, value, ..)| (line, value))
        .collect::<Vec<_>>();
    assert_eq!(lines, [(0, 0x11), (128, 0x22)]);
}

// @source: anomie-regs#dma-and-hdma/hdma
// @source: anomie-timing#s-cpu/dma
// @source: anomie-timing#s-cpu/hdma
// @source: fullsnes#snestiminghvevents
// @source: adr-0019
// @source: adr-0026
#[test]
fn setup_and_a_line_pause_the_cpu_as_dma_does() {
    let mut bus = bus();
    bus.wram[0x100..0x103].copy_from_slice(&[0x01, 0x5A, 0x00]);
    set_hdma(&mut bus, 0, 0x00, 0xFF, 0x7E_0100);
    bus.dma.write_at(0x420C, 0x01);
    let (pauses, log) = idle_until(&mut bus, 2600);
    // Setup falls due at 24, the end of the fourth idle, and pauses the fifth: 8 to the
    // next multiple of 8, 18 and the channel's 8, then 2 to a whole 6-cycle clock, then the
    // idle's 6. The refresh at 538 follows the idle from 534. Line 0's transfer falls due
    // at 1112, inside the idle from 1108; the pause from 1114 waits 6 to 1120, then 18, 8
    // and the byte's 8 end at 1154, and 2 more reach a whole clock. Line 1 has no channel
    // left, so no pause; its refresh is at 1898 (ADR 0019).
    assert_eq!(pauses, [(24, 66), (534, 580), (1114, 1162), (1894, 1940)]);
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].at, Cycles(1146));

    // An indirect channel's setup reads two more bytes, 16 cycles more: 18 + 24 in all.
    let mut bus = self::bus();
    set_hdma(&mut bus, 0, 0x40, 0xFF, 0x7E_0100);
    bus.dma.write_at(0x420C, 0x01);
    let (pauses, _) = idle_until(&mut bus, 100);
    assert_eq!(pauses, [(24, 84)]);
}

// @source: anomie-regs#420c
// @source: anomie-regs#dma-and-hdma/hdma
// @source: adr-0026
#[test]
fn a_cleared_hdmaen_bit_pauses_its_channel_and_a_set_one_resumes_it() {
    let mut bus = bus();
    bus.wram[0x100..0x107].copy_from_slice(&[0x85, 1, 2, 3, 4, 5, 0x00]);
    set_hdma(&mut bus, 0, 0x00, 0xFF, 0x7E_0100);
    bus.dma.write_at(0x420C, 0x01);
    let (_, mut log) = idle_until(&mut bus, 2 * 1364);
    bus.dma.write_at(0x420C, 0x00);
    log.extend(idle_until(&mut bus, 4 * 1364).1);
    bus.dma.write_at(0x420C, 0x01);
    log.extend(idle_until(&mut bus, 10 * 1364).1);
    let lines = by_line(&log)
        .into_iter()
        .map(|(line, value, ..)| (line, value))
        .collect::<Vec<_>>();
    assert_eq!(lines, [(0, 1), (1, 2), (4, 3), (5, 4), (6, 5)]);
}

// @source: anomie-regs#420c
// @source: anomie-regs#dma-and-hdma/hdma
// @source: fullsnes#snesdmaandhdmanotes
// @source: adr-0026
#[test]
fn a_channel_enabled_mid_frame_skips_its_first_line_and_stays_terminated() {
    let mut bus = bus();
    bus.wram[0x100..0x103].copy_from_slice(&[0x81, 0x77, 0x00]);
    // $4308-$430A set by hand: the table at $0100 after a count of 1.
    set_hdma(&mut bus, 0, 0x00, 0xFF, 0x7E_0000);
    bus.dma.write_at(0x4308, 0x00);
    bus.dma.write_at(0x4309, 0x01);
    bus.dma.write_at(0x430A, 0x01);
    idle_until(&mut bus, 100);
    bus.dma.write_at(0x420C, 0x01);
    let (_, log) = idle_until(&mut bus, 5 * 1364);
    // Line 0 moves nothing and reloads; line 1 moves $77 and reaches the $00.
    let lines = by_line(&log)
        .into_iter()
        .map(|(line, value, ..)| (line, value))
        .collect::<Vec<_>>();
    assert_eq!(lines, [(1, 0x77)]);
    // Enabling it again does not restart it this frame.
    bus.dma.write_at(0x420C, 0x00);
    bus.dma.write_at(0x430A, 0x01);
    bus.dma.write_at(0x420C, 0x01);
    let (_, log) = idle_until(&mut bus, 10 * 1364);
    assert_eq!(log, []);
}

// @source: anomie-regs#dma-and-hdma/hdma
// @source: anomie-timing#s-cpu/hdma
// @source: fullsnes#snestiminghvevents
// @source: adr-0020
// @source: adr-0026
#[test]
fn each_frame_sets_up_again_and_vertical_blank_moves_nothing() {
    let mut bus = bus();
    // Repeat for 127 lines and the last 127 lines of the frame: data beyond line 224 is
    // never moved.
    bus.wram[0x100] = 0xFF;
    bus.wram[0x180] = 0x00;
    set_hdma(&mut bus, 0, 0x00, 0xFF, 0x7E_0100);
    bus.dma.write_at(0x420C, 0x01);
    let frame = 262 * 1364 - 4;
    let (_, log) = idle_until(&mut bus, frame + 2 * 1364);
    let first = log.iter().filter(|event| event.at.0 < frame).count();
    // 127 lines, then the $00 at $0180 ends it.
    assert_eq!(first, 127);
    assert!(log[126].at.0 < 127 * 1364);
    // The next frame starts the table again.
    assert_eq!(log[127].source, 0x7E_0101);
    assert!(log[127].at.0 > frame);
}

// @source: anomie-regs#420b
// @source: anomie-regs#420c
// @source: anomie-regs#dma-and-hdma
// @source: anomie-timing#s-cpu/hdma
// @source: adr-0019
// @source: adr-0025
// @source: adr-0026
#[test]
fn hdma_runs_between_dma_bytes_and_ends_a_dma_on_its_channel() {
    let table = [0x01, 0x5A, 0x00];
    // DMA on channel 0 from fixed ROM; HDMA on channel 1. The pause starts at 14: 2 to 16
    // and the transfer's 8 to 24, when setup falls due and runs to 50. Channel 0's 8 puts
    // the bytes at 58 + 8n; the refresh after the byte at 538 moves them to 586 + 8n. Line
    // 0's step falls due in the slot from 1106 and runs at 1114: 18 and 8 put its byte at
    // 1140, and DMA goes on at 1148.
    let mut bus = bus();
    bus.wram[0x100..0x103].copy_from_slice(&table);
    set_hdma(&mut bus, 1, 0x00, 0xFF, 0x7E_0100);
    bus.dma.write_at(0x420C, 0x02);
    set_channel(&mut bus, 0, 0x08, 0xFF, 0x00_8000, 400);
    bus.now = Cycles(0);
    // set_channel's writes took cycles; start again at 0 on a fresh frame.
    bus.timing = Timing::new(Region::Ntsc, Cycles(0));
    let (_, log) = start_dma(&mut bus, 0x01, 0x00_8000);
    assert_eq!(log.len(), 401);
    assert_eq!((log[126].channel, log[126].at), (0, Cycles(1106)));
    assert_eq!(
        (log[127].channel, log[127].hdma, log[127].value, log[127].at),
        (1, true, 0x5A, Cycles(1140))
    );
    assert_eq!((log[128].channel, log[128].at), (0, Cycles(1148)));
    assert_eq!(bus.peek(0x00_4305), (0x00, false));

    // DMA on channel 1 itself: the same times, and the transfer ends at the step with
    // 400 - 127 bytes left. 1148 - 14 is a whole number of 6-cycle clocks, so 6 more.
    let mut bus = self::bus();
    bus.wram[0x100..0x103].copy_from_slice(&table);
    set_channel(&mut bus, 1, 0x08, 0xFF, 0x7E_0100, 400);
    bus.dma.write_at(0x420C, 0x02);
    bus.now = Cycles(0);
    bus.timing = Timing::new(Region::Ntsc, Cycles(0));
    let (cost, log) = start_dma(&mut bus, 0x02, 0x00_8000);
    assert_eq!(log.len(), 128);
    assert!(
        log[..127]
            .iter()
            .all(|event| !event.hdma && event.at.0 <= 1106)
    );
    assert_eq!((log[127].hdma, log[127].at), (true, Cycles(1140)));
    assert_eq!(bus.peek(0x00_4315), (0x11, false));
    assert_eq!(bus.peek(0x00_4316), (0x01, false));
    assert_eq!(cost, 1148 - 14 + 6 + 6);
}

// @source: anomie-regs#2140
// @source: adr-0019
// @source: adr-0027
#[test]
fn the_apu_ports_read_zero_and_keep_the_cpus_writes_for_the_apu() {
    let mut bus = bus();
    for (port, address) in (0..4).zip([0x00_2140, 0x00_2141, 0x80_2142, 0x3F_217F]) {
        write(&mut bus, 0x00_4200, 0x5C);
        // The value written is not the value read back; the read is answered, so the
        // data bus takes its $00 (ADR 0027, point 4).
        write(&mut bus, address, 0xA0 | port);
        assert_eq!(read(&mut bus, address), 0x00, "{address:06X}");
        assert_eq!(bus.open_bus_value, 0x00);
        assert_eq!(bus.peek(address), (0x00, false));
        // The address's low two bits pick the port: $217F is port 3 (anomie-regs#2140).
        assert_eq!(
            bus.apu.written(ApuPort::from_b_bus(port)),
            0xA0 | port,
            "{address:06X}"
        );
    }
    // Every mirror of a port is the same latch.
    write(&mut bus, 0x00_2145, 0x77);
    assert_eq!(bus.apu.written(ApuPort::Port1), 0x77);
    // A port is a fast B-bus register (ADR 0019, point 3).
    assert_eq!(read_cost(&mut bus, 0x00_2140), 6);
}

// @source: adr-0027
#[test]
fn reset_clears_the_apu_ports() {
    let mut bus = bus();
    for address in 0x00_2140..=0x00_2143 {
        write(&mut bus, address, 0xFF);
    }
    bus.reset();
    for port in 0..4 {
        assert_eq!(bus.apu.written(ApuPort::from_b_bus(port)), 0x00);
    }
    assert_eq!(read(&mut bus, 0x00_2140), 0x00);
}

// @source: anomie-regs#2140
// @source: anomie-regs#43x0
// @source: anomie-regs#43x1
// @source: adr-0025
// @source: adr-0027
#[test]
fn dma_reaches_the_apu_ports_as_the_cpu_does() {
    let mut bus = bus();
    // Mode 4, ROM $00:8010 to $2140-$2143: one byte to each port.
    set_channel(&mut bus, 0, 0x04, 0x40, 0x00_8010, 4);
    let (_, log) = start_dma(&mut bus, 0x01, 0x00_8000);
    assert_eq!(log.len(), 4);
    for port in 0..4 {
        assert_eq!(bus.apu.written(ApuPort::from_b_bus(port)), 0x10 + port);
    }
    // B bus to A bus, $2140 to WRAM $00:0100: the port reads $00.
    write(&mut bus, 0x00_0100, 0xEE);
    set_channel(&mut bus, 1, 0x80, 0x40, 0x00_0100, 1);
    let (_, log) = start_dma(&mut bus, 0x02, 0x00_8000);
    assert_eq!(
        log.iter().map(|event| event.value).collect::<Vec<_>>(),
        [0x00]
    );
    assert_eq!(read(&mut bus, 0x00_0100), 0x00);
}

/// Idles until the clock reaches line `line` of the current frame.
// @source: adr-0020
fn idle_to_line(bus: &mut Bus, line: u16) {
    while bus.timing.line() != line {
        idle(bus);
    }
}

// @source: anomie-regs#registers
// @source: fullsnes#snesunpredictablethings
// @source: adr-0028
#[test]
fn ppu1_open_bus_answers_its_write_only_registers_and_ppu2_follows_the_counters() {
    let mut bus = bus();
    write(&mut bus, 0x00_4200, 0x5C);
    // PPU1 open bus is 0 at power-on; $2104 and $212A read it, not the data bus.
    assert_eq!(read(&mut bus, 0x00_2104), 0x00);
    assert_eq!(read(&mut bus, 0x00_212A), 0x00);
    assert_eq!(bus.peek(0x00_2118), (0x00, false));
    // $2134 is CPU open bus until #90; the read above placed $00 on the data bus.
    assert_eq!(read(&mut bus, 0x00_2134), 0x00);
    write(&mut bus, 0x00_4200, 0x5C);
    assert_eq!(read(&mut bus, 0x00_2134), 0x5C);
    // A $213C read becomes PPU2 open bus: OPVCT's high byte shows it in bits 7-1.
    assert_eq!(read(&mut bus, 0x00_213D), 0xFF);
    assert_eq!(read(&mut bus, 0x00_213C), 0xFF);
    assert_eq!(read(&mut bus, 0x00_213C), 0xFF);
    // Latching on line 0: OPVCT's flip-flop is on the high byte, so it reads bit 8 of V,
    // 0, under the last PPU2 value read, $FF.
    write(&mut bus, 0x00_4201, 0x80);
    read(&mut bus, 0x00_2137);
    assert_eq!(read(&mut bus, 0x00_213D), 0xFE);
}

// @source: anomie-timing#s-ppu/rendering
// @source: adr-0028
#[test]
fn a_line_is_drawn_when_the_ppu_is_synced_after_it_starts_or_when_the_frame_ends() {
    let mut bus = bus();
    bus.ppu.cgram_mut()[..2].copy_from_slice(&[0x11, 0x00]);
    idle_to_line(&mut bus, 10);
    // A write to any PPU register syncs it: lines 1 to 10 are drawn with colour $0011.
    write(&mut bus, 0x00_2100, 0x00);
    assert_eq!(bus.ppu.next_line(), 11);
    // A CGRAM change that does not pass through the bus reaches only later lines.
    bus.ppu.cgram_mut()[..2].copy_from_slice(&[0x22, 0x00]);
    idle_to_line(&mut bus, 225);
    let frame = bus.video.frame();
    let pixels = frame.pixels;
    assert_eq!((frame.width, frame.height, frame.field), (256, 224, None));
    let [first, later] = [0x0011, 0x0022].map(|bits| Rgb555::new(bits).unwrap());
    assert!(pixels[..10 * 256].iter().all(|&pixel| pixel == first));
    assert!(pixels[10 * 256..].iter().all(|&pixel| pixel == later));
    // Vertical blank draws nothing, and the next frame starts from line 1.
    assert_eq!(bus.ppu.next_line(), 1);
    write(&mut bus, 0x00_2100, 0x00);
    assert_eq!(bus.ppu.next_line(), 1);
}

// @source: anomie-regs#2133
// @source: anomie-timing#s-ppu/blanking-periods
// @source: adr-0028
#[test]
fn overscan_frames_have_239_lines_and_a_reset_abandons_the_frame() {
    let mut bus = bus();
    write(&mut bus, 0x00_2133, 0x04);
    idle_to_line(&mut bus, 241);
    assert_eq!(bus.video.frame().height, 239);
    idle_to_line(&mut bus, 100);
    write(&mut bus, 0x00_2133, 0x00);
    bus.reset();
    assert_eq!(bus.ppu.next_line(), 1);
    // The frame after the reset starts again from line 1, without overscan.
    idle_to_line(&mut bus, 226);
    assert_eq!(bus.video.frame().height, 224);
}

// @source: anomie-regs#2133
// @source: anomie-regs#213f
// @source: adr-0020
// @source: adr-0028
#[test]
fn an_interlaced_frame_is_448_rows_and_names_its_field() {
    let mut bus = bus();
    write(&mut bus, 0x00_2133, 0x01);
    idle_to_line(&mut bus, 225);
    let frame = bus.video.frame();
    // The first frame after power-on is field 1 (ADR 0020, point 4).
    assert_eq!(
        (frame.width, frame.height, frame.field),
        (256, 448, Some(Field::Second))
    );
}
