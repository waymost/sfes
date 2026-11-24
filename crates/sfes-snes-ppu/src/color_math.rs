// @source: anomie-regs#2100
// @source: anomie-regs#2130
// @source: anomie-regs#2131
// @source: anomie-regs#2132
// @source: anomie-regs#rendering-the-screen/color-math
// @source: anomie-regs#windows
// @source: fullsnes#snesppucolormath
// @source: fullsnes#snesppucontrol
// @source: adr-0035
//! Colour math, the fixed colour and brightness (ADR 0035).

use sfes_api::Rgb555;

use crate::register::Register;

/// `$2130` bits 7–6: clip the main pixel to black where.
const CLIP_SHIFT: u8 = 6;
/// `$2130` bits 5–4: prevent math where.
const PREVENT_SHIFT: u8 = 4;
/// A region field is two bits.
const REGION_MASK: u8 = 0x03;
/// `$2130` bit 1: add the sub screen instead of the fixed colour.
const ADD_SUB_SCREEN: u8 = 0x02;
/// `$2130` bit 0: direct colour.
const DIRECT_COLOR: u8 = 0x01;
/// `$2131` bit 7: subtract.
const SUBTRACT: u8 = 0x80;
/// `$2131` bit 6: halve the result.
const HALF: u8 = 0x40;
/// `$2131` bits 5–0: the layers that take math.
const LAYERS_MASK: u8 = 0x3F;
/// The channels of a colour, each 5 bits.
const CHANNEL_MAX: u8 = 31;
/// `$2132` bit 5: the intensity goes into red; green and blue are the next bits up.
const FIXED_RED: u8 = 0x20;
/// `$2132` bits 4–0: the intensity.
const FIXED_INTENSITY: u8 = 0x1F;
/// A sprite palette below this CGRAM entry (palettes 0 to 3) never takes math.
const FIRST_MATH_SPRITE_ENTRY: u8 = 192;
/// `$2100` bits 3–0: the brightness.
const BRIGHTNESS_MASK: u8 = 0x0F;

/// Which layer a main-screen pixel came from; `$2131` says which take math.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Source {
    /// BG1 to BG4, as an index 0 to 3.
    Background(u8),
    /// A sprite pixel from this CGRAM entry.
    Object(u8),
    /// The backdrop.
    Backdrop,
}

/// What the other operand of a blend is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Operand {
    /// The fixed colour, because `$2130` bit 1 is clear.
    Fixed,
    /// The sub screen's front pixel.
    Pixel(Rgb555),
    /// The sub screen's backdrop, which is the fixed colour.
    Backdrop,
}

/// `$2130–$2132`: the colour math registers and the fixed colour.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct ColorMath {
    /// `$2130 CGWSEL`: clip and prevent regions, the sub screen and direct colour.
    select: u8,
    /// `$2131 CGADSUB`: add or subtract, half, and the layers that take math.
    designation: u8,
    /// `$2132 COLDATA`: the fixed colour's red, green and blue.
    fixed: Rgb555,
}

impl ColorMath {
    /// All 0: the fixed colour black (ADR 0035, point 1).
    // @source: fullsnes#snesiomap
    // @source: adr-0035
    pub(crate) const fn power_on() -> Self {
        Self {
            select: 0,
            designation: 0,
            fixed: Rgb555::BLACK,
        }
    }

    /// A write to `$2130`, `$2131` or `$2132`; others are ignored.
    // @source: anomie-regs#2130
    // @source: anomie-regs#2131
    // @source: anomie-regs#2132
    // @source: fullsnes#snesppucolormath
    // @source: adr-0035
    pub(crate) fn write(&mut self, register: Register, value: u8) {
        match register {
            Register::ColorMathSelect => self.select = value,
            Register::ColorMathDesignation => self.designation = value,
            Register::FixedColor => {
                let intensity = value & FIXED_INTENSITY;
                let pick = |plane: u8, current: u8| {
                    if value & (FIXED_RED << plane) != 0 {
                        intensity
                    } else {
                        current
                    }
                };
                self.fixed = Rgb555::from_channels(
                    pick(0, self.fixed.red()),
                    pick(1, self.fixed.green()),
                    pick(2, self.fixed.blue()),
                );
            }
            _ => {}
        }
    }

    /// Whether `$2130` bit 0 asks for direct colour.
    // @source: anomie-regs#2130
    // @source: fullsnes#snesppucolormath
    pub(crate) const fn direct_color(self) -> bool {
        self.select & DIRECT_COLOR != 0
    }

    /// Whether any layer takes math, so the sub screen can matter.
    // @source: anomie-regs#2131
    // @source: adr-0035
    pub(crate) const fn any_layer(self) -> bool {
        self.designation & LAYERS_MASK != 0
    }

