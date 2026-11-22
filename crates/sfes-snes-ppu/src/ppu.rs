// @source: anomie-regs#registers
// @source: anomie-timing#s-ppu/rendering
// @source: fullsnes#snesunpredictablethings
// @source: adr-0028
//! The PPU's state: its memories, the registers rendering needs so far, PPU1 and PPU2 open
//! bus, the counter latch, and which line it renders next (ADR 0028).

use alloc::{boxed::Box, vec};

use sfes_api::{Rgb555, SplitMix64};

use crate::{
    counters::{CounterLatch, CounterRegister},
    line::{LineOut, LineWidth},
    register::{ReadKind, Register},
};

/// VRAM: 64 KiB.
pub const VRAM_SIZE: usize = 0x1_0000;
/// CGRAM: 256 colours of 2 bytes.
pub const CGRAM_SIZE: usize = 0x200;
/// OAM: 512 bytes of sprite entries and a 32-byte high table.
pub const OAM_SIZE: usize = 0x220;

/// `$2105` at power-on: fullsnes's "xFh", with the unknown high nibble taken as 0
/// (ADR 0028, point 5).
const BACKGROUND_MODE_POWER_ON: u8 = 0x0F;
/// `$2105` bits 2–0: the BG mode.
const BACKGROUND_MODE_MASK: u8 = 0x07;
/// `$2133` bit 3: pseudo-hires.
const PSEUDO_HIRES: u8 = 0x08;
/// `$2133` bit 0: interlace.
const INTERLACE: u8 = 0x01;
/// CGRAM bit 15 is not stored; a colour has 15 bits.
const COLOR_MASK: u16 = 0x7FFF;
/// The first line a frame outputs (anomie-timing#s-ppu/rendering).
const FIRST_OUTPUT_LINE: u16 = 1;

/// The PPU: PPU1 and PPU2 as one unit, as the bus reaches them.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Ppu {
    /// Video RAM, 64 KiB.
    vram: Box<[u8]>,
    /// Palette RAM, 512 bytes.
    cgram: Box<[u8]>,
    /// Sprite attribute RAM, 544 bytes.
    oam: Box<[u8]>,
    /// $2105 BGMODE: the BG mode and character sizes.
    background_mode: u8,
    /// $2133 SETINI: external sync, EXTBG, pseudo-hires, overscan and interlace.
    screen_mode: u8,
    /// The last value read from `$2134–$2136`, `$2138–$213A` or `$213E`.
    ppu1_open_bus: u8,
    /// The last value read from `$213B–$213D` or `$213F`.
    ppu2_open_bus: u8,
    /// $213C OPHCT and $213D OPVCT: the latched counters.
    counters: CounterLatch,
    /// The next line of the frame to render.
    next_line: u16,
}

impl Ppu {
    /// The PPU at power-on (ADR 0028, points 1, 4 and 5):
    ///
    /// - VRAM, CGRAM and OAM are filled from `random`, in that order, each from its first
    ///   byte up;
    /// - `$2105` is `$0F` and `$2133` is 0;
    /// - both open-bus latches are 0, and the counters are `$1FF` (ADR 0021, point 8).
    // @source: fullsnes#snesiomap
    // @source: vigna-splitmix
    // @source: adr-0021
    // @source: adr-0028
    #[must_use]
    pub fn power_on(random: &mut SplitMix64) -> Self {
        let mut vram = vec![0; VRAM_SIZE].into_boxed_slice();
        let mut cgram = vec![0; CGRAM_SIZE].into_boxed_slice();
        let mut oam = vec![0; OAM_SIZE].into_boxed_slice();
        random.fill(&mut vram);
        random.fill(&mut cgram);
        random.fill(&mut oam);
        Self {
            vram,
            cgram,
            oam,
            background_mode: BACKGROUND_MODE_POWER_ON,
            screen_mode: 0,
            ppu1_open_bus: 0,
            ppu2_open_bus: 0,
            counters: CounterLatch::power_on(),
            next_line: FIRST_OUTPUT_LINE,
        }
    }

