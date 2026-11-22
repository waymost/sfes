// @source: anomie-regs#4016
// @source: anomie-regs#4017
// @source: anomie-regs#4218
// @source: anomie-regs#controllers/generic
// @source: anomie-regs#controllers/joypads
// @source: anomie-timing#s-cpu/auto-joypad-read
// @source: fullsnes#snescontrollersioportsautomaticreading
// @source: fullsnes#snescontrollersioportsmanualreading
// @source: fullsnes#snescontrollersjoypad
// @source: adr-0024
//! The controller ports: a standard pad on each, the latch at `$4016`, the serial reads of
//! `$4016/$4017`, the auto-joypad read into `$4218–$421F` and the frame's polled flag
//! (ADR 0024).

use sfes_api::{Buttons, Cycles};

use crate::bus::Registers;

/// The number of controller ports.
const PORTS: usize = 2;

/// The buttons' bits of a pad's 16, in serial order from bit 0: B first, R twelfth.
const BUTTON_BITS: u32 = 0x0FFF;
/// What a pad returns after its 16th bit: ones until latched again.
const AFTER_SIXTEEN: u32 = 0xFFFF_0000;
/// The bit a clock shifts in at the top of a pad's register.
const TOP_BIT: u32 = 0x8000_0000;
/// `$4016` write bit 0: the latch line of both ports.
const LATCH: u8 = 0x01;
/// $4200 NMITIMEN bit 0: the auto-joypad read enable.
const AUTO_READ_ENABLE: u8 = 0x01;
/// The bits the auto-read takes from each data line.
const AUTO_READ_BITS: u8 = 16;
/// Master cycles from the auto-read's start to its first bit, and between its bits.
const AUTO_READ_BIT_INTERVAL: u64 = 256;
/// Master cycles from the auto-read's start to its end (`anomie-timing`).
const AUTO_READ_LENGTH: u64 = 4224;

/// One of the two controller ports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Port {
    /// Port 1, read at `$4016`.
    One,
    /// Port 2, read at `$4017`.
    Two,
}

impl Port {
    /// Both ports, in order.
    const ALL: [Self; PORTS] = [Self::One, Self::Two];
}

/// One of `$4218–$421F` JOY1L–JOY4H, by its offset from `$4218`, 0–7.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct JoypadByte(u8);

impl JoypadByte {
    /// The register at `offset` from `$4218`; only the low three bits count.
    // @source: anomie-regs#4218
    pub(crate) const fn new(offset: u8) -> Self {
        Self(offset & 0x07)
    }
}

/// The ports' registers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PortRegister {
    /// $4016 JOYSER0 (port 1) or $4017 JOYSER1 (port 2): the port's data lines, and at
    /// `$4016` the latch.
    Serial(Port),
    /// $4218–$421F JOY1L–JOY4H: the auto-joypad read's results.
    Joypad(JoypadByte),
}

/// A standard pad: its bits still to shift out, the next in bit 0 (ADR 0024, point 2).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Pad {
    bits: u32,
}

impl Pad {
    /// A pad at power-on: as after a complete read, returning 1 until latched.
    // @source: anomie-regs#controllers/joypads
    // @source: adr-0024
    const fn power_on() -> Self {
        Self { bits: u32::MAX }
    }

    /// A pad just latched with `buttons`: the 12 buttons, a pressed one as 1, then four 0
    /// ID bits, then ones.
    // @source: anomie-regs#4016
    // @source: anomie-regs#controllers/joypads
    // @source: fullsnes#snescontrollershardwareidcodes
    // @source: fullsnes#snescontrollersjoypad
    // @source: adr-0024
    const fn loaded(buttons: Buttons) -> Self {
        Self {
            bits: (buttons.bits() as u32 & BUTTON_BITS) | AFTER_SIXTEEN,
        }
    }

    /// The bit on Data1 now.
    // @source: anomie-regs#controllers/generic
    // @source: adr-0024
    const fn data(self) -> u8 {
        (self.bits & 1) as u8
    }

    /// A clock with Latch low: the current bit, then the pad moves to the next.
    // @source: anomie-regs#controllers/generic
    // @source: anomie-regs#controllers/joypads
    // @source: adr-0024
    const fn shift(&mut self) -> u8 {
        let bit = self.data();
        self.bits = (self.bits >> 1) | TOP_BIT;
        bit
    }
}

