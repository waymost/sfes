// @source: anomie-regs#210d
// @source: anomie-regs#211a
// @source: anomie-regs#211b
// @source: anomie-regs#211c
// @source: anomie-regs#211f
// @source: anomie-regs#2134
// @source: anomie-regs#backgrounds/mode-7
// @source: fullsnes#snesmathsmultiplydivide
// @source: fullsnes#snesppurotationscaling
// @source: adr-0029
// @source: adr-0032
//! The Mode 7 registers and their shared write-twice latch, the multiplier read at
//! `$2134–$2136` (ADR 0029, point 8), and the pixels of BG mode 7 and EXTBG (ADR 0032).

use crate::{
    background::{Color, Sample, direct_color},
    memory::Vram,
};

/// The latch and both M7A and M7B at power-on: fullsnes's "(FFh)(w2)".
const POWER_ON: u8 = 0xFF;

/// `$211A` bit 7: the playfield is larger than the tilemap.
const SCREEN_OVER: u8 = 0x80;
/// `$211A` bit 6: outside the tilemap is character 0, not transparent.
const FILL_CHARACTER_0: u8 = 0x40;
/// `$211A` bit 1: flip the screen vertically.
const SCREEN_FLIP_VERTICAL: u8 = 0x02;
/// `$211A` bit 0: flip the screen horizontally.
const SCREEN_FLIP_HORIZONTAL: u8 = 0x01;
/// A flipped screen coordinate is its exclusive or with this.
const FLIP_MASK: u16 = 0xFF;
/// The 13-bit registers (`M7X`, `M7Y`, `M7HOFS` and `M7VOFS`) are sign-extended from bit 12.
const SIGN_SHIFT: u32 = 3;
/// A difference's sign is its bit 13, and its value has 10 bits.
const DIFFERENCE_SIGN: i32 = 0x2000;
/// The playfield is 1024 pixels across and down.
const PLAYFIELD_MASK: i32 = 0x03FF;
/// The matrix product drops its low 6 bits before the sum.
const PRODUCT_MASK: i32 = !63;
/// A coordinate has 8 fraction bits.
const FRACTION_BITS: u32 = 8;
/// A tile is 8 pixels across and down.
const TILE_MASK: i32 = 7;
/// The tilemap is 128 tiles across.
const MAP_TILES: i32 = 128;
/// EXTBG's BG2: bit 7 of the pixel is the priority and bits 6–0 the colour.
const EXTBG_PRIORITY: u8 = 0x80;
/// EXTBG's BG2 colour bits.
const EXTBG_COLOR_MASK: u8 = 0x7F;

/// The Mode 7 registers and the latch they share with `$210D` and `$210E`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Mode7 {
    /// The last value written to any of the registers sharing it.
    latch: u8,
    /// $211A M7SEL: the screen-over choice and the flips.
    settings: u8,
    /// $211B M7A: matrix parameter A, the multiplicand.
    matrix_a: u16,
    /// $211C M7B: matrix parameter B; its high byte is the multiplier.
    matrix_b: u16,
    /// $211D M7C: matrix parameter C.
    matrix_c: u16,
    /// $211E M7D: matrix parameter D.
    matrix_d: u16,
    /// $211F M7X: the centre's X, 13 bits signed.
    center_x: i16,
    /// $2120 M7Y: the centre's Y, 13 bits signed.
    center_y: i16,
    /// $210D M7HOFS: the horizontal scroll, 13 bits signed.
    scroll_x: i16,
    /// $210E M7VOFS: the vertical scroll, 13 bits signed.
    scroll_y: i16,
}

/// `word` as a signed 16-bit value.
// @source: anomie-regs#211b
const fn signed(word: u16) -> i32 {
    i16::from_le_bytes(word.to_le_bytes()) as i32
}

/// The low 13 bits of `word` as a signed value.
// @source: anomie-regs#211f
// @source: anomie-regs#210d
const fn signed_13(word: u16) -> i16 {
    i16::from_le_bytes((word << SIGN_SHIFT).to_le_bytes()) >> SIGN_SHIFT
}

