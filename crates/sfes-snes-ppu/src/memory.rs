// @source: anomie-regs#2102
// @source: anomie-regs#2104
// @source: anomie-regs#2115
// @source: anomie-regs#2122
// @source: anomie-regs#2139
// @source: fullsnes#snesmemorycgramaccesspalettememory
// @source: fullsnes#snesmemoryoamaccessspriteattributes
// @source: fullsnes#snesmemoryvramaccesstileandbgmap
// @source: adr-0029
//! VRAM and CGRAM, and the VRAM, CGRAM and OAM ports: their addresses, latches and
//! increments (ADR 0029, points 3 to 5). The PPU owns the memories; each port works on its
//! memory.

use alloc::{boxed::Box, vec};
use core::ops::{Deref, DerefMut};

use sfes_api::Rgb555;

/// VRAM: 64 KiB.
pub const VRAM_SIZE: usize = 0x1_0000;
/// CGRAM: 256 colours of 2 bytes.
pub const CGRAM_SIZE: usize = 0x200;
/// How many sprites OAM holds.
pub(crate) const SPRITE_COUNT: u8 = 128;
/// The bytes of one sprite's entry in OAM's low table.
pub(crate) const OAM_ENTRY_BYTES: u16 = 4;
/// Where OAM's high table starts: after the low table's entries, at `$200`.
pub(crate) const OAM_HIGH_TABLE: u16 = SPRITE_COUNT as u16 * OAM_ENTRY_BYTES;
/// The high table holds 2 bits for each sprite: 32 bytes.
const OAM_HIGH_TABLE_SIZE: u16 = SPRITE_COUNT as u16 / 4;
/// OAM: 512 bytes of sprite entries and a 32-byte high table.
pub const OAM_SIZE: usize = (OAM_HIGH_TABLE + OAM_HIGH_TABLE_SIZE) as usize;
/// VRAM word addresses have 15 bits: bit 15 is not connected.
const VRAM_WORD_MASK: u16 = 0x7FFF;
/// VRAM byte addresses have 16 bits.
const VRAM_BYTE_MASK: u32 = 0xFFFF;
/// `$2115` bit 7: increment after the high byte.
const INCREMENT_ON_HIGH: u8 = 0x80;
/// CGRAM byte addresses have 9 bits.
const CGRAM_ADDRESS_MASK: u16 = 0x1FF;
/// The 7 bits of a CGRAM colour's high byte; bit 7 is not stored.
const CGRAM_HIGH_MASK: u8 = 0x7F;
/// OAM byte addresses have 10 bits.
const OAM_ADDRESS_MASK: u16 = 0x3FF;
/// The high table's 32 bytes, mirrored up to `$3FF`.
const OAM_HIGH_TABLE_MASK: u16 = OAM_HIGH_TABLE_SIZE - 1;
/// `$2103` bit 7: the priority bit.
const OAM_PRIORITY: u8 = 0x80;
/// `$2103` bit 0: bit 8 of the reload value.
const OAM_RELOAD_HIGH: u8 = 0x01;
/// `$2115` at power-on: fullsnes's "?Fh" with the unknown high nibble taken as 0.
const VRAM_CONTROL_POWER_ON: u8 = 0x0F;

/// One byte of a 16-bit port or VRAM word. Each variant's value is its place in the word.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u8)]
pub(crate) enum PortByte {
    /// The low byte: `$2118` or `$2139`.
    Low = 0,
    /// The high byte: `$2119` or `$213A`.
    High = 1,
}

/// VRAM, 64 KiB, read by 15-bit word address or 16-bit byte address (ADR 0029, point 3).
/// It derefs to its bytes, which the machine reads and writes between steps.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Vram(Box<[u8]>);

impl Vram {
    /// 64 KiB of zeros.
    // @source: adr-0028
    pub(crate) fn zeroed() -> Self {
        Self(vec![0; VRAM_SIZE].into_boxed_slice())
    }

    /// The offset of `byte` of word `address`, with bit 15 dropped.
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    // @source: adr-0029
    fn offset(address: u16, byte: PortByte) -> usize {
        usize::from(address & VRAM_WORD_MASK) * 2 + usize::from(byte as u8)
    }

