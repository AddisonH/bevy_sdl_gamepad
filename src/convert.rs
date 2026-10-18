use bevy_input::gamepad::{GamepadAxis, GamepadButton};
use sdl3_sys::gamepad::*;

/// Buttons SDL knows that have no [`GamepadButton`] variant, as [`GamepadButton::Other`].
pub mod button {
    use super::*;

    const fn other(button: SDL_GamepadButton) -> GamepadButton {
        GamepadButton::Other(button.0 as u32)
    }

    /// For example the Xbox Series share, PS5 microphone, or Switch Pro capture button.
    pub const MISC1: GamepadButton = other(SDL_GAMEPAD_BUTTON_MISC1);
    /// Upper or primary right paddle.
    pub const RIGHT_PADDLE1: GamepadButton = other(SDL_GAMEPAD_BUTTON_RIGHT_PADDLE1);
    /// Upper or primary left paddle.
    pub const LEFT_PADDLE1: GamepadButton = other(SDL_GAMEPAD_BUTTON_LEFT_PADDLE1);
    /// Lower or secondary right paddle.
    pub const RIGHT_PADDLE2: GamepadButton = other(SDL_GAMEPAD_BUTTON_RIGHT_PADDLE2);
    /// Lower or secondary left paddle.
    pub const LEFT_PADDLE2: GamepadButton = other(SDL_GAMEPAD_BUTTON_LEFT_PADDLE2);
    /// Touchpad click.
    pub const TOUCHPAD: GamepadButton = other(SDL_GAMEPAD_BUTTON_TOUCHPAD);
    pub const MISC2: GamepadButton = other(SDL_GAMEPAD_BUTTON_MISC2);
    pub const MISC3: GamepadButton = other(SDL_GAMEPAD_BUTTON_MISC3);
    pub const MISC4: GamepadButton = other(SDL_GAMEPAD_BUTTON_MISC4);
    pub const MISC5: GamepadButton = other(SDL_GAMEPAD_BUTTON_MISC5);
    pub const MISC6: GamepadButton = other(SDL_GAMEPAD_BUTTON_MISC6);
}

pub(crate) fn button(button: SDL_GamepadButton) -> GamepadButton {
    match button {
        SDL_GAMEPAD_BUTTON_SOUTH => GamepadButton::South,
        SDL_GAMEPAD_BUTTON_EAST => GamepadButton::East,
        SDL_GAMEPAD_BUTTON_WEST => GamepadButton::West,
        SDL_GAMEPAD_BUTTON_NORTH => GamepadButton::North,
        SDL_GAMEPAD_BUTTON_BACK => GamepadButton::Select,
        SDL_GAMEPAD_BUTTON_GUIDE => GamepadButton::Mode,
        SDL_GAMEPAD_BUTTON_START => GamepadButton::Start,
        SDL_GAMEPAD_BUTTON_LEFT_STICK => GamepadButton::LeftThumb,
        SDL_GAMEPAD_BUTTON_RIGHT_STICK => GamepadButton::RightThumb,
        SDL_GAMEPAD_BUTTON_LEFT_SHOULDER => GamepadButton::LeftTrigger,
        SDL_GAMEPAD_BUTTON_RIGHT_SHOULDER => GamepadButton::RightTrigger,
        SDL_GAMEPAD_BUTTON_DPAD_UP => GamepadButton::DPadUp,
        SDL_GAMEPAD_BUTTON_DPAD_DOWN => GamepadButton::DPadDown,
        SDL_GAMEPAD_BUTTON_DPAD_LEFT => GamepadButton::DPadLeft,
        SDL_GAMEPAD_BUTTON_DPAD_RIGHT => GamepadButton::DPadRight,
        other => GamepadButton::Other(other.0 as u32),
    }
}

/// An SDL axis as Bevy sees it: SDL reports triggers as axes, Bevy as analog buttons.
#[derive(Debug, PartialEq)]
pub(crate) enum Input {
    Axis(GamepadAxis, f32),
    Button(GamepadButton, f32),
}

pub(crate) fn axis(axis: SDL_GamepadAxis, value: i16) -> Option<Input> {
    let value = (f32::from(value) / f32::from(i16::MAX)).clamp(-1.0, 1.0);
    // SDL's stick Y axes point down; Bevy's point up.
    Some(match axis {
        SDL_GAMEPAD_AXIS_LEFTX => Input::Axis(GamepadAxis::LeftStickX, value),
        SDL_GAMEPAD_AXIS_LEFTY => Input::Axis(GamepadAxis::LeftStickY, -value),
        SDL_GAMEPAD_AXIS_RIGHTX => Input::Axis(GamepadAxis::RightStickX, value),
        SDL_GAMEPAD_AXIS_RIGHTY => Input::Axis(GamepadAxis::RightStickY, -value),
        SDL_GAMEPAD_AXIS_LEFT_TRIGGER => Input::Button(GamepadButton::LeftTrigger2, value),
        SDL_GAMEPAD_AXIS_RIGHT_TRIGGER => Input::Button(GamepadButton::RightTrigger2, value),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stick_y_is_flipped_and_clamped() {
        assert_eq!(
            axis(SDL_GAMEPAD_AXIS_LEFTY, i16::MIN),
            Some(Input::Axis(GamepadAxis::LeftStickY, 1.0))
        );
        assert_eq!(
            axis(SDL_GAMEPAD_AXIS_RIGHTY, i16::MAX),
            Some(Input::Axis(GamepadAxis::RightStickY, -1.0))
        );
    }

    #[test]
    fn triggers_are_analog_buttons() {
        assert_eq!(
            axis(SDL_GAMEPAD_AXIS_RIGHT_TRIGGER, i16::MAX),
            Some(Input::Button(GamepadButton::RightTrigger2, 1.0))
        );
    }

    #[test]
    fn unnamed_buttons_keep_their_sdl_index() {
        assert_eq!(button(SDL_GAMEPAD_BUTTON_LEFT_PADDLE2), button::LEFT_PADDLE2);
    }
}
