//! Drives the plugin with SDL virtual gamepads.

use core::{ffi::c_void, time::Duration};
use std::sync::{Mutex, MutexGuard};

use bevy_app::{App, AppExit};
use bevy_ecs::message::{MessageCursor, Messages};
use bevy_ecs::prelude::*;
use bevy_input::touch::TouchPhase;
use bevy_input::{
    InputPlugin,
    gamepad::{Gamepad, GamepadButton, GamepadRumbleIntensity, GamepadRumbleRequest},
};
use bevy_math::{Vec2, Vec3};
use bevy_sdl_gamepad::ext::{
    GamepadMotion, GamepadMotionConfig, GamepadMotionSample, GamepadMotionSensors,
    GamepadTouchSensor, GamepadTouchSensors, GamepadTouchpadEvent, GamepadTouchpads, MotionSensor,
    TouchpadFinger,
};
use bevy_sdl_gamepad::{SdlGamepad, SdlGamepadPlugin, button};
use bevy_time::TimePlugin;
use sdl3_sys::{gamepad::*, joystick::*, sensor::*};

const VENDOR: u16 = 0x1209;
const PRODUCT: u16 = 0x0001;
/// Valve's vendor ID, with a product ID no real Valve device uses.
const VALVE_VENDOR: u16 = 0x28de;

// SDL state is process-wide, so tests take turns.
static SDL_LOCK: Mutex<()> = Mutex::new(());
static RUMBLE: Mutex<Vec<(u16, u16)>> = Mutex::new(Vec::new());

unsafe extern "C" fn record_rumble(_: *mut c_void, low: u16, high: u16) -> bool {
    RUMBLE.lock().unwrap().push((low, high));
    true
}

struct Harness {
    app: App,
    _lock: MutexGuard<'static, ()>,
}

impl Harness {
    fn new() -> Self {
        let lock = SDL_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut plugin = SdlGamepadPlugin::default();
        // Keep real gamepads out of the tests.
        plugin.hints.push((
            "SDL_GAMECONTROLLER_IGNORE_DEVICES_EXCEPT".into(),
            format!("{VENDOR:#06x}/{PRODUCT:#06x},{VALVE_VENDOR:#06x}/{PRODUCT:#06x}"),
        ));
        let mut app = App::new();
        app.add_plugins((TimePlugin, InputPlugin, plugin));
        app.update();
        Self { app, _lock: lock }
    }

    fn update(&mut self) {
        self.app.update();
    }

    fn entity(&mut self, pad: &VirtualPad) -> Entity {
        let mut query = self.app.world_mut().query::<(Entity, &SdlGamepad)>();
        query
            .iter(self.app.world())
            .find(|(_, sdl)| sdl.instance_id() == pad.id.0)
            .map(|(entity, _)| entity)
            .expect("virtual gamepad has no entity")
    }

    fn gamepad(&mut self, pad: &VirtualPad) -> &Gamepad {
        let entity = self.entity(pad);
        self.app
            .world()
            .get::<Gamepad>(entity)
            .expect("no Gamepad component")
    }

    fn touchpads(&mut self, pad: &VirtualPad) -> &GamepadTouchpads {
        let entity = self.entity(pad);
        self.app
            .world()
            .get::<GamepadTouchpads>(entity)
            .expect("no touchpads")
    }

    /// Messages of type `M` sent since `cursor` last read them.
    fn messages<M: Message + Copy>(&self, cursor: &mut MessageCursor<M>) -> Vec<M> {
        let messages = self.app.world().resource::<Messages<M>>();
        cursor.read(messages).copied().collect()
    }

    fn rumble(&mut self, request: GamepadRumbleRequest) {
        self.app.world_mut().write_message(request);
        self.update();
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        self.app.world_mut().write_message(AppExit::Success);
        self.app.update();
    }
}

struct VirtualPad {
    id: SDL_JoystickID,
    joystick: *mut SDL_Joystick,
}

impl VirtualPad {
    fn attach() -> Self {
        Self::attach_from(VENDOR, 1)
    }

    /// A Valve gamepad with two touchpads, like the Steam Deck.
    fn valve() -> Self {
        Self::attach_from(VALVE_VENDOR, 2)
    }

