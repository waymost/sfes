// @source: wdc-w65c816s#table-5-5
// @source: wdc-65816-manual#chapter-18
// @source: 6502org-65c816-opcodes#6.2.1.1
// @source: 6502org-65c816-opcodes#6.2.2.1
// @source: adr-0008
//! Branches, jumps, calls and returns: Bxx, BRA, BRL, JMP, JML, JSR, JSL, RTS and RTL.
//!
//! A call pushes the address of its own last byte, high byte first; a return pulls it and
//! adds one. The program counter wraps in its bank; only JML, JSL, RTL and `JMP al` change
//! the Program Bank Register.

use sfes_api::Addr24;

use crate::{
    addressing::StackWrap,
    bus::Bus65816,
    registers::{Cpu65816, Flags},
};

/// The condition a branch tests.
// @source: wdc-w65c816s#table-5-5
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Condition {
    /// BRA: always.
    Always,
    /// BCC: C = 0.
    CarryClear,
    /// BCS: C = 1.
    CarrySet,
    /// BNE: Z = 0.
    NotEqual,
    /// BEQ: Z = 1.
    Equal,
    /// BPL: N = 0.
    Plus,
    /// BMI: N = 1.
    Minus,
    /// BVC: V = 0.
    OverflowClear,
    /// BVS: V = 1.
    OverflowSet,
}

impl Condition {
    /// Whether the branch is taken with status `flags`.
    // @source: wdc-w65c816s#table-5-5
    fn holds(self, flags: Flags) -> bool {
        match self {
            Self::Always => true,
            Self::CarryClear => !flags.contains(Flags::C),
            Self::CarrySet => flags.contains(Flags::C),
            Self::NotEqual => !flags.contains(Flags::Z),
            Self::Equal => flags.contains(Flags::Z),
            Self::Plus => !flags.contains(Flags::N),
            Self::Minus => flags.contains(Flags::N),
            Self::OverflowClear => !flags.contains(Flags::V),
            Self::OverflowSet => flags.contains(Flags::V),
        }
    }
}

impl Cpu65816 {
    /// The relative branches (row 20): fetches the signed offset. A taken branch adds an
    /// internal cycle (note 5) and, in emulation mode, another when the destination is in a
    /// different page from the next instruction (note 6). Both repeat the offset's address.
    // @source: wdc-w65c816s#table-5-7-20
    // @source: wdc-w65c816s#table-5-7-note-5
    // @source: wdc-w65c816s#table-5-7-note-6
    // @source: wdc-w65c816s#3.5.19
    // @source: wdc-65816-manual#chapter-18-bcc
    // @source: 6502org-65c816-opcodes#6.2.1.1
    pub(crate) fn branch(&mut self, bus: &mut impl Bus65816, condition: Condition) {
        let offset = i8::from_le_bytes([self.fetch(bus)]);
        if !condition.holds(self.p) {
            return;
        }
        self.internal_at_last_fetch(bus);
        let destination = self.pc.wrapping_add_signed(i16::from(offset));
        let [_, page] = self.pc.to_le_bytes();
        let [_, destination_page] = destination.to_le_bytes();
        if self.e && page != destination_page {
            self.internal_at_last_fetch(bus);
        }
        self.pc = destination;
    }

    /// BRL (row 21): fetches a 16-bit offset, spends an internal cycle at its high byte, and
    /// adds the offset to the address of the next instruction, wrapping in the bank.
    // @source: wdc-w65c816s#table-5-7-21
    // @source: wdc-w65c816s#3.5.20
    // @source: wdc-65816-manual#chapter-18-brl
    // @source: 6502org-65c816-opcodes#6.2.1.2
    pub(crate) fn brl(&mut self, bus: &mut impl Bus65816) {
        let offset = self.fetch_address(bus);
        self.internal_at_last_fetch(bus);
        self.pc = self.pc.wrapping_add(offset);
    }

    /// JMP a (row 1b): the new program counter is the operand; the bank is unchanged.
    // @source: wdc-w65c816s#table-5-7-1b
    // @source: wdc-65816-manual#chapter-18-jmp
    // @source: 6502org-65c816-opcodes#6.2.2.1
    pub(crate) fn jmp_absolute(&mut self, bus: &mut impl Bus65816) {
        self.pc = self.fetch_address(bus);
    }

