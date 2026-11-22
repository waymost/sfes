// @source: anomie-regs#registers
// @source: fullsnes#snesiomap
// @source: adr-0028
//! The PPU's 64 registers on Address Bus B, `$2100–$213F` (ADR 0028, point 2).

/// One of the PPU's registers, by its B-bus address. The bus decodes a CPU address to one
/// of these; the PPU never sees the address itself.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum Register {
    /// $2100 INIDISP: forced blank and screen brightness.
    DisplayControl = 0x00,
    /// $2101 OBSEL: sprite sizes and character base.
    ObjectSelect = 0x01,
    /// $2102 OAMADDL: bits 0–7 of the OAM address.
    OamAddressLow = 0x02,
    /// $2103 OAMADDH: bit 8 of the OAM address and sprite priority rotation.
    OamAddressHigh = 0x03,
    /// $2104 OAMDATA: an OAM write.
    OamData = 0x04,
    /// $2105 BGMODE: the BG mode and the BGs' character sizes.
    BackgroundMode = 0x05,
    /// $2106 MOSAIC: mosaic size and the BGs it applies to.
    Mosaic = 0x06,
    /// $2107 BG1SC: BG1's tilemap address and size.
    Background1Tilemap = 0x07,
    /// $2108 BG2SC: BG2's tilemap address and size.
    Background2Tilemap = 0x08,
    /// $2109 BG3SC: BG3's tilemap address and size.
    Background3Tilemap = 0x09,
    /// $210A BG4SC: BG4's tilemap address and size.
    Background4Tilemap = 0x0A,
    /// $210B BG12NBA: BG1's and BG2's character addresses.
    Background12Characters = 0x0B,
    /// $210C BG34NBA: BG3's and BG4's character addresses.
    Background34Characters = 0x0C,
    /// $210D BG1HOFS: BG1's horizontal scroll, and Mode 7's.
    Background1HorizontalScroll = 0x0D,
    /// $210E BG1VOFS: BG1's vertical scroll, and Mode 7's.
    Background1VerticalScroll = 0x0E,
    /// $210F BG2HOFS: BG2's horizontal scroll.
    Background2HorizontalScroll = 0x0F,
    /// $2110 BG2VOFS: BG2's vertical scroll.
    Background2VerticalScroll = 0x10,
    /// $2111 BG3HOFS: BG3's horizontal scroll.
    Background3HorizontalScroll = 0x11,
    /// $2112 BG3VOFS: BG3's vertical scroll.
    Background3VerticalScroll = 0x12,
    /// $2113 BG4HOFS: BG4's horizontal scroll.
    Background4HorizontalScroll = 0x13,
    /// $2114 BG4VOFS: BG4's vertical scroll.
    Background4VerticalScroll = 0x14,
    /// $2115 VMAIN: the VRAM address increment and translation.
    VramControl = 0x15,
    /// $2116 VMADDL: bits 0–7 of the VRAM address.
    VramAddressLow = 0x16,
    /// $2117 VMADDH: bits 8–15 of the VRAM address.
    VramAddressHigh = 0x17,
    /// $2118 VMDATAL: a VRAM write, low byte.
    VramDataLow = 0x18,
    /// $2119 VMDATAH: a VRAM write, high byte.
    VramDataHigh = 0x19,
    /// $211A M7SEL: Mode 7's flips and screen-over setting.
    Mode7Settings = 0x1A,
    /// $211B M7A: Mode 7's matrix parameter A, and the multiplicand of `$2134–$2136`.
    Mode7MatrixA = 0x1B,
    /// $211C M7B: Mode 7's matrix parameter B, and the multiplier of `$2134–$2136`.
    Mode7MatrixB = 0x1C,
    /// $211D M7C: Mode 7's matrix parameter C.
    Mode7MatrixC = 0x1D,
    /// $211E M7D: Mode 7's matrix parameter D.
    Mode7MatrixD = 0x1E,
    /// $211F M7X: Mode 7's centre X.
    Mode7CenterX = 0x1F,
    /// $2120 M7Y: Mode 7's centre Y.
    Mode7CenterY = 0x20,
    /// $2121 CGADD: the CGRAM address.
    CgramAddress = 0x21,
    /// $2122 CGDATA: a CGRAM write.
    CgramData = 0x22,
    /// $2123 W12SEL: the window masks of BG1 and BG2.
    WindowMaskBackground12 = 0x23,
    /// $2124 W34SEL: the window masks of BG3 and BG4.
    WindowMaskBackground34 = 0x24,
    /// $2125 WOBJSEL: the window masks of the sprites and the colour window.
    WindowMaskObjectColor = 0x25,
    /// $2126 WH0: window 1's left edge.
    Window1Left = 0x26,
    /// $2127 WH1: window 1's right edge.
    Window1Right = 0x27,
    /// $2128 WH2: window 2's left edge.
    Window2Left = 0x28,
    /// $2129 WH3: window 2's right edge.
    Window2Right = 0x29,
    /// $212A WBGLOG: the window mask logic of the BGs.
    WindowLogicBackgrounds = 0x2A,
    /// $212B WOBJLOG: the window mask logic of the sprites and the colour window.
    WindowLogicObjectColor = 0x2B,
    /// $212C TM: the main screen's layers.
    MainScreen = 0x2C,
    /// $212D TS: the sub screen's layers.
    SubScreen = 0x2D,
    /// $212E TMW: the layers the windows mask on the main screen.
    MainScreenWindow = 0x2E,
    /// $212F TSW: the layers the windows mask on the sub screen.
    SubScreenWindow = 0x2F,
    /// $2130 CGWSEL: colour math control A.
    ColorMathSelect = 0x30,
    /// $2131 CGADSUB: colour math control B.
    ColorMathDesignation = 0x31,
    /// $2132 COLDATA: the fixed colour.
    FixedColor = 0x32,
    /// $2133 SETINI: external sync, EXTBG, pseudo-hires, overscan and interlace.
    ScreenMode = 0x33,
    /// $2134 MPYL: the multiply result, low byte.
    MultiplyLow = 0x34,
    /// $2135 MPYM: the multiply result, middle byte.
    MultiplyMiddle = 0x35,
    /// $2136 MPYH: the multiply result, high byte.
    MultiplyHigh = 0x36,
    /// $2137 SLHV: a read latches the H/V counters (ADR 0021, point 7).
    SoftwareLatch = 0x37,
    /// $2138 OAMDATAREAD: an OAM read.
    OamDataRead = 0x38,
    /// $2139 VMDATALREAD: a VRAM read, low byte.
    VramDataReadLow = 0x39,
    /// $213A VMDATAHREAD: a VRAM read, high byte.
    VramDataReadHigh = 0x3A,
    /// $213B CGDATAREAD: a CGRAM read.
    CgramDataRead = 0x3B,
    /// $213C OPHCT: the latched H counter.
    HorizontalCounter = 0x3C,
    /// $213D OPVCT: the latched V counter.
    VerticalCounter = 0x3D,
    /// $213E STAT77: PPU1's status and version.
    Ppu1Status = 0x3E,
    /// $213F STAT78: PPU2's status and version.
    Ppu2Status = 0x3F,
}