    fn attach_from(vendor: u16, ntouchpads: usize) -> Self {
        let touchpad = SDL_VirtualJoystickTouchpadDesc {
            nfingers: 2,
            ..Default::default()
        };
        let touchpads = vec![touchpad; ntouchpads];
        let sensors = [
            SDL_VirtualJoystickSensorDesc {
                r#type: SDL_SENSOR_GYRO,
                rate: 200.0,
            },
            SDL_VirtualJoystickSensorDesc {
                r#type: SDL_SENSOR_ACCEL,
                rate: 100.0,
            },
        ];
        let desc = SDL_VirtualJoystickDesc {
            r#type: SDL_JOYSTICK_TYPE_GAMEPAD.0 as u16,
            vendor_id: vendor,
            product_id: PRODUCT,
            naxes: SDL_GAMEPAD_AXIS_COUNT.0 as u16,
            nbuttons: SDL_GAMEPAD_BUTTON_COUNT.0 as u16,
            ntouchpads: ntouchpads as u16,
            touchpads: touchpads.as_ptr(),
            nsensors: sensors.len() as u16,
            sensors: sensors.as_ptr(),
            name: c"Virtual Test Pad".as_ptr(),
            Rumble: Some(record_rumble),
            ..Default::default()
        };
        // SAFETY: SDL is initialised by the harness; `desc` and what it points to outlive the call.
        unsafe {
            let id = SDL_AttachVirtualJoystick(&desc);
            assert_ne!(id.0, 0, "SDL_AttachVirtualJoystick failed");
            let joystick = SDL_OpenJoystick(id);
            assert!(!joystick.is_null());
            Self { id, joystick }
        }
    }

    fn button(&self, button: SDL_GamepadButton, down: bool) {
        // SAFETY: the joystick is open.
        assert!(unsafe { SDL_SetJoystickVirtualButton(self.joystick, button.0, down) });
    }

    fn axis(&self, axis: SDL_GamepadAxis, value: i16) {
        // SAFETY: the joystick is open.
        assert!(unsafe { SDL_SetJoystickVirtualAxis(self.joystick, axis.0, value) });
    }

    fn touch(&self, finger: i32, at: Option<(f32, f32)>) {
        let (x, y) = at.unwrap_or_default();
        // SAFETY: the joystick is open.
        let ok = unsafe {
            SDL_SetJoystickVirtualTouchpad(self.joystick, 0, finger, at.is_some(), x, y, 0.5)
        };
        assert!(ok);
    }

    fn sensor(&self, sensor: SDL_SensorType, millis: u64, value: [f32; 3]) {
        let nanos = millis * 1_000_000;
        // SAFETY: the joystick is open; `value` holds the 3 floats passed.
        let ok = unsafe {
            SDL_SendJoystickVirtualSensorData(self.joystick, sensor, nanos, value.as_ptr(), 3)
        };
        assert!(ok);
    }

    fn detach(self) {
        // SAFETY: the joystick is open and was attached by `attach`.
        unsafe {
            SDL_CloseJoystick(self.joystick);
            assert!(SDL_DetachVirtualJoystick(self.id));
        }
    }
}

#[test]
fn connects_with_name_and_ids() {
    let mut h = Harness::new();
    let pad = VirtualPad::attach();
    h.update();

    let entity = h.entity(&pad);
    let gamepad = h.app.world().get::<Gamepad>(entity).unwrap();
    assert_eq!(gamepad.vendor_id(), Some(VENDOR));
    assert_eq!(gamepad.product_id(), Some(PRODUCT));
    let name = h.app.world().get::<Name>(entity).unwrap();
    assert_eq!(name.as_str(), "Virtual Test Pad");

    let mut sdl_gamepads = h.app.world_mut().query::<&SdlGamepad>();
    assert_eq!(sdl_gamepads.iter(h.app.world()).count(), 1);
    pad.detach();
}

#[test]
fn buttons_press_and_release() {
    let mut h = Harness::new();
    let pad = VirtualPad::attach();
    h.update();

    pad.button(SDL_GAMEPAD_BUTTON_SOUTH, true);
    pad.button(SDL_GAMEPAD_BUTTON_LEFT_PADDLE2, true);
    h.update();
    let gamepad = h.gamepad(&pad);
    assert!(gamepad.just_pressed(GamepadButton::South));
    assert!(gamepad.pressed(button::LEFT_PADDLE2));

    pad.button(SDL_GAMEPAD_BUTTON_SOUTH, false);
    h.update();
    let gamepad = h.gamepad(&pad);
    assert!(gamepad.just_released(GamepadButton::South));
    assert!(gamepad.pressed(button::LEFT_PADDLE2));
    pad.detach();
}

#[test]
fn sticks_point_up_and_triggers_are_buttons() {
    let mut h = Harness::new();
    let pad = VirtualPad::attach();
    h.update();

    pad.axis(SDL_GAMEPAD_AXIS_LEFTY, i16::MIN);
    pad.axis(SDL_GAMEPAD_AXIS_RIGHTX, i16::MAX);
    pad.axis(SDL_GAMEPAD_AXIS_RIGHT_TRIGGER, i16::MAX);
    h.update();
    let gamepad = h.gamepad(&pad);
    assert_eq!(gamepad.left_stick().y, 1.0);
    assert_eq!(gamepad.right_stick().x, 1.0);
    assert_eq!(gamepad.get(GamepadButton::RightTrigger2), Some(1.0));
    assert!(gamepad.pressed(GamepadButton::RightTrigger2));
    pad.detach();
}

