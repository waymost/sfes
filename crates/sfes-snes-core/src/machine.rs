// @source: adr-0018
// @source: adr-0004
// @source: adr-0011
//! The `Snes` machine: its state, its builder and the inherent API shaped like `System`
//! (FND-01, FND-06, FND-08; technical design section 3).

use alloc::{sync::Arc, vec::Vec};
use core::{fmt, marker::PhantomData};

use sfes_api::{
    Accuracy, BusId, Buttons, Cycles, Fast, InstrEvent, Interrupt, NoopObserver, Observer, Peek,
    RegionId, Stop, StopReason, VideoFrame,
};
use sfes_snes_cart::{CartError, Cartridge, Mapping, Options};
use sfes_snes_cpu::{Cpu65816, Flags, Pending, RunState};

use crate::{
    Region,
    bus::{Bus, BusView},
    timing::{Boundary, Timing},
};

/// Why a machine could not be built. A failed build never yields a machine.
#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
pub enum BuildError {
    /// The ROM image could not be loaded as a cartridge.
    #[error(transparent)]
    Cartridge(#[from] CartError),
}

/// Machine state that belongs to no chip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Meta {
    /// The seed the machine was last powered on with.
    seed: u64,
    /// The video standard the machine runs.
    region: Region,
    /// Frames completed since power-on.
    frame: u64,
}

/// Everything that is machine state: what `Clone` copies and, from M4, what save states hold.
#[derive(Clone, Debug)]
struct State {
    cpu: Cpu65816,
    bus: Bus,
    meta: Meta,
}

/// What the machine produces that is not state: the frame buffer. A clone starts empty.
#[derive(Debug, Default)]
struct Outputs {
    frame: Vec<u16>,
    width: u16,
    height: u16,
}

/// What a step is about to do, worked out from ADR 0004's cases before it runs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StepKind {
    /// One instruction, or one byte of a block move.
    Instruction,
    /// The hardware interrupt sequence.
    Interrupt(Interrupt),
    /// An idle cycle of a waiting or stopped processor.
    Halted,
}

/// One SNES: an owned value that is `Clone + Send` (FND-01), generic over its accuracy
/// profile and its observer (FND-05).
pub struct Snes<A: Accuracy = Fast, O: Observer = NoopObserver> {
    state: State,
    observer: O,
    outputs: Outputs,
    profile: PhantomData<A>,
}

impl<A: Accuracy, O: Observer> Clone for Snes<A, O> {
    /// Copies the state and the observer. The copy's outputs start empty, so snapshots that
    /// never render never pay for frame buffers (technical design section 5).
    fn clone(&self) -> Self {
        Self {
            state: self.state.clone(),
            observer: self.observer.clone(),
            outputs: Outputs::default(),
            profile: PhantomData,
        }
    }
}

impl<A: Accuracy, O: Observer> fmt::Debug for Snes<A, O> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Snes")
            .field("accuracy", &A::NAME)
            .field("state", &self.state)
            .finish_non_exhaustive()
    }
}

impl<A: Accuracy> Snes<A, NoopObserver> {
    /// A builder for a machine running `rom`, a ROM image with or without a copier header.
    // @source: adr-0016
    #[must_use]
    pub fn builder(rom: Arc<[u8]>) -> SnesBuilder<A, NoopObserver> {
        SnesBuilder {
            rom,
            options: Options::default(),
            seed: 0,
            observer: NoopObserver,
            profile: PhantomData,
        }
    }
}

/// Builds a [`Snes`]: the ROM, then optional region, board, seed and observer.
#[derive(Clone, Debug)]
pub struct SnesBuilder<A: Accuracy, O: Observer> {
    rom: Arc<[u8]>,
    options: Options,
    seed: u64,
    observer: O,
    profile: PhantomData<A>,
}

impl<A: Accuracy, O: Observer> SnesBuilder<A, O> {
    /// Runs this video standard whatever the header says (EMU-09).
    // @source: adr-0016
    #[must_use]
    pub const fn region(mut self, region: Region) -> Self {
        self.options.region = Some(region);
        self
    }

