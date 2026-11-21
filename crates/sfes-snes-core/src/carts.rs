// @source: adr-0013
// @source: adr-0019
// @source: adr-0021
// @source: adr-0023
// @source: adr-0024
// @source: adr-0025
//! Runs the project's own test cartridges (ADR 0013) and checks what they leave against
//! values computed from the documents.

use alloc::{sync::Arc, vec::Vec};

use sfes_api::{Buttons, Cycles, Fast, InstrEvent, Interrupt, Observer, RegionId, Stop};
use sfes_snes_cpu::RunState;

use crate::Snes;

/// Records each instruction's program address and start time.
#[derive(Clone, Debug, Default)]
struct Instructions(Vec<(u32, Cycles)>);

impl Observer for Instructions {
    // @source: adr-0018
    fn on_instruction(&mut self, event: &InstrEvent) {
        self.0.push((event.pc, event.at));
    }
}

/// Records each interrupt entry and its start time.
#[derive(Clone, Debug, Default)]
struct Interrupts(Vec<(Interrupt, Cycles)>);

impl Observer for Interrupts {
    // @source: adr-0018
    fn on_interrupt(&mut self, interrupt: Interrupt, at: Cycles) {
        self.0.push((interrupt, at));
    }
}

// @source: anomie-memmap#memory-map
// @source: anomie-timing#s-cpu/instructions
// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: anomie-regs#2180
// @source: anomie-regs#2181
// @source: anomie-regs#420d
// @source: fullsnes#snesmemorycontrol
// @source: wdc-w65c816s#table-5-7-1a
// @source: wdc-w65c816s#table-5-7-1d
// @source: wdc-w65c816s#table-5-7-4a
// @source: wdc-w65c816s#table-5-7-4b
// @source: wdc-w65c816s#table-5-7-18
// @source: adr-0006
// @source: adr-0019
#[test]
fn bus_timing_cart_runs_fast_rom_the_wram_port_and_a_refresh() {
    let rom = include_bytes!("../../../tests/assets/carts/bus-timing.sfc");
    let mut snes = Snes::<Fast>::builder(Arc::from(rom.as_slice()))
        .observer(Instructions::default())
        .build()
        .expect("builds");
    snes.run(Stop::Instructions(28));
    assert_eq!(snes.cpu().run, RunState::Stopped);

    // Master cycles per CPU cycle (anomie-memmap#memory-map): ROM in bank $00 and WRAM are
    // Slow (8), bank $80 ROM with MEMSEL set is Fast (6), and $2180-$2183 and $420D are
    // Fast. Cycle counts per instruction are the datasheet rows, in emulation mode with
    // 8-bit data: immediate 2 (18), absolute 4 (1a), absolute R-M-W 6 (1d), absolute
    // long 5 (4a), JML 4 (4b).
    let expected = [
        (0x00_8000, 54),  // LDA #$01: 2 Slow fetches; the reset sequence took 54
        (0x00_8002, 70),  // STA $420D: 3 Slow fetches, a Fast write
        (0x00_8005, 100), // JML $80:8009: 4 Slow fetches, bank $00 is never Fast
        (0x80_8009, 132), // LDA #$A5: 2 Fast fetches
        (0x80_800B, 144), // STA $0000: 3 Fast fetches, a Slow WRAM write
        (0x80_800E, 170), // JML $00:8012: 4 Fast fetches
        (0x00_8012, 194), // STZ $2181: 3 Slow fetches, a Fast write
        (0x00_8015, 224), // LDA #$01
        (0x00_8017, 240), // STA $2182
        (0x00_801A, 270), // STA $2183
        (0x00_801D, 300), // LDA #$5A
        (0x00_801F, 316), // STA $2180
        (0x00_8022, 346), // LDA #$C3
        (0x00_8024, 362), // STA $2180
        (0x00_8027, 392), // STZ $2181
        (0x00_802A, 422), // LDA $2180: 3 Slow fetches, a Fast read
        (0x00_802D, 452), // STA $0001: 3 Slow fetches, a Slow write
        (0x00_8030, 484), // LDA $2180
        // STA $0002: its third fetch ends at 538, when the first refresh is due, so its
        // write starts 40 cycles later (anomie-timing#s-cpu/clocks-and-refresh).
        (0x00_8033, 514),
        (0x00_8036, 586), // LDA $2181
        (0x00_8039, 616), // STA $0003
        (0x00_803C, 648), // LDA #$40
        (0x00_803E, 664), // STA $7F:0200: 4 Slow fetches, a Slow write
        (0x00_8042, 704), // STZ $2181
        (0x00_8045, 734), // LDA #$02
        (0x00_8047, 750), // STA $2182
        (0x00_804A, 780), // INC $2180: 3 Slow fetches, a Fast read and two Fast writes
        (0x00_804D, 822), // STP
    ]
    .map(|(pc, at)| (pc, Cycles(at)));
    assert_eq!(snes.observer().0, expected);

    let wram = snes.region(RegionId::Wram);
    // The marker written from bank $80, the two bytes read back through the port, and open
    // bus from the write-only WMADDL: the high byte of its operand.
    assert_eq!(wram[..4], [0xA5, 0x5A, 0xC3, 0x21]);
    assert_eq!(wram[0x1_0100..0x1_0102], [0x5A, 0xC3]);
    // INC $2180 read $7F:0200, then wrote the value read and the result (ADR 0019, point 7).
    assert_eq!(wram[0x1_0200..0x1_0203], [0x40, 0x40, 0x41]);
}

