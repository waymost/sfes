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
// @source: adr-0036
//! BG1 to BG4: their registers, and fetching one pixel of one background in BG modes 0 to
//! 6 (ADR 0030; ADR 0031; ADR 0036). The PPU composes the pixels into a line; mode 7 has its
//! own fetch.

use sfes_api::Rgb555;

use crate::{memory::Vram, register::Register, sprite::Priority};

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
/// A tilemap entry's bits 12–10: the palette.
const ENTRY_PALETTE_SHIFT: u16 = 10;
/// A palette number has 3 bits.
const ENTRY_PALETTE_MASK: u16 = 0x07;
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
/// `$2105` bit 3: BG3's high tiles in front in mode 1.
const BACKGROUND_3_FIRST: u8 = 0x08;
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

/// A BG mode, `$2105` bits 2–0. Each variant's value is the mode's number.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum Mode {
    /// Four 2-bit backgrounds.
    Zero = 0,
    /// Two 4-bit backgrounds and a 2-bit one.
    One = 1,
    /// Two 4-bit backgrounds with offset-per-tile.
    Two = 2,
    /// An 8-bit and a 4-bit background.
    Three = 3,
    /// An 8-bit and a 2-bit background with offset-per-tile.
    Four = 4,
    /// Hi-res: a 4-bit and a 2-bit background.
    Five = 5,
    /// Hi-res: one 4-bit background with offset-per-tile.
    Six = 6,
    /// One rotated and scaled 8-bit background.
    Seven = 7,
}

impl Mode {
    /// The mode `$2105` bits 2–0 name.
    // @source: anomie-regs#2105
    const fn from_bits(value: u8) -> Self {
        match value & MODE_MASK {
            0 => Self::Zero,
            1 => Self::One,
            2 => Self::Two,
            3 => Self::Three,
            4 => Self::Four,
            5 => Self::Five,
            6 => Self::Six,
            _ => Self::Seven,
        }
    }

    /// Whether the mode is true hi-res: 5 and 6 (ADR 0036, point 1).
    // @source: anomie-regs#backgrounds/mode-5
    // @source: anomie-regs#backgrounds/mode-6
    // @source: adr-0036
    pub(crate) const fn hires(self) -> bool {
        matches!(self, Self::Five | Self::Six)
    }

    /// Whether BG3 supplies offsets per tile: modes 2, 4 and 6 (ADR 0031; ADR 0036,
    /// point 9).
    // @source: anomie-regs#backgrounds/mode-2
    // @source: anomie-regs#backgrounds/mode-4
    // @source: anomie-regs#backgrounds/mode-6
    // @source: adr-0031
    // @source: adr-0036
    const fn offset_per_tile(self) -> bool {
        matches!(self, Self::Two | Self::Four | Self::Six)
    }
}

/// `$2105 BGMODE`, decoded: the mode, mode 1's BG3 bit and the four character sizes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BackgroundMode {
    /// Bits 2–0: the BG mode.
    pub(crate) mode: Mode,
    /// Bit 3: in mode 1, BG3's high tiles in front of everything.
    pub(crate) bg3_first: bool,
    /// Bits 7–4: BG4 to BG1 have 16×16 characters, indexed by [`Background`].
    large: [bool; 4],
}

impl From<u8> for BackgroundMode {
    // @source: anomie-regs#2105
    // @source: fullsnes#snesppubgcontrol
    fn from(value: u8) -> Self {
        let large = Background::ALL
            .map(|background| value >> (BGMODE_SIZE_SHIFT + background as u8) & 1 != 0);
        Self {
            mode: Mode::from_bits(value),
            bg3_first: value & BACKGROUND_3_FIRST != 0,
            large,
        }
    }
}

impl BackgroundMode {
    /// The character size of `background`: 16 where its bit is set, else 8.
    // @source: anomie-regs#2105
    // @source: fullsnes#snesppubgcontrol
    const fn character_size(self, background: Background) -> u16 {
        if self.large[background as usize] {
            16
        } else {
            8
        }
    }
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

/// A layer in a mode's priority order: a background's tiles with or without their priority
/// bit, or the sprites of one priority.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Layer {
    /// A background's tiles with `vhopppcc` bit `o` set (`true`) or clear.
    Background {
        /// Which background.
        background: Background,
        /// Whether it is its high-priority tiles.
        high: bool,
    },
    /// The sprites whose OAM priority bits are this.
    Sprites(Priority),
}