    /// The reset button: `$2133` is 0 and the frame starts again from line 1. The memories,
    /// `$2105`, the open-bus latches and the counters are kept (ADR 0028, points 1, 4 and
    /// 5; ADR 0021, point 8).
    // @source: fullsnes#snesiomap
    // @source: adr-0021
    // @source: adr-0028
    pub const fn reset(&mut self) {
        self.screen_mode = 0;
        self.next_line = FIRST_OUTPUT_LINE;
    }

    /// What a read of `register` returns without its side effects; `None` where CPU open
    /// bus answers (ADR 0028, point 3).
    // @source: anomie-regs#registers
    // @source: anomie-regs#213c
    // @source: anomie-regs#213d
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0028
    #[must_use]
    pub const fn peek(&self, register: Register) -> Option<u8> {
        match register.read_kind() {
            ReadKind::Ppu1OpenBus => Some(self.ppu1_open_bus),
            ReadKind::Ppu2 => match counter(register) {
                Some(counter) => Some(self.counters.peek(counter, self.ppu2_open_bus)),
                None => None,
            },
            ReadKind::CpuOpenBus | ReadKind::Unimplemented => None,
        }
    }

    /// Whether a read of `register` changes the PPU, so that a peek is not a read.
    // @source: anomie-regs#213c
    // @source: adr-0028
    #[must_use]
    pub const fn read_has_side_effects(register: Register) -> bool {
        matches!(register.read_kind(), ReadKind::Ppu2)
    }

    /// The side effects of a read of `register`, after [`Ppu::peek`] gave its value: a
    /// counter's flip-flop flips, and the byte read becomes PPU2 open bus (ADR 0028,
    /// point 4).
    // @source: anomie-regs#213c
    // @source: anomie-regs#213d
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0021
    // @source: adr-0028
    pub const fn after_read(&mut self, register: Register) {
        if let Some(counter) = counter(register) {
            self.ppu2_open_bus = self.counters.peek(counter, self.ppu2_open_bus);
            self.counters.flip(counter);
        }
    }

    /// A CPU write of `value` to `register`. Until each register's issue, only `$2105` and
    /// `$2133` are kept (ADR 0028, point 5).
    // @source: anomie-regs#2105
    // @source: anomie-regs#2133
    // @source: adr-0028
    pub const fn write(&mut self, register: Register, value: u8) {
        match register {
            Register::BackgroundMode => self.background_mode = value,
            Register::ScreenMode => self.screen_mode = value,
            _ => {}
        }
    }

    /// Latches `line` and `dot` into `$213C/$213D` (ADR 0021, point 7).
    // @source: anomie-regs#2137
    // @source: anomie-regs#213c
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0021
    pub const fn latch_counters(&mut self, line: u16, dot: u16) {
        self.counters.latch(line, dot);
    }

    /// The next line of the frame to render: 1 at the start of a frame.
    // @source: anomie-timing#s-ppu/rendering
    // @source: adr-0028
    #[must_use]
    pub const fn next_line(&self) -> u16 {
        self.next_line
    }

    /// Starts a new frame: the next line to render is line 1 (ADR 0028, point 6).
    // @source: anomie-timing#s-ppu/rendering
    // @source: adr-0028
    pub const fn start_frame(&mut self) {
        self.next_line = FIRST_OUTPUT_LINE;
    }

    /// Renders `line` into `out` from the registers and memories as they stand, and makes
    /// the next line the one after it (technical design section 5; ADR 0028, points 6 and
    /// 7):
    ///
    /// - the line is 512 pixels in BG mode 5 or 6 or with pseudo-hires, else 256;
    /// - until the layers arrive, every pixel, the sub screen's included, is the backdrop,
    ///   CGRAM entry 0.
    // @source: anomie-regs#2105
    // @source: anomie-regs#2133
    // @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
    // @source: fullsnes#snesppuresolution
    // @source: adr-0028
    pub fn render_line(&mut self, line: u16, out: &mut LineOut) {
        let mode = self.background_mode & BACKGROUND_MODE_MASK;
        // BG modes 5 and 6 are true hi-res.
        let width = if self.screen_mode & PSEUDO_HIRES != 0 || matches!(mode, 5 | 6) {
            LineWidth::Wide
        } else {
            LineWidth::Normal
        };
        let interlaced = self.screen_mode & INTERLACE != 0;
        out.fill(line, width, interlaced, self.color(0));
        self.next_line = line + 1;
    }

