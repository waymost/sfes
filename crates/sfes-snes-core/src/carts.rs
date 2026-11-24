// @source: adr-0013
// @source: adr-0019
// @source: adr-0021
// @source: adr-0023
// @source: adr-0024
// @source: adr-0025
//! Runs the project's own test cartridges (ADR 0013) and checks what they leave against
//! values computed from the documents.

use alloc::{sync::Arc, vec, vec::Vec};
use core::ops::Range;

use sfes_api::{Buttons, Cycles, Fast, InstrEvent, Interrupt, Observer, RegionId, Rgb555, Stop};
use sfes_snes_cpu::RunState;
use sfes_snes_ppu::LINE_COLUMNS;

use crate::{LayerKind, Snes};

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
// @source: anomie-regs#2180
// @source: anomie-regs#2181
// @source: anomie-regs#420d
// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: anomie-timing#s-cpu/instructions
// @source: fullsnes#snesmemorycontrol
// @source: wdc-w65c816s#table-5-7-18
// @source: wdc-w65c816s#table-5-7-1a
// @source: wdc-w65c816s#table-5-7-1d
// @source: wdc-w65c816s#table-5-7-4a
// @source: wdc-w65c816s#table-5-7-4b
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

// @source: anomie-regs#2137
// @source: anomie-regs#213c
// @source: anomie-regs#213d
// @source: anomie-regs#4210
// @source: anomie-regs#4211
// @source: anomie-regs#4212
// @source: anomie-timing#s-cpu/clocks-and-refresh
// @source: anomie-timing#s-cpu/interrupts
// @source: anomie-timing#s-cpu/registers
// @source: fullsnes#snestiminghvevents
// @source: wdc-w65c816s#table-5-7-19d
// @source: wdc-w65c816s#table-5-7-22a
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

// @source: anomie-regs#4201
// @source: anomie-regs#4202
// @source: anomie-regs#4203
// @source: anomie-regs#4204
// @source: anomie-regs#4206
// @source: anomie-regs#4213
// @source: anomie-regs#4214
// @source: anomie-regs#4216
// @source: fullsnes#snescontrollersioportsmanualreading
// @source: fullsnes#snesmathsmultiplydivide
// @source: wdc-w65c816s#table-5-7-19a
// @source: wdc-w65c816s#table-5-7-19b
// @source: wdc-w65c816s#table-5-7-1a
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

// @source: anomie-memmap#hardware/data-bus
// @source: anomie-regs#4016
// @source: anomie-regs#4017
// @source: anomie-regs#4200
// @source: anomie-regs#4212
// @source: anomie-regs#4218
// @source: anomie-regs#controllers/generic
// @source: anomie-regs#controllers/joypads
// @source: anomie-timing#s-cpu/auto-joypad-read
// @source: fullsnes#snescontrollersioportsautomaticreading
// @source: fullsnes#snescontrollersioportsmanualreading
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

// @source: anomie-regs#2180
// @source: anomie-regs#420b
// @source: anomie-regs#43x0
// @source: anomie-regs#43x2
// @source: anomie-regs#43x5
// @source: anomie-regs#43xb
// @source: anomie-regs#dma-and-hdma/dma
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

// @source: anomie-regs#2180
// @source: anomie-regs#420c
// @source: anomie-regs#43x5
// @source: anomie-regs#43x8
// @source: anomie-regs#43xa
// @source: anomie-regs#dma-and-hdma/hdma
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

// The colours the drawing cartridges put in CGRAM, as 15-bit `0bbbbbgggggrrrrr` words
// (fullsnes#snesppucolorpalettememorycgramanddirectcolors).

/// Black, as the colour window clips to.
const BLACK: Rgb555 = Rgb555::from_bits_truncate(0x0000);
/// Full white.
const WHITE: Rgb555 = Rgb555::from_bits_truncate(0x7FFF);
/// Full red.
const RED: Rgb555 = Rgb555::from_bits_truncate(0x001F);
/// Full green.
const GREEN: Rgb555 = Rgb555::from_bits_truncate(0x03E0);
/// Full blue.
const BLUE: Rgb555 = Rgb555::from_bits_truncate(0x7C00);
/// Full yellow (red and green).
const YELLOW: Rgb555 = Rgb555::from_bits_truncate(0x03FF);
/// Full magenta (red and blue).
const MAGENTA: Rgb555 = Rgb555::from_bits_truncate(0x7C1F);
/// Full cyan (green and blue).
const CYAN: Rgb555 = Rgb555::from_bits_truncate(0x7FE0);
/// The grey (10, 10, 10) the colour-math and hi-res carts give the backdrop.
const GREY: Rgb555 = Rgb555::from_channels(10, 10, 10);

/// The rows of a picture: lines 1 to 224, in the order drawn (ADR 0028, point 8).
const PICTURE_ROWS: usize = 224;

