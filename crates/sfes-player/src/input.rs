//! The viewer's fixed keyboard map and its controls (PLY-01). Configurable maps and gamepads
//! are PLY-03.

use sfes_api::Buttons;
use winit::keyboard::KeyCode;

/// A viewer control, apart from the pad.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Command {
    /// Pauses, or resumes.
    TogglePause,
    /// Runs one frame while paused.
    Advance,
    /// Presses the console's reset button.
    Reset,
    /// Closes the window.
    Quit,
}

/// The pad button a key stands for.
///
/// Arrows are the D-pad; `Z` B, `X` A, `A` Y, `S` X, `Q` L, `W` R, `Enter` Start, and
/// `Backspace` or right `Shift` Select.
pub(crate) const fn button(key: KeyCode) -> Option<Buttons> {
    Some(match key {
        KeyCode::ArrowUp => Buttons::UP,
        KeyCode::ArrowDown => Buttons::DOWN,
        KeyCode::ArrowLeft => Buttons::LEFT,
        KeyCode::ArrowRight => Buttons::RIGHT,
        KeyCode::KeyZ => Buttons::B,
        KeyCode::KeyX => Buttons::A,
        KeyCode::KeyA => Buttons::Y,
        KeyCode::KeyS => Buttons::X,
        KeyCode::KeyQ => Buttons::L,
        KeyCode::KeyW => Buttons::R,
        KeyCode::Enter => Buttons::START,
        KeyCode::Backspace | KeyCode::ShiftRight => Buttons::SELECT,
        _ => return None,
    })
}

/// The control a key press stands for: `Space` or `P` pause, `N` or `.` advance a frame, `R`
/// reset, `Escape` quit.
pub(crate) const fn command(key: KeyCode) -> Option<Command> {
    Some(match key {
        KeyCode::Space | KeyCode::KeyP => Command::TogglePause,
        KeyCode::KeyN | KeyCode::Period => Command::Advance,
        KeyCode::KeyR => Command::Reset,
        KeyCode::Escape => Command::Quit,
        _ => return None,
    })
}

/// The pad buttons currently held.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Held {
    buttons: Buttons,
}

impl Held {
    /// Records `key` going down or up. A key that is not a pad button changes nothing.
    pub(crate) fn set(&mut self, key: KeyCode, pressed: bool) {
        if let Some(pressed_button) = button(key) {
            self.buttons.set(pressed_button, pressed);
        }
    }

    /// The buttons held now.
    pub(crate) const fn buttons(self) -> Buttons {
        self.buttons
    }

    /// Releases everything, as when the window loses focus.
    pub(crate) fn release_all(&mut self) {
        self.buttons = Buttons::empty();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_map_to_pad_buttons_and_controls_apart() {
        assert_eq!(button(KeyCode::KeyZ), Some(Buttons::B));
        assert_eq!(button(KeyCode::Enter), Some(Buttons::START));
        assert_eq!(button(KeyCode::KeyP), None);
        assert_eq!(command(KeyCode::KeyP), Some(Command::TogglePause));
        assert_eq!(command(KeyCode::KeyZ), None);
    }

    #[test]
    fn no_key_is_both_a_button_and_a_command() {
        for key in [
            KeyCode::ArrowUp,
            KeyCode::KeyZ,
            KeyCode::KeyX,
            KeyCode::KeyA,
            KeyCode::KeyS,
            KeyCode::KeyQ,
            KeyCode::KeyW,
            KeyCode::Enter,
            KeyCode::Backspace,
            KeyCode::ShiftRight,
            KeyCode::Space,
            KeyCode::KeyP,
            KeyCode::KeyN,
            KeyCode::Period,
            KeyCode::KeyR,
            KeyCode::Escape,
        ] {
            assert!(button(key).is_none() || command(key).is_none(), "{key:?}");
        }
    }

    #[test]
    fn held_buttons_follow_presses_and_releases() {
        let mut held = Held::default();
        held.set(KeyCode::KeyZ, true);
        held.set(KeyCode::ArrowLeft, true);
        held.set(KeyCode::KeyP, true);
        assert_eq!(held.buttons(), Buttons::B | Buttons::LEFT);
        held.set(KeyCode::KeyZ, false);
        assert_eq!(held.buttons(), Buttons::LEFT);
        held.release_all();
        assert_eq!(held.buttons(), Buttons::empty());
    }
}
