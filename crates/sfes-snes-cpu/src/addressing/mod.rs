// @source: wdc-w65c816s#3.5
// @source: wdc-w65c816s#table-5-7
//! Addressing modes: each mode performs its exact sequence of bus cycles.
//!
//! A mode function runs the cycles of its Table 5-7 row from cycle 2 up to the cycle before
//! the data, and returns where the data is as an [`Operand`]. The instruction then reads (or,
//! from #34, writes) one or two bytes there. Where the high byte of 16-bit data lives is part
//! of the mode: data reached through the Data Bank Register or a long address runs on into
//! the next bank, while direct and stack-relative data stays in bank 0.

mod absolute;
mod direct;
mod immediate;
mod modify;
mod stack;

use sfes_api::Addr24;

use crate::{
    bus::{Bus65816, Pins},
    registers::{Cpu65816, Flags},
    width::Width,
};

/// The addressing modes that yield a data operand, named as in the datasheet's Table 5-6.
// @source: wdc-w65c816s#table-5-6
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Mode {
    /// `#`: the operand follows the opcode.
    Immediate,
    /// `a`
    Absolute,
    /// `a,x`
    AbsoluteX,
    /// `a,y`
    AbsoluteY,
    /// `al`
    AbsoluteLong,
    /// `al,x`
    AbsoluteLongX,
    /// `d`
    Direct,
    /// `d,x`
    DirectX,
    /// `d,y`
    DirectY,
    /// `(d)`
    DirectIndirect,
    /// `(d,x)`
    DirectIndexedIndirect,
    /// `(d),y`
    DirectIndirectIndexed,
    /// `[d]`
    DirectIndirectLong,
    /// `[d],y`
    DirectIndirectLongIndexed,
    /// `d,s`
    StackRelative,
    /// `(d,s),y`
    StackRelativeIndirectIndexed,
}

impl Mode {
    /// The memory address mode, or `None` for immediate data, which is not in memory the
    /// instruction can write.
    // @source: wdc-w65c816s#table-5-6
    const fn address(self) -> Option<Address> {
        let address = match self {
            Self::Immediate => return None,
            Self::Absolute => Address::Absolute,
            Self::AbsoluteX => Address::AbsoluteX,
            Self::AbsoluteY => Address::AbsoluteY,
            Self::AbsoluteLong => Address::AbsoluteLong,
            Self::AbsoluteLongX => Address::AbsoluteLongX,
            Self::Direct => Address::Direct,
            Self::DirectX => Address::DirectX,
            Self::DirectY => Address::DirectY,
            Self::DirectIndirect => Address::DirectIndirect,
            Self::DirectIndexedIndirect => Address::DirectIndexedIndirect,
            Self::DirectIndirectIndexed => Address::DirectIndirectIndexed,
            Self::DirectIndirectLong => Address::DirectIndirectLong,
            Self::DirectIndirectLongIndexed => Address::DirectIndirectLongIndexed,
            Self::StackRelative => Address::StackRelative,
            Self::StackRelativeIndirectIndexed => Address::StackRelativeIndirectIndexed,
        };
        Some(address)
    }
}

/// The addressing modes whose operand is in memory: every [`Mode`] but immediate. The store
/// instructions take one of these, so that a store to immediate data cannot be written.
// @source: wdc-w65c816s#table-5-6
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Address {
    /// `a`
    Absolute,
    /// `a,x`
    AbsoluteX,
    /// `a,y`
    AbsoluteY,
    /// `al`
    AbsoluteLong,
    /// `al,x`
    AbsoluteLongX,
    /// `d`
    Direct,
    /// `d,x`
    DirectX,
    /// `d,y`
    DirectY,
    /// `(d)`
    DirectIndirect,
    /// `(d,x)`
    DirectIndexedIndirect,
    /// `(d),y`
    DirectIndirectIndexed,
    /// `[d]`
    DirectIndirectLong,
    /// `[d],y`
    DirectIndirectLongIndexed,
    /// `d,s`
    StackRelative,
    /// `(d,s),y`
    StackRelativeIndirectIndexed,
}

