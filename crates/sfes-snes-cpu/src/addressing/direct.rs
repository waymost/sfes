// @source: wdc-w65c816s#3.5.10
// @source: wdc-w65c816s#3.5.11
// @source: wdc-w65c816s#3.5.12
// @source: wdc-w65c816s#3.5.13
// @source: wdc-w65c816s#3.5.14
// @source: wdc-w65c816s#3.5.15
// @source: wdc-w65c816s#3.5.16
// @source: wdc-w65c816s#3.5.17
// @source: wdc-w65c816s#7.2
//! Direct addressing: an 8-bit offset from the Direct Register, always in bank 0.
//!
//! In emulation mode with a page-aligned Direct Register (DL = 0) the direct page wraps at
//! its page boundary, as the zero page of a 65C02 does. The `[d]` and `[d],y` modes are the
//! exception: their pointers run on into the next page (datasheet 7.2.1 and 7.2.2).

use sfes_api::Addr24;

use crate::{
    addressing::{Operand, indexed},
    bus::Bus65816,
    registers::Cpu65816,
};

impl Cpu65816 {
    /// Row 10a, cycles 2 and 2a. The data is at 0,D+DO.
    // @source: wdc-w65c816s#table-5-7-10a
    // @source: wdc-w65c816s#3.5.17
    pub(super) fn direct(&mut self, bus: &mut impl Bus65816) -> Operand {
        let offset = self.fetch_direct_offset(bus);
        Operand::in_bank_zero(self.direct_address(offset, 0))
    }

    /// Rows 16a (`d,x`) and 17 (`d,y`), cycles 2 to 3: the offset, the optional cycle, and
    /// an internal cycle for the indexing. The data is at 0,D+DO plus the index register.
    // @source: wdc-w65c816s#table-5-7-16a
    // @source: wdc-w65c816s#table-5-7-17
    // @source: wdc-w65c816s#3.5.11
    pub(super) fn direct_indexed(&mut self, bus: &mut impl Bus65816, index: u16) -> Operand {
        let offset = self.fetch_direct_offset(bus);
        self.internal_at_last_fetch(bus);
        Operand::in_bank_zero(self.direct_address(offset, index))
    }

    /// Row 12, cycles 2 to 4: the offset, the optional cycle, and the pointer "AAL", "AAH"
    /// at 0,D+DO. The data is at DBR,AA.
    // @source: wdc-w65c816s#table-5-7-12
    // @source: wdc-w65c816s#3.5.16
    pub(super) fn direct_indirect(&mut self, bus: &mut impl Bus65816) -> Operand {
        let offset = self.fetch_direct_offset(bus);
        let pointer = self.read_pointer(bus, self.direct_address(offset, 0));
        Operand::spanning_banks(Addr24::from_bank_offset(self.dbr, pointer))
    }

    /// Row 11, cycles 2 to 5: the offset, the optional cycle, an internal cycle for the
    /// indexing, and the pointer at 0,D+DO+X. The data is at DBR,AA.
    // @source: wdc-w65c816s#table-5-7-11
    // @source: wdc-w65c816s#3.5.10
    pub(super) fn direct_indexed_indirect(&mut self, bus: &mut impl Bus65816) -> Operand {
        let offset = self.fetch_direct_offset(bus);
        self.internal_at_last_fetch(bus);
        let pointer = self.read_pointer(bus, self.direct_address(offset, self.x));
        Operand::spanning_banks(Addr24::from_bank_offset(self.dbr, pointer))
    }

    /// Row 13, cycles 2 to 4a: the offset, the optional cycle, the pointer at 0,D+DO, and
    /// the optional internal cycle of note 4. The data is at DBR,AA plus Y, a sum that
    /// carries into the bank.
    // @source: wdc-w65c816s#table-5-7-13
    // @source: wdc-w65c816s#3.5.13
    pub(super) fn direct_indirect_indexed(&mut self, bus: &mut impl Bus65816) -> Operand {
        let offset = self.fetch_direct_offset(bus);
        let pointer = self.read_pointer(bus, self.direct_address(offset, 0));
        Operand::spanning_banks(self.index_data_address(bus, pointer, self.y))
    }