    /// Word `address`, little-endian, with bit 15 dropped (ADR 0029, point 3).
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    // @source: adr-0029
    pub(crate) fn word(&self, address: u16) -> u16 {
        let at = |byte| {
            let offset = Self::offset(address, byte);
            self.0.get(offset).copied().unwrap_or_default()
        };
        u16::from_le_bytes([at(PortByte::Low), at(PortByte::High)])
    }

    /// Byte `address`, wrapped to 64 KiB (ADR 0030, point 3).
    // @source: anomie-regs#backgrounds/tile-maps-and-character-maps
    // @source: adr-0030
    pub(crate) fn byte(&self, address: u32) -> u8 {
        let offset = usize::try_from(address & VRAM_BYTE_MASK).unwrap_or_default();
        self.0.get(offset).copied().unwrap_or_default()
    }

    /// Writes `byte` of word `address`, with bit 15 dropped.
    // @source: anomie-regs#2118
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    // @source: adr-0029
    pub(crate) fn write(&mut self, address: u16, byte: PortByte, value: u8) {
        if let Some(slot) = self.0.get_mut(Self::offset(address, byte)) {
            *slot = value;
        }
    }
}

impl Deref for Vram {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl DerefMut for Vram {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

/// CGRAM, 256 colours of 2 bytes. It derefs to its bytes, which the port and the machine
/// read and write.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Cgram(Box<[u8]>);

impl Cgram {
    /// 512 bytes of zeros.
    // @source: adr-0028
    pub(crate) fn zeroed() -> Self {
        Self(vec![0; CGRAM_SIZE].into_boxed_slice())
    }

    /// Entry `index` as a 15-bit colour, little-endian.
    // @source: anomie-regs#213b
    // @source: fullsnes#snesppucolorpalettememorycgramanddirectcolors
    pub(crate) fn color(&self, index: u8) -> Rgb555 {
        let offset = usize::from(index) * 2;
        let low = self.0.get(offset).copied().unwrap_or_default();
        let high = self.0.get(offset + 1).copied().unwrap_or_default();
        Rgb555::from_le_bytes([low, high])
    }
}

impl Deref for Cgram {
    type Target = [u8];

    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl DerefMut for Cgram {
    fn deref_mut(&mut self) -> &mut [u8] {
        &mut self.0
    }
}

/// `$2115` bits 3–2: how many low bits of the word address are rotated left by three. Each
/// variant's value is that width.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
enum Translation {
    /// No translation.
    None = 0,
    /// `aaaaaaaaBBBccccc` becomes `aaaaaaaacccccBBB`.
    Rotate8 = 8,
    /// `aaaaaaaBBBcccccc` becomes `aaaaaaaccccccBBB`.
    Rotate9 = 9,
    /// `aaaaaaBBBccccccc` becomes `aaaaaacccccccBBB`.
    Rotate10 = 10,
}

impl Translation {
    /// The translation `$2115` bits 3–2 select.
    // @source: anomie-regs#2115
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    const fn from_control(control: u8) -> Self {
        match (control >> 2) & 0x03 {
            0 => Self::None,
            1 => Self::Rotate8,
            2 => Self::Rotate9,
            _ => Self::Rotate10,
        }
    }