    /// Maps the ROM as this board, skipping header detection's scoring (ADR 0016).
    // @source: adr-0016
    #[must_use]
    pub const fn board(mut self, mapping: Mapping) -> Self {
        self.options.mapping = Some(mapping);
        self
    }

    /// Fills undefined power-on state from `seed`; 0 by default (FND-02).
    // @source: adr-0018
    #[must_use]
    pub const fn seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Reports to `observer`, which sees the power-on reset too.
    // @source: adr-0018
    #[must_use]
    pub fn observer<P: Observer>(self, observer: P) -> SnesBuilder<A, P> {
        let Self {
            rom,
            options,
            seed,
            observer: _,
            profile,
        } = self;
        SnesBuilder {
            rom,
            options,
            seed,
            observer,
            profile,
        }
    }

    /// Loads the cartridge and powers the machine on.
    ///
    /// # Errors
    ///
    /// [`BuildError::Cartridge`] when the ROM image cannot be loaded (ADR 0016, ADR 0017).
    // @source: adr-0016
    // @source: adr-0017
    // @source: adr-0018
    pub fn build(self) -> Result<Snes<A, O>, BuildError> {
        let cartridge = Cartridge::load(&self.rom, self.options)?;
        let region = cartridge.header().region.unwrap_or(Region::Ntsc);
        let meta = Meta {
            seed: self.seed,
            region,
            frame: 0,
        };
        let bus = Bus::power_on(cartridge, Timing::new(region, Cycles(0)), self.seed);
        let mut snes = Snes {
            state: State {
                cpu: Cpu65816::new(),
                bus,
                meta,
            },
            observer: self.observer,
            outputs: Outputs::default(),
            profile: PhantomData,
        };
        snes.reset_cpu();
        Ok(snes)
    }
}

impl<A: Accuracy, O: Observer> Snes<A, O> {
    /// Presses the reset button: the timing position returns to the start of a frame, the
    /// bus registers take their reset values and the CPU runs its reset sequence at once.
    /// WRAM and cartridge RAM are kept (ADR 0018, ADR 0019).
    // @source: fullsnes#snescontrollersotherinputs
    // @source: fullsnes#snesiomap
    // @source: anomie-timing
    // @source: adr-0018
    // @source: adr-0019
    pub fn reset(&mut self) {
        let now = self.state.bus.now;
        self.state.bus.timing.restart(now);
        self.state.bus.reset();
        self.reset_cpu();
    }

    /// Powers off and on: WRAM refilled from `seed`, the clock and frame count back to 0,
    /// then the reset sequence. Cartridge RAM is kept, as a battery would (ADR 0018).
    // @source: adr-0018
    pub fn power_cycle(&mut self, seed: u64) {
        let region = self.state.meta.region;
        let cartridge = self.state.bus.cartridge.clone();
        let input = self.state.bus.input;
        self.state.bus = Bus::power_on(cartridge, Timing::new(region, Cycles(0)), seed);
        self.state.bus.input = input;
        self.state.cpu = Cpu65816::new();
        self.state.meta = Meta {
            seed,
            region,
            frame: 0,
        };
        self.reset_cpu();
    }

    /// Runs the CPU's reset sequence on the bus: RESB rises now (ADR 0018, point 1).
    // @source: adr-0011
    // @source: adr-0018
    fn reset_cpu(&mut self) {
        let State { cpu, bus, .. } = &mut self.state;
        let mut view = BusView::new(bus, &mut self.observer);
        cpu.reset(&mut view);
        self.advance_timing();
    }

    /// Sets the buttons held on `port`, 0 or 1. Other ports are ignored.
    // @source: adr-0018
    pub fn set_input(&mut self, port: usize, input: Buttons) {
        if let Some(held) = self.state.bus.input.get_mut(port) {
            *held = input;
        }
    }

