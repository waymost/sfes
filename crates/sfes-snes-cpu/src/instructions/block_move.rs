// @source: wdc-w65c816s#table-5-7-9a
// @source: wdc-w65c816s#table-5-7-9b
// @source: wdc-w65c816s#3.5.9
// @source: wdc-w65c816s#7.18
// @source: wdc-65816-manual#chapter-18-mvn
// @source: wdc-65816-manual#chapter-18-mvp
// @source: 6502org-65c816-opcodes#6.6
// @source: adr-0004
//! The block moves, MVN and MVP.
//!
//! One step moves one byte in the seven cycles of Table 5-7 rows 9a and 9b (ADR 0004). While
//! bytes remain, the program counter is put back on the opcode, so the next step fetches the
//! instruction again and an interrupt between two bytes returns to it.

use sfes_api::Addr24;

use crate::{
    addressing::Direction,
    bus::Bus65816,
    registers::{Cpu65816, Flags},
};

/// `register` stepped by one in `direction` after a block move's byte. With the index
/// registers 8 bits wide (`narrow`) the high byte stays zero, so the step wraps within page 0
/// of the bank (7.8.3.2); otherwise it wraps within the bank.
// @source: wdc-w65c816s#3.5.9
// @source: wdc-w65c816s#7.8.3.2
// @source: wdc-65816-manual#chapter-17-block-move-addressing
// @source: 6502org-65c816-opcodes#5.1.2
// @source: 6502org-65c816-opcodes#6.6
const fn step_index(register: u16, direction: Direction, narrow: bool) -> u16 {
    let stepped = direction.step(register);
    if narrow { stepped & 0x00FF } else { stepped }
}

impl Cpu65816 {
    /// MVN: moves the byte at srcbnk,X to dstbnk,Y, then increments X and Y (row 9a).
    // @source: wdc-w65c816s#table-5-7-9a
    // @source: wdc-65816-manual#chapter-18-mvn
    // @source: 6502org-65c816-opcodes#6.6
    pub(crate) fn mvn(&mut self, bus: &mut impl Bus65816) {
        self.block_move(bus, Direction::Up);
    }

    /// MVP: moves the byte at srcbnk,X to dstbnk,Y, then decrements X and Y (row 9b).
    // @source: wdc-w65c816s#table-5-7-9b
    // @source: wdc-65816-manual#chapter-18-mvp
    // @source: 6502org-65c816-opcodes#6.6
    pub(crate) fn mvp(&mut self, bus: &mut impl Bus65816) {
        self.block_move(bus, Direction::Down);
    }