/// Runs a drawing cartridge until its picture is steady: its setup, which clears VRAM by DMA,
/// takes more than a frame, and the frame after is whole.
// @source: adr-0013
fn drawn(rom: &[u8]) -> Snes<Fast> {
    let mut snes = Snes::<Fast>::builder(Arc::from(rom))
        .build()
        .expect("builds");
    snes.run(Stop::Frames(4));
    assert_eq!(snes.cpu().run, RunState::Stopped);
    snes
}

/// A 256-pixel row of runs `(colour, length)`, then `fill` to the end.
// @source: adr-0013
fn runs(list: &[(Rgb555, usize)], fill: Rgb555) -> Vec<Rgb555> {
    let mut row = Vec::new();
    for &(color, length) in list {
        row.extend(core::iter::repeat_n(color, length));
    }
    row.resize(LINE_COLUMNS, fill);
    row
}

/// Checks every pixel of the steady picture, `width` pixels by 224 rows (row 0 is line 1):
/// - a row in a band of `Some(row)` is that row;
/// - a row in a band of `None` is not checked;
/// - every other row is `fill` across.
// @source: adr-0013
fn assert_picture(
    snes: &Snes<Fast>,
    width: usize,
    bands: &[(Range<usize>, Option<&[Rgb555]>)],
    fill: Rgb555,
) {
    let video = snes.video();
    assert_eq!(
        (usize::from(video.width), usize::from(video.height)),
        (width, PICTURE_ROWS)
    );
    let blank = vec![fill; width];
    for (row, pixels) in video.pixels.chunks_exact(width).enumerate() {
        let expected = match bands.iter().find(|(rows, _)| rows.contains(&row)) {
            Some((_, Some(expected))) => expected,
            Some((_, None)) => continue,
            None => blank.as_slice(),
        };
        assert_eq!(pixels, expected, "row {row}");
    }
}

// @source: anomie-regs#210d
// @source: anomie-regs#211a
// @source: anomie-regs#211b
// @source: anomie-regs#2133
// @source: anomie-regs#backgrounds/mode-7
// @source: anomie-regs#dma-and-hdma/hdma
// @source: fullsnes#snesppurotationscaling
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: adr-0013
// @source: adr-0026
// @source: adr-0028
// @source: adr-0032
#[test]
fn bg_mode_7_bands_cart_flips_swaps_axes_fills_outside_and_shows_extbg() {
    let snes = drawn(include_bytes!(
        "../../../tests/assets/carts/bg-mode7-bands.sfc"
    ));
    // Every map row is the same: column 0 red, 1 blue, 127 character 4, the rest clear. Line
    // L, column x is SX = x and SY = L, flipped by M7SEL, and with no centre the position is
    // X = A*(HOFS + SX) + B*(VOFS + SY), Y = C*(HOFS + SX) + D*(VOFS + SY) (ADR 0032,
    // point 2). Band n is rows 16n to 16n + 15, lines 16n + 1 on (ADR 0028, point 6).
    //   0: X = x. Columns 0 and 1 are pixels 0 to 15.
    //   1: X = 255 - x. Column 0 is pixels 255 to 248, column 1 247 to 240.
    //   2: X = x - 8. Pixels 0 to 7 are X -8 to -1, which wraps to column 127, character
    //      4, BG1's cyan (ADR 0032, point 3).
    //   3: outside is transparent: the backdrop.
    //   4: outside is character 0: magenta.
    //   5: X = L - 81 and Y = x: line 81 on is column 0 whole, line 89 on column 1.
    //   6: SY = L ^ $FF, so X = 112 - L: lines 97 to 104 are column 1, 105 to 112 column 0.
    //   7: as 2, with EXTBG's BG2 on: character 4's $84 is BG2 entry 4 at priority 1, in
    //      front of BG1 (yellow); the others are priority 0, behind BG1, and the same
    //      colour (ADR 0032, point 5).
    let straight = runs(&[(RED, 8), (BLUE, 8)], WHITE);
    let flipped = runs(&[(WHITE, 240), (BLUE, 8), (RED, 8)], WHITE);
    let wrapped = runs(&[(CYAN, 8), (RED, 8), (BLUE, 8)], WHITE);
    let transparent = runs(&[(WHITE, 8), (RED, 8), (BLUE, 8)], WHITE);
    let filled = runs(&[(MAGENTA, 8), (RED, 8), (BLUE, 8)], WHITE);
    let red = vec![RED; LINE_COLUMNS];
    let blue = vec![BLUE; LINE_COLUMNS];
    let extbg = runs(&[(YELLOW, 8), (RED, 8), (BLUE, 8)], WHITE);
    assert_picture(
        &snes,
        256,
        &[
            (0..16, Some(&straight)),
            (16..32, Some(&flipped)),
            (32..48, Some(&wrapped)),
            (48..64, Some(&transparent)),
            (64..80, Some(&filled)),
            (80..88, Some(&red)),
            (88..96, Some(&blue)),
            (96..104, Some(&blue)),
            (104..112, Some(&red)),
            (112..PICTURE_ROWS, Some(&extbg)),
        ],
        WHITE,
    );
}