    /// Runs until `until` is reached, at the first step boundary at or after it (FND-08).
    /// Until the scheduler and watches arrive, frames and lines have fixed lengths and
    /// `Stop::Watch` always runs to its frame limit (ADR 0018, point 6).
    // @source: adr-0018
    pub fn run(&mut self, until: Stop) -> StopReason {
        match until {
            Stop::Instructions(count) => {
                for _ in 0..count {
                    self.step();
                }
                StopReason::Completed
            }
            Stop::Cycles(count) => {
                let target = self.state.bus.now.0.saturating_add(count);
                while self.state.bus.now.0 < target {
                    self.step();
                }
                StopReason::Completed
            }
            Stop::Scanlines(count) => {
                let target = self.state.bus.timing.lines + u64::from(count);
                while self.state.bus.timing.lines < target {
                    self.step();
                }
                StopReason::Completed
            }
            Stop::Frames(count) => {
                self.run_frames(count);
                StopReason::Completed
            }
            Stop::Watch { max_frames } => {
                self.run_frames(max_frames);
                StopReason::FrameLimit
            }
        }
    }

    /// Runs `count` frame boundaries.
    // @source: adr-0018
    fn run_frames(&mut self, count: u32) {
        let target = self.state.meta.frame + u64::from(count);
        while self.state.meta.frame < target {
            self.step();
        }
    }

    /// One CPU step with its observer events (ADR 0018, point 5), then any line and frame
    /// boundaries it crossed.
    // @source: adr-0004
    // @source: adr-0018
    fn step(&mut self) {
        let State { cpu, bus, .. } = &mut self.state;
        let at = bus.now;
        let kind = step_kind(cpu, bus.pending());
        let program = (u32::from(cpu.pbr) << 16) | u32::from(cpu.pc);
        let mut view = BusView::new(bus, &mut self.observer);
        cpu.step(&mut view);
        let opcode = view.first_read();
        if O::ACTIVE {
            match (kind, opcode) {
                (StepKind::Instruction, Some(opcode)) => {
                    let event = InstrEvent {
                        bus: BusId::Cpu,
                        pc: program,
                        opcode,
                        at,
                    };
                    self.observer.on_instruction(&event);
                }
                (StepKind::Interrupt(interrupt), _) => self.observer.on_interrupt(interrupt, at),
                (StepKind::Instruction | StepKind::Halted, _) => {}
            }
        }
        self.advance_timing();
    }

    /// Reports and counts every line and frame boundary the clock has passed.
    // @source: adr-0018
    fn advance_timing(&mut self) {
        let now = self.state.bus.now;
        while let Some((boundary, at)) = self.state.bus.timing.next_due(now) {
            match boundary {
                Boundary::Line(line) => {
                    if O::ACTIVE {
                        self.observer.on_scanline(line, at);
                    }
                }
                Boundary::Frame => {
                    self.state.meta.frame += 1;
                    if O::ACTIVE {
                        self.observer.on_frame_end(self.state.meta.frame);
                        self.observer.on_scanline(0, at);
                    }
                }
            }
        }
    }

    /// Reads a memory region: WRAM, the ROM and cartridge RAM. The PPU's and APU's regions
    /// are empty until those chips exist (FND-06).
    // @source: adr-0017
    // @source: adr-0018
    #[must_use]
    pub fn region(&self, id: RegionId) -> &[u8] {
        match id {
            RegionId::Wram => &self.state.bus.wram,
            RegionId::Rom | RegionId::CartRam => self.state.bus.cartridge.region(id),
            RegionId::Vram | RegionId::Cgram | RegionId::Oam | RegionId::Aram => &[],
        }
    }

    /// Writes a memory region between steps: WRAM and cartridge RAM (FND-06).
    // @source: adr-0017
    // @source: adr-0018
    pub fn region_mut(&mut self, id: RegionId) -> &mut [u8] {
        match id {
            RegionId::Wram => &mut self.state.bus.wram,
            RegionId::CartRam | RegionId::Rom => self.state.bus.cartridge.region_mut(id),
            RegionId::Vram | RegionId::Cgram | RegionId::Oam | RegionId::Aram => &mut [],
        }
    }

