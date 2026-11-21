// @source: adr-0019
//! Unit tests of the bus: decode, the WRAM port, open bus, access speed, refresh, the
//! interrupt and timer registers, the math unit, the controller ports and DMA.

use alloc::{vec, vec::Vec};

use sfes_api::{Addr24, Buttons, Cycles, DmaEvent, NoopObserver};
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
        0x00_2100, 0x00_2140, // PPU and APU, until those chips exist
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

// @source: anomie-timing#s-cpu/auto-joypad-read
// @source: anomie-regs#4200
// @source: anomie-regs#4212
// @source: anomie-regs#4218
// @source: adr-0024
#[test]
fn the_auto_read_sets_hvbjoy_bit_0_and_fills_joy1_at_dot_74_5_of_line_225() {
    let mut bus = bus();
    bus.ports.input[0] = Buttons::A | Buttons::START;
    write(&mut bus, 0x00_4200, 0x01);
    let start = 225 * 1364 + 298;
    // Each idle is 6 master cycles: stop in the cycle before the start.
    while bus.now.0 + 6 <= start {
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

// @source: fullsnes#snesiomap
// @source: anomie-regs#4200
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
    let before = bus.now.0;
    BusView::new(bus, &mut log).idle(Addr24::new(0), Pins::empty());
    (bus.now.0 - before, log.0)
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

// @source: anomie-timing#s-cpu/dma
// @source: anomie-regs#dma-and-hdma/dma
// @source: anomie-regs#2180
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

// @source: anomie-regs#dma-and-hdma/dma
// @source: anomie-regs#43x2
// @source: fullsnes#snesunpredictablethings
// @source: anomie-regs#4214
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
    let before = bus.now.0;
    idle(&mut bus);
    assert_eq!(bus.now.0 - before, 6);
    assert_eq!(bus.peek(0x00_4305), (0x04, false));
}
