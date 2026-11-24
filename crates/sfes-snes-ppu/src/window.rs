// @source: anomie-regs#2123
// @source: anomie-regs#2124
// @source: anomie-regs#2125
// @source: anomie-regs#2126
// @source: anomie-regs#2127
// @source: anomie-regs#2128
// @source: anomie-regs#2129
// @source: anomie-regs#212a
// @source: anomie-regs#212b
// @source: anomie-regs#212e
// @source: anomie-regs#212f
// @source: anomie-regs#windows
// @source: fullsnes#snesppuwindow
// @source: adr-0034
// @source: adr-0035
//! The two windows: their edges, which layers they act on, how the two combine, and which
//! layers they mask on the main and sub screens (ADR 0034; ADR 0035). The colour window's
//! area is read by colour math.

use crate::{background::Background, register::Register};

/// `$212E` and `$212F` bits 4–0: BG1 to BG4 and OBJ.
const SCREEN_MASK: u8 = 0x1F;
/// A layer's window setting is four bits: window 1's enable and invert, then window 2's.
const SETTING_MASK: u8 = 0x0F;
/// A setting's bit for window 1 enabled; the invert bit is the one below it.
const WINDOW_1_ENABLE: u8 = 0x02;
/// A setting's bit for window 2 enabled.
const WINDOW_2_ENABLE: u8 = 0x08;
/// A setting's bit for window 1 inverted.
const WINDOW_1_INVERT: u8 = 0x01;
/// A setting's bit for window 2 inverted.
const WINDOW_2_INVERT: u8 = 0x04;
/// A logic field is two bits.
const LOGIC_MASK: u8 = 0x03;

/// A layer a window can mask. Each variant's value is its index and, up to OBJ, its bit in
/// `$212E`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Masked {
    /// BG1.
    Background1 = 0,
    /// BG2.
    Background2 = 1,
    /// BG3.
    Background3 = 2,
    /// BG4.
    Background4 = 3,
    /// The sprites.
    Object = 4,
    /// The colour window; it has no designation bit, and `$2130` reads its area.
    Color = 5,
}

/// Which screen a layer is on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Screen {
    /// The main screen, with `$212E`.
    Main,
    /// The sub screen, with `$212F`.
    Sub,
}

impl From<Background> for Masked {
    // @source: anomie-regs#2123
    // @source: anomie-regs#212e
    fn from(background: Background) -> Self {
        match background {
            Background::One => Self::Background1,
            Background::Two => Self::Background2,
            Background::Three => Self::Background3,
            Background::Four => Self::Background4,
        }
    }
}

/// How two windows combine; each variant's value is the register's field.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Logic {
    /// Either window.
    Or = 0,
    /// Both windows.
    And = 1,
    /// Exactly one window.
    Xor = 2,
    /// Both or neither.
    Xnor = 3,
}

impl Logic {
    /// The operation a two-bit field names.
    // @source: anomie-regs#212a
    // @source: anomie-regs#212b
    // @source: fullsnes#snesppuwindow
    const fn from_field(field: u8) -> Self {
        match field & LOGIC_MASK {
            0 => Self::Or,
            1 => Self::And,
            2 => Self::Xor,
            _ => Self::Xnor,
        }
    }

    /// The operation on two windows' values.
    // @source: anomie-regs#212b
    // @source: fullsnes#snesppuwindow
    const fn apply(self, first: bool, second: bool) -> bool {
        match self {
            Self::Or => first | second,
            Self::And => first & second,
            Self::Xor => first ^ second,
            Self::Xnor => !(first ^ second),
        }
    }
}

/// `$2123–$212B`, `$212E` and `$212F`: the windows' registers, all plain bytes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Windows {
    /// `$2123 W12SEL`, `$2124 W34SEL` and `$2125 WOBJSEL`.
    select: [u8; 3],
    /// `$2126 WH0` and `$2127 WH1`: window 1's left and right edges.
    first: [u8; 2],
    /// `$2128 WH2` and `$2129 WH3`: window 2's left and right edges.
    second: [u8; 2],
    /// `$212A WBGLOG` and `$212B WOBJLOG`.
    logic: [u8; 2],
    /// `$212E TMW`: the layers masked on the main screen.
    main: u8,
    /// `$212F TSW`: the layers masked on the sub screen.
    sub: u8,
}

