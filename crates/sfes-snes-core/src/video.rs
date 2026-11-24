// @source: anomie-timing#s-ppu/rendering
// @source: fullsnes#snesppuresolution
// @source: adr-0028
//! The video outputs: the lines of the frame being drawn and the last presented frame
//! (ADR 0028, points 8 to 10). They are outputs, not state: the bus's clone starts a new,
//! empty [`Video`], and serialization will skip it (technical design section 5).

use alloc::vec::Vec;
use core::mem;

use sfes_api::{Field, Rgb555, VideoFrame};
use sfes_snes_ppu::{LayerLine, LineOut, LineWidth, Ppu};

/// Which optional outputs are on (ADR 0037, point 6).
#[derive(Clone, Copy, Debug, Default)]
struct Settings {
    /// Render skip is on: lines evaluate their sprites and draw nothing (point 1).
    render_skip: bool,
    /// Layer output is on: lines also fill the layer lines.
    layer_output: bool,
}

/// The frame buffers. They are not `Clone`: a copy of the machine starts with
/// `Video::default()` (ADR 0028, point 10).
#[derive(Debug, Default)]
pub(crate) struct Video {
    /// The frame being drawn: line *L* is entry *L* − 1.
    lines: Vec<LineOut>,
    /// The line expected next in `lines`.
    next_line: u16,
    /// Every line of the frame so far was rendered here, from line 1 on.
    complete: bool,
    /// The last presented frame, row-major.
    frame: Vec<Rgb555>,
    /// Its width in pixels.
    width: u16,
    /// Its height in pixels.
    height: u16,
    /// The field it shows, if it is interlaced.
    field: Option<Field>,
    /// What to render: the two output settings of ADR 0037.
    settings: Settings,
    /// The layers of the frame being drawn: line *L* is entry *L* − 1.
    layer_lines: Vec<LayerLine>,
    /// Layer output was on for every line of the frame being drawn so far.
    layers_complete: bool,
    /// The layers of the last presented frame; empty if layer output was not on for it.
    layer_frame: Vec<LayerLine>,
}

impl Video {
    /// Empty buffers with this one's two settings: what a clone of the machine starts with
    /// (ADR 0037, point 6).
    // @source: adr-0028
    // @source: adr-0037
    pub(crate) fn empty_like(&self) -> Self {
        Self {
            settings: self.settings,
            ..Self::default()
        }
    }

    /// Turns render skip on or off (ADR 0037, point 1).
    // @source: adr-0037
    pub(crate) const fn set_render_skip(&mut self, skip: bool) {
        self.settings.render_skip = skip;
    }

    /// Whether render skip is on.
    // @source: adr-0037
    pub(crate) const fn render_skip(&self) -> bool {
        self.settings.render_skip
    }

    /// Turns layer output on or off (ADR 0037, point 6).
    // @source: adr-0037
    pub(crate) const fn set_layer_output(&mut self, output: bool) {
        self.settings.layer_output = output;
    }

    /// Whether layer output is on.
    // @source: adr-0037
    pub(crate) const fn layer_output(&self) -> bool {
        self.settings.layer_output
    }

    /// The layers of the last presented frame: one entry per output line, none unless layer
    /// output was on for the whole frame (ADR 0037, point 6).
    // @source: adr-0037
    pub(crate) fn layers(&self) -> &[LayerLine] {
        &self.layer_frame
    }

    /// Renders `line` on `ppu` into the frame being drawn. Line 1 starts a frame; a line
    /// that does not follow the one before marks the frame incomplete (ADR 0028,
    /// point 10). With render skip on the line is skipped instead, which makes its frame
    /// incomplete, and with layer output on its layers are kept (ADR 0037).
    // @source: anomie-timing#s-ppu/rendering
    // @source: adr-0028
    // @source: adr-0037
    pub(crate) fn render(&mut self, ppu: &mut Ppu, line: u16) {
        if line == 1 {
            self.complete = true;
            self.layers_complete = true;
        } else if line != self.next_line {
            self.complete = false;
        }
        self.next_line = line + 1;
        if self.settings.render_skip {
            self.complete = false;
            ppu.skip_line(line);
            return;
        }
        let index = usize::from(line.saturating_sub(1));
        if self.lines.len() <= index {
            self.lines.resize_with(index + 1, LineOut::default);
        }
        if self.settings.layer_output && self.layer_lines.len() <= index {
            self.layer_lines.resize_with(index + 1, LayerLine::default);
        }
        let layers = if self.settings.layer_output {
            self.layer_lines.get_mut(index)
        } else {
            self.layers_complete = false;
            None
        };
        if let Some(out) = self.lines.get_mut(index) {
            ppu.render_line_layers(line, out, layers);
        }
    }