    /// JMP al, also written JML (row 4b): the new program counter and bank are the operand.
    // @source: wdc-w65c816s#table-5-7-4b
    // @source: wdc-65816-manual#chapter-18-jmp
    // @source: 6502org-65c816-opcodes#6.2.2.1
    pub(crate) fn jmp_long(&mut self, bus: &mut impl Bus65816) {
        let address = self.fetch_address(bus);
        self.pbr = self.fetch(bus);
        self.pc = address;
    }

    /// JMP (a) (row 3b): the new program counter is read at 0,AA and 0,AA+1, a pointer in
    /// bank 0 that wraps at the end of the bank.
    // @source: wdc-w65c816s#table-5-7-3b
    // @source: wdc-w65c816s#3.5.4
    // @source: wdc-65816-manual#chapter-18-jmp
    // @source: 6502org-65c816-opcodes#6.2.2.1
    pub(crate) fn jmp_indirect(&mut self, bus: &mut impl Bus65816) {
        let pointer = self.fetch_address(bus);
        self.pc = self.read_bank_zero_word(bus, pointer);
    }

    /// JML (a) (row 3a; the manual and the tutorial write it `JMP [a]`, since the pointer is
    /// 24 bits): the new program counter and bank are read at 0,AA to 0,AA+2.
    // @source: wdc-w65c816s#table-5-7-3a
    // @source: wdc-w65c816s#3.5.4
    // @source: wdc-65816-manual#chapter-18-jmp
    // @source: 6502org-65c816-opcodes#6.2.2.1
    pub(crate) fn jml_indirect(&mut self, bus: &mut impl Bus65816) {
        let pointer = self.fetch_address(bus);
        let address = self.read_bank_zero_word(bus, pointer);
        let bank = Addr24::from_bank_offset(0, pointer.wrapping_add(2));
        self.pbr = self.read_byte(bus, bank);
        self.pc = address;
    }

    /// JMP (a,x) (row 2a): fetches AA, spends an internal cycle at its high byte, and reads
    /// the new program counter at PBR,AA+X, a pointer that wraps in the program bank. The
    /// pointer reads are data cycles (ADR 0008).
    // @source: wdc-w65c816s#table-5-7-2a
    // @source: wdc-w65c816s#3.5.2
    // @source: adr-0008
    // @source: wdc-65816-manual#chapter-18-jmp
    // @source: 6502org-65c816-opcodes#6.2.2.1
    pub(crate) fn jmp_indexed_indirect(&mut self, bus: &mut impl Bus65816) {
        let base = self.fetch_address(bus);
        self.internal_at_last_fetch(bus);
        self.pc = self.read_program_bank_pointer(bus, base);
    }

    /// JSR a (row 1c): fetches the new program counter, spends an internal cycle at its high
    /// byte, and pushes the address of that byte, high byte first. In emulation mode the
    /// pushes stay in page 1.
    // @source: wdc-w65c816s#table-5-7-1c
    // @source: adr-0008
    // @source: wdc-65816-manual#chapter-18-jsr
    // @source: 6502org-65c816-opcodes#6.2.2.1
    pub(crate) fn jsr_absolute(&mut self, bus: &mut impl Bus65816) {
        let destination = self.fetch_address(bus);
        self.internal_at_last_fetch(bus);
        self.push_return_address(bus, StackWrap::PageOne);
        self.pc = destination;
    }

    /// JSR (a,x) (row 2b): fetches AAL, pushes the address of the next byte (the
    /// instruction's last), fetches AAH, spends an internal cycle there, and reads the new
    /// program counter at PBR,AA+X. The pushes may leave page 1 in emulation mode (7.1).
    // @source: wdc-w65c816s#table-5-7-2b
    // @source: wdc-w65c816s#7.1
    // @source: adr-0008
    // @source: wdc-65816-manual#chapter-18-jsr
    // @source: 6502org-65c816-opcodes#6.2.2.1
    pub(crate) fn jsr_indexed_indirect(&mut self, bus: &mut impl Bus65816) {
        let low = self.fetch(bus);
        let [return_low, return_high] = self.pc.to_le_bytes();
        self.push(bus, return_high, StackWrap::BankZero);
        self.push(bus, return_low, StackWrap::BankZero);
        let high = self.fetch(bus);
        self.internal_at_last_fetch(bus);
        self.pc = self.read_program_bank_pointer(bus, u16::from_le_bytes([low, high]));
        self.restore_stack_page();
    }