/// A layer of BG1 to BG4.
// @source: anomie-regs#2105
// @source: adr-0030
const fn layer(background: Background, high: bool) -> Layer {
    Layer::Background { background, high }
}
/// Mode 0, front to back (anomie-regs#backgrounds/mode-0).
const MODE_0: [Layer; 12] = [
    Layer::Sprites(Priority::Three),
    layer(Background::One, true),
    layer(Background::Two, true),
    Layer::Sprites(Priority::Two),
    layer(Background::One, false),
    layer(Background::Two, false),
    Layer::Sprites(Priority::One),
    layer(Background::Three, true),
    layer(Background::Four, true),
    Layer::Sprites(Priority::Zero),
    layer(Background::Three, false),
    layer(Background::Four, false),
];
/// Mode 1 with `$2105` bit 3 clear.
const MODE_1: [Layer; 10] = [
    Layer::Sprites(Priority::Three),
    layer(Background::One, true),
    layer(Background::Two, true),
    Layer::Sprites(Priority::Two),
    layer(Background::One, false),
    layer(Background::Two, false),
    Layer::Sprites(Priority::One),
    layer(Background::Three, true),
    Layer::Sprites(Priority::Zero),
    layer(Background::Three, false),
];
/// Mode 1 with `$2105` bit 3 set: BG3's high tiles in front.
const MODE_1_BG3_FIRST: [Layer; 10] = [
    layer(Background::Three, true),
    Layer::Sprites(Priority::Three),
    layer(Background::One, true),
    layer(Background::Two, true),
    Layer::Sprites(Priority::Two),
    layer(Background::One, false),
    layer(Background::Two, false),
    Layer::Sprites(Priority::One),
    Layer::Sprites(Priority::Zero),
    layer(Background::Three, false),
];
/// Modes 2 to 5.
const TWO_BACKGROUNDS: [Layer; 8] = [
    Layer::Sprites(Priority::Three),
    layer(Background::One, true),
    Layer::Sprites(Priority::Two),
    layer(Background::Two, true),
    Layer::Sprites(Priority::One),
    layer(Background::One, false),
    Layer::Sprites(Priority::Zero),
    layer(Background::Two, false),
];
/// Mode 6: BG1 alone.
const MODE_6: [Layer; 6] = [
    Layer::Sprites(Priority::Three),
    layer(Background::One, true),
    Layer::Sprites(Priority::Two),
    Layer::Sprites(Priority::One),
    layer(Background::One, false),
    Layer::Sprites(Priority::Zero),
];

/// Mode 7: BG1 alone.
const MODE_7: [Layer; 5] = [
    Layer::Sprites(Priority::Three),
    Layer::Sprites(Priority::Two),
    Layer::Sprites(Priority::One),
    layer(Background::One, false),
    Layer::Sprites(Priority::Zero),
];
/// Mode 7 with EXTBG: BG2's high pixels, BG1, then BG2's low pixels.
const MODE_7_EXTBG: [Layer; 7] = [
    Layer::Sprites(Priority::Three),
    Layer::Sprites(Priority::Two),
    layer(Background::Two, true),
    Layer::Sprites(Priority::One),
    layer(Background::One, false),
    Layer::Sprites(Priority::Zero),
    layer(Background::Two, false),
];

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

/// A tilemap entry, `vhopppcc cccccccc`: flips, priority, palette and tile number.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TilemapEntry(u16);

impl TilemapEntry {
    /// Bits 9–0: the tile number.
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    const fn tile(self) -> u16 {
        self.0 & TILE_NUMBER_MASK
    }

    /// Bits 12–10: the palette, 0 to 7.
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    const fn palette(self) -> u8 {
        (self.0 >> ENTRY_PALETTE_SHIFT & ENTRY_PALETTE_MASK) as u8
    }