// @source: anomie-timing#s-cpu/interrupts
// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: anomie-timing#s-cpu/registers
// @source: anomie-regs#4210
// @source: anomie-regs#4211
// @source: anomie-regs#4212
// @source: anomie-regs#2137
// @source: anomie-regs#213c
// @source: anomie-regs#213d
// @source: fullsnes#snestiminghvevents
// @source: wdc-w65c816s#table-5-7-22a
// @source: wdc-w65c816s#table-5-7-19d
// @source: adr-0004
// @source: adr-0019
// @source: adr-0020
// @source: adr-0021
// @source: adr-0022
#[test]
fn interrupts_cart_takes_the_hv_irq_and_the_nmi_on_time() {
    let rom = include_bytes!("../../../tests/assets/carts/interrupts.sfc");
    let mut snes = Snes::<Fast>::builder(Arc::from(rom.as_slice()))
        .observer(Interrupts::default())
        .build()
        .expect("builds");
    snes.run(Stop::Frames(2));
    assert_eq!(snes.cpu().run, RunState::Stopped);

    // The set-up runs from 54, after the reset sequence, in emulation mode from Slow ROM
    // (anomie-memmap#memory-map): LDA # 16 cycles, STA/STZ abs to $42xx 3 × 8 + 6, CLI and
    // WAI 8 + 6 and 8 + 6 + 6. WAI ends at 286, then the CPU idles in 6-cycle steps.
    // Refreshes pause it 40 cycles after the cycle each falls in: at 538, 1898, 3266, 4626
    // and 5994 (ADR 0019), so its idle cycles restart at 578, 1938, 3310, 4670 and 6036.
    // The trigger point is 5 × 1364 + 14 + 4 × 100 = 7234 (anomie-timing), inside the
    // cycle 7230-7236. The cycle 7236-7242 is the first to start with the line low, so the
    // step that ends the wait starts at 7242 (ADR 0021, point 1). The observer stamps the
    // interrupt with that step's start; its two internal cycles end the wait at 7254, where
    // row 22a begins (ADR 0022).
    let [irq, nmi] = <[_; 2]>::try_from(snes.observer().0.clone()).expect("two entries");
    assert_eq!(irq, (Interrupt::Irq, Cycles(7242)));
    // The NMI flag is set at 225 × 1364 + 2 (fullsnes#snestiminghvevents); the first
    // 6-cycle idle that starts at or after it is followed by the entry.
    let (kind, at) = nmi;
    assert_eq!(kind, Interrupt::Nmi);
    assert!(
        (Cycles(306_908)..Cycles(306_914)).contains(&at),
        "NMI entry at {at:?}"
    );

    // The IRQ entry is 54 cycles: a Slow opcode read, an internal cycle, three Slow pushes
    // and two Slow vector reads (row 22a), so the handler starts at 7308.
    // - LDA $4211 reads at 7332: the flag over open bus $42, the operand's high byte.
    // - STA $0000's operand low byte is read at 7346-7354, the cycle the refresh at 7354
    //   ends; 40 later its operand high byte and the write follow, and LDA $2137 reads at
    //   7434, 614 cycles into line 5 (from 6820): dot 153.
    // - Each high byte has bit 8 of the counter, 0, over the last PPU2 value read.
    // The NMI handler reads $4210 with the flag set over open bus $42 and version 2, $4212 in
    // vertical blank and outside horizontal blank, and $4210 again with the flag cleared.
    let wram = snes.region(RegionId::Wram);
    assert_eq!(wram[..8], [0xC2, 153, 152, 5, 4, 0xC2, 0x82, 0x42]);
}