    /// Whether the other operand is the sub screen rather than the fixed colour.
    // @source: anomie-regs#2130
    // @source: fullsnes#snesppucolormath
    pub(crate) const fn adds_sub_screen(self) -> bool {
        self.select & ADD_SUB_SCREEN != 0
    }

    /// The fixed colour.
    // @source: anomie-regs#2132
    // @source: adr-0035
    pub(crate) const fn fixed_color(self) -> Rgb555 {
        self.fixed
    }

    /// Whether a region field of `$2130` holds at a place where the colour window's area is
    /// `area`: never, outside, inside, always (ADR 0035, point 4).
    // @source: anomie-regs#2130
    // @source: fullsnes#snesppucolormath
    // @source: adr-0035
    const fn region(field: u8, area: bool) -> bool {
        match field & REGION_MASK {
            0 => false,
            1 => !area,
            2 => area,
            _ => true,
        }
    }

    /// Whether the main pixel is clipped to black where the colour window's area is `area`.
    // @source: anomie-regs#2130
    // @source: anomie-regs#windows
    // @source: adr-0035
    pub(crate) const fn clips(self, area: bool) -> bool {
        Self::region(self.select >> CLIP_SHIFT, area)
    }

    /// Whether math is prevented where the colour window's area is `area`.
    // @source: anomie-regs#2130
    // @source: anomie-regs#windows
    // @source: adr-0035
    pub(crate) const fn prevents(self, area: bool) -> bool {
        Self::region(self.select >> PREVENT_SHIFT, area)
    }

    /// Whether a main pixel from `source` takes math: its layer's bit in `$2131` is set,
    /// and a sprite's palette is 4 to 7 (ADR 0035, point 5).
    // @source: anomie-regs#2131
    // @source: anomie-regs#rendering-the-screen/color-math
    // @source: fullsnes#snesppucolormath
    // @source: adr-0035
    pub(crate) const fn takes_math(self, source: Source) -> bool {
        let bit = match source {
            Source::Background(index) => index,
            Source::Object(entry) => {
                if entry < FIRST_MATH_SPRITE_ENTRY {
                    return false;
                }
                4
            }
            Source::Backdrop => 5,
        };
        self.designation >> bit & 1 != 0
    }

    /// `main` with `operand` added or subtracted, per channel, halved when `$2131` says so
    /// and the half applies, and clamped to 0 to 31 (ADR 0035, point 6). `clipped` is
    /// whether `main` was clipped to black: a clipped pixel is not halved, and neither is the
    /// sub backdrop added through the sub screen.
    // @source: anomie-regs#2131
    // @source: anomie-regs#rendering-the-screen/color-math
    // @source: fullsnes#snesppucolormath
    // @source: adr-0035
    pub(crate) fn blend(self, main: Rgb555, operand: Operand, clipped: bool) -> Rgb555 {
        let other = match operand {
            Operand::Pixel(pixel) => pixel,
            Operand::Fixed | Operand::Backdrop => self.fixed_color(),
        };
        let halves = self.designation & HALF != 0 && !clipped && operand != Operand::Backdrop;
        main.zip_map(other, |left, right| {
            let sum = if self.designation & SUBTRACT != 0 {
                i16::from(left) - i16::from(right)
            } else {
                i16::from(left) + i16::from(right)
            };
            let sum = if halves { sum >> 1 } else { sum };
            // Clamped to 0..=31, so it fits.
            u8::try_from(sum.clamp(0, i16::from(CHANNEL_MAX))).unwrap_or_default()
        })
    }
}

