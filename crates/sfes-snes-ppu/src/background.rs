// @source: anomie-regs#2105
// @source: anomie-regs#2107
// @source: anomie-regs#210b
// @source: anomie-regs#210d
// @source: anomie-regs#backgrounds
// @source: fullsnes#snesppubgcontrol
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
// @source: fullsnes#snesppuvideomemoryvram
// @source: adr-0030
// @source: adr-0031
//! BG1 to BG4: their registers, and fetching one pixel of one background in BG modes 0 to
//! 4 (ADR 0030). The PPU composes the pixels into a line.

use crate::register::Register;

/// A VRAM word address has 15 bits (ADR 0029, point 3).
const VRAM_WORD_MASK: u16 = 0x7FFF;
/// A VRAM byte address has 16 bits.
const VRAM_BYTE_MASK: u32 = 0xFFFF;
/// A scroll register holds 10 bits.
const SCROLL_MASK: u16 = 0x03FF;
/// `BGnHOFS`'s write rule keeps the old register's bits 10–8 in the new word's bits 2–0.
const SCROLL_CARRY_MASK: u16 = 0x07;
/// A tilemap screen is 32 tiles wide and high.
const SCREEN_TILES: u16 = 32;
/// Words in one 32×32 tilemap screen.
const SCREEN_WORDS: u16 = SCREEN_TILES * SCREEN_TILES;
/// `$2107–$210A` bit 0: a second screen to the right.
const TILEMAP_WIDE: u8 = 0x01;
/// `$2107–$210A` bit 1: a second screen below.
const TILEMAP_TALL: u8 = 0x02;
/// A tilemap entry's bits 9–0: the tile number.
const TILE_NUMBER_MASK: u16 = 0x03FF;
/// A tilemap entry's bit 13: the tile's priority.
const ENTRY_PRIORITY: u16 = 0x2000;
/// A tilemap entry's bit 14: flip horizontally.
const ENTRY_FLIP_HORIZONTAL: u16 = 0x4000;
/// A tilemap entry's bit 15: flip vertically.
const ENTRY_FLIP_VERTICAL: u16 = 0x8000;
/// An offset-per-tile entry's bits 9–0: the offset.
const OFFSET_MASK: u16 = 0x03FF;
/// A horizontal offset ignores its low 3 bits.
const OFFSET_HORIZONTAL_MASK: u16 = 0x03F8;
/// An offset-per-tile entry's bit 13: the offset applies to BG1.
const OFFSET_BACKGROUND_1: u16 = 0x2000;
/// An offset-per-tile entry's bit 14: the offset applies to BG2.
const OFFSET_BACKGROUND_2: u16 = 0x4000;
/// A mode 4 offset entry's bit 15: the offset is vertical.
const OFFSET_VERTICAL: u16 = 0x8000;
/// Pixels in the column of one offset entry.
const OFFSET_COLUMN: u16 = 8;
/// `$2105` bits 2–0: the BG mode.
const MODE_MASK: u8 = 0x07;
/// `$2105` bit 4 is BG1's character size; each BG's is the next bit up.
const BGMODE_SIZE_SHIFT: u8 = 4;

/// One of the four backgrounds. Each variant's value is its index.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Background {
    /// BG1.
    One = 0,
    /// BG2.
    Two = 1,
    /// BG3.
    Three = 2,
    /// BG4.
    Four = 3,
}

/// How many bitplanes a background's characters have in a mode; each variant's value is the
/// count.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
enum Depth {
    /// 4 colours.
    Two = 2,
    /// 16 colours.
    Four = 4,
    /// 256 colours.
    Eight = 8,
}

/// A background's layer in a mode's priority order: the background, and whether it is its
/// tiles with the priority bit set.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Layer {
    /// Which background.
    pub(crate) background: Background,
    /// Its tiles with `vhopppcc` bit `o` set (`true`) or clear.
    pub(crate) high: bool,
}

// @source: anomie-regs#2105
// @source: adr-0030
/// A layer of BG1 to BG4.
const fn layer(background: Background, high: bool) -> Layer {
    Layer { background, high }
}

use Background::{Four, One, Three, Two};

/// Mode 0, front to back.
const MODE_0: [Layer; 8] = [
    layer(One, true),
    layer(Two, true),
    layer(One, false),
    layer(Two, false),
    layer(Three, true),
    layer(Four, true),
    layer(Three, false),
    layer(Four, false),
];
/// Mode 1 with `$2105` bit 3 clear.
const MODE_1: [Layer; 6] = [
    layer(One, true),
    layer(Two, true),
    layer(One, false),
    layer(Two, false),
    layer(Three, true),
    layer(Three, false),
];
/// Mode 1 with `$2105` bit 3 set: BG3's high tiles in front.
const MODE_1_BG3_FIRST: [Layer; 6] = [
    layer(Three, true),
    layer(One, true),
    layer(Two, true),
    layer(One, false),
    layer(Two, false),
    layer(Three, false),
];
/// Modes 2, 3 and 4.
const TWO_BACKGROUNDS: [Layer; 4] = [
    layer(One, true),
    layer(Two, true),
    layer(One, false),
    layer(Two, false),
];

