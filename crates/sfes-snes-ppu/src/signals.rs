// @source: anomie-timing#s-ppu/blanking-periods
// @source: adr-0029
//! What the PPU sees from the machine at an access to its registers (ADR 0029, point 1).

use sfes_api::Field;

/// Which part of a line the beam is in, as the machine's scheduler sees it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Period {
    /// Neither blank: a line is being drawn, or line 0 is being prepared.
    Active,
    /// Horizontal blank, H=274 to H=1, on a line outside vertical blank.
    HorizontalBlank,
    /// Vertical blank, from H=0 of its first line to H=0 of line 0.
    VerticalBlank,
}

/// The machine's state the PPU's registers depend on, passed in before each access.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Signals {
    /// Where the beam is.
    pub period: Period,
    /// $213F STAT78 bit 7: the interlace field.
    pub field: Field,
    /// $213F STAT78 bit 4: the console is PAL.
    pub pal: bool,
    /// $4201 WRIO bit 7: the external latch is enabled.
    pub external_latch: bool,
}

impl Signals {
    /// The signals the PPU holds until the machine first passes its own: active display,
    /// the first field, NTSC and the latch disabled.
    // @source: adr-0029
    pub(crate) const POWER_ON: Self = Self {
        period: Period::Active,
        field: Field::First,
        pal: false,
        external_latch: false,
    };
}