/// The difference of a scroll and a centre, kept to 10 bits and its sign (anomie-regs's
/// `CLIP`, fullsnes's `ORG`).
// @source: anomie-regs#211f
// @source: fullsnes#snesppurotationscaling
const fn clip(difference: i32) -> i32 {
    if difference & DIFFERENCE_SIGN != 0 {
        difference | !PLAYFIELD_MASK
    } else {
        difference & PLAYFIELD_MASK
    }
}

impl Mode7 {
    /// The registers at power-on: the latch `$FF` and M7A and M7B `$FFFF` (ADR 0029,
    /// point 8), every other register 0 (ADR 0032, point 1). Reset keeps them.
    // @source: fullsnes#snesiomap
    // @source: adr-0029
    // @source: adr-0032
    pub(crate) const fn power_on() -> Self {
        let word = u16::from_le_bytes([POWER_ON, POWER_ON]);
        Self {
            latch: POWER_ON,
            settings: 0,
            matrix_a: word,
            matrix_b: word,
            matrix_c: 0,
            matrix_d: 0,
            center_x: 0,
            center_y: 0,
            scroll_x: 0,
            scroll_y: 0,
        }
    }

    /// A write of `value` to a register that shares the latch: the register's new value is
    /// `value` over the latch, and the latch becomes `value`.
    // @source: anomie-regs#211b
    // @source: adr-0029
    pub(crate) const fn write(&mut self, value: u8) -> u16 {
        let word = u16::from_le_bytes([self.latch, value]);
        self.latch = value;
        word
    }

    /// $211B M7A.
    // @source: anomie-regs#211b
    // @source: fullsnes#snesmathsmultiplydivide
    pub(crate) const fn write_matrix_a(&mut self, value: u8) {
        self.matrix_a = self.write(value);
    }

    /// $211C M7B.
    // @source: anomie-regs#211c
    // @source: fullsnes#snesmathsmultiplydivide
    pub(crate) const fn write_matrix_b(&mut self, value: u8) {
        self.matrix_b = self.write(value);
    }

    /// $211A M7SEL.
    // @source: anomie-regs#211a
    // @source: fullsnes#snesppurotationscaling
    // @source: adr-0032
    pub(crate) const fn write_settings(&mut self, value: u8) {
        self.settings = value;
    }

    /// $211D M7C.
    // @source: anomie-regs#211d
    // @source: adr-0032
    pub(crate) const fn write_matrix_c(&mut self, value: u8) {
        self.matrix_c = self.write(value);
    }

    /// $211E M7D.
    // @source: anomie-regs#211e
    // @source: adr-0032
    pub(crate) const fn write_matrix_d(&mut self, value: u8) {
        self.matrix_d = self.write(value);
    }

    /// $211F M7X.
    // @source: anomie-regs#211f
    // @source: adr-0032
    pub(crate) const fn write_center_x(&mut self, value: u8) {
        self.center_x = signed_13(self.write(value));
    }

    /// $2120 M7Y.
    // @source: anomie-regs#2120
    // @source: adr-0032
    pub(crate) const fn write_center_y(&mut self, value: u8) {
        self.center_y = signed_13(self.write(value));
    }

    /// $210D M7HOFS.
    // @source: anomie-regs#210d
    // @source: adr-0032
    pub(crate) const fn write_scroll_x(&mut self, value: u8) {
        self.scroll_x = signed_13(self.write(value));
    }

    /// $210E M7VOFS.
    // @source: anomie-regs#210e
    // @source: adr-0032
    pub(crate) const fn write_scroll_y(&mut self, value: u8) {
        self.scroll_y = signed_13(self.write(value));
    }