// @source: anomie-regs#2105
// @source: anomie-regs#210d
// @source: anomie-regs#211b
// @source: anomie-regs#211e
// @source: anomie-regs#211f
// @source: anomie-regs#212c
// @source: anomie-regs#backgrounds/mode-7
// @source: fullsnes#snesppurotationscaling
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: adr-0013
// @source: adr-0032
#[test]
fn bg_mode_7_cart_scales_scrolls_and_wraps_the_playfield() {
    let snes = drawn(include_bytes!("../../../tests/assets/carts/bg-mode7.sfc"));
    // With A = 2.0 and a scroll of -8, screen column x is playfield column 2 * (x - 8) (the
    // centre is 0). Columns 0 to 3 are playfield -16 to -10, which wrap to tilemap column
    // 126 (character 0, transparent); 4 to 7 are -8 to -2, tilemap column 127 (character 4);
    // then 8 to 11, 12 to 15 and 16 to 19 are the first three columns, characters 1 to 3;
    // column 20 is playfield 24, tilemap column 3 (character 0 again).
    let expected = runs(
        &[(WHITE, 4), (MAGENTA, 4), (RED, 4), (BLUE, 4), (GREEN, 4)],
        WHITE,
    );
    // Columns 20 to 255 are playfield 24 to 494, tilemap columns 3 to 61: character 0.
    // D = 2.0 too, so lines 1 to 3 are playfield rows 2, 4 and 6, still in the first row of
    // tiles, which are solid colours; line 4 is row 8, in the next tile row, and every
    // line down to 224 (row 448) is in a tilemap row that is all character 0.
    assert_picture(&snes, 256, &[(0..3, Some(&expected))], WHITE);
}

// @source: anomie-regs#2101
// @source: anomie-regs#212c
// @source: anomie-regs#213e
// @source: anomie-regs#sprites
// @source: anomie-regs#sprites/drawing-the-sprites
// @source: anomie-regs#sprites/sprite-priority
// @source: fullsnes#snesppuspritesobjs
// @source: fullsnes#snespputimersandstatus
// @source: adr-0013
// @source: adr-0033
#[test]
fn sprites_cart_draws_sprites_in_order_with_flips_and_sets_range_over() {
    let snes = drawn(include_bytes!(
        "../../../tests/assets/carts/obj-sprites.sfc"
    ));
    // Sprites 0 (X 8, red) and 1 (X 12, blue) are 8 pixels wide and rows high, and sprite 0
    // is in front, so blue shows only at 16 to 19. Sprite 2 is 16 by 16 at X 32 flipped both
    // ways: its screen's top row of tiles is the character table's second row, right to left,
    // so tile $31 (cyan) comes first and then $30 (yellow); the second row is $21 (magenta)
    // then $20 (green).
    let top = runs(
        &[
            (WHITE, 8),
            (RED, 8),
            (BLUE, 4),
            (WHITE, 12),
            (CYAN, 8),
            (YELLOW, 8),
        ],
        WHITE,
    );
    let bottom = runs(&[(WHITE, 32), (MAGENTA, 8), (GREEN, 8)], WHITE);
    // Sprites at Y 0 are on lines 1 to 8 (rows 0 to 7) for the small ones, to line 16 (row
    // 15) for the large one. Sprites 3 to 35, 8x8 at X 0 and Y 100, are on lines 101 to 108,
    // and sprites 36 to 127 at Y 128 on lines 129 to 136: tile 0, palette 0, red.
    let corner = runs(&[(RED, 8)], WHITE);
    assert_picture(
        &snes,
        256,
        &[
            (0..8, Some(&top)),
            (8..16, Some(&bottom)),
            (100..108, Some(&corner)),
            (128..136, Some(&corner)),
        ],
        WHITE,
    );
    // 33 sprites on one line set range over (bit 6) but not time over (33 tiles); the
    // version is 1.
    let status = snes.peek_bus(0x00_213E).value;
    assert_eq!(status & 0xC0, 0x40);
    assert_eq!(status & 0x0F, 1);
}

