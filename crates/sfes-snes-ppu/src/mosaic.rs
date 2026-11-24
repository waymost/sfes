// @source: anomie-regs#2106
// @source: anomie-regs#rendering-the-screen/mosaic
// @source: anomie-regs#backgrounds/mode-7
// @source: fullsnes#snesppubgcontrol
// @source: adr-0035
//! `$2106 MOSAIC`: which backgrounds are pixelated, the block size and where the block rows
//! start (ADR 0035, point 8).

use crate::background::Background;

/// `$2106` bits 7–4: the block size minus 1.
const SIZE_SHIFT: u8 = 4;
/// `$2106` bit 0 as the mode 7 EXTBG layer's vertical enable.
const EXTBG_VERTICAL: u8 = 0x01;
/// `$2106` bit 1 as the mode 7 EXTBG layer's horizontal enable.
const EXTBG_HORIZONTAL: u8 = 0x02;

/// The mosaic register and the line its block rows count from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Mosaic {
    /// `$2106 MOSAIC`: size and the four backgrounds' enable bits.
    value: u8,
    /// The first line of the first block row.
    start: u16,
}

impl Mosaic {
    /// `$2106` is 0 and the block rows start at `first_line` (ADR 0035, points 1 and 8).
    // @source: fullsnes#snesiomap
    // @source: adr-0035
    pub(crate) const fn power_on(first_line: u16) -> Self {
        Self {
            value: 0,
            start: first_line,
        }
    }

    /// A write of `value` while `next_line` is the line to render next: a changed value
    /// restarts the block rows there.
    // @source: anomie-regs#2106
    // @source: adr-0035
    pub(crate) const fn write(&mut self, value: u8, next_line: u16) {
        if value != self.value {
            self.start = next_line;
        }
        self.value = value;
    }

    /// A new frame: the block rows start at its first line.
    // @source: anomie-regs#rendering-the-screen/mosaic
    // @source: fullsnes#snesppubgcontrol
    // @source: adr-0035
    pub(crate) const fn start_frame(&mut self, first_line: u16) {
        self.start = first_line;
    }

    /// The block size in pixels, 1 to 16.
    // @source: anomie-regs#2106
    // @source: adr-0035
    const fn size(self) -> u16 {
        (self.value >> SIZE_SHIFT) as u16 + 1
    }

    /// The pixel a block of `x` and `line` shows: its top-left one, with the block rows
    /// counted from the start line.
    // @source: anomie-regs#rendering-the-screen/mosaic
    // @source: fullsnes#snesppubgcontrol
    // @source: adr-0035
    const fn corner(self, x: u16, line: u16, horizontal: bool, vertical: bool) -> (u16, u16) {
        let size = self.size();
        let column = if horizontal { x - x % size } else { x };
        let row = if vertical {
            line - line.saturating_sub(self.start) % size
        } else {
            line
        };
        (column, row)
    }

    /// Whether `$2106` pixelates `background`.
    // @source: anomie-regs#2106
    // @source: adr-0036
    pub(crate) const fn pixelates(self, background: Background) -> bool {
        self.value >> background as u8 & 1 != 0
    }

    /// The column and line `background` takes its pixel from at `x` and `line`: those
    /// themselves, or the top-left of the block when its enable bit is set.
    // @source: anomie-regs#2106
    // @source: adr-0035
    pub(crate) const fn block(self, background: Background, x: u16, line: u16) -> (u16, u16) {
        let enabled = self.pixelates(background);
        self.corner(x, line, enabled, enabled)
    }

    /// The same for mode 7's EXTBG layer: bit 1 pixelates across, bit 0 down.
    // @source: anomie-regs#rendering-the-screen/mosaic
    // @source: anomie-regs#backgrounds/mode-7
    // @source: adr-0035
    pub(crate) const fn extbg_block(self, x: u16, line: u16) -> (u16, u16) {
        self.corner(
            x,
            line,
            self.value & EXTBG_HORIZONTAL != 0,
            self.value & EXTBG_VERTICAL != 0,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: anomie-regs#2106
    // @source: anomie-regs#rendering-the-screen/mosaic
    // @source: adr-0035
    #[test]
    fn a_block_is_size_by_size_from_the_start_line() {
        let mut mosaic = Mosaic::power_on(1);
        // 4x4 on BG1 only.
        mosaic.write(0x31, 1);
        assert_eq!(mosaic.block(Background::One, 7, 1), (4, 1));
        assert_eq!(mosaic.block(Background::One, 7, 4), (4, 1));
        assert_eq!(mosaic.block(Background::One, 8, 5), (8, 5));
        // BG2's bit is clear.
        assert_eq!(mosaic.block(Background::Two, 7, 3), (7, 3));
    }

    // @source: anomie-regs#2106
    // @source: anomie-regs#rendering-the-screen/mosaic
    // @source: adr-0035
    #[test]
    fn a_changed_write_restarts_the_rows_and_a_repeated_one_does_not() {
        let mut mosaic = Mosaic::power_on(1);
        mosaic.write(0x11, 10);
        assert_eq!(mosaic.block(Background::One, 0, 11), (0, 10));
        mosaic.write(0x11, 11);
        assert_eq!(mosaic.block(Background::One, 0, 11), (0, 10));
        mosaic.start_frame(1);
        assert_eq!(mosaic.block(Background::One, 0, 11), (0, 11));
    }

    // @source: anomie-regs#rendering-the-screen/mosaic
    // @source: adr-0035
    #[test]
    fn extbg_takes_bit_1_across_and_bit_0_down() {
        let mut mosaic = Mosaic::power_on(1);
        // $F1: 1 wide by 16 tall for BG2; $F2: 16 wide by 1 tall; $F3: 16 by 16.
        mosaic.write(0xF1, 1);
        assert_eq!(mosaic.extbg_block(20, 20), (20, 17));
        mosaic.write(0xF2, 1);
        assert_eq!(mosaic.extbg_block(20, 20), (16, 20));
        mosaic.write(0xF3, 1);
        assert_eq!(mosaic.extbg_block(20, 20), (16, 17));
    }
}