/// Whether the instruction reads or writes its operand. Only the optional indexing cycle of
/// note 4 depends on it: a write always spends it.
// @source: wdc-w65c816s#table-5-7-note-4
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Access {
    /// The data cycles read.
    Read,
    /// The data cycles write.
    Write,
}

/// Where a data operand is: the address of its low byte and, for 16-bit data, of its high
/// byte.
// @source: wdc-w65c816s#3.5
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Operand {
    /// The address of the low (or only) byte.
    pub(crate) low: Addr24,
    /// The address of the high byte of 16-bit data.
    pub(crate) high: Addr24,
}

impl Operand {
    /// An operand at a 24-bit effective address. The high byte is at the next address in
    /// 24 bits: the data address space is contiguous, and data may span a bank boundary.
    // @source: wdc-w65c816s#3.5
    // @source: 6502org-65c816-opcodes#5.1.2
    fn spanning_banks(low: Addr24) -> Self {
        let high = Addr24::new(low.get().wrapping_add(1));
        Self { low, high }
    }

    /// An operand in bank 0, which direct and stack-relative addressing never leave: the
    /// high byte is at the next offset, wrapping from `FFFF` to `0000`.
    // @source: wdc-w65c816s#7.1
    // @source: wdc-w65c816s#7.2
    fn in_bank_zero(offset: u16) -> Self {
        Self {
            low: Addr24::from_bank_offset(0, offset),
            high: Addr24::from_bank_offset(0, offset.wrapping_add(1)),
        }
    }
}

/// `address` plus an index register, as a 24-bit sum that carries into the bank byte.
// @source: wdc-w65c816s#7.3
// @source: 6502org-65c816-opcodes#5.3
fn indexed(address: Addr24, index: u16) -> Addr24 {
    Addr24::new(address.get().wrapping_add(u32::from(index)))
}

impl Cpu65816 {
    /// Runs the cycles of `mode` after the opcode fetch and reads the operand at width `W`:
    /// one data cycle for 8 bits, two for 16 bits (note 1 of Table 5-7).
    // @source: wdc-w65c816s#table-5-7
    // @source: wdc-w65c816s#table-5-7-note-1
    pub(crate) fn read_operand<W: Width>(&mut self, bus: &mut impl Bus65816, mode: Mode) -> W {
        let Some(address) = mode.address() else {
            return self.immediate(bus);
        };
        let operand = self.operand(bus, address, Access::Read);
        self.read_data(bus, operand)
    }

    /// Runs the cycles of `address` after the opcode fetch and writes `value` at width `W`:
    /// the same rows as the reads, with RWB low in the data cycles.
    // @source: wdc-w65c816s#table-5-7
    // @source: wdc-w65c816s#table-5-7-note-1
    pub(crate) fn write_operand<W: Width>(
        &mut self,
        bus: &mut impl Bus65816,
        address: Address,
        value: W,
    ) {
        let operand = self.operand(bus, address, Access::Write);
        self.write_data(bus, operand, value);
    }

    /// The cycles of `address` from cycle 2 up to the first data cycle, and where the data
    /// is.
    // @source: wdc-w65c816s#table-5-7
    fn operand(&mut self, bus: &mut impl Bus65816, address: Address, access: Access) -> Operand {
        match address {
            Address::Absolute => self.absolute(bus),
            Address::AbsoluteX => self.absolute_indexed(bus, self.x, access),
            Address::AbsoluteY => self.absolute_indexed(bus, self.y, access),
            Address::AbsoluteLong => self.absolute_long(bus),
            Address::AbsoluteLongX => self.absolute_long_x(bus),
            Address::Direct => self.direct(bus),
            Address::DirectX => self.direct_indexed(bus, self.x),
            Address::DirectY => self.direct_indexed(bus, self.y),
            Address::DirectIndirect => self.direct_indirect(bus),
            Address::DirectIndexedIndirect => self.direct_indexed_indirect(bus),
            Address::DirectIndirectIndexed => self.direct_indirect_indexed(bus, access),
            Address::DirectIndirectLong => self.direct_indirect_long(bus),
            Address::DirectIndirectLongIndexed => self.direct_indirect_long_indexed(bus),
            Address::StackRelative => self.stack_relative(bus),
            Address::StackRelativeIndirectIndexed => self.stack_relative_indirect_indexed(bus),
        }
    }

