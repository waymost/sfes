// @source: anomie-regs#2105
// @source: anomie-regs#2133
// @source: fullsnes#snesppuresolution
// @source: adr-0028
//! One rendered line, as `render_line` hands it to the machine (ADR 0028, point 7).

use sfes_api::Rgb555;

/// How many pixels a line has; each variant's value is its width.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[repr(usize)]
pub enum LineWidth {
    /// 256 pixels.
    #[default]
    Normal = 256,
    /// 512 pixels: BG mode 5 or 6, or pseudo-hires.
    Wide = 512,
}

impl LineWidth {
    /// Its width in pixels.
    // @source: anomie-regs#2105
    // @source: anomie-regs#2133
    // @source: adr-0028
    #[must_use]
    pub const fn pixels(self) -> usize {
        self as usize
    }
}

/// The pixels a line can hold: as many as a wide line has.
const MAX_PIXELS: usize = LineWidth::Wide.pixels();

/// One output line: its pixels, CGRAM colours, and how it was rendered. The default is an
/// empty line for `render_line` to fill.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineOut {
    /// The line it holds, 1 to 239.
    line: u16,
    /// Its width.
    width: LineWidth,
    /// `$2133` bit 0 was set at its render.
    interlaced: bool,
    /// The pixels; only the first `width` count.
    pixels: [Rgb555; MAX_PIXELS],
}

impl Default for LineOut {
    fn default() -> Self {
        Self {
            line: 0,
            width: LineWidth::Normal,
            interlaced: false,
            pixels: [Rgb555::BLACK; MAX_PIXELS],
        }
    }
}

impl LineOut {
    /// The line it holds.
    // @source: adr-0028
    #[must_use]
    pub const fn line(&self) -> u16 {
        self.line
    }

    /// Its width.
    // @source: anomie-regs#2105
    // @source: anomie-regs#2133
    // @source: adr-0028
    #[must_use]
    pub const fn width(&self) -> LineWidth {
        self.width
    }

    /// Whether interlace was on at its render.
    // @source: anomie-regs#2133
    // @source: adr-0028
    #[must_use]
    pub const fn interlaced(&self) -> bool {
        self.interlaced
    }

    /// Its pixels: as many as its width.
    // @source: adr-0028
    #[must_use]
    pub fn pixels(&self) -> &[Rgb555] {
        &self.pixels[..self.width.pixels()]
    }

    /// Starts `line` with the given shape and every pixel `color`.
    // @source: adr-0028
    pub(crate) fn fill(&mut self, line: u16, width: LineWidth, interlaced: bool, color: Rgb555) {
        self.line = line;
        self.width = width;
        self.interlaced = interlaced;
        self.pixels.fill(color);
    }
}