    /// JSL al (row 4c): fetches the new program counter, pushes the Program Bank Register,
    /// spends an internal cycle at the stack address it was written to, fetches the new bank,
    /// and pushes the address of the instruction's last byte. The pushes may leave page 1 in
    /// emulation mode (7.1).
    // @source: wdc-w65c816s#table-5-7-4c
    // @source: wdc-w65c816s#7.1
    // @source: adr-0008
    // @source: wdc-65816-manual#chapter-18-jsl
    // @source: 6502org-65c816-opcodes#6.2.2.1
    pub(crate) fn jsl(&mut self, bus: &mut impl Bus65816) {
        let destination = self.fetch_address(bus);
        let pushed_at = self.stack_address();
        self.push(bus, self.pbr, StackWrap::BankZero);
        self.internal(bus, pushed_at);
        let bank = self.fetch(bus);
        self.push_return_address(bus, StackWrap::BankZero);
        self.pbr = bank;
        self.pc = destination;
        self.restore_stack_page();
    }

    /// RTS (row 22h): two internal cycles at PBR,PC+1, pulls the program counter, adds one,
    /// and spends an internal cycle at the stack address of the last byte pulled. In
    /// emulation mode the pulls stay in page 1.
    // @source: wdc-w65c816s#table-5-7-22h
    // @source: adr-0008
    // @source: wdc-65816-manual#chapter-18-rts
    // @source: 6502org-65c816-opcodes#6.2.2.2
    pub(crate) fn rts(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.internal_at_program_counter(bus);
        let address = self.pull_word(bus, StackWrap::PageOne);
        self.internal(bus, self.stack_address());
        self.pc = address.wrapping_add(1);
    }

    /// RTL (row 22i): two internal cycles at PBR,PC+1, then pulls the program counter and the
    /// bank and adds one to the program counter, within the new bank. The pulls may leave
    /// page 1 in emulation mode (7.1).
    // @source: wdc-w65c816s#table-5-7-22i
    // @source: wdc-w65c816s#7.1
    // @source: adr-0008
    // @source: wdc-65816-manual#chapter-18-rtl
    // @source: 6502org-65c816-opcodes#6.2.2.2
    pub(crate) fn rtl(&mut self, bus: &mut impl Bus65816) {
        self.internal_at_program_counter(bus);
        self.internal_at_program_counter(bus);
        let address = self.pull_word(bus, StackWrap::BankZero);
        self.pbr = self.pull(bus, StackWrap::BankZero);
        self.pc = address.wrapping_add(1);
        self.restore_stack_page();
    }

    /// Pushes the address of the last byte fetched, the instruction's last: "PCH", then
    /// "PCL".
    // @source: wdc-w65c816s#table-5-7-1c
    fn push_return_address(&mut self, bus: &mut impl Bus65816, wrap: StackWrap) {
        let [low, high] = self.pc.wrapping_sub(1).to_le_bytes();
        self.push(bus, high, wrap);
        self.push(bus, low, wrap);
    }

    /// Pulls a 16-bit value, low byte first.
    // @source: wdc-w65c816s#table-5-7-22h
    fn pull_word(&mut self, bus: &mut impl Bus65816, wrap: StackWrap) -> u16 {
        let low = self.pull(bus, wrap);
        let high = self.pull(bus, wrap);
        u16::from_le_bytes([low, high])
    }

    /// The 16-bit value at 0,`pointer` and 0,`pointer`+1, wrapping at the end of bank 0.
    // @source: wdc-w65c816s#table-5-7-3b
    fn read_bank_zero_word(&self, bus: &mut impl Bus65816, pointer: u16) -> u16 {
        let low = self.read_byte(bus, Addr24::from_bank_offset(0, pointer));
        let high = self.read_byte(bus, Addr24::from_bank_offset(0, pointer.wrapping_add(1)));
        u16::from_le_bytes([low, high])
    }