// @source: anomie-regs#4202
// @source: anomie-regs#4203
// @source: anomie-regs#4204
// @source: anomie-regs#4206
// @source: anomie-regs#4214
// @source: anomie-regs#4216
// @source: anomie-regs#4201
// @source: anomie-regs#4213
// @source: fullsnes#snesmathsmultiplydivide
// @source: fullsnes#snescontrollersioportsmanualreading
// @source: wdc-w65c816s#table-5-7-1a
// @source: wdc-w65c816s#table-5-7-19a
// @source: wdc-w65c816s#table-5-7-19b
// @source: adr-0023
#[test]
fn math_cart_reads_results_when_they_appear() {
    let rom = include_bytes!("../../../tests/assets/carts/math.sfc");
    let mut snes = Snes::<Fast>::builder(Arc::from(rom.as_slice()))
        .build()
        .expect("builds");
    snes.run(Stop::Frames(1));
    assert_eq!(snes.cpu().run, RunState::Stopped);

    let wram = snes.region(RegionId::Wram);
    let expected = [
        // 200 × 250 = $C350: 0 in the 8th cycle, then the product, the multiplier and 0.
        0x00, 0x50, 0xC3, 250, 0x00, // 200 × 3 = $0258 in the 9th cycle.
        0x58, // 12345 / 100: the multiplier 3 in the 16th cycle, then 123 remainder 45.
        3, 123, 0, 45, 0, // 12345 / 7 = $06E3 remainder 4, in the 17th cycle.
        0xE3, 4, // 12345 / 0: $FFFF remainder $3039.
        0xFF, 0xFF, 0x39, 0x30,
        // RDIO: $FF from power-on, then $40 after the $4201 write.
        0xFF, 0x40,
    ];
    assert_eq!(wram[..expected.len()], expected);
}

// @source: anomie-regs#controllers/generic
// @source: anomie-regs#controllers/joypads
// @source: anomie-regs#4016
// @source: anomie-regs#4017
// @source: anomie-regs#4200
// @source: anomie-regs#4212
// @source: anomie-regs#4218
// @source: anomie-memmap#hardware/data-bus
// @source: anomie-timing#s-cpu/auto-joypad-read
// @source: fullsnes#snescontrollersioportsmanualreading
// @source: fullsnes#snescontrollersioportsautomaticreading
// @source: wdc-w65c816s#table-5-7-1a
// @source: adr-0024
#[test]
fn joypad_cart_reads_the_pads_by_hand_and_by_the_auto_read() {
    let rom = include_bytes!("../../../tests/assets/carts/joypad.sfc");
    let mut snes = Snes::<Fast>::builder(Arc::from(rom.as_slice()))
        .build()
        .expect("builds");
    snes.set_input(
        0,
        Buttons::B | Buttons::SELECT | Buttons::RIGHT | Buttons::X,
    );
    snes.set_input(1, Buttons::Y | Buttons::UP | Buttons::L);
    // The reads by hand fall in the first frame, the auto-read just after it ends.
    snes.run(Stop::Frames(1));
    assert!(snes.polled());
    snes.run(Stop::Frames(1));
    assert_eq!(snes.cpu().run, RunState::Stopped);
    assert!(snes.polled());
    snes.run(Stop::Frames(1));
    assert!(!snes.polled(), "a stopped CPU reads nothing: a lag frame");

    let wram = snes.region(RegionId::Wram);
    // Port 1 in serial order, B Y Select Start Up Down Left Right A X L R, then ID 0000
    // and a 17th bit of 1, each over the open-bus $40 of the address's high byte.
    let by_hand = [
        0x41, 0x40, 0x41, 0x40, 0x40, 0x40, 0x40, 0x41, 0x40, 0x41, 0x40, 0x40, 0x40, 0x40, 0x40,
        0x40, 0x41,
    ];
    assert_eq!(wram[..0x11], by_hand);
    // Port 2's B and Y, with bits 4-2 of $4017 set.
    assert_eq!(wram[0x11..0x13], [0x5C, 0x5D]);
    // With the latch high, B every time.
    assert_eq!(wram[0x13..0x16], [0x41; 3]);
    // byetUDLR axlr0000: port 1 is $A140 and port 2 $4820; Data2 is not connected.
    assert_eq!(wram[0x16..0x1E], [0x40, 0xA1, 0x20, 0x48, 0, 0, 0, 0]);
}