    /// The playfield position, with 8 fraction bits, that column `x` of output line `line`
    /// shows (ADR 0032, point 2).
    // @source: anomie-regs#211f
    // @source: fullsnes#snesppurotationscaling
    // @source: adr-0032
    fn position(&self, x: u16, line: u16) -> (i32, i32) {
        let flip = |value: u16, bit: u8| {
            i32::from(if self.settings & bit != 0 {
                value ^ FLIP_MASK
            } else {
                value
            })
        };
        let screen_x = flip(x, SCREEN_FLIP_HORIZONTAL);
        let screen_y = flip(line, SCREEN_FLIP_VERTICAL);
        let origin_x = clip(i32::from(self.scroll_x) - i32::from(self.center_x));
        let origin_y = clip(i32::from(self.scroll_y) - i32::from(self.center_y));
        let (matrix_a, matrix_b) = (signed(self.matrix_a), signed(self.matrix_b));
        let (matrix_c, matrix_d) = (signed(self.matrix_c), signed(self.matrix_d));
        // Each term is below 2^27, so the sums stay inside an `i32`.
        let across = ((matrix_a * origin_x) & PRODUCT_MASK)
            + ((matrix_b * origin_y) & PRODUCT_MASK)
            + ((matrix_b * screen_y) & PRODUCT_MASK)
            + matrix_a * screen_x
            + (i32::from(self.center_x) << FRACTION_BITS);
        let down = ((matrix_c * origin_x) & PRODUCT_MASK)
            + ((matrix_d * origin_y) & PRODUCT_MASK)
            + ((matrix_d * screen_y) & PRODUCT_MASK)
            + matrix_c * screen_x
            + (i32::from(self.center_y) << FRACTION_BITS);
        (across, down)
    }

    /// The 8-bit pixel of the playfield at column `x` of line `line`; 0 is transparent
    /// (ADR 0032, points 3 and 4).
    // @source: anomie-regs#211a
    // @source: anomie-regs#backgrounds/mode-7
    // @source: fullsnes#snesppurotationscaling
    // @source: adr-0032
    fn pixel(&self, vram: &Vram, x: u16, line: u16) -> u8 {
        let (across, down) = self.position(x, line);
        let (across, down) = (across >> FRACTION_BITS, down >> FRACTION_BITS);
        let outside = (across | down) & !PLAYFIELD_MASK != 0;
        let character = if outside && self.settings & SCREEN_OVER != 0 {
            if self.settings & FILL_CHARACTER_0 == 0 {
                return 0;
            }
            0
        } else {
            let column = (across & PLAYFIELD_MASK) / 8;
            let row = (down & PLAYFIELD_MASK) / 8;
            let word = u32::try_from(row * MAP_TILES + column).unwrap_or_default();
            vram.byte(word * 2)
        };
        let within =
            u32::try_from((down & TILE_MASK) * 8 + (across & TILE_MASK)).unwrap_or_default();
        vram.byte((u32::from(character) * 64 + within) * 2 + 1)
    }

    /// What BG1 to BG4 show at column `x` of line `line`: BG1 the pixel, BG2 the same pixel
    /// when EXTBG is on, and BG3 and BG4 nothing (ADR 0032, points 4 to 6).
    // @source: anomie-regs#2133
    // @source: anomie-regs#backgrounds/mode-7
    // @source: fullsnes#snesppubgcontrol
    // @source: adr-0032
    pub(crate) fn samples(
        &self,
        vram: &Vram,
        direct: bool,
        extbg: bool,
        x: u16,
        line: u16,
    ) -> [Option<Sample>; 4] {
        let value = self.pixel(vram, x, line);
        let color = if direct {
            Color::Direct(direct_color(value, 0))
        } else {
            Color::Palette(value)
        };
        let first = (value != 0).then_some(Sample { color, high: false });
        let second = (extbg && value & EXTBG_COLOR_MASK != 0).then_some(Sample {
            color: Color::Palette(value & EXTBG_COLOR_MASK),
            high: value & EXTBG_PRIORITY != 0,
        });
        [first, second, None, None]
    }

