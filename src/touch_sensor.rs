use core::ptr::NonNull;

use sdl3_sys::gamepad::*;

use crate::{
    ext::{GamepadTouchSensor, GamepadTouchSensors},
    sdl::VALVE_VENDOR_ID,
};

/// Until SDL's own touch sensor API is released, SDL reports Valve gamepads' touch sensors as
/// these buttons.
const VALVE_TOUCH_BUTTONS: [(SDL_GamepadButton, GamepadTouchSensor); 4] = [
    (SDL_GAMEPAD_BUTTON_MISC3, GamepadTouchSensor::LeftStick),
    (SDL_GAMEPAD_BUTTON_MISC4, GamepadTouchSensor::RightStick),
    (SDL_GAMEPAD_BUTTON_MISC5, GamepadTouchSensor::LeftGrip),
    (SDL_GAMEPAD_BUTTON_MISC6, GamepadTouchSensor::RightGrip),
];

/// The SDL buttons that stand for this gamepad's touch sensors.
pub(crate) type TouchButtons = Vec<(SDL_GamepadButton, GamepadTouchSensor)>;

pub(crate) fn touch_buttons(gamepad: NonNull<SDL_Gamepad>) -> TouchButtons {
    let gamepad = gamepad.as_ptr();
    // SAFETY: `gamepad` is open.
    if unsafe { SDL_GetGamepadVendor(gamepad) } != VALVE_VENDOR_ID {
        return TouchButtons::new();
    }
    VALVE_TOUCH_BUTTONS
        .into_iter()
        // SAFETY: `gamepad` is open.
        .filter(|&(button, _)| unsafe { SDL_GamepadHasButton(gamepad, button) })
        .collect()
}

/// The touch sensors and which are already touched, if the gamepad has any.
pub(crate) fn touch_sensors(
    gamepad: NonNull<SDL_Gamepad>,
    buttons: &TouchButtons,
) -> Option<GamepadTouchSensors> {
    let mut sensors = GamepadTouchSensors::default();
    for &(button, sensor) in buttons {
        sensors.sensors.push(sensor);
        // SAFETY: `gamepad` is open.
        if unsafe { SDL_GetGamepadButton(gamepad.as_ptr(), button) } {
            sensors.touched.press(sensor);
        }
    }
    (!sensors.sensors.is_empty()).then_some(sensors)
}