impl Register {
    /// The register at B-bus address `address`, `$00–$3F`; `None` above `$3F`.
    // @source: anomie-regs#registers
    // @source: fullsnes#snesiomap
    // @source: adr-0028
    #[must_use]
    pub const fn from_b_bus(address: u8) -> Option<Self> {
        Some(match address {
            0x00 => Self::DisplayControl,
            0x01 => Self::ObjectSelect,
            0x02 => Self::OamAddressLow,
            0x03 => Self::OamAddressHigh,
            0x04 => Self::OamData,
            0x05 => Self::BackgroundMode,
            0x06 => Self::Mosaic,
            0x07 => Self::Background1Tilemap,
            0x08 => Self::Background2Tilemap,
            0x09 => Self::Background3Tilemap,
            0x0A => Self::Background4Tilemap,
            0x0B => Self::Background12Characters,
            0x0C => Self::Background34Characters,
            0x0D => Self::Background1HorizontalScroll,
            0x0E => Self::Background1VerticalScroll,
            0x0F => Self::Background2HorizontalScroll,
            0x10 => Self::Background2VerticalScroll,
            0x11 => Self::Background3HorizontalScroll,
            0x12 => Self::Background3VerticalScroll,
            0x13 => Self::Background4HorizontalScroll,
            0x14 => Self::Background4VerticalScroll,
            0x15 => Self::VramControl,
            0x16 => Self::VramAddressLow,
            0x17 => Self::VramAddressHigh,
            0x18 => Self::VramDataLow,
            0x19 => Self::VramDataHigh,
            0x1A => Self::Mode7Settings,
            0x1B => Self::Mode7MatrixA,
            0x1C => Self::Mode7MatrixB,
            0x1D => Self::Mode7MatrixC,
            0x1E => Self::Mode7MatrixD,
            0x1F => Self::Mode7CenterX,
            0x20 => Self::Mode7CenterY,
            0x21 => Self::CgramAddress,
            0x22 => Self::CgramData,
            0x23 => Self::WindowMaskBackground12,
            0x24 => Self::WindowMaskBackground34,
            0x25 => Self::WindowMaskObjectColor,
            0x26 => Self::Window1Left,
            0x27 => Self::Window1Right,
            0x28 => Self::Window2Left,
            0x29 => Self::Window2Right,
            0x2A => Self::WindowLogicBackgrounds,
            0x2B => Self::WindowLogicObjectColor,
            0x2C => Self::MainScreen,
            0x2D => Self::SubScreen,
            0x2E => Self::MainScreenWindow,
            0x2F => Self::SubScreenWindow,
            0x30 => Self::ColorMathSelect,
            0x31 => Self::ColorMathDesignation,
            0x32 => Self::FixedColor,
            0x33 => Self::ScreenMode,
            0x34 => Self::MultiplyLow,
            0x35 => Self::MultiplyMiddle,
            0x36 => Self::MultiplyHigh,
            0x37 => Self::SoftwareLatch,
            0x38 => Self::OamDataRead,
            0x39 => Self::VramDataReadLow,
            0x3A => Self::VramDataReadHigh,
            0x3B => Self::CgramDataRead,
            0x3C => Self::HorizontalCounter,
            0x3D => Self::VerticalCounter,
            0x3E => Self::Ppu1Status,
            0x3F => Self::Ppu2Status,
            _ => return None,
        })
    }

