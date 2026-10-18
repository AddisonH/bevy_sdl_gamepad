//! Gamepad features beyond `bevy_input`.
//!
//! Experimental: these types will change. None of them depend on SDL.

use core::time::Duration;

use bevy_ecs::{component::Component, entity::Entity, message::Message};
use bevy_math::{Vec2, Vec3};

/// A gamepad's touchpads, in the order its backend reports them.
///
/// Clicking a touchpad is a button, not part of this state.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct GamepadTouchpads(pub Vec<Touchpad>);

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Touchpad {
    /// One slot per finger the touchpad can track; `None` while that finger is lifted.
    pub fingers: Vec<Option<TouchpadFinger>>,
}

impl Touchpad {
    pub fn touched(&self) -> bool {
        self.fingers.iter().any(Option::is_some)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TouchpadFinger {
    /// From (0, 0) at the top left to (1, 1) at the bottom right, like window coordinates.
    pub position: Vec2,
    /// From 0 to 1. Touchpads without pressure sensing report 0.
    pub pressure: f32,
}

/// Which motion sensors to turn on for a gamepad. Without this component its sensors stay off.
///
/// Sensors cost bandwidth and battery, so they are opt-in. The setting survives reconnection.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct GamepadMotionConfig {
    pub gyro: bool,
    pub accel: bool,
}

/// The motion sensors a gamepad has, each with its sample rate in Hz, or 0 if unknown.
///
/// Present only on gamepads with at least one motion sensor.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct GamepadMotionSensors {
    pub gyro: Option<f32>,
    pub accel: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotionSensor {
    Gyro,
    Accel,
}

/// One motion sensor reading, sent for every sample the gamepad reports.
///
/// Axes match Bevy's: X to the right, Y up, and Z towards the player, for a gamepad held in
/// front of you.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct GamepadMotionSample {
    pub gamepad: Entity,
    pub sensor: MotionSensor,
    /// Gyro: angular velocity in radians per second, counter-clockwise positive.
    /// Accel: acceleration in m/s², including gravity.
    pub value: Vec3,
    /// When the gamepad took the reading, on its own clock. Only differences between samples
    /// from the same gamepad are meaningful.
    pub timestamp: Duration,
}

/// The latest motion readings of a gamepad, updated every frame while its sensors are on.
///
/// Present on the same gamepads as [`GamepadMotionSensors`].
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct GamepadMotion {
    /// The latest gyro reading, in radians per second.
    pub angular_velocity: Vec3,
    /// The latest accelerometer reading, in m/s², including gravity.
    pub acceleration: Vec3,
    /// Rotation since the previous frame, in radians around each axis, summed over every gyro
    /// sample. The building block for gyro aiming.
    pub rotation_delta: Vec3,
}