    /// $2134–$2136 MPYL/MPYM/MPYH: the 24-bit signed product of M7A and M7B's high byte,
    /// low byte first.
    // @source: anomie-regs#2134
    // @source: anomie-regs#2135
    // @source: anomie-regs#2136
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: adr-0029
    pub(crate) const fn product(self) -> [u8; 3] {
        let [_, multiplier] = self.matrix_b.to_le_bytes();
        let multiplicand = i16::from_le_bytes(self.matrix_a.to_le_bytes()) as i32;
        let product = multiplicand * i8::from_le_bytes([multiplier]) as i32;
        let [low, middle, high, _] = product.to_le_bytes();
        [low, middle, high]
    }
}

#[cfg(test)]
mod tests {
    use sfes_api::Rgb555;

    use super::*;

    // @source: anomie-regs#211b
    // @source: anomie-regs#2134
    // @source: fullsnes#snesiomap
    // @source: fullsnes#snesmathsmultiplydivide
    // @source: adr-0029
    #[test]
    fn the_product_is_signed_16_by_8_bits() {
        let mut mode7 = Mode7::power_on();
        // $FFFF times $FF is -1 times -1.
        assert_eq!(mode7.product(), [0x01, 0x00, 0x00]);
        mode7.write_matrix_a(0x00);
        mode7.write_matrix_a(0x80);
        mode7.write_matrix_b(0x7F);
        // -32768 times 127 is -$3F8000.
        assert_eq!(mode7.product(), [0x00, 0x80, 0xC0]);
        mode7.write_matrix_a(0x34);
        mode7.write_matrix_a(0x12);
        mode7.write_matrix_b(0x02);
        assert_eq!(mode7.product(), [0x68, 0x24, 0x00]);
    }

    // @source: anomie-regs#211b
    // @source: adr-0029
    #[test]
    fn the_latch_is_shared() {
        let mut mode7 = Mode7::power_on();
        mode7.write_matrix_a(0x34);
        // A write to another register that shares the latch, such as M7C.
        mode7.write(0x56);
        mode7.write_matrix_a(0x12);
        assert_eq!(mode7.matrix_a, 0x1256);
        assert_eq!(mode7.write(0x78), 0x7812);
    }

    // @source: anomie-regs#211b
    // @source: anomie-regs#backgrounds/mode-7
    // @source: adr-0032
    /// A Mode 7 whose matrix is the identity: A and D are 1.0, the rest 0.
    fn identity() -> Mode7 {
        let mut mode7 = Mode7::power_on();
        write_word(&mut mode7, Mode7::write_matrix_a, 0x0100);
        write_word(&mut mode7, Mode7::write_matrix_b, 0);
        write_word(&mut mode7, Mode7::write_matrix_c, 0);
        write_word(&mut mode7, Mode7::write_matrix_d, 0x0100);
        mode7
    }

    // @source: anomie-regs#211b
    // @source: adr-0032
    /// Writes `value` low byte first through `write`.
    fn write_word(mode7: &mut Mode7, write: fn(&mut Mode7, u8), value: u16) {
        let [low, high] = value.to_le_bytes();
        write(mode7, low);
        write(mode7, high);
    }

    // @source: anomie-regs#backgrounds/mode-7
    // @source: adr-0032
    /// A VRAM image with tilemap entry `(row, column)` set to `character`.
    fn map(vram: &mut [u8], row: usize, column: usize, character: u8) {
        vram[(row * 128 + column) * 2] = character;
    }

    // @source: anomie-regs#backgrounds/mode-7
    // @source: adr-0032
    /// Sets pixel `(row, column)` of `character` to `value`.
    fn dot(vram: &mut [u8], character: usize, row: usize, column: usize, value: u8) {
        vram[(character * 64 + row * 8 + column) * 2 + 1] = value;
    }

    // @source: anomie-regs#backgrounds/mode-7
    // @source: adr-0032
    /// The playfield of the tests: character 1 at the tilemap's corner and at its last
    /// column, with a dot of `$42` at row 1, column 3 of character 1 and `$99` at row 2,
    /// column 4 of character 0.
    fn playfield() -> Vram {
        let mut vram = Vram::zeroed();
        map(&mut vram, 0, 0, 1);
        map(&mut vram, 0, 127, 1);
        dot(&mut vram, 1, 1, 3, 0x42);
        dot(&mut vram, 0, 2, 4, 0x99);
        vram
    }

