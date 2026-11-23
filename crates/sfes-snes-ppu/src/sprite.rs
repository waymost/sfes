// @source: anomie-regs#2101
// @source: anomie-regs#2103
// @source: anomie-regs#213e
// @source: anomie-regs#sprites
// @source: anomie-timing#detailed-renderer-timing
// @source: fullsnes#snesppuspritesobjs
// @source: fullsnes#snespputimersandstatus
// @source: adr-0033
//! Sprites: `$2101 OBSEL`, the per-line evaluation with its 32-sprite and 34-tile limits
//! and the flags in `$213E`, and the pixels of the tiles that evaluation loads (ADR 0033).

use crate::background::vram_word;

/// How many sprites OAM holds.
const SPRITE_COUNT: u8 = 128;
/// A sprite index has 7 bits.
const INDEX_MASK: u8 = 0x7F;
/// How many sprites in range a line keeps.
const MAX_SPRITES: usize = 32;
/// How many tiles a line loads.
const MAX_TILES: usize = 34;
/// Where OAM's high table starts.
const HIGH_TABLE: usize = 0x200;
/// `$2101` bits 7–5: the size pair.
const SIZE_SHIFT: u8 = 5;
/// `$2101` bits 4–3: the gap before the second tile table.
const NAME_SHIFT: u8 = 3;
/// A two-bit field.
const TWO_BITS: u8 = 0x03;
/// OAM attribute bits 3–1: the palette.
const PALETTE_MASK: u8 = 0x07;
/// `$2101` bits 2–0: the base of the first tile table.
const BASE_MASK: u8 = 0x07;
/// The first tile table is at `base << 13` words.
const BASE_SHIFT: u32 = 13;
/// The second tile table is `(name + 1) << 12` words after it.
const NAME_WORDS_SHIFT: u32 = 12;
/// VRAM is 32K words.
const WORD_MASK: u16 = 0x7FFF;
/// A tile is 16 words of 4-bit pixels: planes 0 and 1 in the first eight.
const TILE_SHIFT: u8 = 4;
/// The words from a tile's first row of planes 0 and 1 to its first row of planes 2 and 3.
const PLANE_PAIR_WORDS: u16 = 8;
/// A tile is 8 pixels across and down.
const TILE_PIXELS: i16 = 8;
/// OAM attribute bit 0: the tile's name table.
const NAME_TABLE: u8 = 0x01;
/// OAM attribute bit 6: flip horizontally.
const FLIP_HORIZONTAL: u8 = 0x40;
/// OAM attribute bit 7: flip vertically.
const FLIP_VERTICAL: u8 = 0x80;
/// A line is 256 pixels.
const LINE_PIXELS: i16 = 256;
/// A sprite at this X counts as being at 0 for range and time.
const WRAPPED_X: i16 = -256;
/// A sprite's Y and a line's row have 8 bits.
const ROW_MASK: u16 = 0xFF;
/// The first CGRAM entry of the sprites' palettes.
const PALETTE_START: u8 = 128;
/// The size of a palette.
const PALETTE_COLORS: u8 = 16;
/// `$213E` bit 7: more than 34 tiles.
const TIME_OVER: u8 = 0x80;
/// `$213E` bit 6: more than 32 sprites.
const RANGE_OVER: u8 = 0x40;

/// The pair of sizes `$2101` bits 7 to 5 choose, as (width, height) of the small and large
/// sprites.
// @source: anomie-regs#2101
// @source: fullsnes#snesppuspritesobjs
// @source: adr-0033
const SIZES: [[(i16, i16); 2]; 8] = [
    [(8, 8), (16, 16)],
    [(8, 8), (32, 32)],
    [(8, 8), (64, 64)],
    [(16, 16), (32, 32)],
    [(16, 16), (64, 64)],
    [(32, 32), (64, 64)],
    [(16, 32), (32, 64)],
    [(16, 32), (32, 32)],
];