/// Mode 7: BG1 alone.
const MODE_7: [Layer; 1] = [layer(One, false)];
/// Mode 7 with EXTBG: BG2's high pixels, BG1, then BG2's low pixels.
const MODE_7_EXTBG: [Layer; 3] = [layer(Two, true), layer(One, false), layer(Two, false)];

/// The registers of BG1 to BG4, and the write-twice latch their scrolls share.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Backgrounds {
    /// $2107–$210A BG1SC–BG4SC: each tilemap's address and size.
    tilemaps: [u8; 4],
    /// $210B–$210C BG12NBA and BG34NBA: the four character bases, two to a byte.
    characters: [u8; 2],
    /// $210D, $210F, $2111, $2113 BG1HOFS–BG4HOFS: horizontal scrolls. A register has 10
    /// bits, but keeps the last byte written in bits 15–8, which the next write uses.
    horizontal: [u16; 4],
    /// $210E, $2110, $2112, $2114 BG1VOFS–BG4VOFS: vertical scrolls, as the horizontal.
    vertical: [u16; 4],
    /// The last byte written to any of the eight scroll registers. The Mode 7 registers
    /// have a latch of their own.
    latch: u8,
}

/// What a fetch needs of the registers outside the backgrounds'.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Setup {
    /// $2105 BGMODE: the mode, the BG3 bit and the character sizes.
    pub(crate) control: u8,
    /// $2130 CGWSEL bit 0: direct colour.
    pub(crate) direct_color: bool,
}

/// What a background shows at a pixel: its colour, and its tile's priority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Sample {
    /// The colour.
    pub(crate) color: Color,
    /// The tile's priority bit.
    pub(crate) high: bool,
}

/// Where a pixel's colour comes from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Color {
    /// This CGRAM entry.
    Palette(u8),
    /// This 15-bit colour, from direct colour mode.
    Direct(u16),
}

impl Background {
    /// All four, BG1 first.
    pub(crate) const ALL: [Self; 4] = [One, Two, Three, Four];
}

/// The layers of `mode` from front to back; `bg3_first` is `$2105` bit 3, which moves
/// BG3's high tiles in front in mode 1.
/// Sprites will take their places between them (#94). Modes 5 and 6 draw none yet; `extbg` is `$2133` bit 6, which adds mode 7's BG2.
// @source: anomie-regs#2105
// @source: anomie-regs#backgrounds/mode-0
// @source: anomie-regs#backgrounds/mode-1
// @source: anomie-regs#backgrounds/mode-2
// @source: anomie-regs#backgrounds/mode-3
// @source: anomie-regs#backgrounds/mode-4
// @source: anomie-regs#backgrounds/mode-7
// @source: adr-0030
// @source: adr-0032
pub(crate) const fn priority_order(mode: u8, bg3_first: bool, extbg: bool) -> &'static [Layer] {
    match (mode, bg3_first) {
        (0, _) => &MODE_0,
        (1, true) => &MODE_1_BG3_FIRST,
        (1, false) => &MODE_1,
        (2..=4, _) => &TWO_BACKGROUNDS,
        (7, _) if extbg => &MODE_7_EXTBG,
        (7, _) => &MODE_7,
        _ => &[],
    }
}

/// How many bitplanes `background` has in `mode`; `None` where it has no pixels or the
/// mode is not drawn yet.
// @source: anomie-regs#backgrounds/bg-modes
// @source: fullsnes#snesppubgcontrol
// @source: adr-0030
const fn depth(mode: u8, background: Background) -> Option<Depth> {
    match (mode, background) {
        (0, _) | (1, Three) | (4, Two) => Some(Depth::Two),
        (1 | 2, One | Two) | (3, Two) => Some(Depth::Four),
        (3 | 4, One) => Some(Depth::Eight),
        _ => None,
    }
}

impl Backgrounds {
    /// Every register 0 (ADR 0030, point 1).
    // @source: fullsnes#snesiomap
    // @source: adr-0030
    pub(crate) const fn power_on() -> Self {
        Self {
            tilemaps: [0; 4],
            characters: [0; 2],
            horizontal: [0; 4],
            vertical: [0; 4],
            latch: 0,
        }
    }