    /// Bit 13: the tile's priority.
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    const fn high(self) -> bool {
        self.0 & ENTRY_PRIORITY != 0
    }

    /// Bit 14: flipped horizontally.
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    const fn flip_horizontal(self) -> bool {
        self.0 & ENTRY_FLIP_HORIZONTAL != 0
    }

    /// Bit 15: flipped vertically.
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    const fn flip_vertical(self) -> bool {
        self.0 & ENTRY_FLIP_VERTICAL != 0
    }
}

/// What a fetch needs of the registers outside the backgrounds'.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Setup {
    /// $2105 BGMODE: the mode, the BG3 bit and the character sizes.
    pub(crate) background_mode: BackgroundMode,
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

/// Which half-pixel of each pair a hi-res tile's column shows: the main screen shows the
/// odd ones and the sub screen the even ones (ADR 0036, point 3). Each variant's value is
/// the half-pixel's place in its pair.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum HalfPixel {
    /// The left half-pixel, the sub screen's.
    Even = 0,
    /// The right half-pixel, the main screen's.
    Odd = 1,
}

/// Where a pixel's colour comes from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Color {
    /// This CGRAM entry.
    Palette(u8),
    /// This 15-bit colour, from direct colour mode.
    Direct(Rgb555),
}

impl Background {
    /// All four, BG1 first.
    pub(crate) const ALL: [Self; 4] = [Self::One, Self::Two, Self::Three, Self::Four];
}

/// The layers of `background_mode` from front to back, sprites among them (ADR 0033,
/// point 6): `$2105` bit 3 moves BG3's high tiles in front in mode 1, and `extbg`, `$2133`
/// bit 6, adds mode 7's BG2.
// @source: anomie-regs#2105
// @source: anomie-regs#backgrounds/mode-0
// @source: anomie-regs#backgrounds/mode-1
// @source: anomie-regs#backgrounds/mode-2
// @source: anomie-regs#backgrounds/mode-3
// @source: anomie-regs#backgrounds/mode-4
// @source: anomie-regs#backgrounds/mode-5
// @source: anomie-regs#backgrounds/mode-6
// @source: anomie-regs#backgrounds/mode-7
// @source: adr-0030
// @source: adr-0032
// @source: adr-0033
pub(crate) const fn priority_order(
    background_mode: BackgroundMode,
    extbg: bool,
) -> &'static [Layer] {
    match background_mode.mode {
        Mode::Zero => &MODE_0,
        Mode::One if background_mode.bg3_first => &MODE_1_BG3_FIRST,
        Mode::One => &MODE_1,
        Mode::Two | Mode::Three | Mode::Four | Mode::Five => &TWO_BACKGROUNDS,
        Mode::Six => &MODE_6,
        Mode::Seven if extbg => &MODE_7_EXTBG,
        Mode::Seven => &MODE_7,
    }
}