// @source: anomie-regs#2101
// @source: anomie-regs#2103
// @source: anomie-regs#213e
// @source: anomie-regs#sprites
// @source: anomie-regs#sprites/drawing-the-sprites
// @source: anomie-regs#sprites/sprite-priority
// @source: fullsnes#snesppuspritesobjs
// @source: fullsnes#snespputimersandstatus
// @source: adr-0013
// @source: adr-0029
// @source: adr-0033
#[test]
fn obj_limits_cart_rotates_priority_flips_by_halves_and_drops_tiles_on_time_over() {
    let snes = drawn(include_bytes!("../../../tests/assets/carts/obj-limits.sfc"));
    // Sprites are 16x32; a sprite at Y is on lines Y + 1 to Y + 32, rows Y to Y + 31.
    //   rows 0-31: sprite 1 (cyan) at 0 and sprite 2 (magenta) at 8. Sprite 2 is the first
    //        ($2103 bit 7 with word address 4), so it is in front at 8 to 15 (ADR 0033,
    //        point 6).
    //   rows 40-71: sprite 3 at 0 has its tile rows in order, red, green, blue, yellow.
    //        Sprite 4 at 32 is flipped vertically, which for a 16x32 sprite flips each 16x16
    //        half: green, red, yellow, blue (ADR 0033, point 4).
    //   rows 80-111: 18 sprites of 2 tiles on a line. Loaded from the last, sprites 22 to 6
    //        fill the 34 tiles, so sprite 5 at 0, the first of them, is not drawn (ADR
    //        0033, point 5).
    //   rows 120-151: sprites 39 to 24 take 32 tiles and sprite 40, at X -256 read as 0
    //        for its tiles' time, 2 more; sprite 23 at 32 is dropped, and sprite 40 draws
    //        nothing at its true X.
    let rotated = runs(&[(CYAN, 8), (MAGENTA, 16)], WHITE);
    let band = |left: Rgb555, right: Rgb555| runs(&[(left, 16), (WHITE, 16), (right, 16)], WHITE);
    let crowded = runs(&[(WHITE, 200), (CYAN, 16)], WHITE);
    assert_picture(
        &snes,
        256,
        &[
            (0..32, Some(&rotated)),
            (40..48, Some(&band(RED, GREEN))),
            (48..56, Some(&band(GREEN, RED))),
            (56..64, Some(&band(BLUE, YELLOW))),
            (64..72, Some(&band(YELLOW, BLUE))),
            (80..112, Some(&crowded)),
            (120..152, Some(&crowded)),
        ],
        WHITE,
    );
    // Time over is set by the crowded lines; no line has more than 32 sprites.
    assert_eq!(snes.peek_bus(0x00_213E).value & 0xC0, 0x80);
}

// @source: anomie-regs#2123
// @source: anomie-regs#2126
// @source: anomie-regs#212a
// @source: anomie-regs#212e
// @source: anomie-regs#windows
// @source: fullsnes#snesppuwindow
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: adr-0013
// @source: adr-0034
#[test]
fn windows_cart_masks_layers_by_window_logic_inversion_and_designation() {
    let snes = drawn(include_bytes!("../../../tests/assets/carts/windows.sfc"));
    // Window 1 is pixels 8 to 23 and window 2 is 16 to 31 (edges inclusive). BG1 is masked
    // where exactly one is on (XOR): 8 to 15 and 24 to 31. BG2 is masked outside window 1,
    // so it shows only at 8 to 23. Both are behind BG1 where BG1 is not masked; the
    // backdrop shows where both are masked. BG3 has an inverted window 1 but is not in
    // $212E, so it is not masked at 40 to 63.
    //   0-7: BG1.   8-15: BG1 masked, BG2 shows.   16-23: BG1.
    //   24-31: both masked, the backdrop.   32-39: BG1.   40-63: BG3.
    let expected = runs(
        &[
            (RED, 8),
            (GREEN, 8),
            (RED, 8),
            (BLUE, 8),
            (RED, 8),
            (WHITE, 24),
        ],
        BLUE,
    );
    // Lines 1 to 7 are rows 1 to 7 of the tiles; line 8 on is the next tile row, empty.
    assert_picture(&snes, 256, &[(0..7, Some(&expected))], BLUE);
}

// @source: anomie-regs#2105
// @source: anomie-regs#2107
// @source: anomie-regs#210b
// @source: anomie-regs#212c
// @source: anomie-regs#backgrounds/mode-0
// @source: anomie-regs#backgrounds/tile-maps-and-character-maps
// @source: fullsnes#snesppubgcontrol
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: adr-0013
// @source: adr-0030
#[test]
fn bg_mode_0_cart_draws_four_2_bit_backgrounds_in_priority_order() {
    let snes = drawn(include_bytes!("../../../tests/assets/carts/bg-mode0.sfc"));
    // Pixels 0-1: only BG1 (red). 2-5: BG2's priority-1 tile (green) is in front of BG1's
    // priority-0 one, and of BG3's priority-1 tile at 4-5, since BG2 comes first at the
    // same priority. 6-7: BG3 (white). 8-15: BG4 (magenta). Then the backdrop (blue).
    let expected = runs(&[(RED, 2), (GREEN, 4), (WHITE, 2), (MAGENTA, 8)], BLUE);
    // Lines 1 to 7 are rows 1 to 7 of the tile, which repeat; line 8 on is the next tile
    // row, which the maps leave empty.
    assert_picture(&snes, 256, &[(0..7, Some(&expected))], BLUE);
}

