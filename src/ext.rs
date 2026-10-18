//! Gamepad features beyond `bevy_input`.
//!
//! Experimental: these types will change. None of them depend on SDL.

use core::time::Duration;

use bevy_ecs::{component::Component, entity::Entity, message::Message};
use bevy_input::{ButtonInput, gamepad::GamepadButton, touch::TouchPhase};
use bevy_math::{Vec2, Vec3};

/// A gamepad's touchpads and the fingers currently on them, updated every frame.
///
/// Touchpads are in the order the backend reports them. Present only on gamepads with a
/// touchpad.
#[derive(Component, Debug, Clone, Default, PartialEq)]
pub struct GamepadTouchpads(pub Vec<Touchpad>);

/// One touchpad of a gamepad.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Touchpad {
    /// One slot per finger the touchpad can track; `None` while that finger is lifted.
    pub fingers: Vec<Option<TouchpadFinger>>,
    /// The button that clicking this touchpad presses, if it clicks.
    ///
    /// Read whether it is pressed from [`Gamepad`](bevy_input::gamepad::Gamepad) as for any
    /// other button.
    pub click: Option<GamepadButton>,
}

impl Touchpad {
    /// Whether any finger is on this touchpad.
    pub fn touched(&self) -> bool {
        self.fingers.iter().any(Option::is_some)
    }
}

/// A finger touching, moving on, or leaving a gamepad touchpad, sent for every sample.
///
/// Fingers still down when their gamepad disconnects get [`TouchPhase::Ended`].
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct GamepadTouchpadEvent {
    /// The gamepad entity.
    pub gamepad: Entity,
    /// Index into [`GamepadTouchpads`].
    pub touchpad: usize,
    /// Index into [`Touchpad::fingers`].
    pub finger: usize,
    /// Whether the finger started touching, moved, or lifted.
    pub phase: TouchPhase,
    /// Where the finger is, or last was for [`TouchPhase::Ended`].
    pub at: TouchpadFinger,
    /// When the backend received the sample. Only differences between events are meaningful.
    pub timestamp: Duration,
}

/// Where a finger is on a touchpad.
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
    /// Turn on the gyroscope.
    pub gyro: bool,
    /// Turn on the accelerometer.
    pub accel: bool,
}

/// The motion sensors a gamepad has, each with its sample rate in Hz, or 0 if unknown.
///
/// Present only on gamepads with at least one motion sensor.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq)]
pub struct GamepadMotionSensors {
    /// The gyroscope's sample rate, if the gamepad has one.
    pub gyro: Option<f32>,
    /// The accelerometer's sample rate, if the gamepad has one.
    pub accel: Option<f32>,
}

/// A kind of motion sensor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MotionSensor {
    /// Measures angular velocity.
    Gyro,
    /// Measures acceleration, including gravity.
    Accel,
}

/// One motion sensor reading, sent for every sample the gamepad reports.
///
/// Axes match Bevy's: X to the right, Y up, and Z towards the player, for a gamepad held in
/// front of you.
#[derive(Message, Debug, Clone, Copy, PartialEq)]
pub struct GamepadMotionSample {
    /// The gamepad entity.
    pub gamepad: Entity,
    /// Which sensor took the reading.
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

/// A sensor that only knows whether it is being touched, such as on top of a stick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GamepadTouchSensor {
    /// On top of the left stick.
    LeftStick,
    /// On top of the right stick.
    RightStick,
    /// On the left handle, touched while holding it.
    LeftGrip,
    /// On the right handle, touched while holding it.
    RightGrip,
}

/// A gamepad's touch sensors and which of them are touched, updated every frame.
///
/// Present only on gamepads with touch sensors.
#[derive(Component, Debug, Clone, Default)]
pub struct GamepadTouchSensors {
    /// The touch sensors this gamepad has.
    pub sensors: Vec<GamepadTouchSensor>,
    /// Which sensors are touched. `just_pressed` and `just_released` cover this frame.
    pub touched: ButtonInput<GamepadTouchSensor>,
}