    // @source: anomie-regs#211b
    // @source: anomie-regs#backgrounds/mode-7
    // @source: fullsnes#snesppurotationscaling
    // @source: adr-0032
    #[test]
    fn an_identity_matrix_shows_the_playfield_at_the_screen_position() {
        let vram = playfield();
        let mode7 = identity();
        // Line 1 is the playfield's row 1.
        assert_eq!(mode7.pixel(&vram, 3, 1), 0x42);
        assert_eq!(mode7.pixel(&vram, 4, 1), 0);
        assert_eq!(mode7.pixel(&vram, 3, 2), 0);
    }

    // @source: anomie-regs#210d
    // @source: anomie-regs#211f
    // @source: fullsnes#snesppurotationscaling
    // @source: adr-0032
    #[test]
    fn the_scroll_moves_the_view_and_the_centre_is_the_fixed_point() {
        let vram = playfield();
        let mut mode7 = identity();
        write_word(&mut mode7, Mode7::write_scroll_x, 3);
        write_word(&mut mode7, Mode7::write_scroll_y, 0);
        // The playfield's column 3 is at the screen's column 0.
        assert_eq!(mode7.pixel(&vram, 0, 1), 0x42);
        // The centre is the point that stays put when the matrix scales: at 2.0, with the
        // scroll 0, the screen's column 3 is the playfield's 2 * (3 - 3) + 3.
        write_word(&mut mode7, Mode7::write_scroll_x, 0);
        write_word(&mut mode7, Mode7::write_matrix_a, 0x0200);
        write_word(&mut mode7, Mode7::write_center_x, 3);
        assert_eq!(mode7.pixel(&vram, 3, 1), 0x42);
        assert_eq!(mode7.pixel(&vram, 0, 1), 0);
        assert_eq!(mode7.position(5, 1).0, (2 * (5 - 3) + 3) << 8);
    }

    // @source: anomie-regs#211b
    // @source: anomie-regs#211c
    // @source: anomie-regs#211d
    // @source: anomie-regs#211e
    // @source: fullsnes#snesppurotationscaling
    // @source: adr-0032
    #[test]
    fn a_matrix_with_swapped_axes_transposes_the_view() {
        let vram = playfield();
        let mut mode7 = identity();
        write_word(&mut mode7, Mode7::write_matrix_a, 0);
        write_word(&mut mode7, Mode7::write_matrix_b, 0x0100);
        write_word(&mut mode7, Mode7::write_matrix_c, 0x0100);
        write_word(&mut mode7, Mode7::write_matrix_d, 0);
        // Screen (x = 1, line 3) is the playfield's (column 3, row 1).
        assert_eq!(mode7.pixel(&vram, 1, 3), 0x42);
    }

    // @source: anomie-regs#211b
    // @source: anomie-regs#211f
    // @source: fullsnes#snesppurotationscaling
    // @source: adr-0032
    #[test]
    fn products_drop_their_low_six_bits_before_the_sum() {
        let vram = playfield();
        let mut mode7 = identity();
        write_word(&mut mode7, Mode7::write_matrix_a, 63);
        write_word(&mut mode7, Mode7::write_matrix_b, 255);
        // At x = 1, line 1: 0 + 0 + (255 & !63 = 192) + 63 = 255 is below one pixel, so
        // the column is 0 (an unmasked sum, 318, would be column 1).
        assert_eq!(mode7.position(1, 1).0, 255);
        assert_eq!(mode7.pixel(&vram, 1, 1), 0);
    }

    // @source: anomie-regs#211a
    // @source: anomie-regs#211f
    // @source: fullsnes#snesppurotationscaling
    // @source: adr-0032
    #[test]
    fn the_flips_xor_the_screen_coordinate_with_ff() {
        let vram = playfield();
        let mut mode7 = identity();
        mode7.write_settings(0x01);
        // Column 0 shows the playfield's column 255, which is character 0.
        assert_eq!(mode7.position(0, 1).0, 255 << 8);
        mode7.write_settings(0x02);
        assert_eq!(mode7.position(0, 1).1, (1 ^ 0xFF) << 8);
        assert_eq!(mode7.pixel(&vram, 3, 1), 0);
    }