    /// Row 15, cycles 2 to 5: the offset, the optional cycle, and the 24-bit pointer "AAL",
    /// "AAH", "AAB" at 0,D+DO. The data is at AAB,AA.
    // @source: wdc-w65c816s#table-5-7-15
    // @source: wdc-w65c816s#3.5.15
    pub(super) fn direct_indirect_long(&mut self, bus: &mut impl Bus65816) -> Operand {
        Operand::spanning_banks(self.read_long_pointer(bus))
    }

    /// Row 14, cycles 2 to 5: as row 15. The data is at AAB,AA plus Y. The row has no
    /// optional indexing cycle.
    // @source: wdc-w65c816s#table-5-7-14
    // @source: wdc-w65c816s#3.5.14
    pub(super) fn direct_indirect_long_indexed(&mut self, bus: &mut impl Bus65816) -> Operand {
        let base = self.read_long_pointer(bus);
        Operand::spanning_banks(indexed(base, self.y))
    }

    /// Cycles 2 and 2a of every direct row: fetches the offset "DO", then spends an internal
    /// cycle when the low byte of the Direct Register is not zero (note 2).
    // @source: wdc-w65c816s#table-5-7-10a
    // @source: wdc-w65c816s#table-5-7-note-2
    fn fetch_direct_offset(&mut self, bus: &mut impl Bus65816) -> u8 {
        let offset = self.fetch(bus);
        if !self.direct_page_aligned() {
            self.internal_at_last_fetch(bus);
        }
        offset
    }

    /// Whether DL, the low byte of the Direct Register, is zero.
    // @source: wdc-w65c816s#3.5.17
    fn direct_page_aligned(&self) -> bool {
        self.d.to_le_bytes()[0] == 0
    }

    /// Whether direct addresses wrap at the page boundary: emulation mode with DL = 0.
    // @source: wdc-w65c816s#7.2
    // @source: 6502org-65c816-opcodes#5.1.1
    fn direct_page_wraps(&self) -> bool {
        self.e && self.direct_page_aligned()
    }

    /// The bank-0 offset of D + `offset` + `index`. When the direct page wraps, the sum is
    /// taken in 8 bits and stays in page DH; otherwise it is a 16-bit sum that wraps at the
    /// end of bank 0.
    // @source: wdc-w65c816s#7.2
    // @source: 6502org-65c816-opcodes#5.8
    fn direct_address(&self, offset: u8, index: u16) -> u16 {
        if self.direct_page_wraps() {
            let [_, page] = self.d.to_le_bytes();
            let [index_low, _] = index.to_le_bytes();
            u16::from_le_bytes([offset.wrapping_add(index_low), page])
        } else {
            self.d.wrapping_add(u16::from(offset)).wrapping_add(index)
        }
    }

    /// Reads the 16-bit pointer "AAL", "AAH" of the `(d)`, `(d,x)` and `(d),y` rows. Its
    /// high byte is at the next direct address: in the same page when the direct page wraps,
    /// otherwise at the next offset of bank 0.
    // @source: wdc-w65c816s#table-5-7-12
    // @source: wdc-w65c816s#7.2
    // @source: 6502org-65c816-opcodes#5.9
    fn read_pointer(&self, bus: &mut impl Bus65816, address: u16) -> u16 {
        let next = if self.direct_page_wraps() {
            let [low, page] = address.to_le_bytes();
            u16::from_le_bytes([low.wrapping_add(1), page])
        } else {
            address.wrapping_add(1)
        };
        let low = self.read_byte(bus, Addr24::from_bank_offset(0, address));
        let high = self.read_byte(bus, Addr24::from_bank_offset(0, next));
        u16::from_le_bytes([low, high])
    }