/// The frame's polled flag and the last ended frame's (ADR 0024, point 8).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Polled {
    /// Whether the CPU has read the controllers in the frame now running.
    frame: bool,
    /// Whether it did in the last frame that ended.
    last_frame: bool,
}

/// Both controller ports, the auto-read and the polled flag.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Ports {
    /// The buttons held on each port, set by `Snes::set_input`.
    pub(crate) input: [Buttons; PORTS],
    /// The pad on each port.
    pads: [Pad; PORTS],
    /// $4016 JOYSER0 write bit 0: the latch line.
    latch: bool,
    /// $4200 NMITIMEN bit 0: the auto-joypad read enable.
    auto_read_enabled: bool,
    /// $4218–$421F JOY1–JOY4: port 1 Data1, port 2 Data1, port 1 Data2, port 2 Data2.
    joypads: [u16; 4],
    /// The bits the running auto-read has taken; `None` when none runs.
    auto_read: Option<u8>,
    /// Whether the controllers were read in this frame and the last.
    polled: Polled,
}

impl Ports {
    /// The ports at power-on: no buttons, the latch low, the auto-read off and
    /// `$4218–$421F` 0 (`fullsnes#snesiomap`; ADR 0024, points 2, 3 and 7).
    // @source: anomie-regs#4200
    // @source: fullsnes#snesiomap
    // @source: adr-0024
    pub(crate) const fn power_on() -> Self {
        Self {
            input: [Buttons::empty(); PORTS],
            pads: [Pad::power_on(); PORTS],
            latch: false,
            auto_read_enabled: false,
            joypads: [0; 4],
            auto_read: None,
            polled: Polled {
                frame: false,
                last_frame: false,
            },
        }
    }

    /// Reset: the latch low and the auto-read off, with a read in progress abandoned. The
    /// pads, the held buttons, `$4218–$421F` and the polled flags are kept (ADR 0024,
    /// points 3, 5 and 7).
    // @source: anomie-regs#4200
    // @source: fullsnes#snesiomap
    // @source: adr-0024
    pub(crate) const fn reset(&mut self) {
        self.latch = false;
        self.auto_read_enabled = false;
        self.auto_read = None;
    }

    /// $4016 JOYSER0 write: bit 0 drives Latch. Writing 1 loads both pads, and so does
    /// writing 0 while Latch was high (ADR 0024, point 3).
    // @source: anomie-regs#4016
    // @source: fullsnes#snescontrollersioportsmanualreading
    // @source: adr-0024
    fn write_latch(&mut self, value: u8) {
        let latch = value & LATCH != 0;
        if latch || self.latch {
            self.load();
        }
        self.latch = latch;
    }

    /// $4200 NMITIMEN write: bit 0 enables the auto-read (ADR 0024, point 5).
    // @source: anomie-regs#4200
    // @source: adr-0024
    pub(crate) const fn write_auto_read_enable(&mut self, value: u8) {
        self.auto_read_enabled = value & AUTO_READ_ENABLE != 0;
    }

    /// Loads both pads with the buttons held now.
    // @source: anomie-regs#controllers/generic
    // @source: adr-0024
    fn load(&mut self) {
        self.pads = self.input.map(Pad::loaded);
    }

    /// Clocks the pad on `port`: Data1 in bit 0 and Data2, never connected, 0 in bit 1. With
    /// Latch high the pad reloads and returns B without advancing (ADR 0024, point 2).
    // @source: anomie-regs#controllers/generic
    // @source: anomie-regs#controllers/joypads
    // @source: fullsnes#snescontrollersioportsautomaticreading
    // @source: adr-0024
    fn clock(&mut self, port: Port) -> u8 {
        let buttons = self.input[port as usize];
        let pad = &mut self.pads[port as usize];
        if self.latch {
            *pad = Pad::loaded(buttons);
            pad.data()
        } else {
            pad.shift()
        }
    }

