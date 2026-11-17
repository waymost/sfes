// @source: wdc-w65c816s#table-5-7-1d
// @source: wdc-w65c816s#table-5-7-10b
// @source: wdc-w65c816s#2.19
// @source: adr-0006
//! Read-modify-write: read the operand, spend one cycle modifying it, and write the result
//! back, with the Memory Lock signal active throughout.
//!
//! Table 5-7 prints the modify cycle and the write of the low byte at "AA+1" and "AA" for
//! every width. The vectors put the modify cycle at the address of the last byte read, which
//! is AA for 8-bit data, and in emulation mode they write the unmodified byte there. ADR 0006
//! records both.

use sfes_api::Addr24;

use crate::{
    addressing::{Access, Address, Operand},
    bus::{Bus65816, Pins},
    registers::Cpu65816,
    width::Width,
};

impl Cpu65816 {
    /// Runs the cycles of `address` after the opcode fetch, reads the operand at width `W`,
    /// and writes back what `operation` makes of it. The data cycles are those of rows 1d
    /// and 10b: "Data Low", "Data High" (16 bits), the modify cycle, then the writes, high
    /// byte first. MLB is active from the first data read to the last write, which is the
    /// "last three or five cycles" of 2.19.
    // @source: wdc-w65c816s#table-5-7-1d
    // @source: wdc-w65c816s#table-5-7-10b
    // @source: wdc-w65c816s#table-5-7-note-17
    // @source: wdc-w65c816s#2.19
    // @source: adr-0006
    pub(crate) fn modify_operand<W: Width>(
        &mut self,
        bus: &mut impl Bus65816,
        address: Address,
        operation: impl FnOnce(&mut Self, W) -> W,
    ) {
        let Operand { low, high } = self.operand(bus, address, Access::Write);
        let read_pins = Pins::VDA | Pins::RWB | Pins::MLB | self.status_pins();
        let low_byte = bus.read(low, read_pins);
        let value = W::from_bytes(low_byte, || bus.read(high, read_pins));

        let (last_address, last_byte) = match value.to_bytes() {
            (_, Some(high_byte)) => (high, high_byte),
            (low_byte, None) => (low, low_byte),
        };
        self.modify_cycle(bus, last_address, last_byte);

        let result = operation(self, value);
        let write_pins = Pins::VDA | Pins::MLB | self.status_pins();
        let (low_byte, high_byte) = result.to_bytes();
        if let Some(high_byte) = high_byte {
            bus.write(high, high_byte, write_pins);
        }
        bus.write(low, low_byte, write_pins);
    }

    /// The modify cycle (cycle 5 of row 1d, cycle 4 of row 10b) at the address of the last
    /// byte read. In native mode it is an internal operation; in emulation mode RWB is low
    /// (note 17) and the unmodified byte is written back (ADR 0006).
    // @source: wdc-w65c816s#table-5-7-1d
    // @source: wdc-w65c816s#table-5-7-note-17
    // @source: adr-0006
    fn modify_cycle(&self, bus: &mut impl Bus65816, address: Addr24, unmodified: u8) {
        let pins = Pins::MLB | self.status_pins();
        if self.e {
            bus.write(address, unmodified, pins);
        } else {
            bus.idle(address, Pins::RWB | pins);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        bus::Pins,
        registers::Flags,
        testing::{Cycle, emulation, native, run},
    };

    // @source: wdc-w65c816s#table-5-7-1d
    // @source: wdc-w65c816s#2.19
    // @source: adr-0006
    #[test]
    fn sixteen_bit_data_is_written_back_high_byte_first_under_memory_lock() {
        // TSB $5678 with C = $0101 and $1234 at 34:5678.
        let mut cpu = native(Flags::empty());
        cpu.dbr = 0x34;
        cpu.a = 0x0101;
        let bus = run(&mut cpu, &[0x0C, 0x78, 0x56], &[(0x34_5678, &[0x34, 0x12])]);
        assert_eq!(
            bus.cycles[3..],
            [
                Cycle::Data(0x34_5678),
                Cycle::Data(0x34_5679),
                Cycle::Internal(0x34_5679),
                Cycle::Write(0x34_5679, 0x13),
                Cycle::Write(0x34_5678, 0x35),
            ]
        );
        let locked = bus.pins[3..].iter().all(|pins| pins.contains(Pins::MLB));
        assert!(locked);
        assert!(!bus.pins[..3].iter().any(|pins| pins.contains(Pins::MLB)));
    }

    // @source: wdc-w65c816s#table-5-7-10b
    // @source: adr-0006
    #[test]
    fn eight_bit_data_is_modified_at_its_own_address() {
        let mut cpu = native(Flags::M);
        cpu.a = 0x01;
        let bus = run(&mut cpu, &[0x04, 0x10], &[(0x00_0010, &[0x80])]);
        assert_eq!(
            bus.cycles[2..],
            [
                Cycle::Data(0x00_0010),
                Cycle::Internal(0x00_0010),
                Cycle::Write(0x00_0010, 0x81),
            ]
        );
    }

    // @source: wdc-w65c816s#table-5-7-note-17
    // @source: adr-0006
    #[test]
    fn emulation_mode_writes_the_unmodified_byte_in_the_modify_cycle() {
        let mut cpu = emulation();
        cpu.a = 0x01;
        let bus = run(&mut cpu, &[0x04, 0x10], &[(0x00_0010, &[0x80])]);
        assert_eq!(
            bus.cycles[2..],
            [
                Cycle::Data(0x00_0010),
                Cycle::Write(0x00_0010, 0x80),
                Cycle::Write(0x00_0010, 0x81),
            ]
        );
        assert!(!bus.pins[3].intersects(Pins::VDA | Pins::VPA | Pins::RWB));
    }
}
