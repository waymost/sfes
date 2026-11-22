// @source: anomie-regs#2105
// @source: anomie-regs#2133
// @source: fullsnes#snesppuresolution
// @source: adr-0028
//! One rendered line, as `render_line` hands it to the machine (ADR 0028, point 7).

/// The pixels of a wide line: 512.
pub const WIDE_LINE: usize = 512;
/// The pixels of a normal line: 256.
pub const NORMAL_LINE: usize = 256;

/// One output line: its pixels, CGRAM colours with red in bits 4–0, green in bits 9–5 and
/// blue in bits 14–10, and how it was rendered.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LineOut {
    /// The line it holds, 1 to 239.
    line: u16,
    /// 512 pixels (BG mode 5 or 6, or pseudo-hires) rather than 256.
    wide: bool,
    /// `$2133` bit 0 was set at its render.
    interlaced: bool,
    /// The pixels; only the first 256 count unless the line is wide.
    pixels: [u16; WIDE_LINE],
}

impl LineOut {
    /// An empty line, for `render_line` to fill.
    // @source: adr-0028
    #[must_use]
    pub const fn new() -> Self {
        Self {
            line: 0,
            wide: false,
            interlaced: false,
            pixels: [0; WIDE_LINE],
        }
    }

    /// The line it holds.
    // @source: adr-0028
    #[must_use]
    pub const fn line(&self) -> u16 {
        self.line
    }

    /// Whether it is 512 pixels wide.
    // @source: anomie-regs#2105
    // @source: anomie-regs#2133
    // @source: adr-0028
    #[must_use]
    pub const fn wide(&self) -> bool {
        self.wide
    }

    /// Whether interlace was on at its render.
    // @source: anomie-regs#2133
    // @source: adr-0028
    #[must_use]
    pub const fn interlaced(&self) -> bool {
        self.interlaced
    }

    /// Its pixels: 256, or 512 for a wide line.
    // @source: adr-0028
    #[must_use]
    pub fn pixels(&self) -> &[u16] {
        let width = if self.wide { WIDE_LINE } else { NORMAL_LINE };
        &self.pixels[..width]
    }

    /// Starts `line` with the given shape and every pixel `color`.
    // @source: adr-0028
    pub(crate) fn fill(&mut self, line: u16, wide: bool, interlaced: bool, color: u16) {
        self.line = line;
        self.wide = wide;
        self.interlaced = interlaced;
        self.pixels.fill(color);
    }
}

impl Default for LineOut {
    fn default() -> Self {
        Self::new()
    }
}