#[test]
fn input_held_while_connecting_is_reported() {
    let mut h = Harness::new();
    let pad = VirtualPad::attach();
    pad.button(SDL_GAMEPAD_BUTTON_EAST, true);
    pad.axis(SDL_GAMEPAD_AXIS_LEFTX, i16::MAX);
    h.update();

    let gamepad = h.gamepad(&pad);
    assert!(gamepad.pressed(GamepadButton::East));
    assert_eq!(gamepad.left_stick().x, 1.0);
    pad.detach();
}

#[test]
fn reconnecting_reuses_the_entity() {
    let mut h = Harness::new();
    let pad = VirtualPad::attach();
    h.update();
    let first = h.entity(&pad);

    pad.detach();
    h.update();
    let world = h.app.world();
    assert!(world.get::<Gamepad>(first).is_none());
    assert!(world.get::<SdlGamepad>(first).is_none());

    let pad = VirtualPad::attach();
    h.update();
    assert_eq!(h.entity(&pad), first);
    pad.detach();
}

#[test]
fn rumble_stacks_expires_and_stops() {
    let mut h = Harness::new();
    let pad = VirtualPad::attach();
    h.update();
    let gamepad = h.entity(&pad);
    RUMBLE.lock().unwrap().clear();
    let last = || *RUMBLE.lock().unwrap().last().expect("no rumble sent");

    let add = |intensity, duration| GamepadRumbleRequest::Add {
        gamepad,
        intensity,
        duration,
    };
    h.rumble(add(
        GamepadRumbleIntensity::strong_motor(0.5),
        Duration::from_secs(10),
    ));
    assert_eq!(last(), (32767, 0));

    h.rumble(add(
        GamepadRumbleIntensity::WEAK_MAX,
        Duration::from_millis(50),
    ));
    assert_eq!(last(), (32767, u16::MAX));

    std::thread::sleep(Duration::from_millis(100));
    h.update();
    assert_eq!(last(), (32767, 0));

    h.rumble(GamepadRumbleRequest::Stop { gamepad });
    assert_eq!(last(), (0, 0));
    pad.detach();
}

#[test]
fn rumble_resumes_after_reconnecting() {
    let mut h = Harness::new();
    let pad = VirtualPad::attach();
    h.update();
    let gamepad = h.entity(&pad);
    h.rumble(GamepadRumbleRequest::Add {
        gamepad,
        intensity: GamepadRumbleIntensity::STRONG_MAX,
        duration: Duration::from_secs(10),
    });

    pad.detach();
    h.update();
    RUMBLE.lock().unwrap().clear();
    let pad = VirtualPad::attach();
    h.update();
    assert_eq!(RUMBLE.lock().unwrap().last(), Some(&(u16::MAX, 0)));
    pad.detach();
}

#[test]
fn valve_touchpads_click_with_touchpad_and_misc2() {
    let mut h = Harness::new();
    let pad = VirtualPad::valve();
    h.update();
    let clicks: Vec<_> = h.touchpads(&pad).0.iter().map(|pad| pad.click).collect();
    assert_eq!(clicks, [Some(button::TOUCHPAD), Some(button::MISC2)]);

    pad.button(SDL_GAMEPAD_BUTTON_MISC2, true);
    h.update();
    assert!(h.gamepad(&pad).pressed(button::MISC2));
    pad.detach();
}

#[test]
fn touchpad_fingers_follow_touches() {
    let mut h = Harness::new();
    let pad = VirtualPad::attach();
    h.update();
    assert_eq!(h.touchpads(&pad).0[0].click, Some(button::TOUCHPAD));
    let mut events = MessageCursor::<GamepadTouchpadEvent>::default();
    let mut phases = |h: &Harness| -> Vec<_> {
        let events = h.messages(&mut events);
        events
            .iter()
            .map(|e| (e.touchpad, e.finger, e.phase))
            .collect()
    };

    pad.touch(1, Some((0.25, 0.75)));
    h.update();
    let finger = TouchpadFinger {
        position: Vec2::new(0.25, 0.75),
        pressure: 0.5,
    };
    assert_eq!(h.touchpads(&pad).0[0].fingers, vec![None, Some(finger)]);
    assert_eq!(phases(&h), [(0, 1, TouchPhase::Started)]);

    pad.touch(1, Some((0.5, 0.5)));
    h.update();
    assert_eq!(phases(&h), [(0, 1, TouchPhase::Moved)]);

    pad.touch(1, None);
    h.update();
    assert!(!h.touchpads(&pad).0[0].touched());
    assert_eq!(phases(&h), [(0, 1, TouchPhase::Ended)]);

    pad.touch(0, Some((0.5, 0.5)));
    h.update();
    phases(&h);
    pad.detach();
    h.update();
    assert_eq!(phases(&h), [(0, 0, TouchPhase::Ended)]);
}