    /// What a read of this register returns (ADR 0028, point 3).
    // @source: anomie-regs#registers
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0028
    #[must_use]
    pub const fn read_kind(self) -> ReadKind {
        match self {
            // $21x4-$21x6 and $21x8-$21xA, x = 0, 1, 2.
            Self::OamData
            | Self::BackgroundMode
            | Self::Mosaic
            | Self::Background2Tilemap
            | Self::Background3Tilemap
            | Self::Background4Tilemap
            | Self::Background4VerticalScroll
            | Self::VramControl
            | Self::VramAddressLow
            | Self::VramDataLow
            | Self::VramDataHigh
            | Self::Mode7Settings
            | Self::WindowMaskBackground34
            | Self::WindowMaskObjectColor
            | Self::Window1Left
            | Self::Window2Left
            | Self::Window2Right
            | Self::WindowLogicBackgrounds => ReadKind::Ppu1OpenBus,
            Self::HorizontalCounter | Self::VerticalCounter => ReadKind::Ppu2,
            Self::MultiplyLow
            | Self::MultiplyMiddle
            | Self::MultiplyHigh
            | Self::OamDataRead
            | Self::VramDataReadLow
            | Self::VramDataReadHigh
            | Self::CgramDataRead
            | Self::Ppu1Status
            | Self::Ppu2Status => ReadKind::Unimplemented,
            // $21x0-$21x3 and $21x7, x = 0 to 3, and $21xB-$21xF, x = 0, 1, 2.
            Self::DisplayControl
            | Self::ObjectSelect
            | Self::OamAddressLow
            | Self::OamAddressHigh
            | Self::Background1Tilemap
            | Self::Background12Characters
            | Self::Background34Characters
            | Self::Background1HorizontalScroll
            | Self::Background1VerticalScroll
            | Self::Background2HorizontalScroll
            | Self::Background2VerticalScroll
            | Self::Background3HorizontalScroll
            | Self::Background3VerticalScroll
            | Self::Background4HorizontalScroll
            | Self::VramAddressHigh
            | Self::Mode7MatrixA
            | Self::Mode7MatrixB
            | Self::Mode7MatrixC
            | Self::Mode7MatrixD
            | Self::Mode7CenterX
            | Self::Mode7CenterY
            | Self::CgramAddress
            | Self::CgramData
            | Self::WindowMaskBackground12
            | Self::Window1Right
            | Self::WindowLogicObjectColor
            | Self::MainScreen
            | Self::SubScreen
            | Self::MainScreenWindow
            | Self::SubScreenWindow
            | Self::ColorMathSelect
            | Self::ColorMathDesignation
            | Self::FixedColor
            | Self::ScreenMode
            | Self::SoftwareLatch => ReadKind::CpuOpenBus,
        }
    }
}

/// What answers a read of a PPU register (ADR 0028, point 3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadKind {
    /// A write-only register that returns PPU1 open bus in all 8 bits.
    Ppu1OpenBus,
    /// A write-only register, or the `$2137` strobe, that returns CPU open bus.
    CpuOpenBus,
    /// A PPU2 register the PPU answers, which sets PPU2 open bus.
    Ppu2,
    /// A readable register #90 implements; until then it reads as CPU open bus.
    Unimplemented,
}

#[cfg(test)]
mod tests {
    use super::*;

    // @source: fullsnes#snesiomap
    // @source: adr-0028
    #[test]
    fn each_b_bus_address_names_its_own_register() {
        for address in 0x00..=0x3F_u8 {
            let register = Register::from_b_bus(address).unwrap();
            assert_eq!(register as u8, address);
        }
        assert_eq!(Register::from_b_bus(0x40), None);
    }

    // @source: anomie-regs#registers
    // @source: fullsnes#snesunpredictablethings
    // @source: adr-0028
    #[test]
    fn reads_follow_the_open_bus_lists() {
        for address in 0x00..=0x3F_u8 {
            let low = address & 0x0F;
            let row = address >> 4;
            let expected = match (row, low) {
                (0..=2, 0x4..=0x6 | 0x8..=0xA) => ReadKind::Ppu1OpenBus,
                (_, 0x0..=0x3 | 0x7) | (0..=2, 0xB..=0xF) => ReadKind::CpuOpenBus,
                (3, 0xC | 0xD) => ReadKind::Ppu2,
                _ => ReadKind::Unimplemented,
            };
            let register = Register::from_b_bus(address).unwrap();
            assert_eq!(
                register.read_kind(),
                expected,
                "${:04X}",
                0x2100 + u16::from(address)
            );
        }
    }
}
