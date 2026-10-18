use core::{ptr::NonNull, time::Duration};

use bevy_ecs::{
    change_detection::DetectChangesMut,
    entity::{Entity, EntityHashMap},
    message::MessageWriter,
    resource::Resource,
    system::{Commands, NonSendMarker, Query, Res, ResMut, SystemParam},
};
use bevy_input::gamepad::{
    GamepadAxis, GamepadButton, GamepadConnection, GamepadConnectionEvent,
    RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent, RawGamepadEvent,
};
use bevy_math::Vec3;
use bevy_platform::collections::HashMap;
use sdl3_sys::{events::*, gamepad::*, joystick::SDL_JoystickID};

use crate::{
    SdlGamepad,
    convert::{self, Input},
    ext::{
        GamepadMotion, GamepadMotionConfig, GamepadMotionSample, GamepadMotionSensors,
        GamepadTouchSensors, GamepadTouchpadEvent, GamepadTouchpads,
    },
    motion::{self, DefaultMotionConfig},
    sdl::{self, Sdl},
    touch_sensor::{self, TouchButtons},
    touchpad,
};

/// Identifies a physical device across reconnections, so it gets back its entity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DeviceKey {
    guid: [u8; 16],
    /// Serial number, or failing that the OS device path.
    serial_or_path: Option<String>,
}

#[derive(Resource, Default)]
pub(crate) struct Gamepads {
    entities: HashMap<u32, (Entity, DeviceKey)>,
    pub(crate) ids: EntityHashMap<u32>,
    disconnected: HashMap<DeviceKey, Entity>,
    last_gyro: HashMap<u32, Duration>,
    touch_buttons: HashMap<u32, TouchButtons>,
}

/// The extension components the backend keeps up to date.
#[derive(SystemParam)]
pub(crate) struct Extensions<'w, 's> {
    touchpads: Query<'w, 's, &'static mut GamepadTouchpads>,
    motions: Query<'w, 's, &'static mut GamepadMotion>,
    touch_sensors: Query<'w, 's, &'static mut GamepadTouchSensors>,
}

#[derive(SystemParam)]
pub(crate) struct Writers<'w> {
    raw: MessageWriter<'w, RawGamepadEvent>,
    connection: MessageWriter<'w, GamepadConnectionEvent>,
    button: MessageWriter<'w, RawGamepadButtonChangedEvent>,
    axis: MessageWriter<'w, RawGamepadAxisChangedEvent>,
    motion: MessageWriter<'w, GamepadMotionSample>,
    touchpad: MessageWriter<'w, GamepadTouchpadEvent>,
}

impl Writers<'_> {
    fn connection(&mut self, gamepad: Entity, connection: GamepadConnection) {
        let event = GamepadConnectionEvent::new(gamepad, connection);
        self.raw.write(event.clone().into());
        self.connection.write(event);
    }

    fn button(&mut self, gamepad: Entity, button: GamepadButton, value: f32) {
        let event = RawGamepadButtonChangedEvent::new(gamepad, button, value);
        self.raw.write(event.into());
        self.button.write(event);
    }

    fn axis(&mut self, gamepad: Entity, axis: GamepadAxis, value: f32) {
        let event = RawGamepadAxisChangedEvent::new(gamepad, axis, value);
        self.raw.write(event.into());
        self.axis.write(event);
    }

    fn sdl_axis(&mut self, gamepad: Entity, axis: SDL_GamepadAxis, value: i16) {
        match convert::axis(axis, value) {
            Some(Input::Axis(axis, value)) => self.axis(gamepad, axis, value),
            Some(Input::Button(button, value)) => self.button(gamepad, button, value),
            None => {}
        }
    }
}