    /// Presents the frame whose last output line is `last`, if every line from 1 to `last`
    /// was rendered here (ADR 0028, points 8 and 10):
    ///
    /// - it is 512 wide if any line was wide, and a 256-pixel line then has each pixel
    ///   twice;
    /// - it is `last` rows high, or twice that if any line was interlaced, and each line
    ///   then fills two rows.
    ///
    /// An interlaced frame records `field`, the field it shows (API-03). The next frame must
    /// start again from line 1.
    // @source: anomie-regs#2133
    // @source: anomie-regs#213f
    // @source: fullsnes#snesppuresolution
    // @source: adr-0028
    pub(crate) fn present(&mut self, last: u16, field: Field) {
        let count = usize::from(last);
        let complete = self.complete && self.next_line == last + 1 && count > 0;
        self.complete = false;
        let Some(lines) = self.lines.get(..count).filter(|_| complete) else {
            return;
        };
        let width = if lines.iter().any(|line| line.width() == LineWidth::Wide) {
            LineWidth::Wide
        } else {
            LineWidth::Normal
        };
        let rows = if lines.iter().any(LineOut::interlaced) {
            2
        } else {
            1
        };
        self.frame.clear();
        self.frame.reserve(width.pixels() * count * rows);
        for line in lines {
            let start = self.frame.len();
            if line.width() == width {
                self.frame.extend_from_slice(line.pixels());
            } else {
                for &pixel in line.pixels() {
                    self.frame.extend([pixel, pixel]);
                }
            }
            if rows == 2 {
                self.frame.extend_from_within(start..);
            }
        }
        if self.layers_complete {
            mem::swap(&mut self.layer_frame, &mut self.layer_lines);
            self.layer_frame.truncate(count);
        } else {
            self.layer_frame.clear();
        }
        self.layers_complete = false;
        #[expect(
            clippy::cast_possible_truncation,
            reason = "a line is at most 512 pixels, and a frame at most 239 lines of 2 rows"
        )]
        let (width, height) = (width.pixels() as u16, (count * rows) as u16);
        self.width = width;
        self.height = height;
        self.field = (rows == 2).then_some(field);
    }

    /// Abandons the frame being drawn, as the reset button does (ADR 0028, point 10).
    // @source: adr-0028
    pub(crate) const fn abandon(&mut self) {
        self.complete = false;
        self.layers_complete = false;
    }

    /// The last presented frame; 0 × 0 before the first.
    // @source: adr-0028
    pub(crate) fn frame(&self) -> VideoFrame<'_> {
        VideoFrame {
            width: self.width,
            height: self.height,
            pixels: &self.frame,
            field: self.field,
        }
    }
}

#[cfg(test)]
mod tests {
    use sfes_api::SplitMix64;
    use sfes_snes_ppu::Register;

    use super::*;

    /// A PPU with the screen on at full brightness, whose backdrop is `color`.
    // @source: anomie-regs#2100
    // @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
    // @source: adr-0028
    // @source: adr-0035
    fn ppu(color: Rgb555) -> Ppu {
        let mut ppu = Ppu::power_on(&mut SplitMix64::new(0));
        ppu.write(Register::DisplayControl, 0x0F);
        set_backdrop(&mut ppu, color);
        ppu
    }

    /// Sets the backdrop, CGRAM entry 0, to `color`.
    // @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
    // @source: adr-0028
    fn set_backdrop(ppu: &mut Ppu, color: Rgb555) {
        ppu.cgram_mut()[..2].copy_from_slice(&color.to_le_bytes());
    }

    /// `bits` as a pixel.
    // @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
    fn pixel(bits: u16) -> Rgb555 {
        Rgb555::from_bits_truncate(bits)
    }

    // @source: anomie-timing#s-ppu/rendering
    // @source: adr-0028
    #[test]
    fn a_plain_frame_is_256_by_its_line_count() {
        let mut ppu = ppu(pixel(0x1234));
        let mut video = Video::default();
        for line in 1..=224 {
            video.render(&mut ppu, line);
        }
        video.present(224, Field::First);
        let VideoFrame {
            width,
            height,
            pixels,
            field,
        } = video.frame();
        assert_eq!(field, None);
        assert_eq!((width, height), (256, 224));
        assert!(pixels.iter().all(|&found| found == pixel(0x1234)));
        assert_eq!(pixels.len(), 256 * 224);
    }