/// How many bitplanes `background` has in `mode`; `None` where it has no pixels, and in
/// mode 7, whose pixels `mode7.rs` fetches.
// @source: anomie-regs#backgrounds/bg-modes
// @source: fullsnes#snesppubgcontrol
// @source: adr-0030
// @source: adr-0036
const fn depth(mode: Mode, background: Background) -> Option<Depth> {
    match (mode, background) {
        (Mode::Zero, _)
        | (Mode::One, Background::Three)
        | (Mode::Four | Mode::Five, Background::Two) => Some(Depth::Two),
        (Mode::One | Mode::Two, Background::One | Background::Two)
        | (Mode::Three, Background::Two)
        | (Mode::Five | Mode::Six, Background::One) => Some(Depth::Four),
        (Mode::Three | Mode::Four, Background::One) => Some(Depth::Eight),
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
    /// 10–8 in the low three (ADR 0030, point 2).
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
    fn entry(&self, vram: &Vram, background: Background, tile_x: u16, tile_y: u16) -> u16 {
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
        vram.word(word)
    }

    /// The horizontal and vertical scroll `background` has at column `x`: its registers',
    /// or in modes 2, 4 and 6 what BG3's entry for the column replaces them with (ADR 0031;
    /// ADR 0036, point 9).
    // @source: anomie-regs#backgrounds/mode-2
    // @source: anomie-regs#backgrounds/mode-4
    // @source: anomie-regs#backgrounds/mode-6
    // @source: fullsnes#snesppuvideomemoryvram
    // @source: adr-0031
    // @source: adr-0036
    fn scrolls(&self, vram: &Vram, setup: Setup, background: Background, x: u16) -> (u16, u16) {
        let index = background as usize;
        let horizontal = self.horizontal[index] & SCROLL_MASK;
        let vertical = self.vertical[index] & SCROLL_MASK;
        let mode = setup.background_mode.mode;
        let valid = match background {
            Background::One => OFFSET_BACKGROUND_1,
            Background::Two => OFFSET_BACKGROUND_2,
            _ => return (horizontal, vertical),
        };
        let tile = (x + (horizontal & 7)) / OFFSET_COLUMN;
        if !mode.offset_per_tile() || tile == 0 {
            return (horizontal, vertical);
        }
        let third = Background::Three as usize;
        let across = (self.horizontal[third] & OFFSET_HORIZONTAL_MASK) + (tile - 1) * OFFSET_COLUMN;
        let down = self.vertical[third] & SCROLL_MASK;
        let first = self.pixel_entry(vram, setup, Background::Three, across, down);
        let (horizontal_entry, vertical_entry) = if mode == Mode::Four {
            if first & OFFSET_VERTICAL != 0 {
                (0, first)
            } else {
                (first, 0)
            }
        } else {
            let second =
                self.pixel_entry(vram, setup, Background::Three, across, down + OFFSET_COLUMN);
            (first, second)
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
    // @source: anomie-regs#backgrounds/mode-6
    // @source: adr-0031
    // @source: adr-0036
    fn pixel_entry(
        &self,
        vram: &Vram,
        setup: Setup,
        background: Background,
        across: u16,
        down: u16,
    ) -> u16 {
        let size = setup.background_mode.character_size(background);
        let tilemap = self.tilemaps[background as usize];
        // Mode 6's tiles are 8 pixels wide whatever the size bit says, BG3's too.
        let width = if setup.background_mode.mode == Mode::Six {
            8
        } else {
            size
        };
        let columns = if tilemap & TILEMAP_WIDE != 0 { 64 } else { 32 };
        let rows = if tilemap & TILEMAP_TALL != 0 { 64 } else { 32 };
        self.entry(
            vram,
            background,
            across % (columns * width) / width,
            down % (rows * size) / size,
        )
    }

    /// What `background` shows at column `x` of `line`, or `None` where it is transparent
    /// or not drawn in this mode: scrolled by its registers, wrapped at its edges. In
    /// modes 5 and 6 a tile is 16 half-pixels wide and `half` picks the column of each
    /// pair the pixel shows (ADR 0036, point 3); `line` is then a row in half-lines if
    /// interlace says so. Other modes ignore `half`.
    // @source: anomie-regs#backgrounds/bg-scrolling
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: anomie-regs#backgrounds/direct-color-mode
    // @source: anomie-regs#backgrounds/mode-5
    // @source: anomie-regs#backgrounds/mode-6
    // @source: fullsnes#snesppubgcontrol
    // @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
    // @source: adr-0030
    // @source: adr-0031
    // @source: adr-0036
    pub(crate) fn sample(
        &self,
        vram: &Vram,
        setup: Setup,
        background: Background,
        x: u16,
        line: u16,
        half: HalfPixel,
    ) -> Option<Sample> {
        let mode = setup.background_mode.mode;
        let depth = depth(mode, background)?;
        let index = background as usize;
        let height = setup.background_mode.character_size(background);
        let hires = mode.hires();
        // A hi-res tile is 8 pixels of two half-pixels each, 16 half-pixels, whatever the
        // size bit; the other modes' tiles are as wide as high.
        let (width, tile_width) = if hires { (8, 16) } else { (height, height) };
        let tilemap = self.tilemaps[index];
        let columns = if tilemap & TILEMAP_WIDE != 0 { 64 } else { 32 };
        let rows = if tilemap & TILEMAP_TALL != 0 { 64 } else { 32 };
        let (horizontal, vertical) = self.scrolls(vram, setup, background, x);
        let across = (x + horizontal) % (columns * width);
        let down = (line + vertical) % (rows * height);

        let entry = TilemapEntry(self.entry(vram, background, across / width, down / height));
        let mut column = if hires {
            across % width * 2 + u16::from(half as u8)
        } else {
            across % width
        };
        let mut row = down % height;
        if entry.flip_horizontal() {
            column = tile_width - 1 - column;
        }
        if entry.flip_vertical() {
            row = height - 1 - row;
        }
        // A 16×16 tile is the tile, the next, and the two a row of 16 on; a 16×8 one of
        // modes 5 and 6 is the tile and the next.
        let tile = (entry.tile() + (row / 8) * 16 + column / 8) & TILE_NUMBER_MASK;

        let planes = u32::from(depth as u8);
        let start = self.character_base(background) + u32::from(tile) * 8 * planes;
        let row_start = start + u32::from(row % 8) * 2;
        let shift = 7 - column % 8;
        let mut value: u8 = 0;
        for plane in 0..planes {
            // Planes come in pairs, a byte each in a word, a pair to every 16 bytes.
            let address = row_start + (plane / 2) * 16 + plane % 2;
            let byte = vram.byte(address);
            value |= (byte >> shift & 1) << plane;
        }
        if value == 0 {
            return None;
        }

        let palette = entry.palette();
        let color = match depth {
            Depth::Eight if setup.direct_color => Color::Direct(direct_color(value, palette)),
            Depth::Eight => Color::Palette(value),
            Depth::Four => Color::Palette(palette * 16 + value),
            Depth::Two if mode == Mode::Zero => {
                Color::Palette(index_offset(background) + palette * 4 + value)
            }
            Depth::Two => Color::Palette(palette * 4 + value),
        };
        Some(Sample {
            color,
            high: entry.high(),
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
pub(crate) fn direct_color(value: u8, palette: u8) -> Rgb555 {
    let red = (value & 0x07) << 2 | (palette & 1) << 1;
    let green = (value >> 3 & 0x07) << 2 | (palette >> 1 & 1) << 1;
    let blue = (value >> 6) << 3 | (palette >> 2 & 1) << 2;
    Rgb555::from_channels(red, green, blue)
}

#[cfg(test)]
mod tests {
    use alloc::{format, vec::Vec};

    use super::*;

    // @source: anomie-regs#2105
    #[test]
    fn every_2105_byte_names_the_mode_of_its_bits_2_to_0() {
        for value in 0..=u8::MAX {
            assert_eq!(Mode::from_bits(value) as u8, value & 0x07, "{value:#04X}");
        }
    }

    /// A VRAM image of 64 KiB.
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: adr-0030
    fn vram() -> Vram {
        Vram::zeroed()
    }

    /// The setup for `$2105` = `control`, without direct colour.
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: adr-0030
    fn setup(control: u8) -> Setup {
        Setup {
            background_mode: BackgroundMode::from(control),
            direct_color: false,
        }
    }

    /// Writes row `row` of the 2-bit tile `number` at character base 0: `plane0` and `plane1`.
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: adr-0030
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
        let sample =
            |x| backgrounds.sample(&vram, setup(0), Background::One, x, 0, HalfPixel::Even);
        assert_eq!(
            sample(0).map(|sampled| sampled.color),
            Some(Color::Palette(8 + 1))
        );
        assert_eq!(
            sample(1).map(|sampled| sampled.color),
            Some(Color::Palette(8 + 2))
        );
        assert_eq!(
            sample(2).map(|sampled| sampled.color),
            Some(Color::Palette(8 + 3))
        );
        assert_eq!(sample(3), None);
        assert_eq!(sample(0).map(|sampled| sampled.high), Some(false));
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
                .sample(&vram, setup(mode), background, 0, 0, HalfPixel::Even)
                .map(|sampled| sampled.color)
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
        // Mode 5: BG1 has 16 colours and BG2 four; mode 6 has BG1 alone, and BG3 only in
        // mode 6 as offsets (ADR 0036, point 3).
        assert_eq!(color(5, Background::One), Some(Color::Palette(48 + 15)));
        assert_eq!(color(5, Background::Two), Some(Color::Palette(12 + 3)));
        assert_eq!(color(6, Background::One), Some(Color::Palette(48 + 15)));
        assert_eq!(color(6, Background::Two), None);
        assert_eq!(color(6, Background::Three), None);
        // Mode 7 has its own renderer.
        assert_eq!(color(7, Background::One), None);
    }

    // @source: anomie-regs#backgrounds/direct-color-mode
    // @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
    // @source: adr-0030
    #[test]
    fn direct_colour_spreads_the_character_and_palette_bits() {
        // Character data BBGGGRRR = 11 101 110 and palette bgr = 5 (b and r).
        assert_eq!(direct_color(0b1110_1110, 0b101).bits(), 0x729A);
        // The least-significant bit of each channel is the palette's, the bottom one 0.
        assert_eq!(direct_color(0x00, 0b111).bits(), 0b00100_00010_00010);
        assert_eq!(direct_color(0xFF, 0).bits(), 0b11000_11100_11100);

        let mut vram = vram();
        let mut backgrounds = Backgrounds::power_on();
        backgrounds.write(Register::Background1Tilemap, 0x04);
        // Palette 7, and an 8-plane tile 0 whose pixel 0 is $FF.
        vram[0x801] = 0x1C;
        for plane in 0..8 {
            vram[plane / 2 * 16 + plane % 2] = 0x80;
        }
        let direct = Setup {
            background_mode: BackgroundMode::from(3),
            direct_color: true,
        };
        let sample = backgrounds.sample(&vram, direct, Background::One, 0, 0, HalfPixel::Even);
        assert_eq!(
            sample.map(|sampled| sampled.color),
            Some(Color::Direct(Rgb555::from_bits_truncate(0x73DE)))
        );
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
        let at = |x| backgrounds.sample(&vram, setup(0), Background::One, x, 0, HalfPixel::Even);
        assert_eq!(at(1).map(|sampled| sampled.color), Some(Color::Palette(1)));
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
        let large = setup(0x10);
        let at = |x, line| {
            backgrounds
                .sample(&vram, large, Background::One, x, line, HalfPixel::Even)
                .map(|sampled| sampled.color)
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
                .sample(&vram, large, Background::One, x, line, HalfPixel::Even)
                .map(|sampled| sampled.color)
        };
        assert_eq!(at(15, 15), Some(Color::Palette(1)));
        assert_eq!(at(7, 15), Some(Color::Palette(2)));
        assert_eq!(at(15, 7), Some(Color::Palette(3)));
        assert_eq!(at(0, 0), Some(Color::Palette(1)));
    }

    /// Sets VRAM word `address`.
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: adr-0031
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
                .sample(&vram, large, background, x, line, HalfPixel::Even)
                .map(|sampled| sampled.color)
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
                .sample(&vram, large, Background::One, x, 0, HalfPixel::Even)
                .map(|sampled| sampled.color)
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
                .sample(&vram, setup(4), Background::Two, x, 0, HalfPixel::Even)
                .map(|sampled| sampled.color)
        };
        assert_eq!(at(0), Some(Color::Palette(1)));
        assert_eq!(at(8), Some(Color::Palette(3 * 4 + 1)));
        // The vertical entry leaves the horizontal scroll alone: column 2, the row below.
        assert_eq!(at(16), Some(Color::Palette(2 * 4 + 2)));
        // BG3 only supplies offsets in mode 4 and shows nothing itself.
        assert_eq!(
            backgrounds.sample(&vram, setup(4), Background::Three, 8, 0, HalfPixel::Even),
            None
        );
    }

    // @source: anomie-regs#2105
    // @source: anomie-regs#backgrounds/mode-0
    // @source: anomie-regs#backgrounds/mode-1
    // @source: anomie-regs#backgrounds/mode-2
    // @source: anomie-regs#backgrounds/mode-5
    // @source: anomie-regs#backgrounds/mode-6
    // @source: anomie-regs#backgrounds/mode-7
    // @source: adr-0030
    // @source: adr-0032
    // @source: adr-0033
    #[test]
    fn the_priority_order_follows_the_mode_and_mode_1_s_bg3_bit() {
        // `S3` is the sprites of priority 3, `1h` BG1's tiles with the priority bit and
        // `1l` those without; the lists are anomie's, front to back.
        let names = |control, extbg| {
            priority_order(BackgroundMode::from(control), extbg)
                .iter()
                .map(|layer| match layer {
                    Layer::Sprites(priority) => format!("S{}", *priority as u8),
                    Layer::Background { background, high } => {
                        format!("{}{}", *background as u8 + 1, if *high { 'h' } else { 'l' })
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        };
        assert_eq!(names(0x00, false), "S3 1h 2h S2 1l 2l S1 3h 4h S0 3l 4l");
        assert_eq!(names(0x01, false), "S3 1h 2h S2 1l 2l S1 3h S0 3l");
        assert_eq!(names(0x09, false), "3h S3 1h 2h S2 1l 2l S1 S0 3l");
        for mode in 2..=5 {
            assert_eq!(names(mode, false), "S3 1h S2 2h S1 1l S0 2l");
        }
        // Bit 3 matters only in mode 1.
        assert_eq!(names(0x0A, false), "S3 1h S2 2h S1 1l S0 2l");
        assert_eq!(names(0x06, false), "S3 1h S2 S1 1l S0");
        assert_eq!(names(0x07, false), "S3 S2 S1 1l S0");
        assert_eq!(names(0x07, true), "S3 S2 2h S1 1l S0 2l");
    }

    // @source: anomie-regs#backgrounds/mode-5
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: fullsnes#snesppubgcontrol
    // @source: adr-0036
    #[test]
    fn a_mode_5_tile_is_two_characters_and_each_screen_shows_half_the_columns() {
        let mut vram = vram();
        let mut backgrounds = Backgrounds::power_on();
        backgrounds.write(Register::Background2Tilemap, 0x04);
        vram[0x800] = 0x01;
        // Tile 1, the left character: the even columns colour 1. Tile 2, the right one: the
        // odd columns colour 2.
        two_bit_row(&mut vram, 1, 0, 0xAA, 0x00);
        two_bit_row(&mut vram, 2, 0, 0x00, 0x55);
        let at = |backgrounds: &Backgrounds, x, half| {
            backgrounds
                .sample(&vram, setup(5), Background::Two, x, 0, half)
                .map(|sampled| sampled.color)
        };
        // The sub screen shows the even half-pixels, 0 to 14: pixels 0 to 3 are the left
        // character's even columns, and 4 to 7 the right's, which are clear.
        for x in 0..4 {
            assert_eq!(
                at(&backgrounds, x, HalfPixel::Even),
                Some(Color::Palette(1)),
                "sub {x}"
            );
            assert_eq!(at(&backgrounds, x, HalfPixel::Odd), None, "main {x}");
        }
        // The main screen shows the odd ones: the right character's odd columns.
        for x in 4..8 {
            assert_eq!(at(&backgrounds, x, HalfPixel::Even), None, "sub {x}");
            assert_eq!(
                at(&backgrounds, x, HalfPixel::Odd),
                Some(Color::Palette(2)),
                "main {x}"
            );
        }
        // A horizontal flip turns the 16 half-pixels, so the screens swap columns.
        vram[0x801] = 0x40;
        let at = |x, half| {
            backgrounds
                .sample(&vram, setup(5), Background::Two, x, 0, half)
                .map(|sampled| sampled.color)
        };
        assert_eq!(at(0, HalfPixel::Even), Some(Color::Palette(2)));
        assert_eq!(at(4, HalfPixel::Even), None);
        assert_eq!(at(0, HalfPixel::Odd), None);
        assert_eq!(at(4, HalfPixel::Odd), Some(Color::Palette(1)));
    }

    // @source: anomie-regs#backgrounds/mode-5
    // @source: anomie-regs#2105
    // @source: fullsnes#snesppubgcontrol
    // @source: adr-0036
    #[test]
    fn a_mode_5_tile_is_8_pixels_across_and_8_or_16_rows_high_by_the_size_bit() {
        let mut vram = vram();
        let mut backgrounds = Backgrounds::power_on();
        backgrounds.write(Register::Background2Tilemap, 0x04);
        // Tile 1 at map column 0 and tile 5 at column 1.
        vram[0x800] = 0x01;
        vram[0x802] = 0x05;
        two_bit_row(&mut vram, 1, 0, 0x80, 0x00); // 8 rows: (0, 0), colour 1
        two_bit_row(&mut vram, 5, 0, 0x00, 0x80); // column 1: colour 2
        two_bit_row(&mut vram, 0x11, 0, 0x80, 0x80); // 16 rows: (0, 8), colour 3
        let at = |control, x, line| {
            let setup = setup(control);
            backgrounds
                .sample(&vram, setup, Background::Two, x, line, HalfPixel::Even)
                .map(|sampled| sampled.color)
        };
        // 8x8 characters: a tile is 16x8 half-pixels, so the next entry starts at pixel 8.
        assert_eq!(at(5, 0, 0), Some(Color::Palette(1)));
        assert_eq!(at(5, 8, 0), Some(Color::Palette(2)));
        // Row 8 is the next tile row, a map row of its own, which is clear.
        assert_eq!(at(5, 0, 8), None);
        // The size bit gives 16 rows and keeps the width: row 8 is tile $11, and the entry
        // at pixel 8 is still the second one.
        assert_eq!(at(5 | 0x20, 0, 8), Some(Color::Palette(3)));
        assert_eq!(at(5 | 0x20, 8, 0), Some(Color::Palette(2)));
    }

    // @source: anomie-regs#backgrounds/mode-6
    // @source: anomie-regs#backgrounds/mode-2
    // @source: adr-0031
    // @source: adr-0036
    #[test]
    fn mode_6_takes_bg3_offsets_per_8_pixel_column_even_with_a_16_pixel_bg3() {
        let mut vram = vram();
        let mut backgrounds = Backgrounds::power_on();
        backgrounds.write(Register::Background1Tilemap, 0x04); // word $0400
        backgrounds.write(Register::Background3Tilemap, 0x08); // word $0800
        // 4-bit tiles at base 0: tile 1 (the left character) colour 1 and tile 2 (the right)
        // colour 2.
        vram[32] = 0xFF;
        vram[64 + 1] = 0xFF;
        // BG1's rows 0 to 2 show tile 1 with the column number as the palette.
        for row in 0..3 {
            for column in 0..32_u16 {
                let address = 0x400 + row * 32 + usize::from(column);
                put(&mut vram, address, (column & 7) << 10 | 1);
            }
        }
        // BG3, 16x16 tiles, so one entry serves both offsets: column 0 moves BG1 8 across and
        // 8 down, column 1 16 across and 16 down.
        put(&mut vram, 0x800, 0x2000 | 8);
        put(&mut vram, 0x801, 0x2000 | 16);
        let control = 6 | 0x40;
        let at = |x, half| {
            let setup = setup(control);
            backgrounds
                .sample(&vram, setup, Background::One, x, 0, half)
                .map(|sampled| sampled.color)
        };
        // Pixel 0 is tile 0, which takes no offset.
        assert_eq!(at(0, HalfPixel::Odd), Some(Color::Palette(1)));
        // Pixel 8 is BG1's tile 1, with BG3's column 0 entry: 8 right, map column 2.
        assert_eq!(at(8, HalfPixel::Even), Some(Color::Palette(2 * 16 + 1)));
        // Pixel 16 uses BG3's column 1 entry, one 8-pixel column on whatever BG3's size bit
        // says: 16 right, map column 4.
        assert_eq!(at(16, HalfPixel::Even), Some(Color::Palette(4 * 16 + 1)));
        // Half-pixels 8 to 15 of a tile are the right character.
        assert_eq!(at(4, HalfPixel::Even), Some(Color::Palette(2)));
        // BG3 itself is not drawn.
        let setup = setup(control);
        assert_eq!(
            backgrounds.sample(&vram, setup, Background::Three, 0, 0, HalfPixel::Even),
            None
        );
    }
}