pub(crate) fn poll(
    _: NonSendMarker,
    mut commands: Commands,
    mut gamepads: ResMut<Gamepads>,
    mut out: Writers,
    mut ext: Extensions,
    default_motion: Res<DefaultMotionConfig>,
) {
    for mut motion in &mut ext.motions {
        if motion.rotation_delta != Vec3::ZERO {
            motion.rotation_delta = Vec3::ZERO;
        }
    }
    for mut sensors in &mut ext.touch_sensors {
        sensors.bypass_change_detection().touched.clear();
    }
    sdl::with(|sdl| {
        let mut event = SDL_Event::default();
        // SAFETY: SDL is initialised and this runs on the main thread.
        while unsafe { SDL_PollEvent(&mut event) } {
            // SAFETY: each union field is read only for the event types that use it.
            unsafe {
                match SDL_EventType(event.r#type) {
                    SDL_EVENT_GAMEPAD_ADDED => {
                        let e = event.gdevice;
                        let timestamp = Duration::from_nanos(e.timestamp);
                        let config = default_motion.0;
                        connect(
                            sdl,
                            e.which,
                            timestamp,
                            config,
                            &mut commands,
                            &mut gamepads,
                            &mut out,
                        );
                    }
                    SDL_EVENT_GAMEPAD_REMOVED => {
                        disconnect(
                            sdl,
                            event.gdevice.which,
                            &mut commands,
                            &mut gamepads,
                            &mut out,
                        );
                    }
                    SDL_EVENT_GAMEPAD_BUTTON_DOWN | SDL_EVENT_GAMEPAD_BUTTON_UP => {
                        let e = event.gbutton;
                        let Some(&(entity, _)) = gamepads.entities.get(&e.which.0) else {
                            continue;
                        };
                        let button = SDL_GamepadButton(e.button.into());
                        let touch_sensor =
                            gamepads.touch_buttons.get(&e.which.0).and_then(|buttons| {
                                buttons
                                    .iter()
                                    .find(|&&(b, _)| b == button)
                                    .map(|&(_, sensor)| sensor)
                            });
                        match touch_sensor {
                            Some(sensor) => {
                                if let Ok(mut sensors) = ext.touch_sensors.get_mut(entity) {
                                    if e.down {
                                        sensors.touched.press(sensor);
                                    } else {
                                        sensors.touched.release(sensor);
                                    }
                                }
                            }
                            None => {
                                let value = if e.down { 1.0 } else { 0.0 };
                                out.button(entity, convert::button(button), value);
                            }
                        }
                    }
                    SDL_EVENT_GAMEPAD_AXIS_MOTION => {
                        let e = event.gaxis;
                        if let Some(&(entity, _)) = gamepads.entities.get(&e.which.0) {
                            out.sdl_axis(entity, SDL_GamepadAxis(e.axis.into()), e.value);
                        }
                    }
                    kind @ (SDL_EVENT_GAMEPAD_TOUCHPAD_DOWN
                    | SDL_EVENT_GAMEPAD_TOUCHPAD_MOTION
                    | SDL_EVENT_GAMEPAD_TOUCHPAD_UP) => {
                        let e = event.gtouchpad;
                        let Some(&(entity, _)) = gamepads.entities.get(&e.which.0) else {
                            continue;
                        };
                        let pads = ext.touchpads.get_mut(entity).ok();
                        if let Some(event) = touchpad::event(entity, kind, &e, pads) {
                            out.touchpad.write(event);
                        }
                    }
                    SDL_EVENT_GAMEPAD_SENSOR_UPDATE => {
                        let e = event.gsensor;
                        let Some(&(entity, _)) = gamepads.entities.get(&e.which.0) else {
                            continue;
                        };
                        let mut last = gamepads.last_gyro.get(&e.which.0).copied();
                        let motion = ext.motions.get_mut(entity).ok();
                        if let Some(sample) = motion::sample(entity, &e, motion, &mut last) {
                            out.motion.write(sample);
                        }
                        if let Some(last) = last {
                            gamepads.last_gyro.insert(e.which.0, last);
                        }
                    }
                    _ => {}
                }
            }
        }
    });
}