    /// The data bits a read of `port` would return, without clocking the pad (ADR 0024,
    /// point 4).
    // @source: anomie-regs#4016
    // @source: anomie-regs#4017
    // @source: adr-0024
    const fn data(&self, port: Port) -> u8 {
        if self.latch {
            Pad::loaded(self.input[port as usize]).data()
        } else {
            self.pads[port as usize].data()
        }
    }

    /// One of `$4218–$421F`.
    // @source: anomie-regs#4218
    // @source: fullsnes#snescontrollersioportsautomaticreading
    // @source: adr-0024
    const fn joypad(&self, byte: JoypadByte) -> u8 {
        let JoypadByte(offset) = byte;
        let [low, high] = self.joypads[(offset / 2) as usize].to_le_bytes();
        if offset.is_multiple_of(2) { low } else { high }
    }

    /// $4212 HVBJOY bit 0: whether an auto-read is running.
    // @source: anomie-regs#4212
    // @source: anomie-timing#s-cpu/auto-joypad-read
    // @source: adr-0024
    pub(crate) const fn auto_read_busy(&self) -> bool {
        self.auto_read.is_some()
    }

    /// The auto-read's event due at `at`. With no read running it starts one if `$4200`
    /// bit 0 is set: Latch pulses and the busy flag sets. A running read clocks both ports
    /// and rotates each data line's bit into its register, or, after the 16th bit, ends.
    /// Returns when its next event is due (ADR 0024, points 5 and 6).
    // @source: anomie-regs#4218
    // @source: anomie-regs#controllers/generic
    // @source: anomie-timing#s-cpu/auto-joypad-read
    // @source: fullsnes#snescontrollersioportsautomaticreading
    // @source: adr-0024
    pub(crate) fn auto_read_event(&mut self, at: Cycles) -> Option<Cycles> {
        match self.auto_read {
            None if self.auto_read_enabled => {
                self.load();
                self.auto_read = Some(0);
                Some(at + AUTO_READ_BIT_INTERVAL)
            }
            None => None,
            Some(AUTO_READ_BITS) => {
                self.auto_read = None;
                None
            }
            Some(bits) => {
                for port in Port::ALL {
                    let data = self.clock(port);
                    let data1 = &mut self.joypads[port as usize];
                    *data1 = (*data1 << 1) | u16::from(data & 1);
                    let data2 = &mut self.joypads[port as usize + PORTS];
                    *data2 = (*data2 << 1) | u16::from(data >> 1);
                }
                let taken = bits + 1;
                self.auto_read = Some(taken);
                let next = if taken == AUTO_READ_BITS {
                    AUTO_READ_LENGTH - u64::from(AUTO_READ_BITS) * AUTO_READ_BIT_INTERVAL
                } else {
                    AUTO_READ_BIT_INTERVAL
                };
                Some(at + next)
            }
        }
    }

    /// A frame ended: its polled flag becomes the last frame's, and the next frame starts
    /// unpolled (ADR 0024, point 8).
    // @source: adr-0020
    // @source: adr-0024
    pub(crate) const fn end_frame(&mut self) {
        self.polled = Polled {
            frame: false,
            last_frame: self.polled.frame,
        };
    }

    /// Whether the CPU read the controllers in the last frame that ended.
    // @source: adr-0024
    pub(crate) const fn last_frame_polled(&self) -> bool {
        self.polled.last_frame
    }
}

impl Registers for Ports {
    type Register = PortRegister;

    /// $4016 or $4017: the port's data bits, which the bus puts under open bus and, at
    /// `$4017`, the grounded lines (ADR 0024, point 4); or one of `$4218–$421F`.
    // @source: anomie-regs#4016
    // @source: anomie-regs#4017
    // @source: anomie-regs#4218
    // @source: adr-0024
    fn peek(&self, register: PortRegister) -> Option<u8> {
        match register {
            PortRegister::Serial(port) => Some(self.data(port)),
            PortRegister::Joypad(byte) => Some(self.joypad(byte)),
        }
    }

    /// A read polls the controllers, and a read of `$4016` or `$4017` then moves the port's
    /// pad on (ADR 0024, points 4 and 8).
    // @source: anomie-regs#4016
    // @source: anomie-regs#4017
    // @source: fullsnes#snescontrollersioportsmanualreading
    // @source: adr-0024
    fn after_read(&mut self, register: PortRegister) {
        self.polled.frame = true;
        if let PortRegister::Serial(port) = register {
            self.clock(port);
        }
    }