    /// `address` translated: its low bits rotated left by three.
    // @source: anomie-regs#2115
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    const fn apply(self, address: u16) -> u16 {
        let width = self as u16;
        if width == 0 {
            return address;
        }
        let mask = (1 << width) - 1;
        let low = address & mask;
        let rotated = ((low << 3) | (low >> (width - 3))) & mask;
        (address & !mask) | rotated
    }
}

/// `$2115` bits 1–0: the word-address step. Each variant's value is the step.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u16)]
enum Step {
    /// 1 word.
    One = 1,
    /// 32 words.
    ThirtyTwo = 32,
    /// 128 words.
    OneHundredTwentyEight = 128,
}

impl Step {
    /// The step `$2115` bits 1–0 select: 1, 32, 128 or 128.
    // @source: anomie-regs#2115
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    const fn from_control(control: u8) -> Self {
        match control & 0x03 {
            0 => Self::One,
            1 => Self::ThirtyTwo,
            _ => Self::OneHundredTwentyEight,
        }
    }
}

/// $2115 VMAIN, decoded.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VramControl {
    /// The byte whose access increments the address.
    increment_on: PortByte,
    /// The address translation.
    translation: Translation,
    /// The increment.
    step: Step,
}

impl VramControl {
    /// `$2115` as written.
    // @source: anomie-regs#2115
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    const fn from_byte(control: u8) -> Self {
        let increment_on = if control & INCREMENT_ON_HIGH == 0 {
            PortByte::Low
        } else {
            PortByte::High
        };
        Self {
            increment_on,
            translation: Translation::from_control(control),
            step: Step::from_control(control),
        }
    }
}

/// The VRAM port: `$2115–$2119`, `$2139` and `$213A` (ADR 0029, point 3).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct VramPort {
    /// $2115 VMAIN: the increment and the translation.
    control: VramControl,
    /// $2116/$2117 VMADDL/VMADDH: the word address, untranslated.
    address: u16,
    /// The word `$2139/$213A` return.
    prefetch: u16,
}

impl VramPort {
    /// The port at power-on: `$2115` is `$0F`, the address and prefetch 0 (ADR 0029,
    /// points 3 and 12). Reset keeps it.
    // @source: fullsnes#snesiomap
    // @source: adr-0029
    pub(crate) const fn power_on() -> Self {
        Self {
            control: VramControl::from_byte(VRAM_CONTROL_POWER_ON),
            address: 0,
            prefetch: 0,
        }
    }

    /// $2115 VMAIN: the increment and translation.
    // @source: anomie-regs#2115
    // @source: adr-0029
    pub(crate) const fn write_control(&mut self, value: u8) {
        self.control = VramControl::from_byte(value);
    }

    /// $2116/$2117 VMADDL/VMADDH: one byte of the word address, then a prefetch from it.
    // @source: anomie-regs#2116
    // @source: anomie-regs#2117
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    // @source: adr-0029
    pub(crate) fn write_address(&mut self, vram: &Vram, byte: PortByte, value: u8) {
        let [low, high] = self.address.to_le_bytes();
        self.address = match byte {
            PortByte::Low => u16::from_le_bytes([value, high]),
            PortByte::High => u16::from_le_bytes([low, value]),
        };
        self.prefetch = self.load(vram);
    }

    /// $2118/$2119 VMDATAL/VMDATAH: writes one byte of the addressed word if `allowed`,
    /// then increments the address if `$2115` names this byte. The prefetch is untouched.
    // @source: anomie-regs#2118
    // @source: anomie-regs#2119
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    // @source: adr-0029
    pub(crate) fn write_data(&mut self, vram: &mut Vram, byte: PortByte, value: u8, allowed: bool) {
        if allowed {
            vram.write(self.translated(), byte, value);
        }
        if self.control.increment_on == byte {
            self.increment();
        }
    }

    /// $2139/$213A VMDATALREAD/VMDATAHREAD: one byte of the prefetch.
    // @source: anomie-regs#2139
    // @source: anomie-regs#213a
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    pub(crate) const fn peek(&self, byte: PortByte) -> u8 {
        let [low, high] = self.prefetch.to_le_bytes();
        match byte {
            PortByte::Low => low,
            PortByte::High => high,
        }
    }

    /// After a read of `byte`: if `$2115` names it, the prefetch loads from the address and
    /// then the address increments.
    // @source: anomie-regs#2139
    // @source: anomie-regs#213a
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    // @source: adr-0029
    pub(crate) fn after_read(&mut self, vram: &Vram, byte: PortByte) {
        if self.control.increment_on == byte {
            self.prefetch = self.load(vram);
            self.increment();
        }
    }

    /// The word at the translated address.
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    fn load(&self, vram: &Vram) -> u16 {
        vram.word(self.translated())
    }

    /// The addressed word, translated as `$2115` says.
    // @source: anomie-regs#2115
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    const fn translated(&self) -> u16 {
        self.control.translation.apply(self.address)
    }

    /// Steps the address on, wrapping at 16 bits.
    // @source: anomie-regs#2115
    const fn increment(&mut self) {
        self.address = self.address.wrapping_add(self.control.step as u16);
    }