    /// A write to one of `$2107–$2114`; any other register does nothing here.
    // @source: anomie-regs#2107
    // @source: anomie-regs#210b
    // @source: anomie-regs#210d
    // @source: fullsnes#snesppubgcontrol
    // @source: adr-0030
    pub(crate) fn write(&mut self, register: Register, value: u8) {
        match register {
            Register::Background1Tilemap => self.tilemaps[Background::One as usize] = value,
            Register::Background2Tilemap => self.tilemaps[Background::Two as usize] = value,
            Register::Background3Tilemap => self.tilemaps[Background::Three as usize] = value,
            Register::Background4Tilemap => self.tilemaps[Background::Four as usize] = value,
            Register::Background12Characters => self.characters[0] = value,
            Register::Background34Characters => self.characters[1] = value,
            Register::Background1HorizontalScroll => self.write_horizontal(Background::One, value),
            Register::Background2HorizontalScroll => self.write_horizontal(Background::Two, value),
            Register::Background3HorizontalScroll => {
                self.write_horizontal(Background::Three, value);
            }
            Register::Background4HorizontalScroll => {
                self.write_horizontal(Background::Four, value);
            }
            Register::Background1VerticalScroll => self.write_vertical(Background::One, value),
            Register::Background2VerticalScroll => self.write_vertical(Background::Two, value),
            Register::Background3VerticalScroll => self.write_vertical(Background::Three, value),
            Register::Background4VerticalScroll => self.write_vertical(Background::Four, value),
            _ => {}
        }
    }

    /// `BGnHOFS`: the new byte over the latch's high five bits and the old register's bits
    /// 9–8 in the low three.
    // @source: anomie-regs#210d
    // @source: fullsnes#snesppubgcontrol
    // @source: adr-0030
    fn write_horizontal(&mut self, background: Background, value: u8) {
        let old = self.horizontal[background as usize];
        let carried = (old >> 8) & SCROLL_CARRY_MASK;
        let word = u16::from(value) << 8 | u16::from(self.latch & 0xF8) | carried;
        self.horizontal[background as usize] = word;
        self.latch = value;
    }

    /// `BGnVOFS`: the new byte over the latch.
    // @source: anomie-regs#210d
    // @source: fullsnes#snesppubgcontrol
    // @source: adr-0030
    fn write_vertical(&mut self, background: Background, value: u8) {
        let word = u16::from_le_bytes([self.latch, value]);
        self.vertical[background as usize] = word;
        self.latch = value;
    }

    /// Where `background`'s characters start: a byte address.
    // @source: anomie-regs#210b
    // @source: fullsnes#snesppubgcontrol
    fn character_base(&self, background: Background) -> u32 {
        let index = background as usize;
        let nibble = self.characters[index / 2] >> (4 * (index % 2)) & 0x0F;
        u32::from(nibble) << 13
    }

    /// The tilemap entry for tile (`tile_x`, `tile_y`) of `background`.
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: fullsnes#snesppubgcontrol
    fn entry(&self, vram: &[u8], background: Background, tile_x: u16, tile_y: u16) -> u16 {
        let register = self.tilemaps[background as usize];
        let wide = register & TILEMAP_WIDE != 0;
        let tall = register & TILEMAP_TALL != 0;
        let mut word = u16::from(register >> 2) << 10;
        word += (tile_y % SCREEN_TILES) * SCREEN_TILES + tile_x % SCREEN_TILES;
        if tall && tile_y >= SCREEN_TILES {
            word += if wide { 2 * SCREEN_WORDS } else { SCREEN_WORDS };
        }
        if wide && tile_x >= SCREEN_TILES {
            word += SCREEN_WORDS;
        }
        vram_word(vram, word)
    }

    /// The horizontal and vertical scroll `background` has at column `x`: its registers',
    /// or in modes 2 and 4 what BG3's entry for the column replaces them with (ADR 0031).
    // @source: anomie-regs#backgrounds/mode-2
    // @source: anomie-regs#backgrounds/mode-4
    // @source: fullsnes#snesppuvideomemoryvram
    // @source: adr-0031
    fn scrolls(&self, vram: &[u8], setup: Setup, background: Background, x: u16) -> (u16, u16) {
        let index = background as usize;
        let horizontal = self.horizontal[index] & SCROLL_MASK;
        let vertical = self.vertical[index] & SCROLL_MASK;
        let mode = setup.control & MODE_MASK;
        let valid = match background {
            Background::One => OFFSET_BACKGROUND_1,
            Background::Two => OFFSET_BACKGROUND_2,
            _ => return (horizontal, vertical),
        };
        let tile = (x + (horizontal & 7)) / OFFSET_COLUMN;
        if !matches!(mode, 2 | 4) || tile == 0 {
            return (horizontal, vertical);
        }
        let third = Background::Three as usize;
        let across = (self.horizontal[third] & OFFSET_HORIZONTAL_MASK) + (tile - 1) * OFFSET_COLUMN;
        let down = self.vertical[third] & SCROLL_MASK;
        let first = self.pixel_entry(vram, setup, Background::Three, across, down);
        let (horizontal_entry, vertical_entry) = if mode == 2 {
            let second =
                self.pixel_entry(vram, setup, Background::Three, across, down + OFFSET_COLUMN);
            (first, second)
        } else if first & OFFSET_VERTICAL != 0 {
            (0, first)
        } else {
            (first, 0)
        };
        let horizontal = if horizontal_entry & valid != 0 {
            (horizontal & 7) | (horizontal_entry & OFFSET_HORIZONTAL_MASK)
        } else {
            horizontal
        };
        let vertical = if vertical_entry & valid != 0 {
            vertical_entry & OFFSET_MASK
        } else {
            vertical
        };
        (horizontal, vertical)
    }