    /// The data cycles that end a read instruction: "Data Low" and, for 16-bit data, "Data
    /// High", each with VDA active.
    // @source: wdc-w65c816s#table-5-7-1a
    // @source: wdc-w65c816s#table-5-7-note-1
    fn read_data<W: Width>(&self, bus: &mut impl Bus65816, operand: Operand) -> W {
        let Operand { low, high } = operand;
        let low = self.read_byte(bus, low);
        W::from_bytes(low, || self.read_byte(bus, high))
    }

    /// The data cycles that end a write instruction: "Data Low" and, for 16-bit data, "Data
    /// High", in that order, each with VDA active and RWB low.
    // @source: wdc-w65c816s#table-5-7-1a
    // @source: wdc-w65c816s#table-5-7-note-1
    // @source: wdc-w65c816s#2.23
    fn write_data<W: Width>(&self, bus: &mut impl Bus65816, operand: Operand, value: W) {
        let Operand { low, high } = operand;
        let (low_byte, high_byte) = value.to_bytes();
        self.write_byte(bus, low, low_byte);
        if let Some(high_byte) = high_byte {
            self.write_byte(bus, high, high_byte);
        }
    }

    /// One data-address cycle: a read with VDA active and VPA inactive.
    // @source: wdc-w65c816s#2.26
    fn read_byte(&self, bus: &mut impl Bus65816, address: Addr24) -> u8 {
        bus.read(address, Pins::VDA | Pins::RWB | self.status_pins())
    }

    /// One data-address write cycle: VDA active, VPA inactive, RWB low.
    // @source: wdc-w65c816s#2.26
    // @source: wdc-w65c816s#2.23
    fn write_byte(&self, bus: &mut impl Bus65816, address: Addr24, value: u8) {
        bus.write(address, value, Pins::VDA | self.status_pins());
    }

    /// One program-address cycle: reads the operand byte at PBR,PC with VPA active and VDA
    /// inactive, and increments the program counter within the program bank.
    // @source: wdc-w65c816s#2.26
    // @source: wdc-w65c816s#2.10
    fn fetch(&mut self, bus: &mut impl Bus65816) -> u8 {
        let pins = Pins::VPA | Pins::RWB | self.status_pins();
        let value = bus.read(self.program_address(), pins);
        self.pc = self.pc.wrapping_add(1);
        value
    }

    /// Cycles 2 and 3 of the absolute rows: fetches the 16-bit address "AAL", "AAH".
    // @source: wdc-w65c816s#table-5-7-1a
    fn fetch_address(&mut self, bus: &mut impl Bus65816) -> u16 {
        let low = self.fetch(bus);
        let high = self.fetch(bus);
        u16::from_le_bytes([low, high])
    }

    /// One internal-operation cycle ("IO" in Table 5-7): VDA and VPA inactive, RWB high. The
    /// processor drives `address`, but no memory access takes place (ADR 0004).
    // @source: wdc-w65c816s#2.26
    // @source: adr-0004
    fn internal(&self, bus: &mut impl Bus65816, address: Addr24) {
        bus.idle(address, Pins::RWB | self.status_pins());
    }