impl Windows {
    /// All registers 0, as at power-on (ADR 0034, point 1).
    // @source: fullsnes#snesiomap
    // @source: adr-0034
    pub(crate) const fn power_on() -> Self {
        Self {
            select: [0; 3],
            first: [0; 2],
            second: [0; 2],
            logic: [0; 2],
            main: 0,
            sub: 0,
        }
    }

    /// A write to one of the window registers; others are ignored.
    // @source: anomie-regs#2123
    // @source: anomie-regs#2124
    // @source: anomie-regs#2125
    // @source: anomie-regs#2126
    // @source: anomie-regs#2127
    // @source: anomie-regs#2128
    // @source: anomie-regs#2129
    // @source: anomie-regs#212a
    // @source: anomie-regs#212b
    // @source: anomie-regs#212e
    // @source: anomie-regs#212f
    // @source: adr-0034
    // @source: adr-0035
    pub(crate) const fn write(&mut self, register: Register, value: u8) {
        match register {
            Register::WindowMaskBackground12 => self.select[0] = value,
            Register::WindowMaskBackground34 => self.select[1] = value,
            Register::WindowMaskObjectColor => self.select[2] = value,
            Register::Window1Left => self.first[0] = value,
            Register::Window1Right => self.first[1] = value,
            Register::Window2Left => self.second[0] = value,
            Register::Window2Right => self.second[1] = value,
            Register::WindowLogicBackgrounds => self.logic[0] = value,
            Register::WindowLogicObjectColor => self.logic[1] = value,
            Register::MainScreenWindow => self.main = value & SCREEN_MASK,
            Register::SubScreenWindow => self.sub = value & SCREEN_MASK,
            _ => {}
        }
    }

    /// Whether a window with these edges is inside at `column`: the edges are inclusive,
    /// and an empty window (left above right) is never inside.
    // @source: anomie-regs#2126
    // @source: fullsnes#snesppuwindow
    fn inside(edges: [u8; 2], column: u16) -> bool {
        u16::from(edges[0]) <= column && column <= u16::from(edges[1])
    }

    /// The area of `layer` at `column`: false with neither window enabled, the one enabled
    /// window's value (inverted if its bit says so), else the layer's logic of the two
    /// (ADR 0034, points 2 and 4).
    // @source: anomie-regs#2123
    // @source: anomie-regs#212a
    // @source: anomie-regs#windows
    // @source: fullsnes#snesppuwindow
    // @source: adr-0034
    pub(crate) fn area(&self, layer: Masked, column: u16) -> bool {
        let index = layer as u8;
        let shift = 4 * (index % 2);
        let select = self.select.get(usize::from(index / 2));
        let setting = select.copied().unwrap_or_default() >> shift & SETTING_MASK;
        let first = (setting & WINDOW_1_ENABLE != 0)
            .then(|| Self::inside(self.first, column) != (setting & WINDOW_1_INVERT != 0));
        let second = (setting & WINDOW_2_ENABLE != 0)
            .then(|| Self::inside(self.second, column) != (setting & WINDOW_2_INVERT != 0));
        match (first, second) {
            (None, None) => false,
            (Some(only), None) | (None, Some(only)) => only,
            (Some(first), Some(second)) => {
                let logic = self.logic.get(usize::from(index / 4));
                let field = logic.copied().unwrap_or_default() >> (2 * (index % 4));
                Logic::from_field(field).apply(first, second)
            }
        }
    }

    /// Whether `layer` is masked at `column` on `screen`: its `$212E` (main) or `$212F`
    /// (sub) bit is set and its area is true there (ADR 0034, point 5; ADR 0035, point 3).
    /// The colour window has no bit and is never masked.
    // @source: anomie-regs#212e
    // @source: anomie-regs#212f
    // @source: fullsnes#snesppuwindow
    // @source: adr-0034
    // @source: adr-0035
    pub(crate) fn masked(&self, screen: Screen, layer: Masked, column: u16) -> bool {
        let designated = match screen {
            Screen::Main => self.main,
            Screen::Sub => self.sub,
        };
        designated >> (layer as u8) & 1 != 0 && self.area(layer, column)
    }
}