/// One sprite's record, decoded.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Sprite {
    /// The X position, -256 to 255.
    x: i16,
    /// The Y position.
    y: u8,
    /// The first tile's number in the 16 by 16 table.
    tile: u8,
    /// `vhoopppN`.
    attributes: u8,
    /// The width in pixels.
    width: i16,
    /// The height in pixels.
    height: i16,
}

impl Sprite {
    /// Sprite `index` of `oam`, with the sizes `select` (`$2101`) gives.
    // @source: anomie-regs#sprites/oam
    // @source: fullsnes#snesmemoryoamaccessspriteattributes
    // @source: adr-0033
    fn read(oam: &[u8], select: u8, index: u8) -> Self {
        let at = |offset: usize| oam.get(offset).copied().unwrap_or_default();
        let base = usize::from(index) * 4;
        let high = at(HIGH_TABLE + usize::from(index) / 4) >> ((index % 4) * 2);
        // A 9-bit two's complement number: bit 8 weighs -256.
        let x = i16::from(at(base)) - if high & 1 != 0 { 256 } else { 0 };
        let large = usize::from(high & 2 != 0);
        let (width, height) = SIZES
            .get(usize::from(select >> SIZE_SHIFT))
            .and_then(|pair| pair.get(large))
            .copied()
            .unwrap_or_default();
        Self {
            x,
            y: at(base + 1),
            tile: at(base + 2),
            attributes: at(base + 3),
            width,
            height,
        }
    }

    /// Which row of the sprite is on output line `line`, if it is on it: line 1 is where Y
    /// 0 starts, and rows wrap past 255.
    // @source: anomie-regs#sprites
    // @source: anomie-regs#backgrounds
    // @source: fullsnes#snesppuspritesobjs
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0033
    fn row_on(self, line: u16) -> Option<i16> {
        let row = line.wrapping_sub(1).wrapping_sub(u16::from(self.y)) & ROW_MASK;
        i16::try_from(row).ok().filter(|&row| row < self.height)
    }

    /// The X range and time use: -256 counts as 0.
    // @source: anomie-regs#sprites/drawing-the-sprites
    const fn counted_x(self) -> i16 {
        if self.x == WRAPPED_X { 0 } else { self.x }
    }

    /// Whether any of it is on the line horizontally, for range.
    // @source: anomie-regs#sprites/drawing-the-sprites
    const fn in_range(self) -> bool {
        self.counted_x() > -self.width && self.counted_x() < LINE_PIXELS
    }

    /// The row of the character `row` shows after the vertical flip: the sprite is flipped
    /// as one square, or as each square half of a rectangular one.
    // @source: anomie-regs#sprites
    // @source: adr-0033
    const fn source_row(self, row: i16) -> i16 {
        if self.attributes & FLIP_VERTICAL == 0 {
            return row;
        }
        let square = if self.width < self.height {
            self.width
        } else {
            self.height
        };
        (row / square) * square + (square - 1 - row % square)
    }
}

/// A tile loaded for a line: one row of 8 pixels of a sprite.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct Tile {
    /// The screen position of its left pixel; negative or past 255 where it is off the line.
    x: i16,
    /// The VRAM word of its row's planes 0 and 1.
    word: u16,
    /// Whether its pixels run right to left.
    flip: bool,
    /// The palette, 0 to 7.
    palette: u8,
    /// The OAM priority, 0 to 3.
    priority: u8,
}

/// A sprite pixel: its CGRAM entry and its priority against the backgrounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Pixel {
    /// The CGRAM entry, 129 to 255.
    pub(crate) index: u8,
    /// The OAM priority, 0 to 3.
    pub(crate) priority: u8,
}

/// What evaluation loaded for a line: up to 34 tiles, the last sprite's first.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Line {
    /// The loaded tiles.
    tiles: [Tile; MAX_TILES],
    /// How many are loaded.
    count: usize,
}