    /// $4016 JOYSER0 drives Latch; `$4017` and `$4218–$421F` are read-only.
    // @source: anomie-regs#4016
    // @source: adr-0024
    fn write(&mut self, register: PortRegister, value: u8) {
        if register == PortRegister::Serial(Port::One) {
            self.write_latch(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use super::*;

    /// A read of `$4016` (port 1) or `$4017` (port 2): the data bits.
    // @source: anomie-regs#4016
    // @source: anomie-regs#4017
    fn read_data(ports: &mut Ports, port: Port) -> u8 {
        ports.read(PortRegister::Serial(port)).unwrap_or_default()
    }

    /// The 16 Data1 bits of `port` read by hand after a latch, then the 17th.
    // @source: anomie-regs#controllers/generic
    // @source: adr-0024
    fn read_by_hand(ports: &mut Ports, port: Port) -> (Vec<u8>, u8) {
        ports.write_latch(1);
        ports.write_latch(0);
        let bits = (0..16).map(|_| read_data(ports, port)).collect::<Vec<_>>();
        (bits, read_data(ports, port))
    }

    /// Runs a whole auto-read started at `start`.
    // @source: adr-0024
    fn auto_read(ports: &mut Ports, start: Cycles) {
        let mut next = ports.auto_read_event(start);
        while let Some(at) = next {
            next = ports.auto_read_event(at);
        }
    }

    // @source: anomie-regs#4016
    // @source: anomie-regs#controllers/joypads
    // @source: fullsnes#snescontrollersjoypad
    // @source: adr-0024
    #[test]
    fn a_pad_shifts_its_buttons_in_serial_order_then_ones() {
        let mut ports = Ports::power_on();
        ports.input = [
            Buttons::B | Buttons::START | Buttons::A | Buttons::R,
            Buttons::LEFT,
        ];
        // B, Y, Select, Start, Up, Down, Left, Right, A, X, L, R, then ID 0000.
        let (bits, after) = read_by_hand(&mut ports, Port::One);
        assert_eq!(bits, [1, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 1, 0, 0, 0, 0]);
        assert_eq!(after, 1);
        let (bits, after) = read_by_hand(&mut ports, Port::Two);
        assert_eq!(bits, [0, 0, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(after, 1);
        assert!(ports.polled.frame);
    }

    // @source: anomie-regs#controllers/joypads
    // @source: adr-0024
    #[test]
    fn a_pad_returns_ones_until_first_latched() {
        let mut ports = Ports::power_on();
        ports.input[0] = Buttons::empty();
        assert_eq!(read_data(&mut ports, Port::One), 1);
        assert_eq!(ports.data(Port::One), 1);
    }

    // @source: fullsnes#snescontrollersioportsautomaticreading
    // @source: adr-0024
    #[test]
    fn a_latch_held_high_returns_b_every_time() {
        let mut ports = Ports::power_on();
        ports.input[0] = Buttons::B | Buttons::Y;
        ports.write_latch(1);
        assert_eq!(ports.data(Port::One), 1);
        for _ in 0..20 {
            assert_eq!(read_data(&mut ports, Port::One), 1);
        }
        ports.input[0] = Buttons::Y;
        assert_eq!(read_data(&mut ports, Port::One), 0);
        // Latch falls with Y held: the first bit is B, released.
        ports.write_latch(0);
        assert_eq!(read_data(&mut ports, Port::One), 0);
        assert_eq!(read_data(&mut ports, Port::One), 1);
    }

    // @source: anomie-regs#4218
    // @source: anomie-timing#s-cpu/auto-joypad-read
    // @source: fullsnes#snescontrollersioportsautomaticreading
    // @source: adr-0024
    #[test]
    fn the_auto_read_fills_the_registers_with_the_first_bit_in_bit_15() {
        let mut ports = Ports::power_on();
        ports.input = [Buttons::B | Buttons::R, Buttons::UP | Buttons::X];
        ports.write_auto_read_enable(0x81);
        // byetUDLR axlr0000: B and R are $8010, Up and X $0840.
        auto_read(&mut ports, Cycles(1000));
        assert_eq!(ports.joypads, [0x8010, 0x0840, 0, 0]);
        assert_eq!(
            (0..8)
                .map(|offset| ports.joypad(JoypadByte::new(offset)))
                .collect::<Vec<_>>(),
            [0x10, 0x80, 0x40, 0x08, 0, 0, 0, 0]
        );
        // The auto-read's own clocks do not poll; the pads are left returning ones.
        assert!(!ports.polled.frame);
        assert_eq!(read_data(&mut ports, Port::One), 1);
    }

    // @source: anomie-regs#4212
    // @source: anomie-timing#s-cpu/auto-joypad-read
    // @source: adr-0024
    #[test]
    fn the_auto_read_takes_a_bit_every_256_cycles_and_ends_after_4224() {
        let mut ports = Ports::power_on();
        ports.input[0] = Buttons::all();
        ports.write_auto_read_enable(0x01);
        assert_eq!(ports.auto_read_event(Cycles(0)), Some(Cycles(256)));
        assert!(ports.auto_read_busy());
        for bit in 1..16 {
            let next = ports.auto_read_event(Cycles(256 * bit));
            assert_eq!(next, Some(Cycles(256 * (bit + 1))));
        }
        // Mid-read the register holds the bits taken so far: 12 buttons and 3 ID bits.
        assert_eq!(ports.joypads[0], 0x7FF8);
        assert_eq!(ports.auto_read_event(Cycles(4096)), Some(Cycles(4224)));
        // The 16th bit is the last ID bit, 0.
        assert_eq!(ports.joypads[0], 0xFFF0);
        assert!(ports.auto_read_busy());
        assert_eq!(ports.auto_read_event(Cycles(4224)), None);
        assert!(!ports.auto_read_busy());
    }

    // @source: anomie-regs#4200
    // @source: anomie-regs#4218
    // @source: adr-0024
    #[test]
    fn with_the_enable_clear_no_read_starts() {
        let mut ports = Ports::power_on();
        ports.input[0] = Buttons::A;
        assert_eq!(ports.auto_read_event(Cycles(0)), None);
        assert!(!ports.auto_read_busy());
        assert_eq!(ports.joypads, [0; 4]);
    }

    // @source: anomie-timing#s-cpu/auto-joypad-read
    // @source: adr-0024
    #[test]
    fn a_read_by_hand_during_the_auto_read_moves_its_bits() {
        let mut ports = Ports::power_on();
        ports.input[0] = Buttons::B;
        ports.write_auto_read_enable(0x01);
        let mut next = ports.auto_read_event(Cycles(0));
        // The CPU takes the B bit before the auto-read does.
        assert_eq!(read_data(&mut ports, Port::One), 1);
        while let Some(at) = next {
            next = ports.auto_read_event(at);
        }
        // Y to the four ID bits, then one 1: B is lost and everything moves up a bit.
        assert_eq!(ports.joypads[0], 0x0001);
    }

    // @source: fullsnes#snesiomap
    // @source: adr-0024
    #[test]
    fn reset_abandons_the_read_and_keeps_the_registers() {
        let mut ports = Ports::power_on();
        ports.input[1] = Buttons::A;
        ports.write_auto_read_enable(0x01);
        auto_read(&mut ports, Cycles(0));
        ports.auto_read_event(Cycles(10_000));
        ports.write_latch(1);
        ports.reset();
        assert!(!ports.auto_read_busy());
        assert!(!ports.latch);
        assert!(!ports.auto_read_enabled);
        assert_eq!(ports.joypads, [0, 0x0080, 0, 0]);
    }

    // @source: adr-0024
    #[test]
    fn a_frame_end_keeps_the_polled_flag_as_the_last_frames() {
        let mut ports = Ports::power_on();
        ports.read(PortRegister::Joypad(JoypadByte::new(0)));
        ports.end_frame();
        assert!(ports.last_frame_polled());
        ports.end_frame();
        assert!(!ports.last_frame_polled());
        read_data(&mut ports, Port::Two);
        ports.end_frame();
        assert!(ports.last_frame_polled());
    }
}