    /// The word address, untranslated.
    // @source: anomie-regs#2116
    #[cfg(test)]
    pub(crate) const fn address(&self) -> u16 {
        self.address
    }
}

/// The CGRAM port: `$2121`, `$2122` and `$213B` (ADR 0029, point 4).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CgramPort {
    /// The byte address: twice `$2121`, plus the flip-flop in bit 0.
    address: u16,
    /// The low byte a `$2122` write holds until the high byte comes.
    latch: u8,
}

impl CgramPort {
    /// The port at power-on: address and latch 0 (ADR 0029, point 12). Reset keeps it.
    // @source: adr-0029
    pub(crate) const fn power_on() -> Self {
        Self {
            address: 0,
            latch: 0,
        }
    }

    /// $2121 CGADD: the colour, and the flip-flop back to the first access.
    // @source: anomie-regs#2121
    // @source: fullsnes#snesmemorycgramaccesspalettememory
    pub(crate) const fn write_address(&mut self, value: u8) {
        self.address = value as u16 * 2;
    }

    /// $2122 CGDATA: the low byte is held; the high byte writes the colour if `allowed`.
    // @source: anomie-regs#2122
    // @source: fullsnes#snesmemorycgramaccesspalettememory
    // @source: adr-0029
    pub(crate) fn write(&mut self, cgram: &mut Cgram, value: u8, allowed: bool) {
        let index = usize::from(self.address);
        if index.is_multiple_of(2) {
            self.latch = value;
        } else if allowed && let Some(color) = cgram.get_mut(index - 1..=index) {
            color.copy_from_slice(&[self.latch, value & CGRAM_HIGH_MASK]);
        }
        self.step();
    }

    /// $213B CGDATAREAD: the addressed byte, with `open_bus` in bit 7 of a high byte.
    // @source: anomie-regs#213b
    // @source: fullsnes#snesmemorycgramaccesspalettememory
    // @source: adr-0029
    pub(crate) fn peek(self, cgram: &Cgram, open_bus: u8) -> u8 {
        let byte = cgram
            .get(usize::from(self.address))
            .copied()
            .unwrap_or_default();
        if self.address.is_multiple_of(2) {
            byte
        } else {
            (byte & CGRAM_HIGH_MASK) | (open_bus & !CGRAM_HIGH_MASK)
        }
    }

    /// Steps the address on after an access, wrapping at 512.
    // @source: fullsnes#snesmemorycgramaccesspalettememory
    pub(crate) const fn step(&mut self) {
        self.address = (self.address + 1) & CGRAM_ADDRESS_MASK;
    }
}

/// The OAM port: `$2102–$2104` and `$2138` (ADR 0029, points 5 and 6).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OamPort {
    /// $2102/$2103 OAMADDL/OAMADDH: the 9-bit word address last written.
    reload: u16,
    /// $2103 bit 7: the priority bit, for the sprites.
    priority: bool,
    /// The 10-bit byte address: bit 9 picks the high table.
    address: u16,
    /// The low byte a `$2104` write holds until the high byte comes.
    latch: u8,
}

impl OamPort {
    /// The port at power-on: all 0 (ADR 0029, point 12). Reset keeps it.
    // @source: adr-0029
    pub(crate) const fn power_on() -> Self {
        Self {
            reload: 0,
            priority: false,
            address: 0,
            latch: 0,
        }
    }

    /// $2102 OAMADDL: bits 7–0 of the reload value, which then becomes the address.
    // @source: anomie-regs#2102
    // @source: fullsnes#snesmemoryoamaccessspriteattributes
    pub(crate) const fn write_address_low(&mut self, value: u8) {
        self.reload = (self.reload & 0x100) | value as u16;
        self.reload();
    }

    /// $2103 OAMADDH: bit 8 of the reload value and the priority bit; the reload value then
    /// becomes the address.
    // @source: anomie-regs#2103
    // @source: fullsnes#snesmemoryoamaccessspriteattributes
    pub(crate) const fn write_address_high(&mut self, value: u8) {
        let high = if value & OAM_RELOAD_HIGH == 0 {
            0
        } else {
            0x100
        };
        self.reload = (self.reload & 0xFF) | high;
        self.priority = value & OAM_PRIORITY != 0;
        self.reload();
    }