#[cfg(test)]
mod tests {
    use alloc::{format, string::String};

    use super::*;

    /// Windows 1 and 2 at 8..=23 and 16..=31, and the three `$2123`-`$2125` registers and
    /// two logic registers as given.
    // @source: anomie-regs#2126
    // @source: adr-0034
    fn windows(select: [u8; 3], logic: [u8; 2]) -> Windows {
        let mut windows = Windows::power_on();
        windows.write(Register::Window1Left, 8);
        windows.write(Register::Window1Right, 23);
        windows.write(Register::Window2Left, 16);
        windows.write(Register::Window2Right, 31);
        windows.write(Register::WindowMaskBackground12, select[0]);
        windows.write(Register::WindowMaskBackground34, select[1]);
        windows.write(Register::WindowMaskObjectColor, select[2]);
        windows.write(Register::WindowLogicBackgrounds, logic[0]);
        windows.write(Register::WindowLogicObjectColor, logic[1]);
        windows
    }

    /// The columns 0 to 39 where `layer`'s area is true, as a string of `#` and `.`.
    // @source: adr-0034
    fn picture(windows: &Windows, layer: Masked) -> String {
        (0..40)
            .map(|column| {
                if windows.area(layer, column) {
                    '#'
                } else {
                    '.'
                }
            })
            .collect::<String>()
    }

    // @source: anomie-regs#2126
    // @source: anomie-regs#2123
    // @source: fullsnes#snesppuwindow
    // @source: adr-0034
    #[test]
    fn an_enabled_window_is_inside_between_its_inclusive_edges() {
        // BG1: window 1 enabled, bit 1 of its setting.
        let windows = windows([0x02, 0, 0], [0, 0]);
        assert_eq!(
            picture(&windows, Masked::Background1),
            "........################................"
        );
    }

    // @source: anomie-regs#2123
    // @source: adr-0034
    #[test]
    fn the_invert_bit_turns_a_window_inside_out_and_enable_is_separate() {
        // Window 1 enabled and inverted: outside 8 to 23.
        let inverted = windows([0x03, 0, 0], [0, 0]);
        assert_eq!(
            picture(&inverted, Masked::Background1),
            "########................################"
        );
        // Invert alone, without enable, is a disabled window: nothing is masked
        // (anomie-regs#212a, "Neither window enabled").
        let disabled = windows([0x01, 0, 0], [0, 0]);
        assert_eq!(picture(&disabled, Masked::Background1), ".".repeat(40));
    }

    // @source: anomie-regs#2126
    // @source: fullsnes#snesppuwindow
    // @source: adr-0034
    #[test]
    fn a_window_whose_left_edge_is_above_its_right_is_empty() {
        let mut windows = windows([0x02, 0, 0], [0, 0]);
        windows.write(Register::Window1Left, 24);
        assert_eq!(picture(&windows, Masked::Background1), ".".repeat(40));
        // Inverted, an empty window is on everywhere.
        windows.write(Register::WindowMaskBackground12, 0x03);
        assert_eq!(picture(&windows, Masked::Background1), "#".repeat(40));
        // Equal edges are one pixel.
        windows.write(Register::Window1Left, 23);
        windows.write(Register::WindowMaskBackground12, 0x02);
        assert_eq!(
            picture(&windows, Masked::Background1),
            format!("{}#{}", ".".repeat(23), ".".repeat(16))
        );
    }

    // @source: anomie-regs#212a
    // @source: anomie-regs#212b
    // @source: fullsnes#snesppuwindow
    // @source: adr-0034
    #[test]
    fn two_windows_combine_by_or_and_xor_and_xnor() {
        // BG1 with both windows enabled: setting 0x0A; its logic is `$212A` bits 1 to 0.
        let both = |logic| windows([0x0A, 0, 0], [logic, 0]);
        assert_eq!(
            picture(&both(0), Masked::Background1),
            "........########################........"
        );
        assert_eq!(
            picture(&both(1), Masked::Background1),
            "................########................"
        );
        assert_eq!(
            picture(&both(2), Masked::Background1),
            "........########........########........"
        );
        assert_eq!(
            picture(&both(3), Masked::Background1),
            "########........########........########"
        );
    }

