//! An SDL3 gamepad backend for Bevy, replacing `bevy_gilrs`.
//!
//! Gamepads appear through the usual `bevy_input` API ([`Gamepad`],
//! [`GamepadRumbleRequest`](bevy_input::gamepad::GamepadRumbleRequest)), so code and input
//! crates built on it keep working. Features `bevy_input` has no types for, such as gyro and
//! touchpads, are in [`ext`].
//!
//! Bevy's own gamepad backend must be off, or every controller appears twice:
//!
//! ```no_run
//! use bevy::{gilrs::GilrsPlugin, prelude::*};
//! use bevy_sdl_gamepad::SdlGamepadPlugin;
//!
//! App::new()
//!     .add_plugins((
//!         DefaultPlugins.build().disable::<GilrsPlugin>(),
//!         SdlGamepadPlugin::default(),
//!     ))
//!     .run();
//! ```
//!
//! Turning off Bevy's `bevy_gilrs` feature works too.

#![warn(missing_docs)]

pub mod ext;

mod convert;
mod events;
mod motion;
mod rumble;
mod sdl;
mod touch_sensor;
mod touchpad;

pub use convert::button;

use bevy_app::{App, Last, Plugin, PostUpdate, PreStartup, PreUpdate};
use bevy_ecs::{
    component::Component,
    entity::Entity,
    query::{Added, Without},
    schedule::IntoScheduleConfigs,
    system::{NonSendMarker, Query},
};
use bevy_input::{InputSystems, gamepad::Gamepad};
use bevy_log::warn_once;
use sdl3_sys::hints::{
    SDL_HINT_JOYSTICK_ALLOW_BACKGROUND_EVENTS, SDL_HINT_JOYSTICK_HIDAPI_STEAM,
    SDL_HINT_NO_SIGNAL_HANDLERS,
};

/// Connects gamepads through SDL and keeps their `bevy_input` and [`ext`] state up to date.
pub struct SdlGamepadPlugin {
    /// SDL hints set before SDL is initialised, as `(name, value)`.
    ///
    /// An environment variable with the same name overrides the value given here.
    pub hints: Vec<(String, String)>,
    /// Motion sensors to turn on for gamepads that connect without a
    /// [`GamepadMotionConfig`](ext::GamepadMotionConfig). Off by default.
    pub motion: ext::GamepadMotionConfig,
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
            motion: ext::GamepadMotionConfig::default(),
        }
    }
}

impl Plugin for SdlGamepadPlugin {
    fn build(&self, app: &mut App) {
        let hints = self.hints.clone();
        app.init_resource::<events::Gamepads>()
            .init_resource::<rumble::Rumbles>()
            .insert_resource(motion::DefaultMotionConfig(self.motion))
            .add_message::<ext::GamepadMotionSample>()
            .add_message::<ext::GamepadTouchpadEvent>()
            .add_systems(PreStartup, move |_: NonSendMarker| sdl::init(&hints))
            .add_systems(
                PreUpdate,
                (events::poll, motion::apply_config)
                    .chain()
                    .before(InputSystems),
            )
            .add_systems(PreUpdate, warn_other_backends.after(InputSystems))
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

fn warn_other_backends(gamepads: Query<Entity, (Added<Gamepad>, Without<SdlGamepad>)>) {
    if let Some(gamepad) = gamepads.iter().next() {
        warn_once!(
            "Gamepad {gamepad} was connected by another backend, so controllers may appear twice. \
             If Bevy's GilrsPlugin is enabled, disable it with \
             `DefaultPlugins.build().disable::<GilrsPlugin>()`."
        );
    }
}
