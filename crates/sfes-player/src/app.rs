//! The viewer's event loop: pacing, input, and drawing (PLY-01; technical design section 11).

use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Error, Result};
use sfes_api::{Fast, Stop};
use sfes_snes_core::Snes;
use winit::{
    application::ApplicationHandler,
    dpi::LogicalSize,
    event::{ElementState, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::PhysicalKey,
    window::{Window, WindowId},
};

use crate::{
    input::{self, Command, Held},
    layout::{self, Aspect},
    pacing::{self, Pacer},
    video::Video,
};

/// The window's first size: the plain picture at 3×.
const INITIAL_SIZE: LogicalSize<u32> = LogicalSize::new(256 * 3, 224 * 3);
/// How many frames pass between updates of the title.
const TITLE_INTERVAL: u64 = 30;

/// Opens a window for `machine` and runs until it is closed.
///
/// # Errors
///
/// Returns the first error the window, the graphics or the event loop reports.
pub(crate) fn run(machine: Snes<Fast>, name: String, aspect: Aspect) -> Result<()> {
    let event_loop = EventLoop::new()?;
    let period = pacing::frame_period(machine.video_standard());
    let mut app = App {
        machine,
        name,
        aspect,
        held: Held::default(),
        pacer: Pacer::new(period),
        last_tick: pacing::now(),
        paused: false,
        frames: 0,
        shape: None,
        window: None,
        video: None,
        failure: None,
    };
    event_loop.run_app(&mut app)?;
    app.failure.map_or(Ok(()), Err)
}

/// The shape of the last frame, which decides where it is drawn.
#[derive(Clone, Copy, Debug)]
struct Shape {
    height: u32,
    interlaced: bool,
}

/// The viewer's state.
struct App {
    machine: Snes<Fast>,
    name: String,
    aspect: Aspect,
    held: Held,
    pacer: Pacer,
    last_tick: Instant,
    paused: bool,
    frames: u64,
    shape: Option<Shape>,
    window: Option<Arc<Window>>,
    video: Option<Video>,
    /// The error that ended the loop, handed back by [`run`].
    failure: Option<Error>,
}

impl App {
    /// Runs `count` frames with the held buttons, then uploads the last one.
    fn run_frames(&mut self, count: u32) {
        if count == 0 {
            return;
        }
        self.machine.set_input(0, self.held.buttons());
        for _ in 0..count {
            self.machine.run(Stop::Frames(1));
            self.frames += 1;
        }
        let frame = self.machine.video();
        if let Some(video) = &mut self.video {
            video.upload(&frame);
        }
        if frame.width > 0 {
            self.shape = Some(Shape {
                height: u32::from(frame.height),
                interlaced: frame.field.is_some(),
            });
        }
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    /// Puts the frame count and state in the window's title.
    fn update_title(&self) {
        if let Some(window) = &self.window {
            let state = if self.paused { " [paused]" } else { "" };
            window.set_title(&format!(
                "sfes: {}: frame {}{state}",
                self.name, self.frames
            ));
        }
    }

    /// Carries out a control.
    fn command(&mut self, command: Command, event_loop: &ActiveEventLoop) {
        match command {
            Command::TogglePause => {
                self.paused = !self.paused;
                self.pacer.clear();
                self.last_tick = pacing::now();
                self.update_title();
            }
            Command::Advance => {
                if self.paused {
                    self.run_frames(1);
                    self.update_title();
                }
            }
            Command::Reset => {
                self.machine.reset();
                self.pacer.clear();
            }
            Command::Quit => event_loop.exit(),
        }
    }

    /// Ends the loop with `error`.
    fn fail(&mut self, error: Error, event_loop: &ActiveEventLoop) {
        self.failure = Some(error);
        event_loop.exit();
    }

    /// Draws the last frame into the window.
    fn redraw(&mut self, event_loop: &ActiveEventLoop) {
        let (Some(window), Some(video)) = (&self.window, &mut self.video) else {
            return;
        };
        let size = window.inner_size();
        let rect = self.shape.map_or(
            layout::Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            |shape| {
                layout::layout(
                    shape.height,
                    shape.interlaced,
                    size.width,
                    size.height,
                    self.aspect,
                )
            },
        );
        if let Err(error) = video.draw(rect) {
            self.fail(error, event_loop);
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attributes = Window::default_attributes()
            .with_title(format!("sfes: {}", self.name))
            .with_inner_size(INITIAL_SIZE);
        let window = match event_loop.create_window(attributes) {
            Ok(window) => Arc::new(window),
            Err(error) => return self.fail(Error::from(error), event_loop),
        };
        match Video::new(Arc::clone(&window)) {
            Ok(video) => self.video = Some(video),
            Err(error) => return self.fail(error, event_loop),
        }
        self.window = Some(window);
        self.last_tick = pacing::now();
        self.pacer.clear();
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(video) = &mut self.video {
                    video.resize(size.width, size.height);
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::Focused(false) => self.held.release_all(),
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(key) = event.physical_key else {
                    return;
                };
                let pressed = event.state == ElementState::Pressed;
                self.held.set(key, pressed);
                if pressed
                    && !event.repeat
                    && let Some(command) = input::command(key)
                {
                    self.command(command, event_loop);
                }
            }
            WindowEvent::RedrawRequested => self.redraw(event_loop),
            _ => {}
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            return;
        }
        if self.paused {
            event_loop.set_control_flow(ControlFlow::Wait);
            return;
        }
        let now = pacing::now();
        let due = self.pacer.advance(now.duration_since(self.last_tick));
        self.last_tick = now;
        let before = self.frames / TITLE_INTERVAL;
        self.run_frames(due);
        if self.frames / TITLE_INTERVAL != before {
            self.update_title();
        }
        let wait: Duration = self.pacer.until_next();
        event_loop.set_control_flow(ControlFlow::WaitUntil(now + wait));
    }
}