    /// Reads the CPU bus without side effects or time (FND-06). An address whose read has
    /// side effects, such as `$2180 WMDATA`, is reported as latched, with the value a read
    /// would return now.
    // @source: anomie-memmap#memory-map
    // @source: anomie-memmap#hardware/data-bus
    // @source: adr-0019
    #[must_use]
    pub fn peek_bus(&self, address: u32) -> Peek {
        let (value, latched) = self.state.bus.peek(address & 0xFF_FFFF);
        Peek { value, latched }
    }

    /// The most recently rendered frame; 0 × 0 until the PPU renders one.
    // @source: adr-0018
    #[must_use]
    pub fn video(&self) -> VideoFrame<'_> {
        VideoFrame {
            width: self.outputs.width,
            height: self.outputs.height,
            pixels: &self.outputs.frame,
        }
    }

    /// The video standard the machine runs.
    // @source: adr-0018
    #[must_use]
    pub const fn video_standard(&self) -> Region {
        self.state.meta.region
    }

    /// The seed of the last power-on.
    // @source: adr-0018
    #[must_use]
    pub const fn seed(&self) -> u64 {
        self.state.meta.seed
    }

    /// Frames completed since power-on.
    // @source: adr-0018
    #[must_use]
    pub const fn frame(&self) -> u64 {
        self.state.meta.frame
    }

    /// Master cycles since power-on.
    // @source: adr-0018
    #[must_use]
    pub const fn cycles(&self) -> Cycles {
        self.state.bus.now
    }

    /// The CPU's registers and run state.
    // @source: adr-0018
    #[must_use]
    pub const fn cpu(&self) -> &Cpu65816 {
        &self.state.cpu
    }

    /// The cartridge.
    // @source: adr-0018
    #[must_use]
    pub const fn cartridge(&self) -> &Cartridge {
        &self.state.bus.cartridge
    }

    /// The observer.
    // @source: adr-0018
    #[must_use]
    pub const fn observer(&self) -> &O {
        &self.observer
    }

    /// The observer, writable between steps.
    // @source: adr-0018
    pub const fn observer_mut(&mut self) -> &mut O {
        &mut self.observer
    }
}

/// Which of ADR 0004's cases the next step takes: stopped steps idle; an NMI, or an IRQ with
/// I clear, enters its handler, even from a wait; a wait with nothing pending idles; anything
/// else, including a wait ended by an IRQ with I set, runs an instruction.
// @source: adr-0004
// @source: adr-0018
fn step_kind(cpu: &Cpu65816, pending: Pending) -> StepKind {
    let Pending { nmi, irq } = pending;
    if cpu.run == RunState::Stopped {
        StepKind::Halted
    } else if nmi {
        StepKind::Interrupt(Interrupt::Nmi)
    } else if irq && !cpu.p.contains(Flags::I) {
        StepKind::Interrupt(Interrupt::Irq)
    } else if cpu.run == RunState::Waiting && !irq {
        StepKind::Halted
    } else {
        StepKind::Instruction
    }
}

#[cfg(test)]
mod tests {
    use alloc::{vec, vec::Vec};

    use sfes_api::SplitMix64;

    use super::*;

    /// A 32 KiB LoROM image: `code` at `$00:8000`, the RESET vector pointing there, and the
    /// header's country byte `country`.
    // @source: fullsnes#snescartridgeromheader
    // @source: adr-0017
    fn rom(code: &[u8], country: u8) -> Arc<[u8]> {
        let mut image = vec![0; 0x8000];
        image[..code.len()].copy_from_slice(code);
        image[0x7FC0..0x7FD5].fill(b' ');
        image[0x7FD5] = 0x20;
        image[0x7FD7] = 0x08;
        image[0x7FD9] = country;
        image[0x7FFC..0x7FFE].copy_from_slice(&[0x00, 0x80]);
        Arc::from(image)
    }