// @source: anomie-regs#420b
// @source: anomie-regs#43x0
// @source: anomie-regs#43x2
// @source: anomie-regs#43x5
// @source: anomie-regs#43xb
// @source: anomie-regs#dma-and-hdma/dma
// @source: anomie-regs#2180
// @source: anomie-timing#s-cpu/dma
// @source: fullsnes#snesunpredictablethings
// @source: adr-0013
// @source: adr-0025
#[test]
fn dma_cart_moves_bytes_through_the_wram_port_and_leaves_the_registers_stepped() {
    let rom = include_bytes!("../../../tests/assets/carts/dma.sfc");
    let mut snes = Snes::<Fast>::builder(Arc::from(rom.as_slice()))
        .build()
        .expect("builds");
    snes.run(Stop::Frames(1));
    assert_eq!(snes.cpu().run, RunState::Stopped);

    let wram = snes.region(RegionId::Wram);
    let source = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88];
    // Mode 0, incrementing: the source in order (anomie-regs#43x0).
    assert_eq!(wram[0x100..0x108], source);
    // Fixed: the one fill byte five times; decrementing: the last four backwards.
    assert_eq!(wram[0x108..0x10D], [0xA5; 5]);
    assert_eq!(wram[0x10D..0x111], [0x88, 0x77, 0x66, 0x55]);
    // $4017 through the A bus: the NOP's $EA with bits 4-2 set (ADR 0025, points 6 and 8).
    assert_eq!(wram[0x111], 0xFE);
    // $2180 to WRAM fails on the B-bus side: WRAM takes the open-bus $EA (ADR 0025,
    // point 7), and the port did not move, so $0111 above was the next byte it wrote.
    assert_eq!(wram[0x120..0x122], [0xEA, 0xEA]);
    // Channel 0 ends 8 bytes on, bank unchanged, count 0; channel 3 four bytes back from
    // source + 7; $430B keeps its power-on $FF (anomie-regs#43x2, #43x5, #43xb).
    let address = u16::from_le_bytes([wram[0x140], wram[0x141]]);
    let [after_low, after_high] = (address + 8).to_le_bytes();
    assert_eq!(
        wram[0x130..0x135],
        [after_low, after_high, 0x00, 0x00, 0x00]
    );
    let [back_low, back_high] = (address + 3).to_le_bytes();
    assert_eq!(wram[0x135..0x138], [back_low, back_high, 0xFF]);
}

// @source: anomie-regs#420c
// @source: anomie-regs#43x5
// @source: anomie-regs#43x8
// @source: anomie-regs#43xa
// @source: anomie-regs#dma-and-hdma/hdma
// @source: anomie-regs#2180
// @source: anomie-timing#s-cpu/hdma
// @source: fullsnes#snesdmaandhdmachannel07registers
// @source: adr-0013
// @source: adr-0026
#[test]
fn hdma_cart_runs_a_direct_and_an_indirect_table_into_the_wram_port() {
    let rom = include_bytes!("../../../tests/assets/carts/hdma.sfc");
    let mut snes = Snes::<Fast>::builder(Arc::from(rom.as_slice()))
        .build()
        .expect("builds");
    snes.run(Stop::Frames(3));
    assert_eq!(snes.cpu().run, RunState::Stopped);

    let wram = snes.region(RegionId::Wram);
    // Line 0: channel 0's $D1, then channel 1's $E1 from data_a; line 1 nothing; line 2
    // channel 0's $D2; line 3 channel 0's $D3, then channel 1's $E2 from data_b
    // (anomie-regs#dma-and-hdma/hdma; ADR 0026, points 5 and 6).
    assert_eq!(wram[0x200..0x205], [0xD1, 0xE1, 0xD2, 0xD3, 0xE2]);
    // Channel 0's table address is past its $00, which $430A keeps (anomie-regs#43x8,
    // #43xa).
    let direct = u16::from_le_bytes([wram[0x140], wram[0x141]]);
    let [low, high] = (direct + 6).to_le_bytes();
    assert_eq!(wram[0x130..0x133], [low, high, 0x00]);
    // Channel 1 was the last active channel when it read its $00, so it read only the
    // $77 after it into $4316 and set $4315 to $00 (ADR 0026, point 6).
    let indirect = u16::from_le_bytes([wram[0x142], wram[0x143]]);
    let [low, high] = (indirect + 8).to_le_bytes();
    assert_eq!(wram[0x133..0x138], [0x00, 0x77, low, high, 0x00]);
}