    // @source: anomie-regs#211a
    // @source: anomie-regs#211f
    // @source: fullsnes#snesppurotationscaling
    // @source: adr-0032
    #[test]
    fn outside_the_playfield_wraps_or_is_transparent_or_character_0() {
        let vram = playfield();
        let mut mode7 = identity();
        // M7HOFS -4: column 0 is the playfield's column -4.
        write_word(&mut mode7, Mode7::write_scroll_x, 0x1FFC);
        // Wrapped, column -4 is 1020, in character 1; its row 1, column 4 has no dot, and
        // column 7 is column 1023.
        mode7.write_settings(0x00);
        assert_eq!(mode7.pixel(&vram, 7, 1), 0x42);
        // Transparent outside (bit 7 set, bit 6 clear).
        mode7.write_settings(0x80);
        assert_eq!(mode7.pixel(&vram, 0, 2), 0);
        assert_eq!(mode7.pixel(&vram, 7, 1), 0x42);
        // Character 0, at the low three bits: column -4 is 4 and the row is 2.
        mode7.write_settings(0xC0);
        assert_eq!(mode7.pixel(&vram, 0, 2), 0x99);
    }

    // @source: anomie-regs#210d
    // @source: anomie-regs#211f
    // @source: anomie-regs#2120
    // @source: adr-0032
    #[test]
    fn the_13_bit_registers_sign_extend_and_share_the_latch() {
        let mut mode7 = Mode7::power_on();
        write_word(&mut mode7, Mode7::write_center_x, 0x1FFF);
        write_word(&mut mode7, Mode7::write_center_y, 0x0FFF);
        write_word(&mut mode7, Mode7::write_scroll_x, 0x1000);
        write_word(&mut mode7, Mode7::write_scroll_y, 0xE005);
        assert_eq!(
            (
                mode7.center_x,
                mode7.center_y,
                mode7.scroll_x,
                mode7.scroll_y
            ),
            (-1, 0x0FFF, -0x1000, 5)
        );
        // A byte written to one register is the high byte of the next one's word.
        mode7.write_center_x(0x34);
        mode7.write_center_y(0x02);
        assert_eq!(mode7.center_y, 0x0234);
    }

    // @source: anomie-regs#2133
    // @source: anomie-regs#backgrounds/mode-7
    // @source: fullsnes#snesppubgcontrol
    // @source: adr-0032
    #[test]
    fn bg1_is_the_pixel_and_extbg_adds_bg2_with_bit_7_as_priority() {
        let mut vram = playfield();
        dot(&mut vram, 1, 1, 3, 0xC2);
        let mode7 = identity();
        let one = Sample {
            color: Color::Palette(0xC2),
            high: false,
        };
        let two = Sample {
            color: Color::Palette(0x42),
            high: true,
        };
        assert_eq!(
            mode7.samples(&vram, false, false, 3, 1),
            [Some(one), None, None, None]
        );
        assert_eq!(
            mode7.samples(&vram, false, true, 3, 1),
            [Some(one), Some(two), None, None]
        );
        // A pixel of 0x80 is BG1's colour but transparent for BG2.
        dot(&mut vram, 1, 1, 3, 0x80);
        let samples = mode7.samples(&vram, false, true, 3, 1);
        assert!(samples[0].is_some() && samples[1].is_none());
        // Direct colour is BG1's only, with palette bits 0: $C2 is BBGGGRRR = 11 000 010.
        dot(&mut vram, 1, 1, 3, 0xC2);
        let direct = mode7.samples(&vram, true, true, 3, 1);
        assert_eq!(
            direct[0].map(|sample| sample.color),
            Some(Color::Direct(Rgb555::from_bits_truncate(0x6008)))
        );
        assert_eq!(direct[1], Some(two));
    }
}