// @source: anomie-regs#2105
// @source: anomie-regs#2107
// @source: anomie-regs#210b
// @source: anomie-regs#212c
// @source: anomie-regs#backgrounds/mode-1
// @source: anomie-regs#backgrounds/tile-maps-and-character-maps
// @source: fullsnes#snesppubgcontrol
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: adr-0013
// @source: adr-0030
#[test]
fn bg_mode_1_cart_puts_bg3_in_front_when_bit_3_is_set() {
    let snes = drawn(include_bytes!("../../../tests/assets/carts/bg-mode1.sfc"));
    // BG1's priority-1 tile covers pixels 0-3 and BG3's covers 2-5; with $2105 bit 3 set
    // BG3 is in front of BG1 at 2-3. BG2's priority-0 tile shows behind both at 6-7.
    let expected = runs(&[(RED, 2), (GREEN, 4), (BLUE, 2)], WHITE);
    assert_picture(&snes, 256, &[(0..7, Some(&expected))], WHITE);
}

// @source: anomie-regs#2105
// @source: anomie-regs#2107
// @source: anomie-regs#210b
// @source: anomie-regs#212c
// @source: anomie-regs#backgrounds/mode-2
// @source: anomie-regs#backgrounds/tile-maps-and-character-maps
// @source: fullsnes#snesppubgcontrol
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: adr-0013
// @source: adr-0030
#[test]
fn bg_mode_2_cart_draws_16_by_16_tiles_and_flips_them_whole() {
    let snes = drawn(include_bytes!("../../../tests/assets/carts/bg-mode2.sfc"));
    // The top half of a 16x16 tile is tiles 1 and 2 (red, green) and the bottom half tiles
    // $11 and $12 (blue, yellow). Flipped horizontally the two columns swap and, since the
    // tile is turned as a whole, so do the halves of the second one.
    let top = runs(
        &[
            (RED, 8),
            (GREEN, 8),
            (GREEN, 8),
            (RED, 8),
            (BLUE, 8),
            (YELLOW, 8),
        ],
        WHITE,
    );
    let bottom = runs(
        &[
            (BLUE, 8),
            (YELLOW, 8),
            (YELLOW, 8),
            (BLUE, 8),
            (RED, 8),
            (GREEN, 8),
        ],
        WHITE,
    );
    // Lines 1 to 7 are the top half; line 8 is the tile's row 8, the bottom half. Line 16
    // on is the second row of tiles, which the map leaves empty. BG3's entries are all
    // clear, so offset-per-tile changes nothing.
    assert_picture(
        &snes,
        256,
        &[(0..7, Some(&top)), (7..15, Some(&bottom))],
        WHITE,
    );
}

// @source: anomie-regs#2105
// @source: anomie-regs#2107
// @source: anomie-regs#210b
// @source: anomie-regs#210d
// @source: anomie-regs#212c
// @source: anomie-regs#backgrounds/mode-3
// @source: anomie-regs#backgrounds/tile-maps-and-character-maps
// @source: fullsnes#snesppubgcontrol
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: adr-0013
// @source: adr-0030
#[test]
fn bg_mode_3_cart_scrolls_a_256_colour_background_across_its_second_screen() {
    let snes = drawn(include_bytes!("../../../tests/assets/carts/bg-mode3.sfc"));
    // BG1HOFS = 252 puts map column 32 at pixel 4: BG1's priority-0 tile (red, entry $A5)
    // covers 4-11 and is in front of BG2's tile (green) at 4-7, which shows at 0-3.
    // Pixel 255 is map column 63, the last of the 512-pixel playfield, so it does not wrap
    // back to column 0 on this line.
    let expected = runs(&[(GREEN, 4), (RED, 8)], WHITE);
    assert_picture(&snes, 256, &[(0..7, Some(&expected))], WHITE);
}

// @source: anomie-regs#2105
// @source: anomie-regs#2107
// @source: anomie-regs#210b
// @source: anomie-regs#212c
// @source: anomie-regs#2130
// @source: anomie-regs#backgrounds/direct-color-mode
// @source: anomie-regs#backgrounds/mode-4
// @source: fullsnes#snesppubgcontrol
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: adr-0013
// @source: adr-0030
#[test]
fn bg_mode_4_cart_draws_bg1_in_direct_colour_and_bg2_from_the_palette() {
    const DIRECT: Rgb555 = Rgb555::from_channels(0b11010, 0b11000, 0b10100);
    let snes = drawn(include_bytes!("../../../tests/assets/carts/bg-mode4.sfc"));
    // Character $B6 with palette bits 101 is blue 10 1, green 110 0, red 110 1: $531A.
    assert_eq!(DIRECT.bits(), 0x531A);
    let expected = runs(&[(DIRECT, 8), (GREEN, 8)], WHITE);
    assert_picture(&snes, 256, &[(0..7, Some(&expected))], WHITE);
}