    /// LDA #$42; STA $0010; STP.
    const PROGRAM: [u8; 6] = [0xA9, 0x42, 0x8D, 0x10, 0x00, 0xDB];

    // @source: adr-0018
    fn machine(code: &[u8]) -> Snes {
        Snes::<Fast>::builder(rom(code, 0x01))
            .build()
            .expect("builds")
    }

    /// What a recording observer saw.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum Event {
        Instruction(InstrEvent),
        Interrupt(Interrupt, Cycles),
        Read(u32, u8),
        Write(u32, u8),
        FrameEnd(u64),
    }

    #[derive(Clone, Debug, Default)]
    struct Recorder(Vec<Event>);

    impl Observer for Recorder {
        // @source: adr-0018
        fn on_instruction(&mut self, event: &InstrEvent) {
            self.0.push(Event::Instruction(*event));
        }
        // @source: adr-0018
        fn on_read(&mut self, _bus: BusId, address: u32, value: u8, _at: Cycles) {
            self.0.push(Event::Read(address, value));
        }
        // @source: adr-0018
        fn on_write(&mut self, _bus: BusId, address: u32, value: u8, _at: Cycles) {
            self.0.push(Event::Write(address, value));
        }
        // @source: adr-0018
        fn on_interrupt(&mut self, kind: Interrupt, at: Cycles) {
            self.0.push(Event::Interrupt(kind, at));
        }
        // @source: adr-0018
        fn on_frame_end(&mut self, frame: u64) {
            self.0.push(Event::FrameEnd(frame));
        }
    }

    // @source: adr-0018
    fn recorded(code: &[u8]) -> Snes<Fast, Recorder> {
        Snes::<Fast>::builder(rom(code, 0x01))
            .observer(Recorder::default())
            .build()
            .expect("builds")
    }

    // @source: adr-0018
    #[test]
    fn the_machine_is_clone_and_send() {
        // @source: adr-0018
        const fn assert_clone_send<T: Clone + Send>() {}
        assert_clone_send::<Snes>();
        assert_clone_send::<Snes<Fast, Recorder>>();
    }

    // @source: adr-0016
    #[test]
    fn an_invalid_rom_builds_no_machine() {
        let result = Snes::<Fast>::builder(Arc::from(vec![0; 100])).build();
        assert!(matches!(result, Err(BuildError::Cartridge(_))));
    }

    // @source: adr-0011
    // @source: adr-0018
    // @source: anomie-memmap#memory-map
    #[test]
    fn power_on_runs_the_reset_sequence_from_time_0() {
        let snes = machine(&PROGRAM);
        assert_eq!(snes.cpu().pc, 0x8000);
        assert_eq!(snes.cpu().pbr, 0);
        // A WRAM read at $00:0000, an internal cycle, three page-1 WRAM reads and two ROM
        // vector reads: 8 + 6 + 3 × 8 + 2 × 8.
        assert_eq!(snes.cycles(), Cycles(54));
        assert_eq!(snes.frame(), 0);
    }

    // @source: vigna-splitmix
    // @source: adr-0018
    #[test]
    fn power_on_fills_wram_from_the_seed() {
        let mut expected = vec![0; 0x2_0000];
        SplitMix64::new(0).fill(&mut expected);
        let snes = machine(&PROGRAM);
        assert_eq!(snes.region(RegionId::Wram), expected.as_slice());
        let seeded = Snes::<Fast>::builder(rom(&PROGRAM, 0x01))
            .seed(7)
            .build()
            .expect("builds");
        assert_eq!(seeded.seed(), 7);
        SplitMix64::new(7).fill(&mut expected);
        assert_eq!(seeded.region(RegionId::Wram), expected.as_slice());
    }

    // @source: adr-0011
    // @source: adr-0018
    #[test]
    fn the_reset_reads_page_1_and_writes_nothing() {
        let snes = recorded(&PROGRAM);
        let events = &snes.observer().0;
        let wram = snes.region(RegionId::Wram);
        let stack = events
            .iter()
            .filter_map(|event| match event {
                Event::Read(address, value) if (0x0100..0x0200).contains(address) => {
                    Some((*address, *value))
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(stack.len(), 3);
        for (address, value) in stack {
            assert_eq!(value, wram[address as usize]);
        }
        assert!(!events.iter().any(|event| matches!(event, Event::Write(..))));
    }

    // @source: adr-0004
    // @source: adr-0018
    #[test]
    fn each_instruction_step_is_reported_once() {
        let mut snes = recorded(&PROGRAM);
        snes.observer_mut().0.clear();
        snes.run(Stop::Instructions(5));
        let instructions = snes
            .observer()
            .0
            .iter()
            .filter_map(|event| match event {
                Event::Instruction(event) => Some((event.pc, event.opcode, event.at)),
                _ => None,
            })
            .collect::<Vec<_>>();
        // LDA # (two Slow ROM reads), STA abs (three ROM reads and a WRAM write), STP; the
        // two steps after STP idle and report nothing.
        assert_eq!(
            instructions,
            [
                (0x00_8000, 0xA9, Cycles(54)),
                (0x00_8002, 0x8D, Cycles(70)),
                (0x00_8005, 0xDB, Cycles(102)),
            ]
        );
        assert!(snes.observer().0.contains(&Event::Write(0x00_0010, 0x42)));
        assert_eq!(snes.region(RegionId::Wram)[0x10], 0x42);
    }

    // @source: adr-0004
    // @source: adr-0018
    #[test]
    fn an_nmi_is_reported_as_an_interrupt_and_clears_the_latch() {
        let mut snes = recorded(&[0xEA, 0xEA]);
        snes.observer_mut().0.clear();
        snes.state.bus.nmi_latched = true;
        let at = snes.cycles();
        snes.run(Stop::Instructions(1));
        assert_eq!(
            snes.observer().0.first(),
            Some(&Event::Read(0x00_8000, 0xEA))
        );
        assert!(
            snes.observer()
                .0
                .contains(&Event::Interrupt(Interrupt::Nmi, at))
        );
        assert!(
            !snes
                .observer()
                .0
                .iter()
                .any(|event| matches!(event, Event::Instruction(_)))
        );
        assert!(!snes.state.bus.nmi_latched);
    }

    // @source: adr-0004
    #[test]
    fn steps_are_classified_as_adr_0004_orders_them() {
        let mut cpu = Cpu65816::new();
        let none = Pending::default();
        let nmi = Pending {
            nmi: true,
            irq: false,
        };
        let irq = Pending {
            nmi: false,
            irq: true,
        };
        cpu.set_status(Flags::empty());
        assert_eq!(step_kind(&cpu, none), StepKind::Instruction);
        assert_eq!(step_kind(&cpu, nmi), StepKind::Interrupt(Interrupt::Nmi));
        assert_eq!(step_kind(&cpu, irq), StepKind::Interrupt(Interrupt::Irq));
        cpu.set_status(Flags::I);
        assert_eq!(step_kind(&cpu, irq), StepKind::Instruction);
        assert_eq!(step_kind(&cpu, nmi), StepKind::Interrupt(Interrupt::Nmi));
        cpu.run = RunState::Waiting;
        assert_eq!(step_kind(&cpu, none), StepKind::Halted);
        assert_eq!(step_kind(&cpu, irq), StepKind::Instruction);
        cpu.set_status(Flags::empty());
        assert_eq!(step_kind(&cpu, irq), StepKind::Interrupt(Interrupt::Irq));
        cpu.run = RunState::Stopped;
        assert_eq!(step_kind(&cpu, nmi), StepKind::Halted);
    }

    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: fullsnes#snestimingoscillators
    // @source: adr-0018
    #[test]
    fn stops_end_at_the_first_step_at_or_after_their_target() {
        let mut snes = recorded(&PROGRAM);
        snes.run(Stop::Cycles(1000));
        assert!(snes.cycles() >= Cycles(1054) && snes.cycles() < Cycles(1054 + 12));
        snes.run(Stop::Frames(1));
        assert_eq!(snes.frame(), 1);
        assert!(snes.cycles() >= Cycles(357_368) && snes.cycles() < Cycles(357_368 + 12));
        assert!(snes.observer().0.contains(&Event::FrameEnd(1)));
        snes.run(Stop::Scanlines(2));
        assert!(snes.cycles() >= Cycles(357_368 + 2 * 1364));
        assert_eq!(
            snes.run(Stop::Watch { max_frames: 1 }),
            StopReason::FrameLimit
        );
        assert_eq!(snes.frame(), 2);
    }

    // @source: fullsnes#snescartridgeromheader
    // @source: fullsnes#snestimingoscillators
    // @source: adr-0018
    #[test]
    fn the_region_comes_from_the_header_the_builder_or_ntsc() {
        let pal = Snes::<Fast>::builder(rom(&PROGRAM, 0x02))
            .build()
            .expect("builds");
        assert_eq!(pal.video_standard(), Region::Pal);
        let unknown = Snes::<Fast>::builder(rom(&PROGRAM, 0x0E))
            .build()
            .expect("builds");
        assert_eq!(unknown.video_standard(), Region::Ntsc);
        let forced = Snes::<Fast>::builder(rom(&PROGRAM, 0x01))
            .region(Region::Pal)
            .build()
            .expect("builds");
        assert_eq!(forced.video_standard(), Region::Pal);
        let mut pal = pal;
        pal.run(Stop::Frames(1));
        assert!(pal.cycles() >= Cycles(425_568));
    }

    // @source: fullsnes#snescontrollersotherinputs
    // @source: anomie-timing
    // @source: adr-0018
    #[test]
    fn reset_keeps_wram_and_power_cycle_refills_it() {
        let mut snes = machine(&PROGRAM);
        snes.run(Stop::Instructions(3));
        snes.reset();
        assert_eq!(snes.region(RegionId::Wram)[0x10], 0x42);
        assert_eq!(snes.cpu().pc, 0x8000);
        assert_eq!(snes.state.bus.timing.line, 0);
        snes.power_cycle(0);
        let mut expected = vec![0; 0x2_0000];
        SplitMix64::new(0).fill(&mut expected);
        assert_eq!(snes.region(RegionId::Wram), expected.as_slice());
        assert_eq!(snes.cycles(), Cycles(54));
    }

    // @source: anomie-memmap#memory-map
    // @source: anomie-memmap#hardware/data-bus
    // @source: adr-0017
    #[test]
    fn regions_and_peek_see_the_same_memory() {
        let mut snes = machine(&PROGRAM);
        snes.region_mut(RegionId::Wram)[0x1234] = 0x77;
        assert_eq!(snes.peek_bus(0x7E_1234).value, 0x77);
        assert_eq!(snes.peek_bus(0x80_1234).value, 0x77);
        assert_eq!(snes.peek_bus(0x00_8000).value, 0xA9);
        assert_eq!(snes.region(RegionId::Rom).len(), 0x8000);
        assert_eq!(snes.region_mut(RegionId::Rom).len(), 0);
        assert_eq!(snes.region(RegionId::Vram).len(), 0);
        // Unanswered I/O addresses show the last value on the data bus: the vector's high
        // byte.
        assert_eq!(snes.peek_bus(0x00_2000).value, 0x80);
        assert!(!snes.peek_bus(0x00_2000).latched);
        // WMDATA's read advances its address, so a peek is latched.
        snes.region_mut(RegionId::Wram)[0] = 0x31;
        assert_eq!(
            snes.peek_bus(0x00_2180),
            Peek {
                value: 0x31,
                latched: true
            }
        );
    }

    // @source: anomie-regs#2180
    // @source: wdc-w65c816s#table-5-7-1d
    // @source: adr-0006
    // @source: adr-0019
    #[test]
    fn an_emulation_mode_inc_of_wmdata_writes_twice() {
        // LDA #$40; STA $0000; STZ $2181; STZ $2182; STZ $2183; INC $2180; STP. The CPU is
        // in emulation mode after reset, so INC writes the value read, then the result.
        let code = [
            0xA9, 0x40, 0x8D, 0x00, 0x00, 0x9C, 0x81, 0x21, 0x9C, 0x82, 0x21, 0x9C, 0x83, 0x21,
            0xEE, 0x80, 0x21, 0xDB,
        ];
        let mut snes = recorded(&code);
        snes.run(Stop::Instructions(7));
        let wram = snes.region(RegionId::Wram);
        assert_eq!(wram[..3], [0x40, 0x40, 0x41]);
        assert_eq!(snes.state.bus.wram_port_address, 3);
        let port = snes
            .observer()
            .0
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    Event::Read(0x00_2180, _) | Event::Write(0x00_2180, _)
                )
            })
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(
            port,
            [
                Event::Read(0x00_2180, 0x40),
                Event::Write(0x00_2180, 0x40),
                Event::Write(0x00_2180, 0x41),
            ]
        );
    }

    // @source: anomie-timing#s-cpu/clocks-and-refresh
    // @source: anomie-timing#s-cpu/instructions
    // @source: adr-0019
    #[test]
    fn a_refresh_shows_as_a_40_cycle_gap_between_accesses() {
        // NOPs from $00:8000: each is an 8-cycle ROM opcode fetch and a 6-cycle internal
        // cycle (wdc-w65c816s#table-5-7-19a), so fetches start at 54 + 14n.
        let mut snes = recorded(&[0xEA; 64]);
        snes.observer_mut().0.clear();
        snes.run(Stop::Instructions(40));
        let fetches = snes
            .observer()
            .0
            .iter()
            .filter_map(|event| match event {
                Event::Instruction(event) => Some(event.at.0),
                _ => None,
            })
            .collect::<Vec<_>>();
        // The fetch at 530 ends at 538, when the refresh is due; the internal cycle after it
        // starts 40 cycles later.
        let gaps = fetches
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .collect::<Vec<_>>();
        let paused = fetches
            .iter()
            .position(|&at| at == 530)
            .expect("a fetch at 530");
        assert_eq!(gaps[paused], 14 + 40);
        assert!(
            gaps.iter()
                .enumerate()
                .all(|(index, &gap)| index == paused || gap == 14)
        );
    }

    // @source: anomie-regs#420d
    // @source: fullsnes#snesmemorycontrol
    // @source: adr-0019
    #[test]
    fn reset_clears_memsel_and_the_wram_port_address() {
        // LDA #$01; STA $420D; STA $2183; STP.
        let mut snes = machine(&[0xA9, 0x01, 0x8D, 0x0D, 0x42, 0x8D, 0x83, 0x21, 0xDB]);
        snes.run(Stop::Instructions(4));
        assert!(snes.state.bus.fast_rom);
        assert_eq!(snes.state.bus.wram_port_address, 0x1_0000);
        snes.reset();
        assert!(!snes.state.bus.fast_rom);
        assert_eq!(snes.state.bus.wram_port_address, 0);
    }

    // @source: adr-0018
    #[test]
    fn a_clone_runs_the_same_and_starts_without_outputs() {
        let mut snes = machine(&PROGRAM);
        let mut copy = snes.clone();
        snes.run(Stop::Frames(1));
        copy.run(Stop::Frames(1));
        assert_eq!(copy.cycles(), snes.cycles());
        assert_eq!(copy.region(RegionId::Wram), snes.region(RegionId::Wram));
        assert_eq!(copy.video().pixels.len(), 0);
        snes.set_input(1, Buttons::START);
        snes.set_input(9, Buttons::A);
        assert_eq!(snes.state.bus.input, [Buttons::empty(), Buttons::START]);
    }
}