    /// The OAM address reset: the address becomes the reload value's first byte (ADR 0029,
    /// point 6).
    // @source: anomie-regs#2102
    // @source: anomie-timing#s-ppu/oam-reset
    // @source: fullsnes#snesmemoryoamaccessspriteattributes
    // @source: adr-0029
    pub(crate) const fn reload(&mut self) {
        self.address = self.reload << 1;
    }

    /// $2104 OAMDATA: an even byte is held; an odd low-table byte writes the word, and a
    /// high-table byte writes itself, if `allowed`.
    // @source: anomie-regs#2104
    // @source: anomie-regs#sprites/oam
    // @source: fullsnes#snesmemoryoamaccessspriteattributes
    // @source: adr-0029
    pub(crate) fn write(&mut self, oam: &mut [u8], value: u8, allowed: bool) {
        let index = usize::from(self.address);
        if index.is_multiple_of(2) {
            self.latch = value;
        }
        if allowed {
            if self.address >= OAM_HIGH_TABLE {
                if let Some(byte) = oam.get_mut(Self::index(self.address)) {
                    *byte = value;
                }
            } else if !index.is_multiple_of(2)
                && let Some(word) = oam.get_mut(index - 1..=index)
            {
                word.copy_from_slice(&[self.latch, value]);
            }
        }
        self.step();
    }

    /// $2138 OAMDATAREAD: the addressed byte.
    // @source: anomie-regs#2138
    // @source: fullsnes#snesmemoryoamaccessspriteattributes
    pub(crate) fn peek(self, oam: &[u8]) -> u8 {
        oam.get(Self::index(self.address))
            .copied()
            .unwrap_or_default()
    }

    /// Steps the address on after an access, wrapping at `$400`.
    // @source: anomie-regs#sprites/oam
    // @source: fullsnes#snesmemoryoamaccessspriteattributes
    pub(crate) const fn step(&mut self) {
        self.address = (self.address + 1) & OAM_ADDRESS_MASK;
    }

    /// The sprite in front: sprite 0, or with `$2103` bit 7 set the one the internal word
    /// address names, `(word >> 1) & 127` (ADR 0033, point 6).
    // @source: anomie-regs#2103
    // @source: anomie-regs#sprites/sprite-priority
    // @source: adr-0033
    pub(crate) const fn first_sprite(self) -> u8 {
        if self.priority {
            ((self.address >> 2) & 0x7F) as u8
        } else {
            0
        }
    }

    /// The OAM offset of byte address `address`: the high table's 32 bytes repeat from
    /// `$200` up.
    // @source: anomie-regs#sprites/oam
    // @source: fullsnes#snesmemoryoamaccessspriteattributes
    fn index(address: u16) -> usize {
        if address >= OAM_HIGH_TABLE {
            usize::from(OAM_HIGH_TABLE + (address & OAM_HIGH_TABLE_MASK))
        } else {
            usize::from(address)
        }
    }

