// @source: anomie-regs#registers
// @source: anomie-timing#s-ppu/rendering
// @source: fullsnes#snesunpredictablethings
// @source: adr-0028
// @source: adr-0029
//! The PPU's state: its memories and their ports, the registers rendering needs so far,
//! the multiplier, PPU1 and PPU2 open bus, the counter latch, and which line it renders
//! next (ADR 0028; ADR 0029).

use alloc::{boxed::Box, vec};

use sfes_api::{Rgb555, SplitMix64};

use crate::{
    counters::{CounterLatch, CounterRegister},
    line::{LineOut, LineWidth},
    memory::{CgramPort, Half, OamPort, VramPort},
    mode7::Mode7,
    register::{ReadKind, Register},
    signals::{Period, Signals},
};

/// VRAM: 64 KiB.
pub const VRAM_SIZE: usize = 0x1_0000;
/// CGRAM: 256 colours of 2 bytes.
pub const CGRAM_SIZE: usize = 0x200;
/// OAM: 512 bytes of sprite entries and a 32-byte high table.
pub const OAM_SIZE: usize = 0x220;

/// `$2100` at power-on and reset: fullsnes's "8xh", with the unknown brightness taken as 0
/// (ADR 0029, point 2).
const DISPLAY_CONTROL_RESET: u8 = 0x80;
/// `$2100` bit 7: forced blank.
const FORCED_BLANK: u8 = 0x80;
/// `$213E` bit 4: PPU1 open bus.
const STAT77_OPEN_BUS: u8 = 0x10;
/// `$213E` bits 3–0: the 5C77's version (ADR 0029, point 9).
const PPU1_VERSION: u8 = 1;
/// `$213F` bit 6: the counters were latched.
const STAT78_LATCHED: u8 = 0x40;
/// `$213F` bit 5: PPU2 open bus.
const STAT78_OPEN_BUS: u8 = 0x20;
/// `$213F` bit 4: a PAL console.
const STAT78_PAL: u8 = 0x10;
/// `$213F` bits 3–0: the 5C78's version (ADR 0029, point 10).
const PPU2_VERSION: u8 = 3;
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
    /// $2100 INIDISP: forced blank and screen brightness.
    display_control: u8,
    /// $2102–$2104 and $2138: the OAM port.
    oam_port: OamPort,
    /// $2115–$2119, $2139 and $213A: the VRAM port.
    vram_port: VramPort,
    /// $2121, $2122 and $213B: the CGRAM port.
    cgram_port: CgramPort,
    /// $211B–$2120's shared latch, M7A and M7B: the multiplier.
    mode7: Mode7,
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
    /// What the machine passed in before the latest access (ADR 0029, point 1).
    signals: Signals,
}