    /// Cycles 2 to 5 of the `[d]` and `[d],y` rows: the offset, the optional cycle, and the
    /// 24-bit pointer at 0,D+DO, 0,D+DO+1 and 0,D+DO+2. The pointer never wraps at a page
    /// boundary, even in emulation mode; it does wrap at the end of bank 0.
    // @source: wdc-w65c816s#table-5-7-15
    // @source: wdc-w65c816s#7.2
    // @source: 6502org-65c816-opcodes#5.10
    fn read_long_pointer(&mut self, bus: &mut impl Bus65816) -> Addr24 {
        let offset = self.fetch_direct_offset(bus);
        let address = self.d.wrapping_add(u16::from(offset));
        let low = self.read_byte(bus, Addr24::from_bank_offset(0, address));
        let high = self.read_byte(bus, Addr24::from_bank_offset(0, address.wrapping_add(1)));
        let bank = self.read_byte(bus, Addr24::from_bank_offset(0, address.wrapping_add(2)));
        Addr24::from_bank_offset(bank, u16::from_le_bytes([low, high]))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        registers::Flags,
        testing::{Cycle, emulation, native, run},
    };

    // @source: wdc-w65c816s#table-5-7-10a
    // @source: wdc-w65c816s#table-5-7-note-2
    #[test]
    fn direct_adds_a_cycle_when_the_direct_register_is_not_page_aligned() {
        let mut cpu = native(Flags::M);
        cpu.d = 0x3400;
        let bus = run(&mut cpu, &[0xA5, 0x56], &[(0x00_3456, &[0x99])]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Data(0x00_3456),
            ]
        );
        assert_eq!((cpu.a, cpu.pc), (0x0099, 0x8002));

