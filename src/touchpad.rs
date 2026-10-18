use core::{ptr::NonNull, time::Duration};

use bevy_ecs::{entity::Entity, world::Mut};
use bevy_input::touch::TouchPhase;
use bevy_math::Vec2;
use sdl3_sys::{events::*, gamepad::*};

use crate::{
    button,
    ext::{GamepadTouchpadEvent, GamepadTouchpads, Touchpad, TouchpadFinger},
};

const VALVE: u16 = 0x28de;

/// The touchpad layout and any fingers already down, if the gamepad has touchpads.
pub(crate) fn touchpads(gamepad: NonNull<SDL_Gamepad>) -> Option<GamepadTouchpads> {
    let gamepad = gamepad.as_ptr();
    // SAFETY: `gamepad` is open; indices stay within the counts SDL reports.
    unsafe {
        let count = SDL_GetNumGamepadTouchpads(gamepad);
        let clicks: &[_] = match count {
            1 if SDL_GamepadHasButton(gamepad, SDL_GAMEPAD_BUTTON_TOUCHPAD) => &[button::TOUCHPAD],
            // SDL maps the right pad's click on Valve controllers to misc2.
            2 if SDL_GetGamepadVendor(gamepad) == VALVE => &[button::TOUCHPAD, button::MISC2],
            _ => &[],
        };
        let pads: Vec<_> = (0..count)
            .map(|pad| {
                let fingers = (0..SDL_GetNumGamepadTouchpadFingers(gamepad, pad)).map(|finger| {
                    let (mut down, mut x, mut y, mut pressure) = (false, 0.0, 0.0, 0.0);
                    SDL_GetGamepadTouchpadFinger(
                        gamepad,
                        pad,
                        finger,
                        &mut down,
                        &mut x,
                        &mut y,
                        &mut pressure,
                    );
                    down.then(|| TouchpadFinger {
                        position: Vec2::new(x, y),
                        pressure,
                    })
                });
                Touchpad {
                    fingers: fingers.collect(),
                    click: clicks.get(pad as usize).copied(),
                }
            })
            .collect();
        (!pads.is_empty()).then_some(GamepadTouchpads(pads))
    }
}

/// `Started` events for the fingers already down on a gamepad that just connected.
pub(crate) fn started(
    gamepad: Entity,
    pads: &GamepadTouchpads,
    timestamp: Duration,
) -> impl Iterator<Item = GamepadTouchpadEvent> + '_ {
    pads.0.iter().enumerate().flat_map(move |(touchpad, pad)| {
        pad.fingers
            .iter()
            .enumerate()
            .filter_map(move |(finger, at)| {
                Some(GamepadTouchpadEvent {
                    gamepad,
                    touchpad,
                    finger,
                    phase: TouchPhase::Started,
                    at: (*at)?,
                    timestamp,
                })
            })
    })
}

/// Turns an SDL touchpad event into ours, and applies it to the gamepad's touchpad state.
pub(crate) fn event(
    gamepad: Entity,
    kind: SDL_EventType,
    event: &SDL_GamepadTouchpadEvent,
    pads: Option<Mut<GamepadTouchpads>>,
) -> Option<GamepadTouchpadEvent> {
    let phase = match kind {
        SDL_EVENT_GAMEPAD_TOUCHPAD_DOWN => TouchPhase::Started,
        SDL_EVENT_GAMEPAD_TOUCHPAD_MOTION => TouchPhase::Moved,
        SDL_EVENT_GAMEPAD_TOUCHPAD_UP => TouchPhase::Ended,
        _ => return None,
    };
    let (touchpad, finger) = (event.touchpad as usize, event.finger as usize);
    let at = TouchpadFinger {
        position: Vec2::new(event.x, event.y),
        pressure: event.pressure,
    };
    if let Some(mut pads) = pads
        && let Some(slot) = pads
            .0
            .get_mut(touchpad)
            .and_then(|pad| pad.fingers.get_mut(finger))
    {
        *slot = (phase != TouchPhase::Ended).then_some(at);
    }
    Some(GamepadTouchpadEvent {
        gamepad,
        touchpad,
        finger,
        phase,
        at,
        timestamp: Duration::from_nanos(event.timestamp),
    })
}
