//! Where a frame is drawn in the window: integer scale, centred, black borders (ADR 0039).

/// The logical picture's width: a 512-wide frame is drawn over the same area as a 256-wide one
/// (ADR 0028, point 8; ADR 0039, point 3).
const LOGICAL_WIDTH: u32 = 256;

/// How the picture's pixels are shaped on screen (ADR 0039, points 4 and 5).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum Aspect {
    /// Every source pixel is a square block.
    #[default]
    Square,
    /// The picture is 8/7 wider than with square pixels, for a TV-like view.
    Tv,
}

impl Aspect {
    /// The width of the logical picture at integer scale `scale`.
    const fn display_width(self, scale: u32) -> u32 {
        match self {
            Self::Square => LOGICAL_WIDTH * scale,
            Self::Tv => LOGICAL_WIDTH * scale * 8 / 7,
        }
    }
}

/// A rectangle of the window, in pixels from its top-left corner.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Rect {
    /// Left edge.
    pub(crate) x: u32,
    /// Top edge.
    pub(crate) y: u32,
    /// Width.
    pub(crate) width: u32,
    /// Height.
    pub(crate) height: u32,
}

/// The size of the logical picture of a `frame_height`-row frame: its height, halved when the
/// frame is interlaced (ADR 0039, point 3).
pub(crate) const fn logical_height(frame_height: u32, interlaced: bool) -> u32 {
    if interlaced {
        frame_height / 2
    } else {
        frame_height
    }
}

/// The rectangle that shows a frame in a window.
///
/// - The picture is drawn at the largest whole scale that fits, and at least 1.
/// - A window smaller than the picture at 1× gets the picture scaled down to fit instead,
///   keeping its shape.
/// - The rectangle is centred.
pub(crate) fn layout(
    frame_height: u32,
    interlaced: bool,
    window_width: u32,
    window_height: u32,
    aspect: Aspect,
) -> Rect {
    let logical = logical_height(frame_height, interlaced).max(1);
    let width_at_one = aspect.display_width(1);
    let mut scale = (window_height / logical).max(1);
    while scale > 1 && aspect.display_width(scale) > window_width {
        scale -= 1;
    }
    let mut width = aspect.display_width(scale);
    let mut height = logical * scale;
    if width > window_width || height > window_height {
        // Smaller than 1×: shrink to fit, keeping the shape.
        width = window_width.min(width_at_one * window_height / logical);
        height = window_height.min(logical * window_width / width_at_one);
    }
    Rect {
        x: (window_width - width.min(window_width)) / 2,
        y: (window_height - height.min(window_height)) / 2,
        width,
        height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: u32, y: u32, width: u32, height: u32) -> Rect {
        Rect {
            x,
            y,
            width,
            height,
        }
    }

    #[test]
    fn a_plain_frame_fills_a_window_of_its_own_size() {
        assert_eq!(
            layout(224, false, 256, 224, Aspect::Square),
            rect(0, 0, 256, 224)
        );
    }

    #[test]
    fn the_scale_is_the_largest_whole_one_that_fits_both_ways() {
        // 1000 / 256 = 3, 700 / 224 = 3.
        assert_eq!(
            layout(224, false, 1000, 700, Aspect::Square),
            rect(116, 14, 768, 672)
        );
        // Width limits: 800 / 256 = 3, though 800 / 224 = 3 too; height 600 / 224 = 2.
        assert_eq!(
            layout(224, false, 800, 600, Aspect::Square),
            rect(144, 76, 512, 448)
        );
        // Height limits: 2000 / 256 = 7, 900 / 224 = 4.
        assert_eq!(
            layout(224, false, 2000, 900, Aspect::Square),
            rect(488, 2, 1024, 896)
        );
    }

    #[test]
    fn hi_res_and_interlaced_frames_take_the_area_of_a_plain_one() {
        let plain = layout(224, false, 1024, 896, Aspect::Square);
        // 512 wide, 224 high: the same logical picture.
        assert_eq!(layout(224, false, 1024, 896, Aspect::Square), plain);
        // 256 or 512 wide, 448 high and interlaced.
        assert_eq!(layout(448, true, 1024, 896, Aspect::Square), plain);
        // Overscan: 239 rows, 478 interlaced.
        assert_eq!(
            layout(478, true, 1024, 956, Aspect::Square),
            layout(239, false, 1024, 956, Aspect::Square)
        );
        assert_eq!(
            layout(239, false, 1024, 956, Aspect::Square),
            rect(0, 0, 1024, 956)
        );
    }

    #[test]
    fn a_window_smaller_than_the_picture_shrinks_it_to_fit() {
        let fitted = layout(224, false, 128, 200, Aspect::Square);
        assert_eq!(fitted, rect(0, 44, 128, 112));
        let tall = layout(224, false, 300, 100, Aspect::Square);
        assert_eq!(tall.height, 100);
        assert_eq!(tall.width, 114);
        assert_eq!(tall.x, 93);
    }

    #[test]
    fn a_zero_window_gives_an_empty_rectangle_without_overflow() {
        let empty = layout(224, false, 0, 0, Aspect::Tv);
        assert_eq!((empty.width, empty.height), (0, 0));
    }

    #[test]
    fn the_tv_aspect_is_8_7_wider_with_the_same_rows() {
        // At 1×: 256 * 8 / 7 = 292.
        assert_eq!(
            layout(224, false, 292, 224, Aspect::Tv),
            rect(0, 0, 292, 224)
        );
        // 3× needs 256 * 3 * 8 / 7 = 877 columns; 876 only allows 2×.
        assert_eq!(
            layout(224, false, 877, 672, Aspect::Tv),
            rect(0, 0, 877, 672)
        );
        assert_eq!(
            layout(224, false, 876, 672, Aspect::Tv),
            rect(145, 112, 585, 448)
        );
    }
}