/// `color` at the brightness of `$2100`'s low four bits: black at 0, else each channel times
/// `N + 1` over 16, rounded down (ADR 0035, point 2).
// @source: anomie-regs#2100
// @source: fullsnes#snesppucontrol
// @source: adr-0035
pub(crate) fn dim(color: Rgb555, control: u8) -> Rgb555 {
    let level = control & BRIGHTNESS_MASK;
    if level == 0 {
        return Rgb555::BLACK;
    }
    // At most 31 * 16 / 16, so it fits.
    color.map(|channel| {
        u8::try_from(u16::from(channel) * (u16::from(level) + 1) / 16).unwrap_or_default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
    // @source: adr-0035
    fn color(red: u8, green: u8, blue: u8) -> Rgb555 {
        Rgb555::from_channels(red, green, blue)
    }

    // @source: anomie-regs#2132
    // @source: fullsnes#snesppucolormath
    // @source: adr-0035
    #[test]
    fn coldata_sets_only_the_planes_its_bits_name() {
        let mut math = ColorMath::power_on();
        // Fullsnes's orange example in anomie's form: $3F red, $4F green, $80 blue.
        math.write(Register::FixedColor, 0x3F);
        math.write(Register::FixedColor, 0x4F);
        math.write(Register::FixedColor, 0x80);
        assert_eq!(math.fixed_color(), color(31, 15, 0));
        // Two planes at once keep the third.
        math.write(Register::FixedColor, 0xC5);
        assert_eq!(math.fixed_color(), color(31, 5, 5));
    }

    // @source: anomie-regs#2130
    // @source: fullsnes#snesppucolormath
    // @source: adr-0035
    #[test]
    fn a_region_field_holds_never_outside_inside_or_always() {
        for (field, outside, inside) in [
            (0, false, false),
            (1, true, false),
            (2, false, true),
            (3, true, true),
        ] {
            let mut math = ColorMath::power_on();
            math.write(Register::ColorMathSelect, field << 6);
            assert_eq!((math.clips(false), math.clips(true)), (outside, inside));
            assert!(!math.prevents(false) && !math.prevents(true));
            math.write(Register::ColorMathSelect, field << 4);
            assert_eq!(
                (math.prevents(false), math.prevents(true)),
                (outside, inside)
            );
            assert!(!math.clips(false) && !math.clips(true));
        }
    }

    // @source: anomie-regs#2131
    // @source: anomie-regs#rendering-the-screen/color-math
    // @source: fullsnes#snesppucolormath
    // @source: adr-0035
    #[test]
    fn a_layer_takes_math_by_its_bit_and_a_sprite_only_from_palette_4() {
        let mut math = ColorMath::power_on();
        math.write(Register::ColorMathDesignation, 0x3F);
        for index in 0..4 {
            assert!(math.takes_math(Source::Background(index)));
        }
        assert!(math.takes_math(Source::Backdrop));
        assert!(!math.takes_math(Source::Object(191)));
        assert!(math.takes_math(Source::Object(192)));
        math.write(Register::ColorMathDesignation, 0x01);
        assert!(math.takes_math(Source::Background(0)));
        assert!(!math.takes_math(Source::Background(1)));
        assert!(!math.takes_math(Source::Backdrop));
        assert!(!math.takes_math(Source::Object(200)));
    }

    // @source: anomie-regs#rendering-the-screen/color-math
    // @source: fullsnes#snesppucolormath
    // @source: adr-0035
    #[test]
    fn add_and_subtract_clamp_per_channel() {
        let mut math = ColorMath::power_on();
        math.write(Register::FixedColor, 0x40 | 16);
        math.write(Register::FixedColor, 0x80 | 16);
        // Anomie's subtraction example: (31, 31, 0) minus the fixed colour (0, 16, 16).
        math.write(Register::ColorMathDesignation, SUBTRACT);
        assert_eq!(
            math.blend(color(31, 31, 0), Operand::Fixed, false),
            color(31, 15, 0)
        );
        // Addition clamps at 31, and each channel stands alone.
        math.write(Register::ColorMathDesignation, 0);
        assert_eq!(
            math.blend(color(31, 10, 5), Operand::Fixed, false),
            color(31, 26, 21)
        );
    }

    // @source: anomie-regs#2131
    // @source: anomie-regs#rendering-the-screen/color-math
    // @source: fullsnes#snesppucolormath
    // @source: adr-0035
    #[test]
    fn half_rounds_down_and_is_skipped_when_clipped_or_for_the_sub_backdrop() {
        let mut math = ColorMath::power_on();
        math.write(Register::FixedColor, 0x20 | 3);
        math.write(Register::ColorMathDesignation, HALF);
        let pixel = Operand::Pixel(color(2, 0, 0));
        // (1 + 2) / 2 is 1.
        assert_eq!(math.blend(color(1, 0, 0), pixel, false), color(1, 0, 0));
        // The fixed colour as the operand, bit 1 of $2130 clear: halved, (1 + 3) / 2.
        assert_eq!(
            math.blend(color(1, 0, 0), Operand::Fixed, false),
            color(2, 0, 0)
        );
        // Clipped, or the sub backdrop: not halved.
        assert_eq!(
            math.blend(color(0, 0, 0), Operand::Fixed, true),
            color(3, 0, 0)
        );
        assert_eq!(
            math.blend(color(1, 0, 0), Operand::Backdrop, false),
            color(4, 0, 0)
        );
        // Subtract and halve never goes below 0.
        math.write(Register::ColorMathDesignation, SUBTRACT | HALF);
        assert_eq!(math.blend(color(1, 0, 0), pixel, false), color(0, 0, 0));
    }

    // @source: anomie-regs#2100
    // @source: fullsnes#snesppucontrol
    // @source: adr-0035
    #[test]
    fn brightness_scales_by_n_plus_1_over_16_and_0_is_black() {
        let white = color(31, 16, 1);
        assert_eq!(dim(white, 0x0F), white);
        assert_eq!(dim(white, 0), Rgb555::BLACK);
        // N = 7: 8/16, rounded down.
        assert_eq!(dim(white, 0x07), color(15, 8, 0));
        // Bit 7 and the unused bits are not brightness.
        assert_eq!(dim(white, 0x8F), white);
    }
}