        let mut cpu = native(Flags::M);
        cpu.d = 0x3401;
        let bus = run(&mut cpu, &[0xA5, 0x56], &[]);
        assert_eq!(
            bus.cycles[2..],
            [Cycle::Internal(0x12_8001), Cycle::Data(0x00_3457)]
        );
    }

    // @source: 6502org-65c816-opcodes#5.7
    // @source: wdc-w65c816s#7.2
    #[test]
    fn sixteen_bit_direct_data_wraps_within_bank_zero() {
        // Example 2: D $FF00, LDA $FF reads $00FFFF and $000000.
        let mut cpu = native(Flags::empty());
        cpu.d = 0xFF00;
        let bus = run(&mut cpu, &[0xA5, 0xFF], &[]);
        assert_eq!(
            bus.cycles[2..],
            [Cycle::Data(0x00_FFFF), Cycle::Data(0x00_0000)]
        );
    }

    // @source: wdc-w65c816s#table-5-7-16a
    // @source: 6502org-65c816-opcodes#5.8
    #[test]
    fn direct_indexed_crosses_the_page_in_native_mode() {
        // Example 2: D $FF00, X $000A, LDA $FE,X reads $000008 and $000009.
        let mut cpu = native(Flags::empty());
        cpu.d = 0xFF00;
        cpu.x = 0x000A;
        let bus = run(&mut cpu, &[0xB5, 0xFE], &[]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Internal(0x12_8001),
                Cycle::Data(0x00_0008),
                Cycle::Data(0x00_0009),
            ]
        );
    }

    // @source: 6502org-65c816-opcodes#5.8
    // @source: wdc-w65c816s#7.2
    #[test]
    fn direct_indexed_wraps_in_the_page_in_emulation_mode_with_dl_zero() {
        // Example 1: D $FF00, X $0A, LDA $FE,X reads $00FF08.
        let mut cpu = emulation();
        cpu.d = 0xFF00;
        cpu.x = 0x0A;
        let bus = run(&mut cpu, &[0xB5, 0xFE], &[]);
        assert_eq!(bus.cycles[3..], [Cycle::Data(0x00_FF08)]);
    }

    // @source: wdc-w65c816s#7.2
    #[test]
    fn direct_indexed_does_not_wrap_in_emulation_mode_when_dl_is_not_zero() {
        // 7.2.3: the range is then the whole of bank 0.
        let mut cpu = emulation();
        cpu.d = 0x1001;
        cpu.x = 0x0A;
        let bus = run(&mut cpu, &[0xB5, 0xFE], &[]);
        assert_eq!(
            bus.cycles[2..],
            [
                Cycle::Internal(0x12_8001),
                Cycle::Internal(0x12_8001),
                Cycle::Data(0x00_1109),
            ]
        );
    }

    // @source: wdc-w65c816s#table-5-7-17
    #[test]
    fn direct_y_indexes_with_y() {
        let mut cpu = native(Flags::M | Flags::X);
        cpu.x = 0x01;
        cpu.y = 0x02;
        let bus = run(&mut cpu, &[0xB6, 0x10], &[(0x00_0012, &[0x99])]);
        assert_eq!(bus.cycles[3..], [Cycle::Data(0x00_0012)]);
        assert_eq!(cpu.x, 0x0099);
    }

    // @source: wdc-w65c816s#table-5-7-12
    // @source: 6502org-65c816-opcodes#5.9
    #[test]
    fn direct_indirect_reads_its_pointer_in_bank_zero_and_its_data_past_the_bank() {
        // Example 2: D $FF00, DBR $12, pointer bytes $FF at $00FFFF and $000000: LDA ($FF)
        // reads $12FFFF and $130000.
        let mut cpu = native(Flags::empty());
        cpu.d = 0xFF00;
        cpu.dbr = 0x12;
        let memory: [(u32, &[u8]); 2] = [(0x00_FFFF, &[0xFF]), (0x00_0000, &[0xFF])];
        let bus = run(&mut cpu, &[0xB2, 0xFF], &memory);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Data(0x00_FFFF),
                Cycle::Data(0x00_0000),
                Cycle::Data(0x12_FFFF),
                Cycle::Data(0x13_0000),
            ]
        );
    }

    // @source: 6502org-65c816-opcodes#5.9
    // @source: wdc-w65c816s#7.2
    #[test]
    fn the_pointer_wraps_in_the_page_in_emulation_mode_with_dl_zero() {
        // Example 1: D $FF00: the pointer of LDA ($FF) is at $00FFFF and $00FF00.
        let mut cpu = emulation();
        cpu.d = 0xFF00;
        cpu.dbr = 0x12;
        let memory: [(u32, &[u8]); 2] = [(0x00_FFFF, &[0xFF]), (0x00_FF00, &[0xFF])];
        let bus = run(&mut cpu, &[0xB2, 0xFF], &memory);
        assert_eq!(
            bus.cycles[2..],
            [
                Cycle::Data(0x00_FFFF),
                Cycle::Data(0x00_FF00),
                Cycle::Data(0x12_FFFF),
            ]
        );
    }

    // @source: wdc-w65c816s#table-5-7-11
    // @source: 6502org-65c816-opcodes#5.11
    #[test]
    fn direct_indexed_indirect_adds_x_before_reading_the_pointer() {
        // Example 2: D $FF00, X $000A, DBR $12, pointer $FFFF at $000008: LDA ($FE,X) reads
        // $12FFFF and $130000.
        let mut cpu = native(Flags::empty());
        cpu.d = 0xFF00;
        cpu.x = 0x000A;
        cpu.dbr = 0x12;
        let bus = run(&mut cpu, &[0xA1, 0xFE], &[(0x00_0008, &[0xFF, 0xFF])]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Internal(0x12_8001),
                Cycle::Data(0x00_0008),
                Cycle::Data(0x00_0009),
                Cycle::Data(0x12_FFFF),
                Cycle::Data(0x13_0000),
            ]
        );

        // Example 1: in emulation mode the pointer is at $00FF08 and $00FF09.
        let mut cpu = emulation();
        cpu.d = 0xFF00;
        cpu.x = 0x0A;
        cpu.dbr = 0x12;
        let bus = run(&mut cpu, &[0xA1, 0xFE], &[(0x00_FF08, &[0xFF, 0xFF])]);
        assert_eq!(
            bus.cycles[3..],
            [
                Cycle::Data(0x00_FF08),
                Cycle::Data(0x00_FF09),
                Cycle::Data(0x12_FFFF),
            ]
        );
    }

    // @source: wdc-w65c816s#table-5-7-13
    // @source: 6502org-65c816-opcodes#5.12
    #[test]
    fn direct_indirect_indexed_adds_y_to_the_pointer_across_the_bank() {
        // Example 2: D $FF00, DBR $12, Y $000A, pointer $FFFE: LDA ($FF),Y reads $130008
        // and $130009.
        let mut cpu = native(Flags::empty());
        cpu.d = 0xFF00;
        cpu.dbr = 0x12;
        cpu.y = 0x000A;
        let memory: [(u32, &[u8]); 2] = [(0x00_FFFF, &[0xFE]), (0x00_0000, &[0xFF])];
        let bus = run(&mut cpu, &[0xB1, 0xFF], &memory);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Data(0x00_FFFF),
                Cycle::Data(0x00_0000),
                Cycle::Internal(0x12_FF08),
                Cycle::Data(0x13_0008),
                Cycle::Data(0x13_0009),
            ]
        );
    }

    // @source: wdc-w65c816s#table-5-7-13
    // @source: wdc-w65c816s#table-5-7-note-4
    #[test]
    fn direct_indirect_indexed_skips_the_cycle_within_a_page_with_an_eight_bit_index() {
        let mut cpu = native(Flags::M | Flags::X);
        cpu.dbr = 0x34;
        cpu.y = 0x0F;
        let bus = run(&mut cpu, &[0xB1, 0x10], &[(0x00_0010, &[0xF0, 0x56])]);
        assert_eq!(
            bus.cycles[2..],
            [
                Cycle::Data(0x00_0010),
                Cycle::Data(0x00_0011),
                Cycle::Data(0x34_56FF),
            ]
        );
    }

    // @source: wdc-w65c816s#table-5-7-15
    // @source: 6502org-65c816-opcodes#5.10
    #[test]
    fn direct_indirect_long_reads_a_three_byte_pointer() {
        // The example: D $FF00, pointer $12FFFF at $00FFFE, $00FFFF and $000000: LDA [$FE]
        // reads $12FFFF and $130000.
        let mut cpu = native(Flags::empty());
        cpu.d = 0xFF00;
        let memory: [(u32, &[u8]); 2] = [(0x00_FFFE, &[0xFF, 0xFF]), (0x00_0000, &[0x12])];
        let bus = run(&mut cpu, &[0xA7, 0xFE], &memory);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Data(0x00_FFFE),
                Cycle::Data(0x00_FFFF),
                Cycle::Data(0x00_0000),
                Cycle::Data(0x12_FFFF),
                Cycle::Data(0x13_0000),
            ]
        );
    }

    // @source: wdc-w65c816s#table-5-7-14
    // @source: 6502org-65c816-opcodes#5.13
    #[test]
    fn direct_indirect_long_indexed_adds_y_with_no_extra_cycle() {
        // The example: D $FF00, Y $000A, pointer $12FFFC: LDA [$FE],Y reads $130006 and
        // $130007.
        let mut cpu = native(Flags::empty());
        cpu.d = 0xFF00;
        cpu.y = 0x000A;
        let memory: [(u32, &[u8]); 2] = [(0x00_FFFE, &[0xFC, 0xFF]), (0x00_0000, &[0x12])];
        let bus = run(&mut cpu, &[0xB7, 0xFE], &memory);
        assert_eq!(
            bus.cycles[5..],
            [Cycle::Data(0x13_0006), Cycle::Data(0x13_0007)]
        );
    }

    // @source: wdc-w65c816s#7.2
    #[test]
    fn the_long_pointer_leaves_the_page_even_in_emulation_mode() {
        // 7.2.2: [Direct] increments from 00DHFF into the next higher page.
        let mut cpu = emulation();
        cpu.d = 0x2000;
        let bus = run(&mut cpu, &[0xA7, 0xFF], &[(0x00_20FF, &[0x56, 0x34, 0x12])]);
        assert_eq!(
            bus.cycles[2..],
            [
                Cycle::Data(0x00_20FF),
                Cycle::Data(0x00_2100),
                Cycle::Data(0x00_2101),
                Cycle::Data(0x12_3456),
            ]
        );
    }
}