    /// CGRAM entry `index` as a 15-bit colour, little-endian.
    // @source: anomie-regs#213b
    // @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
    fn color(&self, index: u8) -> Rgb555 {
        let offset = usize::from(index) * 2;
        let low = self.cgram.get(offset).copied().unwrap_or_default();
        let high = self.cgram.get(offset + 1).copied().unwrap_or_default();
        // Masked to 15 bits, so it is always a colour.
        Rgb555::new(u16::from_le_bytes([low, high]) & COLOR_MASK).unwrap_or_default()
    }

    /// VRAM, 64 KiB.
    // @source: adr-0028
    #[must_use]
    pub fn vram(&self) -> &[u8] {
        &self.vram
    }

    /// VRAM, for writes between steps (FND-06).
    // @source: adr-0028
    pub fn vram_mut(&mut self) -> &mut [u8] {
        &mut self.vram
    }

    /// CGRAM, 512 bytes.
    // @source: adr-0028
    #[must_use]
    pub fn cgram(&self) -> &[u8] {
        &self.cgram
    }

    /// CGRAM, for writes between steps (FND-06).
    // @source: adr-0028
    pub fn cgram_mut(&mut self) -> &mut [u8] {
        &mut self.cgram
    }

    /// OAM, 544 bytes.
    // @source: adr-0028
    #[must_use]
    pub fn oam(&self) -> &[u8] {
        &self.oam
    }

    /// OAM, for writes between steps (FND-06).
    // @source: adr-0028
    pub fn oam_mut(&mut self) -> &mut [u8] {
        &mut self.oam
    }
}