#[test]
fn motion_sensors_are_listed_and_off_by_default() {
    let mut h = Harness::new();
    let pad = VirtualPad::attach();
    h.update();

    let entity = h.entity(&pad);
    let sensors = h.app.world().get::<GamepadMotionSensors>(entity).unwrap();
    assert_eq!(sensors.gyro, Some(200.0));
    assert_eq!(sensors.accel, Some(100.0));

    let mut samples = MessageCursor::<GamepadMotionSample>::default();
    pad.sensor(SDL_SENSOR_GYRO, 0, [1.0, 2.0, 3.0]);
    h.update();
    assert!(h.messages(&mut samples).is_empty());
    assert_eq!(
        h.app.world().get::<GamepadMotion>(entity),
        Some(&GamepadMotion::default())
    );
    pad.detach();
}

#[test]
fn gyro_samples_sum_into_rotation_delta() {
    let mut h = Harness::new();
    let pad = VirtualPad::attach();
    h.update();
    let entity = h.entity(&pad);
    h.app
        .world_mut()
        .entity_mut(entity)
        .insert(GamepadMotionConfig {
            gyro: true,
            accel: true,
        });
    h.update();

    let mut samples = MessageCursor::<GamepadMotionSample>::default();
    pad.sensor(SDL_SENSOR_GYRO, 0, [1.0, 0.0, 0.0]);
    pad.sensor(SDL_SENSOR_GYRO, 10, [1.0, 0.0, 0.0]);
    pad.sensor(SDL_SENSOR_GYRO, 20, [3.0, 0.0, 0.0]);
    pad.sensor(SDL_SENSOR_ACCEL, 20, [0.0, 9.8, 0.0]);
    h.update();

    let samples = h.messages(&mut samples);
    let gyro = samples
        .iter()
        .filter(|s| s.sensor == MotionSensor::Gyro)
        .count();
    assert_eq!((samples.len(), gyro), (4, 3));
    assert!(samples.iter().all(|s| s.gamepad == entity));

    let motion = *h.app.world().get::<GamepadMotion>(entity).unwrap();
    assert_eq!(motion.angular_velocity, Vec3::new(3.0, 0.0, 0.0));
    assert_eq!(motion.acceleration, Vec3::new(0.0, 9.8, 0.0));
    // The first sample has no predecessor; the others cover 10 ms each.
    assert!((motion.rotation_delta.x - 0.04).abs() < 1e-6, "{motion:?}");

    h.update();
    let motion = *h.app.world().get::<GamepadMotion>(entity).unwrap();
    assert_eq!(motion.rotation_delta, Vec3::ZERO);
    assert_eq!(motion.angular_velocity, Vec3::new(3.0, 0.0, 0.0));
    pad.detach();
}

#[test]
fn valve_touch_sensors_are_not_buttons() {
    let mut h = Harness::new();
    let pad = VirtualPad::valve();
    pad.button(SDL_GAMEPAD_BUTTON_MISC6, true);
    h.update();
    let entity = h.entity(&pad);
    let sensors = |h: &Harness| {
        h.app
            .world()
            .get::<GamepadTouchSensors>(entity)
            .unwrap()
            .clone()
    };
    assert_eq!(
        sensors(&h).sensors,
        [
            GamepadTouchSensor::LeftStick,
            GamepadTouchSensor::RightStick,
            GamepadTouchSensor::LeftGrip,
            GamepadTouchSensor::RightGrip,
        ]
    );
    assert!(sensors(&h).touched.pressed(GamepadTouchSensor::RightGrip));

    pad.button(SDL_GAMEPAD_BUTTON_MISC3, true);
    h.update();
    assert!(
        sensors(&h)
            .touched
            .just_pressed(GamepadTouchSensor::LeftStick)
    );
    let gamepad = h.gamepad(&pad);
    assert!(!gamepad.pressed(button::MISC3));
    assert!(!gamepad.pressed(button::MISC6));

    pad.button(SDL_GAMEPAD_BUTTON_MISC3, false);
    h.update();
    assert!(
        sensors(&h)
            .touched
            .just_released(GamepadTouchSensor::LeftStick)
    );
    pad.detach();
}

#[test]
fn other_gamepads_keep_misc_buttons() {
    let mut h = Harness::new();
    let pad = VirtualPad::attach();
    h.update();
    let entity = h.entity(&pad);
    assert!(h.app.world().get::<GamepadTouchSensors>(entity).is_none());

    pad.button(SDL_GAMEPAD_BUTTON_MISC3, true);
    h.update();
    assert!(h.gamepad(&pad).pressed(button::MISC3));
    pad.detach();
}
