// @source: wdc-w65c816s#3.5.23
// @source: wdc-w65c816s#3.5.24
// @source: wdc-w65c816s#7.1
// @source: adr-0008
// @source: adr-0009
//! The stack: pushes and pulls, and stack-relative addressing (an unsigned 8-bit offset from
//! the stack pointer). The stack is always in bank 0.
//!
//! In emulation mode S is in page 1. Most instructions keep their stack accesses there, so S
//! wraps in its low byte; the instructions that datasheet 7.1 lists (JSL, JSR (a,x), RTL, PEA,
//! PEI, PER, PHD and PLD), and PLB, step S as a 16-bit value while they run and may leave
//! page 1, and the high byte of S returns to `01` when they end (ADRs 0008 and 0009).

use sfes_api::Addr24;

use crate::{
    addressing::{Direction, Operand, indexed},
    bus::Bus65816,
    registers::Cpu65816,
    width::Width,
};

/// How S steps between the bytes of a push or pull in emulation mode. In native mode both
/// are 16-bit steps in bank 0.
// @source: wdc-w65c816s#7.1
// @source: adr-0008
// @source: adr-0009
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StackWrap {
    /// S wraps in its low byte and stays in page 1.
    PageOne,
    /// S steps as a 16-bit value and may leave page 1 (the instructions of 7.1, and PLB).
    BankZero,
}

impl Cpu65816 {
    /// Writes `value` at 0,S, then decrements S: a stack write cycle of the "Stack s" rows,
    /// with VDA active and RWB low.
    // @source: wdc-w65c816s#table-5-7-1c
    // @source: wdc-w65c816s#3.5.21
    // @source: adr-0008
    pub(crate) fn push(&mut self, bus: &mut impl Bus65816, value: u8, wrap: StackWrap) {
        self.write_byte(bus, self.stack_address(), value);
        self.step_stack(wrap, Direction::Down);
    }

    /// Increments S, then reads the byte at 0,S: a stack read cycle with VDA active.
    // @source: wdc-w65c816s#table-5-7-22h
    // @source: wdc-w65c816s#3.5.21
    // @source: adr-0008
    pub(crate) fn pull(&mut self, bus: &mut impl Bus65816, wrap: StackWrap) -> u8 {
        self.step_stack(wrap, Direction::Up);
        self.read_byte(bus, self.stack_address())
    }

    /// Pushes `value`: "REG High" at 0,S when it is 16 bits wide (cycle 3a of row 22c), then
    /// "REG Low". Every multi-byte push writes its high byte first.
    // @source: wdc-w65c816s#table-5-7-22c
    // @source: wdc-w65c816s#table-5-7-note-1
    pub(crate) fn push_value<W: Width>(
        &mut self,
        bus: &mut impl Bus65816,
        value: W,
        wrap: StackWrap,
    ) {
        let (low, high) = value.to_bytes();
        if let Some(high) = high {
            self.push(bus, high, wrap);
        }
        self.push(bus, low, wrap);
    }

    /// Pulls a value of width `W`: "REG Low" at 0,S+1, then, when 16 bits wide, "REG High"
    /// at 0,S+2 (cycles 4 and 4a of row 22b). Every multi-byte pull reads its low byte first.
    // @source: wdc-w65c816s#table-5-7-22b
    // @source: wdc-w65c816s#table-5-7-note-1
    pub(crate) fn pull_value<W: Width>(&mut self, bus: &mut impl Bus65816, wrap: StackWrap) -> W {
        let low = self.pull(bus, wrap);
        W::from_bytes(low, || self.pull(bus, wrap))
    }

    /// 0,S: the address the stack pointer names.
    // @source: wdc-w65c816s#2.11
    pub(crate) fn stack_address(&self) -> Addr24 {
        Addr24::from_bank_offset(0, self.s)
    }