impl Line {
    /// The line's 256 sprite pixels: the front sprite's non-transparent pixel at each, so a
    /// tile loaded later covers one loaded before it.
    // @source: anomie-regs#sprites/palettes
    // @source: anomie-regs#sprites/sprite-priority
    // @source: fullsnes#snesppuvideomemoryvram
    // @source: adr-0033
    pub(crate) fn pixels(&self, vram: &[u8]) -> [Option<Pixel>; 256] {
        let mut pixels = [None; 256];
        for tile in self.tiles.iter().take(self.count) {
            let low = vram_word(vram, tile.word);
            let high = vram_word(vram, tile.word.wrapping_add(PLANE_PAIR_WORDS));
            for column in 0..TILE_PIXELS {
                // Bit 7 is the left pixel, unless the tile is flipped.
                let bit =
                    u32::try_from(if tile.flip { column } else { 7 - column }).unwrap_or_default();
                let value = ((low >> bit) & 1)
                    | (((low >> (8 + bit)) & 1) << 1)
                    | (((high >> bit) & 1) << 2)
                    | (((high >> (8 + bit)) & 1) << 3);
                let Ok(value) = u8::try_from(value) else {
                    continue;
                };
                if value == 0 {
                    continue;
                }
                let slot = usize::try_from(tile.x + column)
                    .ok()
                    .and_then(|x| pixels.get_mut(x));
                if let Some(slot) = slot {
                    *slot = Some(Pixel {
                        index: PALETTE_START + tile.palette * PALETTE_COLORS + value,
                        priority: tile.priority,
                    });
                }
            }
        }
        pixels
    }
}

/// `$2101` and the overflow flags of `$213E`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Sprites {
    /// $2101 OBSEL: the sizes, the name select and the base.
    select: u8,
    /// $213E STAT77 bits 7 and 6: time over and range over.
    flags: u8,
}

impl Sprites {
    /// Both 0 at power-on (ADR 0033, point 1). Reset keeps them.
    // @source: fullsnes#snesiomap
    // @source: adr-0033
    pub(crate) const fn power_on() -> Self {
        Self {
            select: 0,
            flags: 0,
        }
    }

    /// $2101 OBSEL: the byte as written.
    // @source: anomie-regs#2101
    // @source: adr-0033
    pub(crate) const fn write_select(&mut self, value: u8) {
        self.select = value;
    }

    /// $213E bits 7 and 6: time over and range over.
    // @source: anomie-regs#213e
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0033
    pub(crate) const fn status(self) -> u8 {
        self.flags
    }

    /// Clears both flags, as the end of vertical blank does (ADR 0033, point 7).
    // @source: anomie-regs#213e
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0033
    pub(crate) const fn clear_flags(&mut self) {
        self.flags = 0;
    }

    /// Evaluates output line `line` with `first` as the front sprite: takes the first 32
    /// sprites in range, then loads up to 34 of their tiles from the last sprite to the
    /// first, setting the range and time flags when there are more (ADR 0033, point 5).
    // @source: anomie-regs#sprites/drawing-the-sprites
    // @source: anomie-regs#213e
    // @source: anomie-timing#detailed-renderer-timing
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0033
    pub(crate) fn evaluate(&mut self, oam: &[u8], first: u8, line: u16) -> Line {
        let mut kept = [(Sprite::default(), 0_i16); MAX_SPRITES];
        let mut kept_count = 0;
        for offset in 0..SPRITE_COUNT {
            let index = first.wrapping_add(offset) & INDEX_MASK;
            let sprite = Sprite::read(oam, self.select, index);
            let Some(row) = sprite.row_on(line).filter(|_| sprite.in_range()) else {
                continue;
            };
            let Some(slot) = kept.get_mut(kept_count) else {
                self.flags |= RANGE_OVER;
                break;
            };
            *slot = (sprite, row);
            kept_count += 1;
        }

        let mut loaded = Line {
            tiles: [Tile::default(); MAX_TILES],
            count: 0,
        };
        for &(sprite, row) in kept.iter().take(kept_count).rev() {
            let columns = sprite.width / TILE_PIXELS;
            let source_row = sprite.source_row(row);
            for column in 0..columns {
                let counted = sprite.counted_x() + column * TILE_PIXELS;
                if counted <= -TILE_PIXELS || counted >= LINE_PIXELS {
                    continue;
                }
                let Some(slot) = loaded.tiles.get_mut(loaded.count) else {
                    self.flags |= TIME_OVER;
                    return loaded;
                };
                let flip = sprite.attributes & FLIP_HORIZONTAL != 0;
                let source_column = if flip { columns - 1 - column } else { column };
                *slot = self.tile(sprite, source_column, source_row, column);
                loaded.count += 1;
            }
        }
        loaded
    }

