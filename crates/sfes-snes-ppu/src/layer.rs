// @source: anomie-regs#212c
// @source: anomie-regs#212d
// @source: adr-0036
// @source: adr-0037
//! The per-layer images of a line: BG1 to BG4 and the sprites before windows, the layer
//! order, colour math and brightness, on request (ADR 0037).

use sfes_api::Rgb555;

use crate::background::Background;

/// BG1 to BG4 or the sprites: a layer of a line's per-layer image, and the layer a bit of
/// `$212C–$212F` and `$2131` names. Each variant's value is its index and its bit number.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum LayerKind {
    /// BG1.
    Background1 = 0,
    /// BG2.
    Background2 = 1,
    /// BG3.
    Background3 = 2,
    /// BG4.
    Background4 = 3,
    /// The sprites.
    Sprites = 4,
}

impl LayerKind {
    /// Every layer, in the order of [`LayerLine`]'s buffers.
    pub const ALL: [Self; 5] = [
        Self::Background1,
        Self::Background2,
        Self::Background3,
        Self::Background4,
        Self::Sprites,
    ];
}

impl From<Background> for LayerKind {
    // @source: anomie-regs#212c
    fn from(background: Background) -> Self {
        match background {
            Background::One => Self::Background1,
            Background::Two => Self::Background2,
            Background::Three => Self::Background3,
            Background::Four => Self::Background4,
        }
    }
}

bitflags::bitflags! {
    /// A set of layers: `$212C TM`, `$212D TS`, `$212E TMW` and `$212F TSW` bits 4–0, and
    /// `$2131 CGADSUB` bits 4–0. Bit `n` is the layer whose [`LayerKind`] value is `n`.
    // @source: anomie-regs#212c
    // @source: anomie-regs#212d
    // @source: anomie-regs#212e
    // @source: anomie-regs#212f
    // @source: anomie-regs#2131
    #[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
    pub(crate) struct Layers: u8 {
        /// BG1.
        const BACKGROUND_1 = 1 << LayerKind::Background1 as u8;
        /// BG2.
        const BACKGROUND_2 = 1 << LayerKind::Background2 as u8;
        /// BG3.
        const BACKGROUND_3 = 1 << LayerKind::Background3 as u8;
        /// BG4.
        const BACKGROUND_4 = 1 << LayerKind::Background4 as u8;
        /// The sprites.
        const SPRITES = 1 << LayerKind::Sprites as u8;
    }
}

impl From<LayerKind> for Layers {
    // @source: anomie-regs#212c
    fn from(kind: LayerKind) -> Self {
        Self::from_bits_retain(1 << kind as u8)
    }
}

impl From<Background> for Layers {
    // @source: anomie-regs#212c
    fn from(background: Background) -> Self {
        Self::from(LayerKind::from(background))
    }
}

/// How many layers a line holds.
const LAYERS: usize = LayerKind::ALL.len();
/// How many columns a layer's line holds: the pixels of a normal line.
pub const LAYER_COLUMNS: usize = 256;

/// The five layers of one output line, 256 columns each (ADR 0037, points 4 and 5). A
/// column is `None` where the layer is transparent there, absent from the mode or on
/// neither screen, otherwise the layer's colour. The default holds nothing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LayerLine {
    /// Each layer's columns, indexed by [`LayerKind`].
    layers: [[Option<Rgb555>; LAYER_COLUMNS]; LAYERS],
}

impl Default for LayerLine {
    fn default() -> Self {
        Self {
            layers: [[None; LAYER_COLUMNS]; LAYERS],
        }
    }
}

impl LayerLine {
    /// The 256 columns of `kind`.
    // @source: adr-0037
    #[must_use]
    pub const fn layer(&self, kind: LayerKind) -> &[Option<Rgb555>; LAYER_COLUMNS] {
        &self.layers[kind as usize]
    }

    /// The columns of `kind`, for the renderer to set.
    // @source: adr-0037
    pub(crate) const fn layer_mut(
        &mut self,
        kind: LayerKind,
    ) -> &mut [Option<Rgb555>; LAYER_COLUMNS] {
        &mut self.layers[kind as usize]
    }

    /// Empties every layer.
    // @source: adr-0037
    pub(crate) fn clear(&mut self) {
        for layer in &mut self.layers {
            layer.fill(None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: adr-0037
    #[test]
    fn a_new_line_holds_no_layer_pixels() {
        let line = LayerLine::default();
        for kind in LayerKind::ALL {
            assert!(line.layer(kind).iter().all(Option::is_none));
        }
    }

    // @source: adr-0037
    #[test]
    fn a_layer_is_set_and_cleared_on_its_own() {
        let mut line = LayerLine::default();
        line.layer_mut(LayerKind::Background3)[5] = Some(Rgb555::BLACK);
        assert_eq!(line.layer(LayerKind::Background3)[5], Some(Rgb555::BLACK));
        assert_eq!(line.layer(LayerKind::Background2)[5], None);
        line.clear();
        assert_eq!(line.layer(LayerKind::Background3)[5], None);
    }

    // @source: anomie-regs#212c
    // @source: anomie-regs#2131
    #[test]
    fn a_layer_s_bit_is_its_index_and_a_background_s_its_number_less_one() {
        assert_eq!(Layers::from(LayerKind::Sprites).bits(), 0x10);
        assert_eq!(Layers::from(Background::Three), Layers::BACKGROUND_3);
        assert_eq!(Layers::BACKGROUND_3.bits(), 0x04);
        assert_eq!(Layers::all().bits(), 0x1F);
    }
}