/// The counter a register reads, if it is `$213C` or `$213D`.
// @source: anomie-regs#213c
// @source: anomie-regs#213d
const fn counter(register: Register) -> Option<CounterRegister> {
    match register {
        Register::HorizontalCounter => Some(CounterRegister::Horizontal),
        Register::VerticalCounter => Some(CounterRegister::Vertical),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: adr-0028
    fn ppu() -> Ppu {
        Ppu::power_on(&mut SplitMix64::new(0))
    }

    // @source: vigna-splitmix
    // @source: adr-0028
    #[test]
    fn power_on_fills_vram_cgram_and_oam_in_order_from_the_stream() {
        let mut random = SplitMix64::new(7);
        let ppu = Ppu::power_on(&mut random);
        let mut expected = SplitMix64::new(7);
        let mut bytes = vec![0; VRAM_SIZE + CGRAM_SIZE + OAM_SIZE];
        expected.fill(&mut bytes);
        let (vram, rest) = bytes.split_at(VRAM_SIZE);
        let (cgram, oam) = rest.split_at(CGRAM_SIZE);
        assert_eq!(ppu.vram(), vram);
        assert_eq!(ppu.cgram(), cgram);
        assert_eq!(ppu.oam(), oam);
        // The generator moves on past the PPU's bytes.
        assert_eq!(random, expected);
    }

    // @source: anomie-regs#registers
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0028
    #[test]
    fn write_only_registers_read_ppu1_or_cpu_open_bus() {
        let mut ppu = ppu();
        ppu.ppu1_open_bus = 0x5A;
        assert_eq!(ppu.peek(Register::OamData), Some(0x5A));
        assert_eq!(ppu.peek(Register::WindowLogicBackgrounds), Some(0x5A));
        assert_eq!(ppu.peek(Register::DisplayControl), None);
        assert_eq!(ppu.peek(Register::SoftwareLatch), None);
        // Until #90 the readable data and status registers read as CPU open bus.
        assert_eq!(ppu.peek(Register::Ppu2Status), None);
        assert!(!Ppu::read_has_side_effects(Register::OamData));
        assert!(Ppu::read_has_side_effects(Register::VerticalCounter));
    }

    // @source: anomie-regs#213c
    // @source: anomie-regs#213d
    // @source: fullsnes#snesiomap
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0028
    #[test]
    fn each_counter_reads_low_then_high_with_ppu2_open_bus() {
        let mut ppu = ppu();
        let read = |ppu: &mut Ppu, register| {
            let value = ppu.peek(register);
            ppu.after_read(register);
            value
        };
        assert_eq!(read(&mut ppu, Register::HorizontalCounter), Some(0xFF));
        // Bit 0 is bit 8 of $1FF; bits 7-1 are the last PPU2 value read, $FF.
        assert_eq!(read(&mut ppu, Register::HorizontalCounter), Some(0xFF));
        ppu.latch_counters(0x105, 0x0A3);
        assert!(ppu.counters.latched());
        assert_eq!(read(&mut ppu, Register::VerticalCounter), Some(0x05));
        // The last value read was $05: bits 7-1 are $04, bit 0 is bit 8 of $105.
        assert_eq!(read(&mut ppu, Register::VerticalCounter), Some(0x05));
        // OPHCT's flip-flop was left on the low byte; a latch does not reset it.
        assert_eq!(read(&mut ppu, Register::HorizontalCounter), Some(0xA3));
        assert_eq!(ppu.peek(Register::HorizontalCounter), Some(0xA2));
        assert_eq!(read(&mut ppu, Register::HorizontalCounter), Some(0xA2));
        // A PPU1 open-bus read does not change PPU2 open bus.
        assert_eq!(read(&mut ppu, Register::OamData), Some(0));
        assert_eq!(ppu.ppu2_open_bus, 0xA2);
    }

    // @source: anomie-regs#2105
    // @source: anomie-regs#2133
    // @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
    // @source: fullsnes#snesppuresolution
    // @source: adr-0028
    #[test]
    fn a_line_is_the_backdrop_and_wide_in_modes_5_and_6_or_pseudo_hires() {
        let mut ppu = ppu();
        ppu.cgram_mut()[..2].copy_from_slice(&[0x1F, 0xFC]);
        let mut out = LineOut::default();
        ppu.write(Register::BackgroundMode, 0x01);
        ppu.render_line(1, &mut out);
        assert_eq!(out.line(), 1);
        assert_eq!(out.width(), LineWidth::Normal);
        assert!(!out.interlaced());
        // Bit 15 is not part of the colour.
        assert_eq!(out.pixels(), [Rgb555::new(0x7C1F).unwrap(); 256]);
        assert_eq!(ppu.next_line(), 2);
        for (mode, screen_mode, width) in [
            (5, 0, LineWidth::Wide),
            (6, 0, LineWidth::Wide),
            (7, 0, LineWidth::Normal),
            (1, 0x08, LineWidth::Wide),
        ] {
            ppu.write(Register::BackgroundMode, mode);
            ppu.write(Register::ScreenMode, screen_mode);
            ppu.render_line(2, &mut out);
            assert_eq!(
                out.width(),
                width,
                "mode {mode}, $2133 = {screen_mode:#04X}"
            );
            assert_eq!(out.pixels().len(), width.pixels());
        }
        ppu.write(Register::ScreenMode, 0x01);
        ppu.render_line(3, &mut out);
        assert!(out.interlaced());
    }

    // @source: fullsnes#snesiomap
    // @source: adr-0028
    #[test]
    fn reset_clears_setini_and_keeps_the_rest() {
        let mut ppu = ppu();
        ppu.write(Register::BackgroundMode, 0x05);
        ppu.write(Register::ScreenMode, 0x0D);
        ppu.ppu1_open_bus = 0x11;
        ppu.render_line(1, &mut LineOut::default());
        let before = ppu.clone();
        ppu.reset();
        assert_eq!(ppu.screen_mode, 0);
        assert_eq!(ppu.next_line(), 1);
        assert_eq!(ppu.background_mode, 0x05);
        assert_eq!(ppu.ppu1_open_bus, 0x11);
        assert_eq!(ppu.vram(), before.vram());
        assert_eq!(Ppu::power_on(&mut SplitMix64::new(0)).background_mode, 0x0F);
    }
}