    /// The tilemap entry of `background` under playfield pixel (`across`, `down`), wrapped at
    /// the playfield's edges.
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: anomie-regs#backgrounds/bg-scrolling
    // @source: adr-0031
    fn pixel_entry(
        &self,
        vram: &[u8],
        setup: Setup,
        background: Background,
        across: u16,
        down: u16,
    ) -> u16 {
        let size: u16 = if setup.control >> (BGMODE_SIZE_SHIFT + background as u8) & 1 != 0 {
            16
        } else {
            8
        };
        let tilemap = self.tilemaps[background as usize];
        let columns = if tilemap & TILEMAP_WIDE != 0 { 64 } else { 32 };
        let rows = if tilemap & TILEMAP_TALL != 0 { 64 } else { 32 };
        self.entry(
            vram,
            background,
            across % (columns * size) / size,
            down % (rows * size) / size,
        )
    }

    /// What `background` shows at column `x` of `line`, or `None` where it is transparent
    /// or not drawn in this mode: scrolled by its registers, wrapped at its edges.
    // @source: anomie-regs#backgrounds/bg-scrolling
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: anomie-regs#backgrounds/direct-color-mode
    // @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
    // @source: adr-0030
    // @source: adr-0031
    pub(crate) fn sample(
        &self,
        vram: &[u8],
        setup: Setup,
        background: Background,
        x: u16,
        line: u16,
    ) -> Option<Sample> {
        let mode = setup.control & MODE_MASK;
        let depth = depth(mode, background)?;
        let index = background as usize;
        let size: u16 = if setup.control >> (BGMODE_SIZE_SHIFT + background as u8) & 1 != 0 {
            16
        } else {
            8
        };
        let tilemap = self.tilemaps[index];
        let columns = if tilemap & TILEMAP_WIDE != 0 { 64 } else { 32 };
        let rows = if tilemap & TILEMAP_TALL != 0 { 64 } else { 32 };
        let (horizontal, vertical) = self.scrolls(vram, setup, background, x);
        let across = (x + horizontal) % (columns * size);
        let down = (line + vertical) % (rows * size);

        let entry = self.entry(vram, background, across / size, down / size);
        let mut column = across % size;
        let mut row = down % size;
        if entry & ENTRY_FLIP_HORIZONTAL != 0 {
            column = size - 1 - column;
        }
        if entry & ENTRY_FLIP_VERTICAL != 0 {
            row = size - 1 - row;
        }
        // A 16×16 tile is the tile, the next, and the two a row of 16 on.
        let tile = ((entry & TILE_NUMBER_MASK) + (row / 8) * 16 + column / 8) & TILE_NUMBER_MASK;

        let planes = u32::from(depth as u8);
        let start = self.character_base(background) + u32::from(tile) * 8 * planes;
        let row_start = start + u32::from(row % 8) * 2;
        let shift = 7 - column % 8;
        let mut value: u8 = 0;
        for plane in 0..planes {
            // Planes come in pairs, a byte each in a word, a pair to every 16 bytes.
            let address = row_start + (plane / 2) * 16 + plane % 2;
            let byte = vram_byte(vram, address);
            value |= (byte >> shift & 1) << plane;
        }
        if value == 0 {
            return None;
        }

        let palette = u8::try_from(entry >> 10 & 7).unwrap_or_default();
        let color = match depth {
            Depth::Eight if setup.direct_color => Color::Direct(direct_color(value, palette)),
            Depth::Eight => Color::Palette(value),
            Depth::Four => Color::Palette(palette * 16 + value),
            Depth::Two if mode == 0 => {
                Color::Palette(index_offset(background) + palette * 4 + value)
            }
            Depth::Two => Color::Palette(palette * 4 + value),
        };
        Some(Sample {
            color,
            high: entry & ENTRY_PRIORITY != 0,
        })
    }
}

/// Mode 0's palette entries of `background`: 32 for each background before it.
// @source: anomie-regs#backgrounds/mode-0
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
const fn index_offset(background: Background) -> u8 {
    background as u8 * 32
}

/// A direct colour: character data `BBGGGRRR` and palette bits `bgr` make
/// `BBb00:RRRr0:GGGg0`.
// @source: anomie-regs#backgrounds/direct-color-mode
// @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
pub(crate) fn direct_color(value: u8, palette: u8) -> u16 {
    let red = u16::from(value & 0x07) << 2 | u16::from(palette & 1) << 1;
    let green = u16::from(value >> 3 & 0x07) << 2 | u16::from(palette >> 1 & 1) << 1;
    let blue = u16::from(value >> 6) << 3 | u16::from(palette >> 2 & 1) << 2;
    blue << 10 | green << 5 | red
}