    // @source: anomie-regs#2105
    // @source: fullsnes#snesppuresolution
    // @source: adr-0028
    #[test]
    fn one_wide_line_makes_the_frame_512_and_doubles_the_others() {
        let mut ppu = ppu(pixel(0x0001));
        let mut video = Video::default();
        for line in 1..=239 {
            if line == 100 {
                ppu.write(Register::BackgroundMode, 0x05);
                set_backdrop(&mut ppu, pixel(0x0002));
            }
            if line == 101 {
                ppu.write(Register::BackgroundMode, 0x01);
            }
            video.render(&mut ppu, line);
        }
        video.present(239, Field::First);
        let VideoFrame {
            width,
            height,
            pixels,
            field,
        } = video.frame();
        assert_eq!(field, None);
        assert_eq!((width, height), (512, 239));
        let row = |row: usize| &pixels[row * 512..(row + 1) * 512];
        assert!(row(0).iter().all(|&found| found == pixel(0x0001)));
        assert!(row(99).iter().all(|&found| found == pixel(0x0002)));
        assert!(row(100).iter().all(|&found| found == pixel(0x0002)));
    }

    // @source: anomie-regs#2133
    // @source: adr-0028
    #[test]
    fn interlace_fills_two_rows_per_line_from_this_frame_only() {
        let mut ppu = ppu(pixel(0x0003));
        ppu.write(Register::ScreenMode, 0x01);
        let mut video = Video::default();
        for line in 1..=224_u16 {
            set_backdrop(&mut ppu, pixel(line));
            video.render(&mut ppu, line);
        }
        video.present(224, Field::First);
        let VideoFrame {
            width,
            height,
            pixels,
            field,
        } = video.frame();
        assert_eq!((width, height, field), (256, 448, Some(Field::First)));
        for row in 0..448 {
            let line = u16::try_from(row / 2 + 1).unwrap();
            assert_eq!(pixels[row * 256], pixel(line), "row {row}");
        }
    }

    // @source: adr-0028
    #[test]
    fn a_frame_missing_a_line_is_not_presented() {
        let mut ppu = ppu(pixel(0x0004));
        let mut video = Video::default();
        for line in 1..=224 {
            video.render(&mut ppu, line);
        }
        video.present(224, Field::First);
        // A copy of the machine, or a reset, part-way through the next frame.
        let mut copy = Video::default();
        for line in 1..=100 {
            video.render(&mut ppu, line);
        }
        for line in 101..=224 {
            video.render(&mut ppu, line);
            copy.render(&mut ppu, line);
        }
        copy.present(224, Field::First);
        assert_eq!(copy.frame().width, 0);
        video.abandon();
        set_backdrop(&mut ppu, pixel(0x0005));
        video.present(224, Field::First);
        assert!(
            video
                .frame()
                .pixels
                .iter()
                .all(|&found| found == pixel(0x0004))
        );
        // The next whole frame is presented.
        for line in 1..=224 {
            copy.render(&mut ppu, line);
        }
        copy.present(224, Field::First);
        assert_eq!(copy.frame().width, 256);
    }

    // @source: adr-0028
    // @source: adr-0037
    #[test]
    fn a_frame_with_a_skipped_line_is_not_presented_and_the_last_whole_frame_stays() {
        let mut ppu = ppu(pixel(0x0005));
        let mut video = Video::default();
        for line in 1..=224 {
            video.render(&mut ppu, line);
        }
        video.present(224, Field::First);
        ppu.start_frame();
        set_backdrop(&mut ppu, pixel(0x0006));
        video.set_render_skip(true);
        for line in 1..=224 {
            video.render(&mut ppu, line);
        }
        video.present(224, Field::First);
        assert_eq!(ppu.next_line(), 225);
        let VideoFrame { pixels, .. } = video.frame();
        assert!(pixels.iter().all(|&found| found == pixel(0x0005)));
    }

    // @source: adr-0037
    #[test]
    fn layers_are_kept_only_for_a_frame_drawn_whole_with_layer_output_on() {
        let mut ppu = ppu(pixel(0x0007));
        let mut video = Video::default();
        video.set_layer_output(true);
        for line in 1..=224 {
            video.render(&mut ppu, line);
        }
        video.present(224, Field::First);
        assert_eq!(video.layers().len(), 224);
        // Turned off part-way through the next frame: its picture is presented, and no
        // layers, so none are stale.
        ppu.start_frame();
        for line in 1..=224 {
            if line == 100 {
                video.set_layer_output(false);
            }
            video.render(&mut ppu, line);
        }
        video.present(224, Field::First);
        assert_eq!(video.layers().len(), 0);
        assert_eq!(video.frame().pixels.len(), 256 * 224);
        // A copy keeps the settings and starts with empty buffers.
        let copy = video.empty_like();
        assert!(!copy.layer_output() && !copy.render_skip());
        assert_eq!(copy.layers().len(), 0);
        video.set_render_skip(true);
        assert!(video.empty_like().render_skip());
    }
}