// @source: anomie-regs#2105
// @source: anomie-regs#2107
// @source: anomie-regs#2109
// @source: anomie-regs#210b
// @source: anomie-regs#212c
// @source: anomie-regs#backgrounds/mode-2
// @source: anomie-regs#backgrounds/tile-maps-and-character-maps
// @source: fullsnes#snesppuvideomemoryvram
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: adr-0013
// @source: adr-0031
#[test]
fn bg_opt_mode_2_cart_takes_each_columns_offsets_from_bg3() {
    let snes = drawn(include_bytes!(
        "../../../tests/assets/carts/bg-opt-mode2.sfc"
    ));
    // Tile 0 is as normal: column 0, palette 0 (red). Tile 1 takes BG3's first entries,
    // 16 right and 8 down: column 3 of the second row, palette 3 colour 2 (green). Tile 2's
    // entry is empty: column 2 (blue). Tile 3 is 1016 right, which wraps the 256-pixel
    // playfield to 16: column 2 again.
    // Tiles 4 to 31 take BG3's clear entries: no offset, and empty map columns.
    let expected = runs(&[(RED, 8), (GREEN, 8), (BLUE, 8), (BLUE, 8)], WHITE);
    // Lines 1 to 7: rows 1 to 7 of the tile, which repeat. Lines 8 to 15 show the map's
    // second row in palette colours the cart never writes, which power on random (ADR 0028),
    // so they are not checked. Line 16 on is empty.
    assert_picture(&snes, 256, &[(0..7, Some(&expected)), (7..15, None)], WHITE);
}

// @source: anomie-regs#2105
// @source: anomie-regs#2108
// @source: anomie-regs#2109
// @source: anomie-regs#210b
// @source: anomie-regs#212c
// @source: anomie-regs#backgrounds/mode-4
// @source: anomie-regs#backgrounds/tile-maps-and-character-maps
// @source: fullsnes#snesppuvideomemoryvram
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: adr-0013
// @source: adr-0031
#[test]
fn bg_opt_mode_4_cart_takes_one_horizontal_or_vertical_offset_per_column() {
    let snes = drawn(include_bytes!(
        "../../../tests/assets/carts/bg-opt-mode4.sfc"
    ));
    // Tile 0 is as normal (red). Tile 1: a horizontal 16, column 3 (green). Tile 2: a
    // vertical 8, whose bit 15 leaves the horizontal scroll alone: column 2 of the second
    // row (blue). Tile 3: a horizontal 8, column 4, which is empty (the backdrop).
    // Tiles 4 to 31 take BG3's clear entries, whose valid bit is clear: no offset.
    let expected = runs(&[(RED, 8), (GREEN, 8), (BLUE, 8)], WHITE);
    // Lines 8 to 15 show the map's second row in palette colours the cart never writes,
    // which power on random (ADR 0028), so they are not checked. Line 16 on is empty.
    assert_picture(&snes, 256, &[(0..7, Some(&expected)), (7..15, None)], WHITE);
}

// @source: anomie-regs#2106
// @source: anomie-regs#2125
// @source: anomie-regs#2130
// @source: anomie-regs#2131
// @source: anomie-regs#2132
// @source: anomie-regs#212d
// @source: anomie-regs#rendering-the-screen/color-math
// @source: anomie-regs#rendering-the-screen/mosaic
// @source: anomie-regs#windows
// @source: fullsnes#snesppucolormath
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: adr-0013
// @source: adr-0035
#[test]
fn color_math_cart_adds_the_sub_screen_or_fixed_colour_clips_in_the_window_and_pixelates() {
    let snes = drawn(include_bytes!("../../../tests/assets/carts/color-math.sfc"));
    // BG1 (20, 4, 0) is on the main screen, BG2 (3, 6, 9) on the sub screen at pixels 8 to
    // 23, and the fixed colour is (5, 0, 3). `$2131` adds, halves and takes BG1 only.
    //   0-7: the sub screen shows its backdrop, the fixed colour, so the sum is not halved
    //        (anomie-regs#2131): (25, 4, 3).
    //   8-23: BG2 is added and the sum halved, rounded down: (23, 10, 9) / 2 = (11, 5, 4).
    //   24-31: the colour window clips BG1 to black, which is not halved either: the fixed
    //        colour, (5, 0, 3).
    //   32-39: as 0 to 7.
    //   40-63: BG3, not designated for math, in 4x4 blocks of its leftmost pixel: 4 white,
    //        then 4 of the backdrop that BG3's clear pixels let show, three times.
    let expected = runs(
        &[
            (Rgb555::from_channels(25, 4, 3), 8),
            (Rgb555::from_channels(11, 5, 4), 16),
            (Rgb555::from_channels(5, 0, 3), 8),
            (Rgb555::from_channels(25, 4, 3), 8),
            (WHITE, 4),
            (GREY, 4),
            (WHITE, 4),
            (GREY, 4),
            (WHITE, 4),
            (GREY, 4),
        ],
        GREY,
    );
    // Below the tiles, the main screen is the backdrop, which takes no math ($2131 bit 5 is
    // clear), and the colour window still clips it to black at 24 to 31. BG3's blocks are 4
    // lines high from line 1 (ADR 0035, point 8), so line 8 is in the block that shows line
    // 5's row: BG3 shows there once more.
    let clipped = runs(&[(GREY, 24), (BLACK, 8)], GREY);
    let mut last_block = clipped.clone();
    for (index, pixel) in last_block[40..64].iter_mut().enumerate() {
        if index % 8 < 4 {
            *pixel = WHITE;
        }
    }
    assert_picture(
        &snes,
        256,
        &[
            (0..7, Some(&expected)),
            (7..8, Some(&last_block)),
            (8..PICTURE_ROWS, Some(&clipped)),
        ],
        GREY,
    );
}