    /// An internal-operation cycle at the address of the last program byte fetched. Table
    /// 5-7 prints it as "PBR,PC+1" in the two-byte rows, counting from the opcode's address.
    // @source: wdc-w65c816s#table-5-7-10a
    fn internal_at_last_fetch(&self, bus: &mut impl Bus65816) {
        let address = Addr24::from_bank_offset(self.pbr, self.pc.wrapping_sub(1));
        self.internal(bus, address);
    }

    /// The optional cycle of the indexed rows (cycle 3a of rows 6a and 7, cycle 4a of row
    /// 13), then the indexed address. Note 4 adds the cycle "for indexing across page
    /// boundaries, or write, or X=0": always for a write, and for a read when the index
    /// crosses a page boundary or the index registers are 16 bits wide. Only the low byte has
    /// been added when it runs, so its address is the base with the index's low byte added to
    /// its low byte and no carry: "DBR,AAH,AAL+XL".
    // @source: wdc-w65c816s#table-5-7-6a
    // @source: wdc-w65c816s#table-5-7-note-4
    // @source: wdc-w65c816s#7.5
    fn index_data_address(
        &self,
        bus: &mut impl Bus65816,
        base: u16,
        index: u16,
        access: Access,
    ) -> Addr24 {
        let [base_low, base_high] = base.to_le_bytes();
        let [index_low, _] = index.to_le_bytes();
        let (low, page_crossed) = base_low.overflowing_add(index_low);
        if access == Access::Write || page_crossed || !self.p.contains(Flags::X) {
            let partial = u16::from_le_bytes([low, base_high]);
            self.internal(bus, Addr24::from_bank_offset(self.dbr, partial));
        }
        indexed(Addr24::from_bank_offset(self.dbr, base), index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Cycle, native, run};

    // @source: wdc-w65c816s#2.26
    // @source: wdc-w65c816s#2.20
    // @source: adr-0004
    #[test]
    fn each_kind_of_cycle_carries_its_signals() {
        // LDA $5678,X with 16-bit index registers: an opcode fetch, two program bytes, an
        // internal cycle and a data byte.
        let mut cpu = native(Flags::M);
        let bus = run(&mut cpu, &[0xBD, 0x78, 0x56], &[]);
        let status = Pins::RWB | Pins::M;
        assert_eq!(
            bus.pins,
            [
                Pins::VDA | Pins::VPA | status,
                Pins::VPA | status,
                Pins::VPA | status,
                status,
                Pins::VDA | status,
            ]
        );
    }

    // @source: wdc-w65c816s#table-5-7-6a
    // @source: wdc-w65c816s#table-5-7-note-4
    #[test]
    fn an_eight_bit_index_adds_a_cycle_only_across_a_page_boundary() {
        let mut cpu = native(Flags::M | Flags::X);
        cpu.dbr = 0x34;
        cpu.x = 0x0F;
        let bus = run(&mut cpu, &[0xBD, 0xF0, 0x56], &[]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Program(0x12_8002),
                Cycle::Data(0x34_56FF),
            ]
        );

        let mut cpu = native(Flags::M | Flags::X);
        cpu.dbr = 0x34;
        cpu.x = 0x10;
        let bus = run(&mut cpu, &[0xBD, 0xF0, 0x56], &[]);
        // The extra cycle's address has the index added to the low byte only.
        assert_eq!(
            bus.cycles[3..],
            [Cycle::Internal(0x34_5600), Cycle::Data(0x34_5700)]
        );
    }

    // @source: wdc-w65c816s#table-5-7-6a
    // @source: wdc-w65c816s#table-5-7-note-4
    #[test]
    fn a_sixteen_bit_index_always_adds_the_cycle() {
        let mut cpu = native(Flags::M);
        cpu.dbr = 0x34;
        cpu.x = 0x0001;
        let bus = run(&mut cpu, &[0xBD, 0x00, 0x56], &[]);
        assert_eq!(
            bus.cycles[3..],
            [Cycle::Internal(0x34_5601), Cycle::Data(0x34_5601)]
        );
    }
}