/// VRAM word `address`.
// @source: anomie-regs#backgrounds/tile-maps-and-character-maps
fn vram_word(vram: &[u8], address: u16) -> u16 {
    let byte = usize::from(address & VRAM_WORD_MASK) * 2;
    let low = vram.get(byte).copied().unwrap_or_default();
    let high = vram.get(byte + 1).copied().unwrap_or_default();
    u16::from_le_bytes([low, high])
}

/// VRAM byte `address`, wrapped to 64 KiB.
// @source: anomie-regs#backgrounds/tile-maps-and-character-maps
pub(crate) fn vram_byte(vram: &[u8], address: u32) -> u8 {
    let index = usize::try_from(address & VRAM_BYTE_MASK).unwrap_or_default();
    vram.get(index).copied().unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::*;

    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: adr-0030
    /// A VRAM image of 64 KiB.
    fn vram() -> Vec<u8> {
        vec![0; 0x1_0000]
    }

    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: adr-0030
    /// The setup for `mode` with `$2105`'s other bits clear.
    const fn setup(mode: u8) -> Setup {
        Setup {
            control: mode,
            direct_color: false,
        }
    }

    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: adr-0030
    /// Writes the 2-bit tile `number` at character base 0 with its first row `pixels`.
    fn two_bit_row(vram: &mut [u8], number: usize, row: usize, plane0: u8, plane1: u8) {
        vram[number * 16 + row * 2] = plane0;
        vram[number * 16 + row * 2 + 1] = plane1;
    }

    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: adr-0030
    #[test]
    fn a_2_bit_tile_reads_its_planes_left_pixel_in_bit_7() {
        let mut vram = vram();
        let mut backgrounds = Backgrounds::power_on();
        // Tilemap at word $0400, characters at 0; tile 1, palette 2.
        backgrounds.write(Register::Background1Tilemap, 0x04);
        vram[0x800] = 0x01;
        vram[0x801] = 0x08;
        // Pixels 0 to 3 are colours 1, 2, 3, 0 and the rest 0.
        two_bit_row(&mut vram, 1, 0, 0b1010_0000, 0b0110_0000);
        let sample = |x| backgrounds.sample(&vram, setup(0), Background::One, x, 0);
        assert_eq!(sample(0).map(|s| s.color), Some(Color::Palette(8 + 1)));
        assert_eq!(sample(1).map(|s| s.color), Some(Color::Palette(8 + 2)));
        assert_eq!(sample(2).map(|s| s.color), Some(Color::Palette(8 + 3)));
        assert_eq!(sample(3), None);
        assert_eq!(sample(0).map(|s| s.high), Some(false));
    }

    // @source: anomie-regs#backgrounds/mode-0
    // @source: anomie-regs#backgrounds/mode-1
    // @source: anomie-regs#backgrounds/mode-3
    // @source: anomie-regs#backgrounds/mode-4
    // @source: adr-0030
    #[test]
    fn palettes_start_where_the_mode_says() {
        let mut vram = vram();
        let mut backgrounds = Backgrounds::power_on();
        // Every background's map at $0400 words, base 0; tile 0 with palette 3, row 0
        // colour 1 in pixel 0 of every plane that exists.
        for register in [
            Register::Background1Tilemap,
            Register::Background2Tilemap,
            Register::Background3Tilemap,
            Register::Background4Tilemap,
        ] {
            backgrounds.write(register, 0x04);
        }
        vram[0x801] = 0x0C;
        for plane in [0, 1, 16, 17, 32, 33, 48, 49] {
            vram[plane] = 0x80;
        }
        let color = |mode, background| {
            backgrounds
                .sample(&vram, setup(mode), background, 0, 0)
                .map(|s| s.color)
        };
        // Mode 0: palette * 4 and 32 for each background before (data 3 = both planes).
        assert_eq!(color(0, Background::One), Some(Color::Palette(12 + 3)));
        assert_eq!(
            color(0, Background::Four),
            Some(Color::Palette(96 + 12 + 3))
        );
        // Mode 1: BG1 has 16 colours, BG3 has 4; planes 0 and 1 and 2 and 3 are all set.
        assert_eq!(color(1, Background::One), Some(Color::Palette(48 + 15)));
        assert_eq!(color(1, Background::Three), Some(Color::Palette(12 + 3)));
        // Mode 3: BG1 is 256 colours with no palette; BG2 has 16.
        assert_eq!(color(3, Background::One), Some(Color::Palette(0xFF)));
        assert_eq!(color(3, Background::Two), Some(Color::Palette(48 + 15)));
        // Mode 4: BG2 has 4 colours; the others are absent.
        assert_eq!(color(4, Background::Two), Some(Color::Palette(12 + 3)));
        assert_eq!(color(4, Background::Three), None);
        // Modes 5 to 7 are not drawn yet.
        assert_eq!(color(5, Background::One), None);
    }

    // @source: anomie-regs#backgrounds/direct-color-mode
    // @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
    // @source: adr-0030
    #[test]
    fn direct_colour_spreads_the_character_and_palette_bits() {
        // Character data BBGGGRRR = 11 101 110 and palette bgr = 5 (b and r).
        assert_eq!(direct_color(0b1110_1110, 0b101), 0x729A);
        // The least-significant bit of each channel is the palette's, the bottom one 0.
        assert_eq!(direct_color(0x00, 0b111), 0b00100_00010_00010);
        assert_eq!(direct_color(0xFF, 0), 0b11000_11100_11100);

        let mut vram = vram();
        let mut backgrounds = Backgrounds::power_on();
        backgrounds.write(Register::Background1Tilemap, 0x04);
        // Palette 7, and an 8-plane tile 0 whose pixel 0 is $FF.
        vram[0x801] = 0x1C;
        for plane in 0..8 {
            vram[plane / 2 * 16 + plane % 2] = 0x80;
        }
        let direct = Setup {
            control: 3,
            direct_color: true,
        };
        let sample = backgrounds.sample(&vram, direct, Background::One, 0, 0);
        assert_eq!(sample.map(|s| s.color), Some(Color::Direct(0x73DE)));
    }

    // @source: anomie-regs#backgrounds/bg-scrolling
    // @source: adr-0030
    #[test]
    fn scroll_registers_are_write_twice_with_a_shared_latch() {
        let mut backgrounds = Backgrounds::power_on();
        backgrounds.write(Register::Background1VerticalScroll, 0x34);
        backgrounds.write(Register::Background1VerticalScroll, 0x12);
        // BGnVOFS = new << 8 | previous, 10 bits.
        assert_eq!(backgrounds.vertical[0] & SCROLL_MASK, 0x0234);
        // The latch is shared by the eight registers: BG2's V takes BG1's last byte.
        backgrounds.write(Register::Background2VerticalScroll, 0x07);
        assert_eq!(backgrounds.vertical[1] & SCROLL_MASK, 0x0312);
        // BGnHOFS keeps the previous byte's bits 7-3 and the old register's bits 10-8, so
        // two writes of $FF then $01 make $1FF.
        backgrounds.write(Register::Background3HorizontalScroll, 0xFF);
        backgrounds.write(Register::Background3HorizontalScroll, 0x01);
        assert_eq!(backgrounds.horizontal[2] & SCROLL_MASK, 0x01FF);
        backgrounds.write(Register::Background3HorizontalScroll, 0x00);
        assert_eq!(backgrounds.horizontal[2] & SCROLL_MASK, 0x0001);
    }

    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: anomie-regs#backgrounds/bg-scrolling
    // @source: fullsnes#snesppubgcontrol
    // @source: adr-0030
    #[test]
    fn tilemap_screens_follow_the_size_bits_and_wrap_at_the_edges() {
        let mut vram = vram();
        let mut backgrounds = Backgrounds::power_on();
        // 64x64 tiles at word $0400: four screens A B / C D, each $400 words.
        backgrounds.write(Register::Background1Tilemap, 0x04 | 0x03);
        let mark = |vram: &mut [u8], word: usize, tile: u8| vram[word * 2] = tile;
        mark(&mut vram, 0x400, 1); // A: tile (0, 0)
        mark(&mut vram, 0x400 + 0x0400 + 5, 2); // B: tile (32 + 5, 0)
        mark(&mut vram, 0x400 + 0x0800 + 3 * 32, 3); // C: tile (0, 32 + 3)
        mark(&mut vram, 0x400 + 0x0C00 + 33, 4); // D: tile (33, 33)
        let entry = |x, y| backgrounds.entry(&vram, Background::One, x, y) & 0xFF;
        assert_eq!(
            [entry(0, 0), entry(37, 0), entry(0, 35), entry(33, 33)],
            [1, 2, 3, 4]
        );
        // Only the width bit: two screens side by side, the second at +$400.
        backgrounds.write(Register::Background1Tilemap, 0x04 | 0x01);
        assert_eq!(backgrounds.entry(&vram, Background::One, 37, 0) & 0xFF, 2);
        // Only the height bit: the second screen is below, also at +$400.
        backgrounds.write(Register::Background1Tilemap, 0x04 | 0x02);
        assert_eq!(backgrounds.entry(&vram, Background::One, 5, 32) & 0xFF, 2);

        // A scroll past the edge wraps: 64 tiles of 8 pixels are 512 pixels.
        backgrounds.write(Register::Background1Tilemap, 0x04 | 0x03);
        two_bit_row(&mut vram, 1, 0, 0x80, 0x00);
        backgrounds.write(Register::Background1HorizontalScroll, 0xFF);
        backgrounds.write(Register::Background1HorizontalScroll, 0x01);
        // HOFS 511: pixel 0 is column 511, the last of the map; pixel 1 wraps to 0.
        let at = |x| backgrounds.sample(&vram, setup(0), Background::One, x, 0);
        assert_eq!(at(1).map(|s| s.color), Some(Color::Palette(1)));
        assert_eq!(at(0), None);
    }

    // @source: anomie-regs#2105
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: adr-0030
    #[test]
    fn a_16_by_16_tile_is_four_tiles_and_flips_as_one() {
        let mut vram = vram();
        let mut backgrounds = Backgrounds::power_on();
        backgrounds.write(Register::Background1Tilemap, 0x04);
        // Tile 1 as 16x16: tiles 1, 2, $11 and $12.
        vram[0x800] = 0x01;
        two_bit_row(&mut vram, 1, 0, 0x80, 0x00); // (0, 0): colour 1
        two_bit_row(&mut vram, 2, 0, 0x00, 0x80); // (8, 0): colour 2
        two_bit_row(&mut vram, 0x11, 0, 0x80, 0x80); // (0, 8): colour 3
        two_bit_row(&mut vram, 0x12, 7, 0x01, 0x00); // (15, 15): colour 1
        let large = Setup {
            control: 0x10,
            direct_color: false,
        };
        let at = |x, line| {
            backgrounds
                .sample(&vram, large, Background::One, x, line)
                .map(|s| s.color)
        };
        assert_eq!(at(0, 0), Some(Color::Palette(1)));
        assert_eq!(at(8, 0), Some(Color::Palette(2)));
        assert_eq!(at(0, 8), Some(Color::Palette(3)));
        assert_eq!(at(15, 15), Some(Color::Palette(1)));
        assert_eq!(at(1, 0), None);
        // Flipped both ways, the whole 16x16 is turned: (x, y) shows (15 - x, 15 - y).
        vram[0x801] = 0xC0;
        let at = |x, line| {
            backgrounds
                .sample(&vram, large, Background::One, x, line)
                .map(|s| s.color)
        };
        assert_eq!(at(15, 15), Some(Color::Palette(1)));
        assert_eq!(at(7, 15), Some(Color::Palette(2)));
        assert_eq!(at(15, 7), Some(Color::Palette(3)));
        assert_eq!(at(0, 0), Some(Color::Palette(1)));
    }

    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: adr-0031
    /// Sets VRAM word `address`.
    fn put(vram: &mut [u8], address: usize, entry: u16) {
        let [low, high] = entry.to_le_bytes();
        vram[address * 2] = low;
        vram[address * 2 + 1] = high;
    }

    // @source: anomie-regs#2105
    // @source: anomie-regs#backgrounds/mode-2
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: fullsnes#snesppuvideomemoryvram
    // @source: adr-0031
    #[test]
    fn mode_2_takes_each_columns_offsets_from_the_bg3_entry_one_column_before() {
        let mut vram = vram();
        let mut backgrounds = Backgrounds::power_on();
        backgrounds.write(Register::Background1Tilemap, 0x04); // word $0400
        backgrounds.write(Register::Background2Tilemap, 0x0C); // word $0C00
        backgrounds.write(Register::Background3Tilemap, 0x08); // word $0800
        // 4-bit tiles at base 0, 32 bytes each: tile 1 colour 1, tile 2 colour 2.
        vram[32] = 0xFF;
        vram[64 + 1] = 0xFF;
        for column in 0..32 {
            let palette = (column & 7) << 10;
            put(&mut vram, 0x400 + column as usize, palette | 1);
            put(&mut vram, 0x420 + column as usize, palette | 2);
            put(&mut vram, 0xC00 + column as usize, palette | 1);
        }
        // BG3 column 0 (for BG1 and BG2's tile 1): BG1 only, 16 right ($15's low bits are
        // ignored), and 8 down in its next row. Column 1 (tile 2): BG2 only, 8 right.
        put(&mut vram, 0x800, 0x2000 | 0x15);
        put(&mut vram, 0x820, 0x2000 | 8);
        put(&mut vram, 0x801, 0x4000 | 8);
        let large = setup(2);
        let at = |backgrounds: &Backgrounds, background, x, line| {
            backgrounds
                .sample(&vram, large, background, x, line)
                .map(|s| s.color)
        };
        // Tile 0 is as normal; tile 1 of BG1: column 3 of the row below.
        assert_eq!(
            at(&backgrounds, Background::One, 0, 0),
            Some(Color::Palette(1))
        );
        assert_eq!(
            at(&backgrounds, Background::One, 8, 0),
            Some(Color::Palette(3 * 16 + 2))
        );
        assert_eq!(
            at(&backgrounds, Background::One, 15, 0),
            Some(Color::Palette(3 * 16 + 2))
        );
        // BG1's tile 2 has no valid bit set in its entry, and BG2's tile 1 has bit 14 clear.
        assert_eq!(
            at(&backgrounds, Background::One, 16, 0),
            Some(Color::Palette(2 * 16 + 1))
        );
        assert_eq!(
            at(&backgrounds, Background::Two, 8, 0),
            Some(Color::Palette(16 + 1))
        );
        // BG2's tile 2 is 8 right: column 3, and keeps its own vertical scroll.
        assert_eq!(
            at(&backgrounds, Background::Two, 16, 0),
            Some(Color::Palette(3 * 16 + 1))
        );
        // BG3 is not drawn in mode 2.
        assert_eq!(at(&backgrounds, Background::Three, 8, 0), None);
        // BG1HOFS = 3 moves the tile boundaries: pixel 4 is still tile 0, pixel 5 starts
        // tile 1, whose horizontal scroll is 16 with the low bits 3: column 3.
        backgrounds.horizontal[0] = 3;
        let at = |x| {
            backgrounds
                .sample(&vram, large, Background::One, x, 0)
                .map(|s| s.color)
        };
        assert_eq!(at(4), Some(Color::Palette(1)));
        assert_eq!(at(5), Some(Color::Palette(3 * 16 + 2)));
        assert_eq!(at(12), Some(Color::Palette(3 * 16 + 2)));
        assert_eq!(at(13), Some(Color::Palette(2 * 16 + 1)));
    }

    // @source: anomie-regs#backgrounds/mode-4
    // @source: fullsnes#snesppuvideomemoryvram
    // @source: adr-0031
    #[test]
    fn mode_4_takes_one_entry_per_column_for_either_scroll() {
        let mut vram = vram();
        let mut backgrounds = Backgrounds::power_on();
        backgrounds.write(Register::Background2Tilemap, 0x04);
        backgrounds.write(Register::Background3Tilemap, 0x08);
        two_bit_row(&mut vram, 1, 0, 0xFF, 0x00); // tile 1: colour 1
        two_bit_row(&mut vram, 2, 0, 0x00, 0xFF); // tile 2: colour 2
        for column in 0..32 {
            let palette = (column & 7) << 10;
            put(&mut vram, 0x400 + column as usize, palette | 1);
            put(&mut vram, 0x420 + column as usize, palette | 2);
        }
        // Column 0: a horizontal offset of 16; column 1: bit 15, a vertical offset of 8.
        put(&mut vram, 0x800, 0x4000 | 16);
        put(&mut vram, 0x801, 0xC000 | 8);
        let at = |x| {
            backgrounds
                .sample(&vram, setup(4), Background::Two, x, 0)
                .map(|s| s.color)
        };
        assert_eq!(at(0), Some(Color::Palette(1)));
        assert_eq!(at(8), Some(Color::Palette(3 * 4 + 1)));
        // The vertical entry leaves the horizontal scroll alone: column 2, the row below.
        assert_eq!(at(16), Some(Color::Palette(2 * 4 + 2)));
        // BG1 has no valid bit anywhere in these entries; BG3 shows nothing in mode 4.
        assert_eq!(
            backgrounds.sample(&vram, setup(4), Background::Three, 8, 0),
            None
        );
    }

    // @source: anomie-regs#2105
    // @source: anomie-regs#backgrounds/mode-0
    // @source: anomie-regs#backgrounds/mode-1
    // @source: anomie-regs#backgrounds/mode-2
    // @source: adr-0030
    #[test]
    fn the_priority_order_follows_the_mode_and_mode_1_s_bg3_bit() {
        let names = |mode, bg3_first| {
            priority_order(mode, bg3_first, false)
                .iter()
                .map(|layer| (layer.background as u8 + 1, layer.high))
                .collect::<Vec<_>>()
        };
        // 3AB2ab1CD0cd without the sprites.
        assert_eq!(
            names(0, false),
            [
                (1, true),
                (2, true),
                (1, false),
                (2, false),
                (3, true),
                (4, true),
                (3, false),
                (4, false)
            ]
        );
        // 3AB2ab1C 0c, and with the bit C3AB2ab1 0c.
        assert_eq!(
            names(1, false),
            [
                (1, true),
                (2, true),
                (1, false),
                (2, false),
                (3, true),
                (3, false)
            ]
        );
        assert_eq!(
            names(1, true),
            [
                (3, true),
                (1, true),
                (2, true),
                (1, false),
                (2, false),
                (3, false)
            ]
        );
        // 3A 2B 1a 0b.
        for mode in 2..=4 {
            assert_eq!(
                names(mode, false),
                [(1, true), (2, true), (1, false), (2, false)]
            );
        }
        assert_eq!(priority_order(5, false, false), &[] as &[Layer]);
    }
}
