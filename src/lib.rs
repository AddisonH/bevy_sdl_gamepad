//! An SDL3 gamepad backend for Bevy, replacing `bevy_gilrs`.
//!
//! Gamepads appear through the usual `bevy_input` API ([`Gamepad`](bevy_input::gamepad::Gamepad),
//! [`GamepadRumbleRequest`](bevy_input::gamepad::GamepadRumbleRequest)). Disable Bevy's
//! `bevy_gilrs` feature when using this plugin.

pub mod ext;

mod convert;
mod events;
mod rumble;
mod sdl;

pub use convert::button;

use bevy_app::{App, Last, Plugin, PostUpdate, PreStartup, PreUpdate};
use bevy_ecs::{component::Component, schedule::IntoScheduleConfigs, system::NonSendMarker};
use bevy_input::InputSystems;
use sdl3_sys::hints::{
    SDL_HINT_JOYSTICK_ALLOW_BACKGROUND_EVENTS, SDL_HINT_JOYSTICK_HIDAPI_STEAM,
    SDL_HINT_NO_SIGNAL_HANDLERS,
};

pub struct SdlGamepadPlugin {
    /// SDL hints set before SDL is initialised, as `(name, value)`.
    ///
    /// An environment variable with the same name overrides the value given here.
    pub hints: Vec<(String, String)>,
}

impl Default for SdlGamepadPlugin {
    fn default() -> Self {
        let hint = |name, value: &str| (sdl::string(name).unwrap_or_default(), value.to_owned());
        Self {
            hints: vec![
                // Bevy handles Ctrl+C itself.
                hint(SDL_HINT_NO_SIGNAL_HANDLERS, "1"),
                // SDL has no window of its own, so it cannot tell whether the app is focused.
                hint(SDL_HINT_JOYSTICK_ALLOW_BACKGROUND_EVENTS, "1"),
                hint(SDL_HINT_JOYSTICK_HIDAPI_STEAM, "1"),
            ],
        }
    }
}

impl Plugin for SdlGamepadPlugin {
    fn build(&self, app: &mut App) {
        let hints = self.hints.clone();
        app.init_resource::<events::Gamepads>()
            .init_resource::<rumble::Rumbles>()
            .add_systems(PreStartup, move |_: NonSendMarker| sdl::init(&hints))
            .add_systems(PreUpdate, events::poll.before(InputSystems))
            .add_systems(PostUpdate, rumble::play)
            .add_systems(Last, sdl::shutdown_on_exit);
    }
}

/// SDL's identity for a connected gamepad. Removed when the gamepad disconnects.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdlGamepad {
    instance_id: u32,
}

impl SdlGamepad {
    /// The SDL joystick instance ID. SDL never reuses one, even when the same device reconnects.
    pub fn instance_id(&self) -> u32 {
        self.instance_id
    }
}