    /// The 16-bit value at PBR,`base`+X and the next offset, wrapping in the program bank.
    // @source: wdc-w65c816s#table-5-7-2a
    // @source: adr-0008
    fn read_program_bank_pointer(&self, bus: &mut impl Bus65816, base: u16) -> u16 {
        let pointer = base.wrapping_add(self.x);
        let low = self.read_byte(bus, Addr24::from_bank_offset(self.pbr, pointer));
        let next = Addr24::from_bank_offset(self.pbr, pointer.wrapping_add(1));
        let high = self.read_byte(bus, next);
        u16::from_le_bytes([low, high])
    }
}

#[cfg(test)]
mod tests {
    use std::vec::Vec;

    use super::*;
    use crate::testing::{Cycle, emulation, native, run};

    // @source: wdc-w65c816s#table-5-7-20
    // @source: wdc-w65c816s#table-5-7-note-5
    #[test]
    fn a_branch_costs_a_cycle_only_when_taken() {
        let mut cpu = native(Flags::Z);
        let bus = run(&mut cpu, &[0xD0, 0x10], &[]);
        assert_eq!(bus.cycles.len(), 2);
        assert_eq!(cpu.pc, 0x8002);

        let mut cpu = native(Flags::Z);
        let bus = run(&mut cpu, &[0xF0, 0xFE], &[]);
        assert_eq!(bus.cycles[2..], [Cycle::Internal(0x12_8001)]);
        assert_eq!(cpu.pc, 0x8000);
    }

    // @source: wdc-w65c816s#table-5-7-note-6
    #[test]
    fn an_emulation_mode_branch_across_a_page_costs_another_cycle() {
        let mut cpu = emulation();
        let bus = run(&mut cpu, &[0x80, 0x80], &[]);
        assert_eq!(
            bus.cycles[2..],
            [Cycle::Internal(0x12_8001), Cycle::Internal(0x12_8001)]
        );
        assert_eq!(cpu.pc, 0x7F82);
    }

    // @source: wdc-w65c816s#table-5-7-1c
    // @source: wdc-w65c816s#table-5-7-22h
    #[test]
    fn jsr_and_rts_round_trip_through_page_one() {
        let mut cpu = emulation();
        cpu.s = 0x0100;
        let bus = run(&mut cpu, &[0x20, 0x34, 0x12], &[]);
        assert_eq!(
            bus.cycles[3..],
            [
                Cycle::Internal(0x12_8002),
                Cycle::Write(0x00_0100, 0x80),
                Cycle::Write(0x00_01FF, 0x02),
            ]
        );
        assert_eq!((cpu.pc, cpu.s), (0x1234, 0x01FE));

        let mut cpu = emulation();
        cpu.s = 0x01FE;
        run(
            &mut cpu,
            &[0x60],
            &[(0x00_01FF, &[0x02]), (0x00_0100, &[0x80])],
        );
        assert_eq!((cpu.pc, cpu.s), (0x8003, 0x0100));
    }

    // @source: wdc-w65c816s#table-5-7-4c
    // @source: wdc-w65c816s#7.1
    // @source: adr-0008
    #[test]
    fn jsl_may_leave_page_one_and_s_returns_to_it() {
        let mut cpu = emulation();
        cpu.s = 0x0100;
        let bus = run(&mut cpu, &[0x22, 0x56, 0x34, 0x77], &[]);
        assert_eq!(
            bus.cycles[3..],
            [
                Cycle::Write(0x00_0100, 0x12),
                Cycle::Internal(0x00_0100),
                Cycle::Program(0x12_8003),
                Cycle::Write(0x00_00FF, 0x80),
                Cycle::Write(0x00_00FE, 0x03),
            ]
        );
        assert_eq!((cpu.pbr, cpu.pc, cpu.s), (0x77, 0x3456, 0x01FD));
    }

    // @source: wdc-w65c816s#table-5-7-3a
    #[test]
    fn jml_indirect_reads_its_pointer_in_bank_zero() {
        let mut cpu = native(Flags::empty());
        run(
            &mut cpu,
            &[0xDC, 0xFE, 0xFF],
            &[(0x00_FFFE, &[0x34, 0x12]), (0x00_0000, &[0x56])],
        );
        assert_eq!((cpu.pbr, cpu.pc), (0x56, 0x1234));
    }