    /// The priority bit, `$2103` bit 7.
    // @source: anomie-regs#2103
    #[cfg(test)]
    pub(crate) const fn priority(self) -> bool {
        self.priority
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use super::*;

    // @source: anomie-regs#2115
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    #[test]
    fn every_2115_byte_names_the_translation_of_bits_3_to_2_and_the_step_of_bits_1_to_0() {
        let translations = [
            Translation::None,
            Translation::Rotate8,
            Translation::Rotate9,
            Translation::Rotate10,
        ];
        let steps = [
            Step::One,
            Step::ThirtyTwo,
            Step::OneHundredTwentyEight,
            Step::OneHundredTwentyEight,
        ];
        for value in 0..=u8::MAX {
            let translation = translations[usize::from(value >> 2 & 0x03)];
            let step = steps[usize::from(value & 0x03)];
            assert_eq!(
                Translation::from_control(value),
                translation,
                "{value:#04X}"
            );
            assert_eq!(Step::from_control(value), step, "{value:#04X}");
        }
    }

    // @source: anomie-regs#2115
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    #[test]
    fn translation_rotates_the_low_bits_left_by_three() {
        // anomie's example: $0003 in the 8-bit mode is word $0018.
        assert_eq!(Translation::Rotate8.apply(0x0003), 0x0018);
        // aaaaaaaaBBBccccc -> aaaaaaaacccccBBB, and the 9- and 10-bit forms.
        assert_eq!(Translation::Rotate8.apply(0xAB_E1), 0xAB_0F);
        // aaaaaaa 111 000001 -> aaaaaaa 000001 111.
        assert_eq!(Translation::Rotate9.apply(0xABC1), 0xAA0F);
        // aaaaaa 111 0000001 -> aaaaaa 0000001 111.
        assert_eq!(Translation::Rotate10.apply(0xAB81), 0xA80F);
        assert_eq!(Translation::None.apply(0x1234), 0x1234);
    }

    // @source: anomie-regs#2115
    // @source: anomie-regs#2118
    // @source: anomie-regs#2119
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    // @source: adr-0029
    #[test]
    fn vram_writes_increment_on_the_byte_vmain_names() {
        let mut vram = Vram::zeroed();
        let mut port = VramPort::power_on();
        port.write_control(0x80);
        port.write_address(&vram, PortByte::Low, 0x10);
        port.write_address(&vram, PortByte::High, 0x00);
        port.write_data(&mut vram, PortByte::Low, 0x34, true);
        assert_eq!(port.address(), 0x0010);
        port.write_data(&mut vram, PortByte::High, 0x12, true);
        assert_eq!(port.address(), 0x0011);
        assert_eq!(vram[0x20..0x22], [0x34, 0x12]);
        // Low-byte increments of 32; a refused write still increments.
        port.write_control(0x01);
        port.write_data(&mut vram, PortByte::Low, 0x56, false);
        assert_eq!(port.address(), 0x0031);
        assert_eq!(vram[0x22], 0);
        port.write_data(&mut vram, PortByte::Low, 0x78, true);
        assert_eq!(vram[0x62], 0x78);
        assert_eq!(port.address(), 0x0051);
        // Steps of 128; bit 15 is not connected, so $8051 is word $0051.
        port.write_control(0x02);
        port.write_address(&vram, PortByte::High, 0x80);
        port.write_data(&mut vram, PortByte::Low, 0x9A, true);
        assert_eq!(vram[0xA2], 0x9A);
        assert_eq!(port.address(), 0x80D1);
        port.write_control(0x03);
        port.write_data(&mut vram, PortByte::Low, 0x00, true);
        assert_eq!(port.address(), 0x8151);
        // The address wraps at 16 bits.
        port.write_address(&vram, PortByte::Low, 0xFF);
        port.write_address(&vram, PortByte::High, 0xFF);
        port.write_control(0x00);
        port.write_data(&mut vram, PortByte::Low, 0x00, true);
        assert_eq!(port.address(), 0x0000);
    }

    // @source: anomie-regs#2115
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    // @source: adr-0029
    #[test]
    fn translation_applies_to_the_access_not_the_address() {
        let mut vram = Vram::zeroed();
        let mut port = VramPort::power_on();
        port.write_control(0x04);
        port.write_address(&vram, PortByte::Low, 0x03);
        port.write_address(&vram, PortByte::High, 0x00);
        port.write_data(&mut vram, PortByte::Low, 0xAA, true);
        // Word $0018, and the address register moves on from $0003.
        assert_eq!(vram[0x30], 0xAA);
        assert_eq!(port.address(), 0x0004);
    }

    // @source: anomie-regs#2139
    // @source: anomie-regs#213a
    // @source: fullsnes#snesmemoryvramaccesstileandbgmap
    // @source: adr-0029
    #[test]
    fn vram_reads_return_the_prefetch_then_load_before_incrementing() {
        let mut vram = Vram::zeroed();
        vram[0x200..0x206].copy_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]);
        let mut port = VramPort::power_on();
        port.write_control(0x80);
        port.write_address(&vram, PortByte::Low, 0x00);
        // Each address write prefetches; after the high byte the prefetch holds word $0100.
        port.write_address(&vram, PortByte::High, 0x01);
        let read = |port: &mut VramPort, byte| {
            let value = port.peek(byte);
            port.after_read(&vram, byte);
            value
        };
        assert_eq!(read(&mut port, PortByte::Low), 0x11);
        assert_eq!(read(&mut port, PortByte::High), 0x22);
        // The prefetch was loaded from the old address, $0100, before the increment, so
        // the first word comes twice.
        assert_eq!(read(&mut port, PortByte::Low), 0x11);
        assert_eq!(read(&mut port, PortByte::High), 0x22);
        assert_eq!(read(&mut port, PortByte::High), 0x44);
        assert_eq!(port.address(), 0x0103);
        // A write does not touch the prefetch.
        port.write_data(&mut vram, PortByte::Low, 0xEE, true);
        assert_eq!(port.peek(PortByte::Low), 0x55);
    }

    // @source: anomie-regs#2121
    // @source: anomie-regs#2122
    // @source: anomie-regs#213b
    // @source: fullsnes#snesmemorycgramaccesspalettememory
    // @source: adr-0029
    #[test]
    fn cgram_writes_a_word_on_its_high_byte() {
        let mut cgram = Cgram::zeroed();
        let mut port = CgramPort::power_on();
        port.write_address(0x10);
        port.write(&mut cgram, 0x1F, true);
        assert_eq!(cgram[0x20], 0);
        // Bit 15 is not stored.
        port.write(&mut cgram, 0xFC, true);
        assert_eq!(cgram[0x20..0x22], [0x1F, 0x7C]);
        // A refused high byte changes nothing, but the address moves on.
        port.write(&mut cgram, 0x01, true);
        port.write(&mut cgram, 0x02, false);
        assert_eq!(cgram[0x22..0x24], [0, 0]);
        port.write_address(0x10);
        assert_eq!(port.peek(&cgram, 0xFF), 0x1F);
        port.step();
        // The high byte's bit 7 is PPU2 open bus.
        assert_eq!(port.peek(&cgram, 0x80), 0xFC);
        assert_eq!(port.peek(&cgram, 0x00), 0x7C);
        // The address wraps at 512 bytes.
        cgram[0] = 0x42;
        port.write_address(0xFF);
        port.step();
        port.step();
        assert_eq!(port.peek(&cgram, 0), 0x42);
    }

    // @source: anomie-regs#2104
    // @source: fullsnes#snesmemoryoamaccessspriteattributes
    // @source: adr-0029
    #[test]
    fn oam_follows_anomies_two_examples() {
        // Address 0, write 1, 2, read, write 3: "01 02 01 03".
        let mut oam = vec![0; 0x220];
        let mut port = OamPort::power_on();
        port.write(&mut oam, 1, true);
        port.write(&mut oam, 2, true);
        let _ = port.peek(&oam);
        port.step();
        port.write(&mut oam, 3, true);
        assert_eq!(oam[..4], [1, 2, 1, 3]);
        // From zeros: write 1, read, read, write 2, read, write 3: "00 00 01 02 01 03".
        let mut oam = vec![0; 0x220];
        let mut port = OamPort::power_on();
        port.write(&mut oam, 1, true);
        port.step();
        port.step();
        port.write(&mut oam, 2, true);
        port.step();
        port.write(&mut oam, 3, true);
        assert_eq!(oam[..6], [0, 0, 1, 2, 1, 3]);
    }

    // @source: anomie-regs#2102
    // @source: anomie-regs#2103
    // @source: anomie-regs#sprites/oam
    // @source: fullsnes#snesmemoryoamaccessspriteattributes
    // @source: adr-0029
    #[test]
    fn the_high_table_is_written_at_once_and_mirrored() {
        let mut oam = vec![0; 0x220];
        let mut port = OamPort::power_on();
        port.write_address_low(0x00);
        port.write_address_high(0x81);
        assert!(port.priority());
        port.write(&mut oam, 0xAB, true);
        assert_eq!(oam[0x200], 0xAB);
        // A refused write leaves OAM alone.
        port.write(&mut oam, 0xCD, false);
        assert_eq!(oam[0x201], 0);
        // $220-$3FF mirror $200-$21F, and the address wraps at $400.
        port.write_address_low(0x10);
        assert_eq!(port.peek(&oam), 0xAB);
        port.write_address_low(0xFF);
        port.step();
        port.step();
        assert_eq!(port.peek(&oam), oam[0]);
        // Anomie's example: set $104, write 4 bytes, write $1 to $2103: back to word 4.
        port.write_address_low(0x04);
        port.write_address_high(0x01);
        for value in 0..4 {
            port.write(&mut oam, value, true);
        }
        port.write_address_high(0x01);
        port.write(&mut oam, 0x77, true);
        assert_eq!(oam[0x208], 0x77);
    }
}