    /// The tile of `sprite` in character column `source_column` and sprite row
    /// `source_row` (after the vertical flip), drawn at screen column `column`.
    // @source: anomie-regs#sprites/character-table-in-vram
    // @source: fullsnes#snesppuspritesobjs
    // @source: adr-0033
    fn tile(self, sprite: Sprite, source_column: i16, source_row: i16, column: i16) -> Tile {
        let tile_row = u8::try_from(source_row / TILE_PIXELS).unwrap_or_default();
        let tile_column = u8::try_from(source_column).unwrap_or_default();
        // The 16 by 16 table has no carry from a column to the row or from the row out.
        let number_row = (sprite.tile >> TILE_SHIFT).wrapping_add(tile_row) & 0x0F;
        let number_column = (sprite.tile & 0x0F).wrapping_add(tile_column) & 0x0F;
        let number = (number_row << TILE_SHIFT) | number_column;
        let base = u16::from(self.select & BASE_MASK) << BASE_SHIFT;
        let name = if sprite.attributes & NAME_TABLE != 0 {
            (u16::from((self.select >> NAME_SHIFT) & TWO_BITS) + 1) << NAME_WORDS_SHIFT
        } else {
            0
        };
        let first = base
            .wrapping_add(u16::from(number) << TILE_SHIFT)
            .wrapping_add(name)
            & WORD_MASK;
        let pixel_row = u16::try_from(source_row % TILE_PIXELS).unwrap_or_default();
        Tile {
            x: sprite.x + column * TILE_PIXELS,
            word: first.wrapping_add(pixel_row),
            flip: sprite.attributes & FLIP_HORIZONTAL != 0,
            palette: (sprite.attributes >> 1) & PALETTE_MASK,
            priority: (sprite.attributes >> 4) & TWO_BITS,
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use super::*;

    // @source: anomie-regs#sprites/oam
    // @source: adr-0033
    /// An OAM image with every sprite at Y 128, which no sprite of up to 64 rows reaches
    /// on the lines the tests use.
    fn oam() -> Vec<u8> {
        let mut oam = vec![0; 0x220];
        for record in oam.chunks_mut(4).take(128) {
            record[1] = 128;
        }
        oam
    }

    // @source: anomie-regs#sprites/oam
    // @source: adr-0033
    /// Sets sprite `index`: position, tile and attributes, with its high-table bits.
    fn put(oam: &mut [u8], index: usize, x: i16, y: u8, tile: u8, attributes: u8, large: bool) {
        let x = u16::from_le_bytes(x.to_le_bytes()) & 0x1FF;
        oam[index * 4] = u8::try_from(x & 0xFF).unwrap_or_default();
        oam[index * 4 + 1] = y;
        oam[index * 4 + 2] = tile;
        oam[index * 4 + 3] = attributes;
        let shift = (index % 4) * 2;
        let x_high = u8::try_from(x >> 8).unwrap_or_default();
        let bits = (x_high | (u8::from(large) << 1)) << shift;
        oam[0x200 + index / 4] = (oam[0x200 + index / 4] & !(3 << shift)) | bits;
    }

    // @source: anomie-regs#2101
    // @source: fullsnes#snesppuspritesobjs
    // @source: adr-0033
    #[test]
    fn obsel_picks_the_small_and_large_sizes() {
        let mut oam = oam();
        put(&mut oam, 0, 0, 0, 0, 0, false);
        put(&mut oam, 1, 0, 0, 0, 0, true);
        let sizes = [
            [(8, 8), (16, 16)],
            [(8, 8), (32, 32)],
            [(8, 8), (64, 64)],
            [(16, 16), (32, 32)],
            [(16, 16), (64, 64)],
            [(32, 32), (64, 64)],
            [(16, 32), (32, 64)],
            [(16, 32), (32, 32)],
        ];
        for (size, expected) in (0_u8..).zip(sizes) {
            let select = size << 5;
            for (index, (width, height)) in (0_u8..).zip(expected) {
                let sprite = Sprite::read(&oam, select, index);
                assert_eq!((sprite.width, sprite.height), (width, height));
            }
        }
    }

    // @source: anomie-regs#sprites/oam
    // @source: fullsnes#snesmemoryoamaccessspriteattributes
    // @source: adr-0033
    #[test]
    fn the_record_has_a_nine_bit_signed_x_and_its_size_in_the_high_table() {
        let mut oam = oam();
        put(&mut oam, 5, -3, 7, 0x21, 0xA5, true);
        put(&mut oam, 6, 255, 0, 0, 0, false);
        put(&mut oam, 7, -256, 0, 0, 0, false);
        let sprite = Sprite::read(&oam, 0x40, 5);
        assert_eq!(
            (sprite.x, sprite.y, sprite.tile, sprite.attributes),
            (-3, 7, 0x21, 0xA5)
        );
        assert_eq!((sprite.width, sprite.height), (64, 64));
        assert_eq!(Sprite::read(&oam, 0, 6).x, 255);
        assert_eq!(Sprite::read(&oam, 0, 7).x, -256);
        // Sprites 4 to 7 share a byte; sprite 6 is the small one.
        assert_eq!(Sprite::read(&oam, 0, 6).width, 8);
    }

    // @source: anomie-regs#sprites
    // @source: anomie-regs#backgrounds
    // @source: fullsnes#snesppuspritesobjs
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0033
    #[test]
    fn y_0_starts_on_line_1_and_a_tall_sprite_wraps_past_the_bottom() {
        let mut oam = oam();
        put(&mut oam, 0, 0, 0, 0, 0, false);
        let sprite = Sprite::read(&oam, 0, 0);
        assert_eq!(sprite.row_on(1), Some(0));
        assert_eq!(sprite.row_on(8), Some(7));
        assert_eq!(sprite.row_on(9), None);
        assert_eq!(sprite.row_on(0), None);
        // Y 255 is off the top: its first row is on line 0, its second on line 1.
        put(&mut oam, 1, 0, 255, 0, 0, false);
        assert_eq!(Sprite::read(&oam, 0, 1).row_on(1), Some(1));
        // A 64-pixel sprite at Y 200 leaves the bottom at line 224 and returns at line 1.
        put(&mut oam, 2, 0, 200, 0, 0, true);
        let tall = Sprite::read(&oam, 0x40, 2);
        assert_eq!(tall.row_on(201), Some(0));
        assert_eq!(tall.row_on(224), Some(23));
        assert_eq!(tall.row_on(1), Some(56));
        assert_eq!(tall.row_on(9), None);
    }

    // @source: anomie-regs#sprites
    // @source: adr-0033
    #[test]
    fn a_rectangular_sprite_flips_each_square_half() {
        let mut oam = oam();
        // 16 by 32 sprite (size 6, small).
        put(&mut oam, 0, 0, 0, 0, FLIP_VERTICAL, false);
        let sprite = Sprite::read(&oam, 6 << 5, 0);
        assert_eq!((sprite.width, sprite.height), (16, 32));
        assert_eq!(sprite.source_row(0), 15);
        assert_eq!(sprite.source_row(15), 0);
        assert_eq!(sprite.source_row(16), 31);
        assert_eq!(sprite.source_row(31), 16);
        // A square sprite flips whole: 16 by 16 is (size 3, small).
        let square = Sprite::read(&oam, 3 << 5, 0);
        assert_eq!(square.source_row(0), 15);
        assert_eq!(square.source_row(9), 6);
        // Without the flip a row is itself.
        put(&mut oam, 1, 0, 0, 0, 0, false);
        assert_eq!(Sprite::read(&oam, 3 << 5, 1).source_row(9), 9);
    }

    // @source: fullsnes#snesppuvideomemoryvram
    // @source: adr-0033
    /// Writes 4-bit tile row `row` of the tile at word `first`: the left pixel is bit 7.
    fn tile_row(vram: &mut [u8], first: usize, row: usize, planes: [u8; 4]) {
        let low = (first + row) * 2;
        let high = (first + 8 + row) * 2;
        vram[low] = planes[0];
        vram[low + 1] = planes[1];
        vram[high] = planes[2];
        vram[high + 1] = planes[3];
    }

    // @source: anomie-regs#sprites/character-table-in-vram
    // @source: fullsnes#snesppuspritesobjs
    // @source: adr-0033
    #[test]
    fn a_sixteen_pixel_sprite_at_tile_ff_wraps_to_f0_0f_and_00() {
        let mut oam = oam();
        put(&mut oam, 0, 10, 0, 0xFF, 0, true);
        let mut sprites = Sprites::power_on();
        // Size 0: 8x8 and 16x16.
        let line = sprites.evaluate(&oam, 0, 1);
        assert_eq!(line.count, 2);
        // Tiles in the order the screen shows them, the first row: $FF and $F0.
        assert_eq!(line.tiles[0].word, 0xFF0);
        assert_eq!(line.tiles[1].word, 0xF00);
        let line = sprites.evaluate(&oam, 0, 9);
        // The second row: $0F and $00.
        assert_eq!(line.tiles[0].word, 0x0F0);
        assert_eq!(line.tiles[1].word, 0x000);
    }

    // @source: anomie-regs#2101
    // @source: anomie-regs#sprites/character-table-in-vram
    // @source: fullsnes#snesppuspritesobjs
    // @source: adr-0033
    #[test]
    fn the_base_and_the_second_table_set_the_tile_words() {
        let mut oam = oam();
        put(&mut oam, 0, 0, 0, 0x03, 0, false);
        put(&mut oam, 1, 16, 0, 0x03, 1, false);
        let mut sprites = Sprites::power_on();
        // Base 2 (16K words), name select 1 (the second table 8K words on).
        sprites.write_select(0b0000_1010);
        let line = sprites.evaluate(&oam, 0, 1);
        // Sprite 1 is the first sprite loaded from the end: its tile comes first.
        let second = (2 << 13) + (0x03 << 4) + ((1 + 1) << 12);
        let first = (2 << 13) + (0x03 << 4);
        assert_eq!(line.tiles[0].word, second);
        assert_eq!(line.tiles[1].word, first);
    }

    // @source: anomie-regs#sprites/palettes
    // @source: anomie-regs#sprites/sprite-priority
    // @source: fullsnes#snesppuvideomemoryvram
    // @source: adr-0033
    #[test]
    fn pixels_read_four_planes_with_colour_0_transparent_and_palettes_from_128() {
        let mut oam = oam();
        // Palette 2, priority 3, flips off: attributes `vhoopppN` = 0b0011_0100.
        put(&mut oam, 0, 4, 0, 0, 0b0011_0100, false);
        let mut vram = vec![0; 0x10000];
        // Left pixel 1 (plane 0), second 2 (plane 1), third 4 (plane 2), fourth 8 (plane 3),
        // the rest 0.
        tile_row(
            &mut vram,
            0,
            0,
            [0b1000_0000, 0b0100_0000, 0b0010_0000, 0b0001_0000],
        );
        let mut sprites = Sprites::power_on();
        let pixels = sprites.evaluate(&oam, 0, 1).pixels(&vram);
        let index = |value: u8| 128 + 2 * 16 + value;
        assert_eq!(pixels[3], None);
        assert_eq!(pixels[4].map(|pixel| pixel.index), Some(index(1)));
        assert_eq!(pixels[4].map(|pixel| pixel.priority), Some(3));
        assert_eq!(pixels[5].map(|pixel| pixel.index), Some(index(2)));
        assert_eq!(pixels[6].map(|pixel| pixel.index), Some(index(4)));
        assert_eq!(pixels[7].map(|pixel| pixel.index), Some(index(8)));
        assert_eq!(pixels[8], None);
    }

    // @source: anomie-regs#sprites
    // @source: adr-0033
    #[test]
    fn a_horizontal_flip_reverses_the_pixels_and_the_tiles() {
        let mut oam = oam();
        // 16 by 16 at X 0, tile 0, flipped horizontally.
        put(&mut oam, 0, 0, 0, 0, FLIP_HORIZONTAL, true);
        let mut vram = vec![0; 0x10000];
        // Tile 0's left pixel is 1 and tile 1's right pixel is 2.
        tile_row(&mut vram, 0, 0, [0b1000_0000, 0, 0, 0]);
        tile_row(&mut vram, 16, 0, [0, 0b0000_0001, 0, 0]);
        let mut sprites = Sprites::power_on();
        let pixels = sprites.evaluate(&oam, 0, 1).pixels(&vram);
        // Flipped: tile 1 is on the left with its right pixel at the far left of the
        // sprite, and tile 0's left pixel at the far right.
        assert_eq!(pixels[0].map(|pixel| pixel.index), Some(128 + 2));
        assert_eq!(pixels[15].map(|pixel| pixel.index), Some(128 + 1));
        assert_eq!(pixels[1], None);
    }

    // @source: anomie-regs#sprites/sprite-priority
    // @source: adr-0033
    #[test]
    fn the_sprite_nearer_the_first_covers_the_other_and_the_priority_is_its_own() {
        let mut oam = oam();
        put(&mut oam, 2, 0, 0, 0, 0b0001_0000, false);
        put(&mut oam, 3, 0, 0, 1, 0b0010_0000, false);
        let mut vram = vec![0; 0x10000];
        tile_row(&mut vram, 0, 0, [0xFF, 0, 0, 0]);
        tile_row(&mut vram, 16, 0, [0, 0xFF, 0, 0]);
        let mut sprites = Sprites::power_on();
        // Sprite 2 is in front of sprite 3.
        let front = sprites.evaluate(&oam, 0, 1).pixels(&vram);
        assert_eq!(
            front[0],
            Some(Pixel {
                index: 129,
                priority: 1
            })
        );
        // With sprite 3 first it covers sprite 2.
        let rotated = sprites.evaluate(&oam, 3, 1).pixels(&vram);
        assert_eq!(
            rotated[0],
            Some(Pixel {
                index: 130,
                priority: 2
            })
        );
        // A transparent pixel in front shows the sprite behind it.
        tile_row(&mut vram, 16, 0, [0, 0, 0, 0]);
        let behind = sprites.evaluate(&oam, 3, 1).pixels(&vram);
        assert_eq!(behind[0].map(|pixel| pixel.priority), Some(1));
    }

    // @source: anomie-regs#sprites/drawing-the-sprites
    // @source: anomie-regs#213e
    // @source: adr-0033
    #[test]
    fn more_than_32_sprites_on_a_line_set_range_over_and_keep_the_first_32() {
        let mut oam = oam();
        for index in 0..33 {
            put(&mut oam, index, 0, 0, 0, 0, false);
        }
        let mut sprites = Sprites::power_on();
        let line = sprites.evaluate(&oam, 0, 1);
        assert_eq!(sprites.status(), 0x40);
        assert_eq!(line.count, 32);
        // 32 sprites alone leave the flag clear.
        let mut sprites = Sprites::power_on();
        put(&mut oam, 32, 0, 128, 0, 0, false);
        let _ = sprites.evaluate(&oam, 0, 1);
        assert_eq!(sprites.status(), 0);
        // The count starts from the first sprite: from sprite 1 the 32 are 1 to 32.
        let mut sprites = Sprites::power_on();
        put(&mut oam, 32, 0, 0, 0, 0, false);
        put(&mut oam, 0, 0, 128, 0, 0, false);
        let _ = sprites.evaluate(&oam, 1, 1);
        assert_eq!(sprites.status(), 0);
    }

    // @source: anomie-regs#sprites/drawing-the-sprites
    // @source: adr-0033
    #[test]
    fn sprites_beside_the_line_are_not_in_range() {
        let mut oam = oam();
        // 8 wide: -8 is out, -7 is in; 256 counts as 0; 255 is in.
        put(&mut oam, 0, -8, 0, 0, 0, false);
        put(&mut oam, 1, -7, 0, 0, 0, false);
        put(&mut oam, 2, 255, 0, 0, 0, false);
        put(&mut oam, 3, -256, 0, 0, 0, false);
        let mut sprites = Sprites::power_on();
        let line = sprites.evaluate(&oam, 0, 1);
        // Sprite 0 is out; sprites 1, 2 and 3 are in, the last at 0 for the count.
        assert_eq!(line.count, 3);
        // Loaded from the last: -256's tile has its true X.
        assert_eq!(line.tiles[0].x, -256);
        assert_eq!(line.tiles[1].x, 255);
        assert_eq!(line.tiles[2].x, -7);
    }

    // @source: anomie-regs#sprites/drawing-the-sprites
    // @source: anomie-regs#213e
    // @source: adr-0033
    #[test]
    fn more_than_34_tiles_set_time_over_and_load_the_first_34_from_the_last_sprite() {
        let mut oam = oam();
        // Nine 32-pixel sprites at 4 tiles across each: 36 tiles (size 5, small).
        for (index, x) in (0..9).zip((0..).step_by(8)) {
            put(&mut oam, index, x, 0, 0, 0, false);
        }
        let mut sprites = Sprites::power_on();
        sprites.write_select(5 << 5);
        let line = sprites.evaluate(&oam, 0, 1);
        assert_eq!(sprites.status(), 0x80);
        assert_eq!(line.count, 34);
        // The last sprite's tiles are loaded first; sprite 0 keeps only its first two.
        assert_eq!(line.tiles[0].x, 64);
        assert_eq!(line.tiles[33].x, 8);
        // Eight sprites (32 tiles) fit.
        put(&mut oam, 8, 0, 128, 0, 0, false);
        let mut sprites = Sprites::power_on();
        sprites.write_select(5 << 5);
        let _ = sprites.evaluate(&oam, 0, 1);
        assert_eq!(sprites.status(), 0);
    }

    // @source: anomie-regs#sprites/drawing-the-sprites
    // @source: anomie-regs#213e
    // @source: adr-0033
    #[test]
    fn a_tile_off_the_edge_does_not_count_for_time() {
        let mut oam = oam();
        // A 64-pixel sprite at X -32 has 8 tiles at -32 to 24: those at -32, -24, -16 and -8
        // are not counted (the rule is X > -8), the 4 from 0 on are.
        put(&mut oam, 0, -32, 0, 0, 0, true);
        let mut sprites = Sprites::power_on();
        sprites.write_select(2 << 5);
        let line = sprites.evaluate(&oam, 0, 1);
        assert_eq!(line.count, 4);
        assert_eq!(line.tiles[0].x, 0);
    }

    // @source: anomie-regs#213e
    // @source: fullsnes#snespputimersandstatus
    // @source: adr-0033
    #[test]
    fn the_flags_clear_together_and_the_select_is_kept() {
        let mut sprites = Sprites::power_on();
        sprites.write_select(0xA5);
        sprites.flags = 0xC0;
        assert_eq!(sprites.status(), 0xC0);
        sprites.clear_flags();
        assert_eq!(sprites.status(), 0);
        assert_eq!(sprites.select, 0xA5);
    }
}