fn connect(
    sdl: &mut Sdl,
    id: SDL_JoystickID,
    timestamp: Duration,
    default_motion: GamepadMotionConfig,
    commands: &mut Commands,
    gamepads: &mut Gamepads,
    out: &mut Writers,
) {
    if gamepads.entities.contains_key(&id.0) {
        return;
    }
    let Some(gamepad) = sdl.open(id) else { return };
    let (connection, key) = identify(id, gamepad);

    let entity = gamepads
        .disconnected
        .remove(&key)
        .filter(|&entity| commands.get_entity(entity).is_ok())
        .unwrap_or_else(|| commands.spawn_empty().id());
    let mut entity_commands = commands.entity(entity);
    entity_commands.insert(SdlGamepad { instance_id: id.0 });
    if let Some(pads) = touchpad::touchpads(gamepad) {
        let started = touchpad::started(entity, &pads, timestamp);
        out.touchpad.write_batch(started);
        entity_commands.insert(pads);
    }
    let touch_buttons = touch_sensor::touch_buttons(gamepad);
    if let Some(sensors) = touch_sensor::touch_sensors(gamepad, &touch_buttons) {
        entity_commands.insert(sensors);
    }
    if let Some(sensors) = motion::sensors(gamepad) {
        entity_commands.insert((sensors, GamepadMotion::default()));
        if default_motion != GamepadMotionConfig::default() {
            entity_commands.insert_if_new(default_motion);
        }
    }
    gamepads.entities.insert(id.0, (entity, key));
    gamepads.ids.insert(entity, id.0);
    out.connection(entity, connection);

    // SDL only reports changes, so report what is already held or deflected.
    let gamepad = gamepad.as_ptr();
    let buttons = (0..SDL_GAMEPAD_BUTTON_COUNT.0).map(SDL_GamepadButton);
    for button in buttons.filter(|b| !touch_buttons.iter().any(|(t, _)| t == b)) {
        // SAFETY: `gamepad` is open.
        if unsafe { SDL_GetGamepadButton(gamepad, button) } {
            out.button(entity, convert::button(button), 1.0);
        }
    }
    gamepads.touch_buttons.insert(id.0, touch_buttons);
    for axis in (0..SDL_GAMEPAD_AXIS_COUNT.0).map(SDL_GamepadAxis) {
        // SAFETY: `gamepad` is open.
        let value = unsafe { SDL_GetGamepadAxis(gamepad, axis) };
        if value != 0 {
            out.sdl_axis(entity, axis, value);
        }
    }
}

fn identify(id: SDL_JoystickID, gamepad: NonNull<SDL_Gamepad>) -> (GamepadConnection, DeviceKey) {
    let gamepad = gamepad.as_ptr();
    let nonzero = |value: u16| (value != 0).then_some(value);
    // SAFETY: `gamepad` is open and `id` is its instance ID.
    unsafe {
        let connection = GamepadConnection::Connected {
            name: sdl::string(SDL_GetGamepadName(gamepad)).unwrap_or_default(),
            vendor_id: nonzero(SDL_GetGamepadVendor(gamepad)),
            product_id: nonzero(SDL_GetGamepadProduct(gamepad)),
        };
        let key = DeviceKey {
            guid: SDL_GetGamepadGUIDForID(id).data,
            serial_or_path: sdl::string(SDL_GetGamepadSerial(gamepad))
                .filter(|serial| !serial.is_empty())
                .or_else(|| sdl::string(SDL_GetGamepadPath(gamepad))),
        };
        (connection, key)
    }
}

fn disconnect(
    sdl: &mut Sdl,
    id: SDL_JoystickID,
    commands: &mut Commands,
    gamepads: &mut Gamepads,
    out: &mut Writers,
) {
    sdl.close(id);
    let Some((entity, key)) = gamepads.entities.remove(&id.0) else {
        return;
    };
    gamepads.ids.remove(&entity);
    gamepads.last_gyro.remove(&id.0);
    gamepads.touch_buttons.remove(&id.0);
    gamepads.disconnected.insert(key, entity);
    if let Ok(mut entity) = commands.get_entity(entity) {
        entity.remove::<(
            SdlGamepad,
            GamepadTouchpads,
            GamepadMotionSensors,
            GamepadMotion,
            GamepadTouchSensors,
        )>();
    }
    out.connection(entity, GamepadConnection::Disconnected);
}
