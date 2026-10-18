//! Gamepad features beyond `bevy_input`.
//!
//! Experimental: these types will change. None of them depend on SDL.

use bevy_ecs::component::Component;
use bevy_math::Vec2;

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