// @source: anomie-regs#2105
// @source: anomie-regs#2107
// @source: anomie-regs#210b
// @source: anomie-regs#212c
// @source: anomie-regs#212d
// @source: anomie-regs#2130
// @source: anomie-regs#2131
// @source: anomie-regs#2132
// @source: anomie-regs#backgrounds/mode-5
// @source: anomie-regs#backgrounds/tile-maps-and-character-maps
// @source: anomie-regs#rendering-the-screen/color-math
// @source: fullsnes#snesppucolormath
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: fullsnes#snesppuresolution
// @source: adr-0013
// @source: adr-0036
#[test]
fn hires_cart_shows_mode_5_half_pixels_and_maths_the_sub_pixels_after_the_main_pixel() {
    let snes = drawn(include_bytes!("../../../tests/assets/carts/hires.sfc"));
    // BG1's tile 1 is "1.1.1.1." then ".2.2.2.2" across its 16 half-pixels, colour 1
    // (20, 4, 0) and colour 2 (3, 6, 9), on both screens; the sub screen shows the even
    // half-pixels and the main screen the odd, so pixel `2x` is the sub screen's pixel `x`
    // and `2x + 1` the main screen's (anomie-regs#backgrounds/mode-5). Math adds the fixed
    // colour (5, 0, 3) to BG1's main pixels, and to a sub pixel whose left main pixel took
    // it, never to the first (anomie-regs#rendering-the-screen/color-math, ADR 0036).
    //   x 0-3: the sub pixel is colour 1; the main pixel, an odd column of the left
    //          character, is clear and shows the backdrop, which takes no math.
    //   x 4: the sub pixel is clear and shows colour 0, not the fixed colour (fullsnes), and
    //          the pixel to its left is the backdrop, so no math; the main pixel is colour 2
    //          plus the fixed colour, (8, 6, 12).
    //   x 5-7: the same main pixel, and a sub pixel of colour 0 with the math of the one to
    //          its left: (15, 10, 13).
    //   x 8: a sub pixel after BG1's last main pixel takes the math too.
    let one = Rgb555::from_channels(20, 4, 0);
    let two_math = Rgb555::from_channels(8, 6, 12);
    let backdrop_math = Rgb555::from_channels(15, 10, 13);
    let mut expected = Vec::new();
    for _ in 0..4 {
        expected.extend([one, GREY]);
    }
    expected.extend([GREY, two_math]);
    for _ in 5..8 {
        expected.extend([backdrop_math, two_math]);
    }
    expected.extend([backdrop_math, GREY]);
    // The rest of the line is the backdrop on both screens, which takes no math. Line 8 on
    // is the next tile row, which the map leaves empty.
    expected.resize(512, GREY);
    assert_picture(&snes, 512, &[(0..7, Some(&expected))], GREY);
}