impl Ppu {
    /// The PPU at power-on (ADR 0028, points 1, 4 and 5; ADR 0029, points 2, 8 and 12):
    ///
    /// - VRAM, CGRAM and OAM are filled from `random`, in that order, each from its first
    ///   byte up;
    /// - `$2100` is `$80`, `$2105` is `$0F`, `$2115` is `$0F` and `$2133` is 0;
    /// - M7A and M7B are `$FFFF`, and the ports' addresses and latches 0;
    /// - both open-bus latches are 0, and the counters are `$1FF` (ADR 0021, point 8).
    // @source: fullsnes#snesiomap
    // @source: vigna-splitmix
    // @source: adr-0021
    // @source: adr-0028
    // @source: adr-0029
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
            display_control: DISPLAY_CONTROL_RESET,
            oam_port: OamPort::power_on(),
            vram_port: VramPort::power_on(),
            cgram_port: CgramPort::power_on(),
            mode7: Mode7::power_on(),
            background_mode: BACKGROUND_MODE_POWER_ON,
            screen_mode: 0,
            ppu1_open_bus: 0,
            ppu2_open_bus: 0,
            counters: CounterLatch::power_on(),
            next_line: FIRST_OUTPUT_LINE,
            signals: Signals::POWER_ON,
        }
    }

    /// The reset button: `$2100` is `$80`, `$2133` is 0 and the frame starts again from
    /// line 1. The memories and their ports, `$2105`, M7A and M7B, the open-bus latches and
    /// the counters are kept (ADR 0028, points 1, 4 and 5; ADR 0029, points 2, 8 and 12;
    /// ADR 0021, point 8).
    // @source: fullsnes#snesiomap
    // @source: adr-0021
    // @source: adr-0028
    // @source: adr-0029
    pub const fn reset(&mut self) {
        self.display_control = DISPLAY_CONTROL_RESET;
        self.screen_mode = 0;
        self.next_line = FIRST_OUTPUT_LINE;
    }

    /// Takes the machine's signals for the access about to happen (ADR 0029, point 1).
    // @source: adr-0029
    pub const fn set_signals(&mut self, signals: Signals) {
        self.signals = signals;
    }

    /// Vertical blank has begun: the OAM address is reloaded unless forced blank is on
    /// (ADR 0029, point 6).
    // @source: anomie-regs#2102
    // @source: anomie-timing#oam-reset
    // @source: fullsnes#snesmemoryoamaccessspriteattributes
    // @source: adr-0029
    pub const fn start_vertical_blank(&mut self) {
        if !self.forced_blank() {
            self.oam_port.reload();
        }
    }

    /// Whether `$2100` has forced blank on.
    // @source: anomie-regs#2100
    const fn forced_blank(&self) -> bool {
        self.display_control & FORCED_BLANK != 0
    }

    /// What a read of `register` returns without its side effects, with the signals of the
    /// latest access; `None` where CPU open bus answers (ADR 0028, point 3).
    // @source: anomie-regs#registers
    // @source: adr-0028
    // @source: adr-0029
    #[must_use]
    pub fn peek(&self, register: Register) -> Option<u8> {
        self.peek_with(register, self.signals)
    }

    /// What a read of `register` returns without its side effects, with `signals`; `None`
    /// where CPU open bus answers (ADR 0028, point 3; ADR 0029, points 3 to 5 and 8 to 11).
    // @source: anomie-regs#registers
    // @source: anomie-regs#2134
    // @source: anomie-regs#2138
    // @source: anomie-regs#2139
    // @source: anomie-regs#213a
    // @source: anomie-regs#213b
    // @source: anomie-regs#213c
    // @source: anomie-regs#213d
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0028
    // @source: adr-0029
    #[must_use]
    pub fn peek_with(&self, register: Register, signals: Signals) -> Option<u8> {
        let [low, middle, high] = self.mode7.product();
        match register {
            Register::MultiplyLow => Some(low),
            Register::MultiplyMiddle => Some(middle),
            Register::MultiplyHigh => Some(high),
            Register::OamDataRead => Some(self.oam_port.peek(&self.oam)),
            Register::VramDataReadLow => Some(self.vram_port.peek(Half::Low)),
            Register::VramDataReadHigh => Some(self.vram_port.peek(Half::High)),
            Register::CgramDataRead => Some(self.cgram_port.peek(&self.cgram, self.ppu2_open_bus)),
            Register::Ppu1Status => Some(self.ppu1_status()),
            Register::Ppu2Status => Some(self.ppu2_status(signals)),
            _ => match (register.read_kind(), counter(register)) {
                (ReadKind::Ppu1OpenBus, _) => Some(self.ppu1_open_bus),
                (_, Some(counter)) => Some(self.counters.peek(counter, self.ppu2_open_bus)),
                _ => None,
            },
        }
    }

    /// $213E STAT77: the sprite overflow flags, 0 until #94, the master/slave bit 0, PPU1
    /// open bus in bit 4 and the version (ADR 0029, point 9).
    // @source: anomie-regs#213e
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0029
    const fn ppu1_status(&self) -> u8 {
        (self.ppu1_open_bus & STAT77_OPEN_BUS) | PPU1_VERSION
    }

    /// $213F STAT78: the field, the latch flag, PPU2 open bus in bit 5, the PAL bit and the
    /// version (ADR 0029, point 10).
    // @source: anomie-regs#213f
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0029
    const fn ppu2_status(&self, signals: Signals) -> u8 {
        let field = (signals.field as u8) << 7;
        let latched = if self.counters.latched() {
            STAT78_LATCHED
        } else {
            0
        };
        let pal = if signals.pal { STAT78_PAL } else { 0 };
        field | latched | (self.ppu2_open_bus & STAT78_OPEN_BUS) | pal | PPU2_VERSION
    }

    /// Whether a read of `register` changes the PPU, so that a peek is not a read: every
    /// read the PPU answers itself sets an open-bus latch (ADR 0029, point 11).
    // @source: anomie-regs#registers
    // @source: adr-0028
    // @source: adr-0029
    #[must_use]
    pub const fn read_has_side_effects(register: Register) -> bool {
        matches!(register.read_kind(), ReadKind::Ppu1 | ReadKind::Ppu2)
    }

    /// The side effects of a read of `register`, after [`Ppu::peek`] gave its value (ADR
    /// 0028, point 4; ADR 0029, points 3 to 5, 10 and 11):
    ///
    /// - the byte read becomes PPU1 or PPU2 open bus;
    /// - a memory port's address moves on, and VRAM's prefetch may load;
    /// - a counter's flip-flop flips;
    /// - `$213F` resets both flip-flops, and clears the latch flag while `$4201` bit 7 is
    ///   set.
    // @source: anomie-regs#2138
    // @source: anomie-regs#2139
    // @source: anomie-regs#213a
    // @source: anomie-regs#213b
    // @source: anomie-regs#213c
    // @source: anomie-regs#213d
    // @source: anomie-regs#213f
    // @source: fullsnes#snespputimersandstatus
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0021
    // @source: adr-0028
    // @source: adr-0029
    pub fn after_read(&mut self, register: Register) {
        let Some(value) = self.peek(register) else {
            return;
        };
        match register.read_kind() {
            ReadKind::Ppu1 => self.ppu1_open_bus = value,
            ReadKind::Ppu2 => self.ppu2_open_bus = value,
            ReadKind::Ppu1OpenBus | ReadKind::CpuOpenBus => {}
        }
        match register {
            Register::OamDataRead => self.oam_port.step(),
            Register::VramDataReadLow => self.vram_port.after_read(&self.vram, Half::Low),
            Register::VramDataReadHigh => self.vram_port.after_read(&self.vram, Half::High),
            Register::CgramDataRead => self.cgram_port.step(),
            Register::Ppu2Status => self.counters.read_status(self.signals.external_latch),
            _ => {
                if let Some(counter) = counter(register) {
                    self.counters.flip(counter);
                }
            }
        }
    }

    /// A CPU write of `value` to `register` (ADR 0028, point 5; ADR 0029, points 2 to 8):
    ///
    /// - `$2100`, `$2105` and `$2133` are kept, and clearing forced blank reloads the OAM
    ///   address;
    /// - the memory ports run, writing their memory only in the periods point 7 allows;
    /// - M7A and M7B are kept, and the registers that share their latch move it.
    ///
    /// A write to any other register has no effect until its issue.
    // @source: anomie-regs#2100
    // @source: anomie-regs#2102
    // @source: anomie-regs#2103
    // @source: anomie-regs#2104
    // @source: anomie-regs#2105
    // @source: anomie-regs#211b
    // @source: anomie-regs#2115
    // @source: anomie-regs#2116
    // @source: anomie-regs#2117
    // @source: anomie-regs#2118
    // @source: anomie-regs#2119
    // @source: anomie-regs#2121
    // @source: anomie-regs#2122
    // @source: anomie-regs#2133
    // @source: anomie-regs#sprites/oam
    // @source: anomie-timing#oam-reset
    // @source: fullsnes#snesppucontrol
    // @source: adr-0028
    // @source: adr-0029
    pub fn write(&mut self, register: Register, value: u8) {
        let blank = self.forced_blank() || self.signals.period == Period::VerticalBlank;
        match register {
            Register::DisplayControl => {
                if self.forced_blank() && value & FORCED_BLANK == 0 {
                    self.oam_port.reload();
                }
                self.display_control = value;
            }
            Register::OamAddressLow => self.oam_port.write_address_low(value),
            Register::OamAddressHigh => self.oam_port.write_address_high(value),
            Register::OamData => self.oam_port.write(&mut self.oam, value, blank),
            Register::BackgroundMode => self.background_mode = value,
            Register::Background1HorizontalScroll
            | Register::Background1VerticalScroll
            | Register::Mode7MatrixC
            | Register::Mode7MatrixD
            | Register::Mode7CenterX
            | Register::Mode7CenterY => {
                self.mode7.write(value);
            }
            Register::Mode7MatrixA => self.mode7.write_matrix_a(value),
            Register::Mode7MatrixB => self.mode7.write_matrix_b(value),
            Register::VramControl => self.vram_port.write_control(value),
            Register::VramAddressLow => self.vram_port.write_address(&self.vram, Half::Low, value),
            Register::VramAddressHigh => {
                self.vram_port.write_address(&self.vram, Half::High, value);
            }
            Register::VramDataLow => {
                self.vram_port
                    .write_data(&mut self.vram, Half::Low, value, blank);
            }
            Register::VramDataHigh => {
                self.vram_port
                    .write_data(&mut self.vram, Half::High, value, blank);
            }
            Register::CgramAddress => self.cgram_port.write_address(value),
            Register::CgramData => {
                let allowed = blank || self.signals.period == Period::HorizontalBlank;
                self.cgram_port.write(&mut self.cgram, value, allowed);
            }
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
    use sfes_api::Field;

    use super::*;

    // @source: adr-0028
    fn ppu() -> Ppu {
        Ppu::power_on(&mut SplitMix64::new(0))
    }

    /// Signals in `period`, otherwise as at power-on.
    // @source: adr-0029
    const fn signals(period: Period) -> Signals {
        Signals {
            period,
            ..Signals::POWER_ON
        }
    }

    /// A read of `register`: its value, then its side effects.
    // @source: adr-0028
    fn read(ppu: &mut Ppu, register: Register) -> Option<u8> {
        let value = ppu.peek(register);
        ppu.after_read(register);
        value
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
        assert!(!Ppu::read_has_side_effects(Register::OamData));
        assert!(Ppu::read_has_side_effects(Register::MultiplyLow));
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

    // @source: anomie-regs#2100
    // @source: anomie-regs#2104
    // @source: anomie-regs#2118
    // @source: anomie-regs#2122
    // @source: anomie-regs#registers
    // @source: fullsnes#snesppucontrol
    // @source: adr-0029
    #[test]
    fn memory_writes_need_blanking() {
        let mut ppu = ppu();
        let write_all = |ppu: &mut Ppu, value: u8| {
            ppu.write(Register::OamAddressLow, 0x00);
            ppu.write(Register::OamData, value);
            ppu.write(Register::OamData, value);
            ppu.write(Register::VramControl, 0x80);
            ppu.write(Register::VramAddressLow, 0x00);
            ppu.write(Register::VramAddressHigh, 0x00);
            ppu.write(Register::VramDataLow, value);
            ppu.write(Register::VramDataHigh, value);
            ppu.write(Register::CgramAddress, 0x00);
            ppu.write(Register::CgramData, value);
            ppu.write(Register::CgramData, value & 0x7F);
        };
        let memories = |ppu: &Ppu| (ppu.oam()[0], ppu.vram()[1], ppu.cgram()[1]);
        // Power-on is forced blank: every write lands, whatever the period.
        write_all(&mut ppu, 0x11);
        assert_eq!(memories(&ppu), (0x11, 0x11, 0x11));
        ppu.write(Register::DisplayControl, 0x0F);
        // Active display: none lands.
        write_all(&mut ppu, 0x22);
        assert_eq!(memories(&ppu), (0x11, 0x11, 0x11));
        // Horizontal blank: only CGRAM.
        ppu.set_signals(signals(Period::HorizontalBlank));
        write_all(&mut ppu, 0x33);
        assert_eq!(memories(&ppu), (0x11, 0x11, 0x33));
        // Vertical blank: all of them.
        ppu.set_signals(signals(Period::VerticalBlank));
        write_all(&mut ppu, 0x44);
        assert_eq!(memories(&ppu), (0x44, 0x44, 0x44));
    }

    // @source: anomie-regs#2102
    // @source: anomie-regs#sprites/oam
    // @source: anomie-timing#oam-reset
    // @source: fullsnes#snesmemoryoamaccessspriteattributes
    // @source: adr-0029
    #[test]
    fn the_oam_address_reloads_at_vertical_blank_and_when_forced_blank_ends() {
        let mut ppu = ppu();
        ppu.oam_mut()[..6].copy_from_slice(&[0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5]);
        ppu.write(Register::OamAddressLow, 0x01);
        assert_eq!(read(&mut ppu, Register::OamDataRead), Some(0xA2));
        // Forced blank is on, so the start of vertical blank keeps the address.
        ppu.start_vertical_blank();
        assert_eq!(read(&mut ppu, Register::OamDataRead), Some(0xA3));
        // Clearing forced blank reloads it.
        ppu.write(Register::DisplayControl, 0x0F);
        assert_eq!(read(&mut ppu, Register::OamDataRead), Some(0xA2));
        // So does the start of vertical blank with forced blank off.
        ppu.start_vertical_blank();
        assert_eq!(read(&mut ppu, Register::OamDataRead), Some(0xA2));
        // Setting forced blank does not.
        ppu.write(Register::DisplayControl, 0x8F);
        assert_eq!(read(&mut ppu, Register::OamDataRead), Some(0xA3));
        // Reset turns forced blank back on.
        ppu.write(Register::DisplayControl, 0x0F);
        ppu.reset();
        assert_eq!(ppu.display_control, 0x80);
    }

    // @source: anomie-regs#211b
    // @source: anomie-regs#211c
    // @source: anomie-regs#2134
    // @source: fullsnes#snesiomap
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0029
    #[test]
    fn the_multiplier_reads_as_ppu1_and_shares_the_mode_7_latch() {
        let mut ppu = ppu();
        let product = |ppu: &mut Ppu| {
            [
                Register::MultiplyLow,
                Register::MultiplyMiddle,
                Register::MultiplyHigh,
            ]
            .map(|register| read(ppu, register))
        };
        assert_eq!(product(&mut ppu), [Some(0x01), Some(0x00), Some(0x00)]);
        ppu.write(Register::Mode7MatrixA, 0x00);
        // BG1HOFS shares the latch, so M7A becomes $1020.
        ppu.write(Register::Background1HorizontalScroll, 0x20);
        ppu.write(Register::Mode7MatrixA, 0x10);
        ppu.write(Register::Mode7MatrixB, 0xFE);
        // $1020 times -2 is -$2040.
        assert_eq!(product(&mut ppu), [Some(0xC0), Some(0xDF), Some(0xFF)]);
        // The last byte read is PPU1 open bus.
        assert_eq!(ppu.peek(Register::BackgroundMode), Some(0xFF));
        assert_eq!(ppu.ppu2_open_bus, 0);
    }

    // @source: anomie-regs#213e
    // @source: anomie-regs#213f
    // @source: fullsnes#snespputimersandstatus
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0029
    #[test]
    fn the_status_registers_carry_flags_open_bus_and_versions() {
        let mut ppu = ppu();
        ppu.ppu1_open_bus = 0xFF;
        ppu.ppu2_open_bus = 0xFF;
        assert_eq!(read(&mut ppu, Register::Ppu1Status), Some(0x11));
        assert_eq!(ppu.ppu1_open_bus, 0x11);
        assert_eq!(read(&mut ppu, Register::Ppu2Status), Some(0x23));
        assert_eq!(ppu.ppu2_open_bus, 0x23);
        ppu.set_signals(Signals {
            field: Field::Second,
            pal: true,
            ..Signals::POWER_ON
        });
        ppu.latch_counters(0, 0);
        // Bit 5 is the last PPU2 byte read, $23.
        assert_eq!(read(&mut ppu, Register::Ppu2Status), Some(0xF3));
        // With $4201 bit 7 clear the latch flag survives the read.
        assert_eq!(ppu.peek(Register::Ppu2Status), Some(0xF3));
        ppu.set_signals(Signals {
            external_latch: true,
            ..Signals::POWER_ON
        });
        assert_eq!(read(&mut ppu, Register::Ppu2Status), Some(0x63));
        assert_eq!(read(&mut ppu, Register::Ppu2Status), Some(0x23));
        // A peek can be given other signals.
        let pal = Signals {
            pal: true,
            ..Signals::POWER_ON
        };
        assert_eq!(ppu.peek_with(Register::Ppu2Status, pal), Some(0x33));
    }

    // @source: anomie-regs#213b
    // @source: anomie-regs#213c
    // @source: anomie-regs#213f
    // @source: fullsnes#snesmemorycgramaccesspalettememory
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0029
    #[test]
    fn cgram_reads_take_ppu2_open_bus_and_stat78_resets_the_counter_flip_flops() {
        let mut ppu = ppu();
        ppu.cgram_mut()[..2].copy_from_slice(&[0x34, 0x92]);
        ppu.write(Register::CgramAddress, 0x00);
        assert_eq!(read(&mut ppu, Register::CgramDataRead), Some(0x34));
        // Bit 7 of the high byte is PPU2 open bus: bit 7 of $34 is 0.
        assert_eq!(read(&mut ppu, Register::CgramDataRead), Some(0x12));
        assert_eq!(ppu.ppu2_open_bus, 0x12);
        // Both counters left on their high byte; a $213F read puts them back on the low.
        ppu.latch_counters(0x105, 0x0A3);
        assert_eq!(read(&mut ppu, Register::HorizontalCounter), Some(0xA3));
        assert_eq!(read(&mut ppu, Register::VerticalCounter), Some(0x05));
        read(&mut ppu, Register::Ppu2Status);
        assert_eq!(read(&mut ppu, Register::HorizontalCounter), Some(0xA3));
        assert_eq!(read(&mut ppu, Register::VerticalCounter), Some(0x05));
    }

    // @source: anomie-regs#2116
    // @source: anomie-regs#2139
    // @source: anomie-regs#213a
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0029
    #[test]
    fn vram_reads_through_the_registers_set_ppu1_open_bus() {
        let mut ppu = ppu();
        ppu.vram_mut()[0x10..0x14].copy_from_slice(&[0x01, 0x02, 0x03, 0x04]);
        ppu.write(Register::VramControl, 0x00);
        ppu.write(Register::VramAddressLow, 0x08);
        ppu.write(Register::VramAddressHigh, 0x00);
        assert_eq!(read(&mut ppu, Register::VramDataReadHigh), Some(0x02));
        assert_eq!(read(&mut ppu, Register::VramDataReadLow), Some(0x01));
        assert_eq!(read(&mut ppu, Register::VramDataReadLow), Some(0x01));
        assert_eq!(read(&mut ppu, Register::VramDataReadLow), Some(0x03));
        assert_eq!(ppu.ppu1_open_bus, 0x03);
        assert_eq!(ppu.vram_port.address(), 0x000B);
    }
}