    /// Cycles 2 to 7 of rows 9a and 9b, which move one byte:
    ///
    /// 2. "DBA" at PBR,PC+1: the destination bank, the first operand byte (3.5.9).
    /// 3. "SBA" at PBR,PC+2: the source bank.
    /// 4. "SRC Data" read at SBA,X.
    /// 5. "Dest Data" written at DBA,Y.
    /// 6. and 7. "IO" at DBA,Y.
    ///
    /// Then the destination bank goes into the Data Bank Register (7.18), X and Y step, and
    /// the 16-bit C decrements whatever the M flag (the manual's entries). The instruction
    /// ends when C reaches `FFFF`, the program counter past its three bytes; until then the
    /// program counter is put back on the opcode, "the address of the block move
    /// instruction" that an interrupt pushes (the manual) and that the next step fetches
    /// again (tutorial 6.6).
    // @source: wdc-w65c816s#table-5-7-9a
    // @source: wdc-w65c816s#table-5-7-9b
    // @source: wdc-w65c816s#3.5.9
    // @source: wdc-w65c816s#7.18
    // @source: wdc-65816-manual#chapter-18-mvn
    // @source: wdc-65816-manual#chapter-18-mvp
    // @source: 6502org-65c816-opcodes#5.19
    // @source: 6502org-65c816-opcodes#6.6
    // @source: adr-0004
    fn block_move(&mut self, bus: &mut impl Bus65816, direction: Direction) {
        let destination = self.fetch(bus);
        let source = self.fetch(bus);
        let value = self.read_byte(bus, Addr24::from_bank_offset(source, self.x));
        let target = Addr24::from_bank_offset(destination, self.y);
        self.write_byte(bus, target, value);
        self.internal(bus, target);
        self.internal(bus, target);

        self.dbr = destination;
        let narrow = self.p.contains(Flags::X);
        self.x = step_index(self.x, direction, narrow);
        self.y = step_index(self.y, direction, narrow);
        self.a = self.a.wrapping_sub(1);
        if self.a != 0xFFFF {
            self.pc = self.pc.wrapping_sub(3);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{Cycle, emulation, native, run, run_steps};

    // @source: wdc-w65c816s#table-5-7-9a
    // @source: wdc-65816-manual#chapter-18-mvn
    #[test]
    fn one_step_of_mvn_moves_one_byte_and_returns_to_the_opcode() {
        let mut cpu = native(Flags::empty());
        cpu.a = 0x0001;
        cpu.x = 0x1000;
        cpu.y = 0x2000;
        let bus = run(&mut cpu, &[0x54, 0x34, 0x56], &[(0x56_1000, &[0xAB])]);
        assert_eq!(
            bus.cycles,
            [
                Cycle::Opcode(0x12_8000),
                Cycle::Program(0x12_8001),
                Cycle::Program(0x12_8002),
                Cycle::Data(0x56_1000),
                Cycle::Write(0x34_2000, 0xAB),
                Cycle::Internal(0x34_2000),
                Cycle::Internal(0x34_2000),
            ]
        );
        assert_eq!(
            (cpu.a, cpu.x, cpu.y, cpu.dbr),
            (0x0000, 0x1001, 0x2001, 0x34)
        );
        assert_eq!(cpu.pc, 0x8000);
    }

    // @source: wdc-w65c816s#table-5-7-9b
    // @source: wdc-65816-manual#chapter-18-mvp
    #[test]
    fn the_last_step_of_mvp_leaves_the_program_counter_past_the_instruction() {
        let mut cpu = native(Flags::M);
        cpu.a = 0x0000;
        cpu.x = 0x1000;
        cpu.y = 0x2000;
        let bus = run(&mut cpu, &[0x44, 0x34, 0x56], &[(0x56_1000, &[0xAB])]);
        assert_eq!(bus.byte(0x34_2000), 0xAB);
        assert_eq!((cpu.a, cpu.x, cpu.y), (0xFFFF, 0x0FFF, 0x1FFF));
        assert_eq!(cpu.pc, 0x8003);
    }

    // @source: 6502org-65c816-opcodes#5.19
    #[test]
    fn mvn_follows_the_tutorial_addressing_example() {
        // A $0002, X $FFFE, Y $FFFF: MVN #$12,#$34 moves $12FFFE to $34FFFF, $12FFFF to
        // $340000 and $120000 to $340001.
        let mut cpu = native(Flags::empty());
        cpu.pbr = 0x00;
        cpu.a = 0x0002;
        cpu.x = 0xFFFE;
        cpu.y = 0xFFFF;
        let memory: [(u32, &[u8]); 2] = [(0x12_FFFE, &[0x11, 0x22]), (0x12_0000, &[0x33])];
        let bus = run_steps(&mut cpu, &[0x54, 0x34, 0x12], &memory, 3);
        assert_eq!(
            [
                bus.byte(0x34_FFFF),
                bus.byte(0x34_0000),
                bus.byte(0x34_0001)
            ],
            [0x11, 0x22, 0x33]
        );
        assert_eq!(
            (cpu.a, cpu.x, cpu.y, cpu.pc),
            (0xFFFF, 0x0001, 0x0002, 0x8003)
        );
    }

    // @source: 6502org-65c816-opcodes#6.6
    #[test]
    fn overlapping_mvn_copies_a_pattern_as_in_the_tutorial_example() {
        // A $0003, X $1000, Y $1002, $001000 holds $AB, $001001 holds $CD: after MVN #0,#0
        // $001002 to $001005 hold $AB, $CD, $AB, $CD.
        let mut cpu = native(Flags::empty());
        cpu.a = 0x0003;
        cpu.x = 0x1000;
        cpu.y = 0x1002;
        let bus = run_steps(
            &mut cpu,
            &[0x54, 0x00, 0x00],
            &[(0x00_1000, &[0xAB, 0xCD])],
            4,
        );
        let copied = (0x00_1002..=0x00_1005).map(|address| bus.byte(address));
        assert!(copied.eq([0xAB, 0xCD, 0xAB, 0xCD]));
        assert_eq!(bus.cycles.len(), 4 * 7);
    }

    // @source: 6502org-65c816-opcodes#6.6
    #[test]
    fn mvp_follows_the_tutorial_example() {
        // A $0001, X $1000, Y $2000, $000FFF holds $AB, $001000 holds $CD: after MVP #0,#0
        // A is $FFFF, X $0FFE, Y $1FFE, DBR $00, and $001FFF and $002000 hold $AB and $CD.
        let mut cpu = native(Flags::empty());
        cpu.dbr = 0x7E;
        cpu.a = 0x0001;
        cpu.x = 0x1000;
        cpu.y = 0x2000;
        let bus = run_steps(
            &mut cpu,
            &[0x44, 0x00, 0x00],
            &[(0x00_0FFF, &[0xAB, 0xCD])],
            2,
        );
        assert_eq!(
            (cpu.a, cpu.x, cpu.y, cpu.dbr),
            (0xFFFF, 0x0FFE, 0x1FFE, 0x00)
        );
        assert_eq!([bus.byte(0x00_1FFF), bus.byte(0x00_2000)], [0xAB, 0xCD]);
    }

    // @source: wdc-w65c816s#7.8.3.2
    // @source: wdc-65816-manual#chapter-18-mvn
    #[test]
    fn eight_bit_indexes_wrap_within_page_zero_in_emulation_mode() {
        let mut cpu = emulation();
        cpu.a = 0x0001;
        cpu.x = 0x00FF;
        cpu.y = 0x00FF;
        let bus = run_steps(&mut cpu, &[0x54, 0x34, 0x56], &[(0x56_00FF, &[0x11])], 1);
        assert_eq!(bus.byte(0x34_00FF), 0x11);
        assert_eq!((cpu.x, cpu.y), (0x0000, 0x0000));
    }

    // @source: wdc-w65c816s#3.5.9
    #[test]
    fn directions_step_by_one_within_the_index_width() {
        assert_eq!(step_index(0xFFFF, Direction::Up, false), 0x0000);
        assert_eq!(step_index(0x0000, Direction::Down, false), 0xFFFF);
        assert_eq!(step_index(0x00FF, Direction::Up, true), 0x0000);
        assert_eq!(step_index(0x0000, Direction::Down, true), 0x00FF);
    }
}