    /// Puts the high byte of S back to `01` in emulation mode: at the end of an instruction
    /// that stepped S as a 16-bit value, after a page-1 step, and on entering emulation mode.
    /// Native mode leaves S alone.
    // @source: wdc-w65c816s#2.11
    // @source: wdc-w65c816s#7.10
    // @source: adr-0008
    pub(crate) const fn restore_stack_page(&mut self) {
        if self.e {
            self.s = 0x0100 | (self.s & 0x00FF);
        }
    }

    /// Steps S by one in `direction`. With [`StackWrap::PageOne`] in emulation mode the step
    /// wraps in the low byte and S stays in page 1; otherwise it is a 16-bit step.
    // @source: wdc-w65c816s#7.1
    // @source: adr-0008
    pub(crate) fn step_stack(&mut self, wrap: StackWrap, direction: Direction) {
        self.s = direction.step(self.s);
        if wrap == StackWrap::PageOne {
            self.restore_stack_page();
        }
    }

    /// Row 23, cycles 2 and 3: fetches the offset "SO", then an internal cycle at the
    /// offset's own address. The data is at 0,S+SO, a 16-bit sum that stays in bank 0.
    // @source: wdc-w65c816s#table-5-7-23
    // @source: wdc-w65c816s#3.5.23
    // @source: wdc-w65c816s#7.1
    pub(super) fn stack_relative(&mut self, bus: &mut impl Bus65816) -> Operand {
        let offset = self.fetch(bus);
        self.internal_at_last_fetch(bus);
        Operand::in_bank_zero(self.s.wrapping_add(u16::from(offset)))
    }

    /// Row 24, cycles 2 to 6: fetches "SO", runs an internal cycle, reads the 16-bit base
    /// address "AAL", "AAH" at 0,S+SO, and runs a second internal cycle at 0,S+SO+1. The data
    /// is at DBR,AA plus Y, a sum that carries into the bank.
    // @source: wdc-w65c816s#table-5-7-24
    // @source: wdc-w65c816s#3.5.24
    pub(super) fn stack_relative_indirect_indexed(&mut self, bus: &mut impl Bus65816) -> Operand {
        let Operand { low, high } = self.stack_relative(bus);
        let base_low = self.read_byte(bus, low);
        let base_high = self.read_byte(bus, high);
        self.internal(bus, high);
        let base = Addr24::from_bank_offset(self.dbr, u16::from_le_bytes([base_low, base_high]));
        Operand::spanning_banks(indexed(base, self.y))
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        registers::Flags,
        testing::{Cycle, native, run},
    };

    // @source: wdc-w65c816s#table-5-7-23
    // @source: 6502org-65c816-opcodes#5.20
    #[test]
    fn stack_relative_data_stays_in_bank_zero() {
        // The example: S $FF10, LDA $FA,S reads $00000A and $00000B.
        let mut cpu = native(Flags::empty());
        cpu.s = 0xFF10;
        let bus = run(&mut cpu, &[0xA3, 0xFA], &[(0x00_000A, &[0xCD, 0xAB])]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Internal(0x12_8001),
                Cycle::Data(0x00_000A),
                Cycle::Data(0x00_000B),
            ]
        );
        assert_eq!((cpu.a, cpu.s, cpu.pc), (0xABCD, 0xFF10, 0x8002));
    }

    // @source: wdc-w65c816s#table-5-7-24
    // @source: 6502org-65c816-opcodes#5.21
    #[test]
    fn stack_relative_indirect_indexed_reads_through_a_bank_zero_pointer() {
        // The example: S $FF10, DBR $12, Y $0050, pointer $FFF0 at $00000A: LDA ($FA,S),Y
        // reads $130040 and $130041.
        let mut cpu = native(Flags::empty());
        cpu.s = 0xFF10;
        cpu.dbr = 0x12;
        cpu.y = 0x0050;
        let bus = run(&mut cpu, &[0xB3, 0xFA], &[(0x00_000A, &[0xF0, 0xFF])]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Internal(0x12_8001),
                Cycle::Data(0x00_000A),
                Cycle::Data(0x00_000B),
                Cycle::Internal(0x00_000B),
                Cycle::Data(0x13_0040),
                Cycle::Data(0x13_0041),
            ]
        );
    }
}