    // @source: wdc-w65c816s#table-5-7-2a
    // @source: adr-0008
    #[test]
    fn jmp_indexed_indirect_reads_its_pointer_in_the_program_bank_as_data() {
        let mut cpu = native(Flags::empty());
        cpu.x = 0x0002;
        let bus = run(&mut cpu, &[0x7C, 0xFE, 0xFF], &[(0x12_0000, &[0x34, 0x12])]);
        assert_eq!(
            bus.cycles[3..],
            [
                Cycle::Internal(0x12_8002),
                Cycle::Data(0x12_0000),
                Cycle::Data(0x12_0001),
            ]
        );
        assert_eq!(cpu.pc, 0x1234);
    }

    // @source: 6502org-65c816-opcodes#6.2.1.1
    #[test]
    fn bne_follows_the_tutorial_example() {
        // Z = 0: BNE $C023 at $ABC000 (displacement $21) branches to $ABC023.
        let mut cpu = native(Flags::empty());
        cpu.pbr = 0xAB;
        cpu.pc = 0xC000;
        run(&mut cpu, &[0xD0, 0x21], &[]);
        assert_eq!((cpu.pbr, cpu.pc), (0xAB, 0xC023));
    }

    // @source: 6502org-65c816-opcodes#6.2.1.2
    #[test]
    fn brl_follows_the_tutorial_example() {
        // BRL $C045 at $ABC000 (displacement $0042) branches to $ABC045.
        let mut cpu = native(Flags::empty());
        cpu.pbr = 0xAB;
        cpu.pc = 0xC000;
        run(&mut cpu, &[0x82, 0x42, 0x00], &[]);
        assert_eq!((cpu.pbr, cpu.pc), (0xAB, 0xC045));
    }

    // @source: 6502org-65c816-opcodes#6.2.2.1
    #[test]
    fn jsr_follows_the_tutorial_example() {
        // S $01FF, JSR $ABCD at $123456: $34 at $0001FF, $58 at $0001FE, then $12ABCD with
        // S $01FD.
        let mut cpu = native(Flags::empty());
        cpu.s = 0x01FF;
        cpu.pc = 0x3456;
        let bus = run(&mut cpu, &[0x20, 0xCD, 0xAB], &[]);
        assert_eq!(
            bus.cycles[4..],
            [Cycle::Write(0x00_01FF, 0x34), Cycle::Write(0x00_01FE, 0x58)]
        );
        assert_eq!((cpu.pbr, cpu.pc, cpu.s), (0x12, 0xABCD, 0x01FD));
    }

    // @source: 6502org-65c816-opcodes#6.2.2.1
    #[test]
    fn jsl_pushes_a_return_address_that_wraps_in_its_bank() {
        // JSL at $12FFFD pushes $12, $00 and $00, not $13, $00 and $00.
        let mut cpu = native(Flags::empty());
        cpu.s = 0x01FF;
        cpu.pc = 0xFFFD;
        // The program counter wraps in its bank, so the operand's bank byte is at $120000.
        let bus = run(&mut cpu, &[0x22, 0x00, 0x80], &[(0x12_0000, &[0x34])]);
        let writes = bus
            .cycles
            .iter()
            .filter_map(|cycle| match cycle {
                Cycle::Write(_, value) => Some(*value),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(writes, [0x12, 0x00, 0x00]);
        assert_eq!((cpu.pbr, cpu.pc), (0x34, 0x8000));
    }

    // @source: 6502org-65c816-opcodes#6.2.2.2
    #[test]
    fn rts_and_rtl_follow_the_tutorial_examples() {
        // K $12, S $01FD, $56 at $0001FE and $34 at $0001FF: RTS returns to $123457 with S
        // $01FF.
        let mut cpu = native(Flags::empty());
        cpu.s = 0x01FD;
        run(&mut cpu, &[0x60], &[(0x00_01FE, &[0x56, 0x34])]);
        assert_eq!((cpu.pbr, cpu.pc, cpu.s), (0x12, 0x3457, 0x01FF));

        // RTL pulling $FF, $FF and $12 continues at $120000, not $130000.
        let mut cpu = native(Flags::empty());
        cpu.s = 0x01FC;
        cpu.pbr = 0x77;
        run(&mut cpu, &[0x6B], &[(0x00_01FD, &[0xFF, 0xFF, 0x12])]);
        assert_eq!((cpu.pbr, cpu.pc, cpu.s), (0x12, 0x0000, 0x01FF));
    }
}