    // @source: anomie-regs#212a
    // @source: anomie-regs#212b
    // @source: adr-0034
    #[test]
    fn each_layer_uses_its_own_setting_and_logic_field() {
        // BG1 to BG4 and OBJ with both windows enabled; the logic fields are OR, AND, XOR
        // and XNOR for BG1 to BG4, then AND for OBJ.
        let windows = windows([0xAA, 0xAA, 0xAA], [0b11_10_01_00, 0b01]);
        let area = |layer| picture(&windows, layer);
        assert_eq!(
            area(Masked::Background1),
            "........########################........"
        );
        assert_eq!(
            area(Masked::Background2),
            "................########................"
        );
        assert_eq!(
            area(Masked::Background3),
            "........########........########........"
        );
        assert_eq!(
            area(Masked::Background4),
            "########........########........########"
        );
        assert_eq!(
            area(Masked::Object),
            "................########................"
        );
    }

    // @source: anomie-regs#2123
    // @source: anomie-regs#2124
    // @source: anomie-regs#2125
    // @source: adr-0034
    #[test]
    fn a_settings_nibble_is_the_low_half_for_layers_0_2_and_4_and_the_high_half_for_1_and_3() {
        // BG2 (high half of `$2123`) has window 1; BG3 (low half of `$2124`) window 2.
        let windows = windows([0x20, 0x08, 0x80], [0, 0]);
        assert_eq!(picture(&windows, Masked::Background1), ".".repeat(40));
        assert_eq!(
            picture(&windows, Masked::Background2),
            "........################................"
        );
        assert_eq!(
            picture(&windows, Masked::Background3),
            "................################........"
        );
        assert_eq!(picture(&windows, Masked::Background4), ".".repeat(40));
        assert_eq!(picture(&windows, Masked::Object), ".".repeat(40));
    }

    // @source: anomie-regs#212e
    // @source: fullsnes#snesppuwindow
    // @source: adr-0034
    #[test]
    fn a_layer_is_masked_on_the_main_screen_only_where_its_designation_bit_is_set() {
        let mut windows = windows([0x02, 0, 0x02], [0, 0]);
        // BG1 and OBJ both have window 1, but only BG1 is designated.
        windows.write(Register::MainScreenWindow, 0x01);
        assert!(windows.masked(Screen::Main, Masked::Background1, 10));
        assert!(!windows.masked(Screen::Main, Masked::Background1, 4));
        assert!(!windows.masked(Screen::Main, Masked::Object, 10));
        // Bit 4 is OBJ; the colour window has no designation bit.
        windows.write(Register::MainScreenWindow, 0xFF);
        assert!(windows.masked(Screen::Main, Masked::Object, 10));
    }

    // @source: fullsnes#snesiomap
    // @source: adr-0034
    #[test]
    fn nothing_is_masked_at_power_on() {
        let windows = Windows::power_on();
        for layer in [Masked::Background1, Masked::Background4, Masked::Object] {
            assert!(!windows.masked(Screen::Main, layer, 0), "{layer:?}");
        }
    }

    // @source: anomie-regs#212f
    // @source: anomie-regs#2125
    // @source: anomie-regs#212b
    // @source: adr-0035
    #[test]
    fn the_sub_screen_has_its_own_designation_and_the_colour_window_an_area() {
        // The colour window is the high half of $2125: window 2 enabled, bits 3 and 2 of
        // the setting; with window 1 not enabled it is window 2's value, 16..=31.
        let mut windows = windows([0x02, 0, 0x80], [0, 0]);
        windows.write(Register::SubScreenWindow, 0x01);
        assert!(windows.masked(Screen::Sub, Masked::Background1, 10));
        assert!(!windows.masked(Screen::Main, Masked::Background1, 10));
        // Bits 7 and 6 of $212F are not kept, and the colour window is never "masked".
        windows.write(Register::SubScreenWindow, 0xFF);
        assert!(!windows.masked(Screen::Sub, Masked::Color, 20));
        assert_eq!(
            picture(&windows, Masked::Color),
            "................################........"
        );
    }
}