// @source: anomie-regs#2100
// @source: anomie-regs#2105
// @source: anomie-regs#210f
// @source: anomie-regs#2118
// @source: anomie-regs#backgrounds/mode-0
// @source: anomie-regs#backgrounds/mode-1
// @source: anomie-regs#dma-and-hdma/hdma
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: adr-0013
// @source: adr-0026
// @source: adr-0028
// @source: adr-0029
// @source: adr-0035
#[test]
fn ppu_hdma_cart_changes_mode_scroll_and_brightness_by_band_and_refuses_a_vram_write() {
    let snes = drawn(include_bytes!("../../../tests/assets/carts/ppu-hdma.sfc"));
    let grey = Rgb555::from_channels(16, 16, 16);
    let red = Rgb555::from_channels(24, 0, 0);
    let green = Rgb555::from_channels(0, 24, 0);
    // A run list repeated across the 256 pixels.
    let tiled = |list: &[(Rgb555, usize)]| {
        list.iter()
            .flat_map(|&(color, length)| core::iter::repeat_n(color, length))
            .cycle()
            .take(LINE_COLUMNS)
            .collect::<Vec<_>>()
    };
    // Every map entry is tile 1 and every row of a tile is alike, so a line is its tile row
    // across the screen (ADR 0030). The HDMA write on line 16n takes effect on line 16n + 1,
    // picture row 16n (anomie-regs#dma-and-hdma/hdma; ADR 0028, point 6).
    //   rows 0-15: mode 0, pixels 0-3 of each tile colour 1.
    //   rows 16-31: BG1HOFS = 2, so screen x shows playfield x + 2: colour 1 at x mod 8 of
    //        6, 7, 0 and 1. Pixels 254 and 255 wrap to playfield 0 and 1.
    //   rows 32-47: mode 1, the 4-bit tile 1, all colour 2.
    //   rows 48-63: forced blank, black (ADR 0035, point 2).
    //   rows 64-79: as rows 0 to 15.
    //   rows 80-223: brightness 7, each channel times 8 / 16 (ADR 0035, point 2).
    // The VRAM write to row 3 of the 2-bit tile, made outside vertical and forced blank, was
    // refused (ADR 0029, point 7): row 3 of each tile is like the others.
    let plain = tiled(&[(red, 4), (grey, 4)]);
    let scrolled = tiled(&[(red, 2), (grey, 4), (red, 2)]);
    let mode_1 = vec![green; LINE_COLUMNS];
    let blank = vec![BLACK; LINE_COLUMNS];
    let dim = tiled(&[
        (Rgb555::from_channels(12, 0, 0), 4),
        (Rgb555::from_channels(8, 8, 8), 4),
    ]);
    assert_picture(
        &snes,
        256,
        &[
            (0..16, Some(&plain)),
            (16..32, Some(&scrolled)),
            (32..48, Some(&mode_1)),
            (48..64, Some(&blank)),
            (64..80, Some(&plain)),
            (80..PICTURE_ROWS, Some(&dim)),
        ],
        BLACK,
    );
}

// @source: anomie-regs#213e
// @source: fullsnes#snespputimersandstatus
// @source: adr-0013
// @source: adr-0033
// @source: adr-0037
#[test]
fn render_skip_keeps_the_sprite_flags_and_presents_no_frame_until_it_is_turned_off() {
    let rom = include_bytes!("../../../tests/assets/carts/obj-sprites.sfc");
    let mut snes = Snes::<Fast>::builder(Arc::from(rom.as_slice()))
        .build()
        .expect("builds");
    snes.set_render_skip(true);
    assert!(snes.render_skip());
    snes.run(Stop::Frames(4));
    assert_eq!(snes.cpu().run, RunState::Stopped);
    // Nothing was drawn, so nothing was presented; the flags are those a drawn frame leaves:
    // range over set (the cart's 33 sprites on one line), time over clear.
    assert_eq!(snes.video().pixels.len(), 0);
    assert_eq!(snes.peek_bus(0x00_213E).value & 0xC0, 0x40);
    // A clone keeps the setting. Turning it off, the next whole frame is the picture the
    // cart draws.
    let mut copy = snes.clone();
    assert!(copy.render_skip());
    snes.set_render_skip(false);
    snes.run(Stop::Frames(2));
    let reference = drawn(rom);
    assert_eq!(snes.video().pixels, reference.video().pixels);
    assert_eq!(snes.video().pixels.len(), 256 * 224);
    // Skipped still, the copy shows nothing new.
    copy.run(Stop::Frames(2));
    assert_eq!(copy.video().pixels.len(), 0);
}

// @source: anomie-regs#212c
// @source: anomie-regs#sprites/sprite-priority
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: adr-0013
// @source: adr-0033
// @source: adr-0037
#[test]
fn layer_output_holds_the_sprites_before_compositing_and_changes_no_frame() {
    let rom = include_bytes!("../../../tests/assets/carts/obj-sprites.sfc");
    let mut snes = Snes::<Fast>::builder(Arc::from(rom.as_slice()))
        .build()
        .expect("builds");
    assert!(!snes.layer_output());
    snes.set_layer_output(true);
    snes.run(Stop::Frames(4));
    // One entry per output line. Sprite 0 (red, X 8) is in front of sprite 1 (blue, X 12)
    // on line 1, so blue shows at 16 to 19, as in the picture; no background is on a screen.
    let layers = snes.layers();
    assert_eq!(layers.len(), 224);
    let sprites = layers[0].layer(LayerKind::Sprites);
    assert_eq!(sprites[7], None);
    assert!(sprites[8..16].iter().all(|&pixel| pixel == Some(RED)));
    assert!(sprites[16..20].iter().all(|&pixel| pixel == Some(BLUE)));
    assert_eq!(sprites[20], None);
    for kind in [
        LayerKind::Background1,
        LayerKind::Background2,
        LayerKind::Background3,
        LayerKind::Background4,
    ] {
        assert!(layers[0].layer(kind).iter().all(Option::is_none));
    }
    // The picture is the one the cart draws with layer output off, and a machine that never
    // asked for layers has none.
    let reference = drawn(rom);
    assert_eq!(snes.video().pixels, reference.video().pixels);
    assert_eq!(reference.layers().len(), 0);
}
