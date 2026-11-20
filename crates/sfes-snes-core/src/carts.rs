// @source: adr-0013
// @source: adr-0019
//! Runs the project's own test cartridges (ADR 0013) and checks what they leave against
//! values computed from the documents.

use alloc::{sync::Arc, vec::Vec};

use sfes_api::{Cycles, Fast, InstrEvent, Observer, RegionId, Stop};
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
