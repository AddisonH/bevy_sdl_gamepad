use core::{ptr::NonNull, time::Duration};

use bevy_ecs::{
    entity::{Entity, EntityHashMap},
    message::MessageWriter,
    resource::Resource,
    system::{Commands, NonSendMarker, Query, Res, ResMut, SystemParam},
};
use bevy_input::gamepad::{
    GamepadAxis, GamepadButton, GamepadConnection, GamepadConnectionEvent,
    RawGamepadAxisChangedEvent, RawGamepadButtonChangedEvent, RawGamepadEvent,
};
use bevy_math::{Vec2, Vec3};
use bevy_platform::collections::HashMap;
use sdl3_sys::{events::*, gamepad::*, joystick::SDL_JoystickID};

use crate::{
    SdlGamepad,
    convert::{self, Input},
    ext::{
        GamepadMotion, GamepadMotionConfig, GamepadMotionSample, GamepadMotionSensors,
        GamepadTouchpads, Touchpad, TouchpadFinger,
    },
    motion::{self, DefaultMotionConfig},
    sdl::{self, Sdl},
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
}

#[derive(SystemParam)]
pub(crate) struct Writers<'w> {
    raw: MessageWriter<'w, RawGamepadEvent>,
    connection: MessageWriter<'w, GamepadConnectionEvent>,
    button: MessageWriter<'w, RawGamepadButtonChangedEvent>,
    axis: MessageWriter<'w, RawGamepadAxisChangedEvent>,
    motion: MessageWriter<'w, GamepadMotionSample>,
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
    mut touchpads: Query<&mut GamepadTouchpads>,
    mut motions: Query<&mut GamepadMotion>,
    default_motion: Res<DefaultMotionConfig>,
) {
    for mut motion in &mut motions {
        if motion.rotation_delta != Vec3::ZERO {
            motion.rotation_delta = Vec3::ZERO;
        }
    }
    sdl::with(|sdl| {
        let mut event = SDL_Event::default();
        // SAFETY: SDL is initialised and this runs on the main thread.
        while unsafe { SDL_PollEvent(&mut event) } {
            // SAFETY: each union field is read only for the event types that use it.
            unsafe {
                match SDL_EventType(event.r#type) {
                    SDL_EVENT_GAMEPAD_ADDED => {
                        let id = event.gdevice.which;
                        connect(sdl, id, default_motion.0, &mut commands, &mut gamepads, &mut out);
                    }
                    SDL_EVENT_GAMEPAD_REMOVED => {
                        disconnect(sdl, event.gdevice.which, &mut commands, &mut gamepads, &mut out);
                    }
                    SDL_EVENT_GAMEPAD_BUTTON_DOWN | SDL_EVENT_GAMEPAD_BUTTON_UP => {
                        let e = event.gbutton;
                        if let Some(&(entity, _)) = gamepads.entities.get(&e.which.0) {
                            let button = convert::button(SDL_GamepadButton(e.button.into()));
                            out.button(entity, button, if e.down { 1.0 } else { 0.0 });
                        }
                    }
                    SDL_EVENT_GAMEPAD_AXIS_MOTION => {
                        let e = event.gaxis;
                        if let Some(&(entity, _)) = gamepads.entities.get(&e.which.0) {
                            out.sdl_axis(entity, SDL_GamepadAxis(e.axis.into()), e.value);
                        }
                    }
                    t @ (SDL_EVENT_GAMEPAD_TOUCHPAD_DOWN
                    | SDL_EVENT_GAMEPAD_TOUCHPAD_MOTION
                    | SDL_EVENT_GAMEPAD_TOUCHPAD_UP) => {
                        let e = event.gtouchpad;
                        let finger = (t != SDL_EVENT_GAMEPAD_TOUCHPAD_UP).then(|| TouchpadFinger {
                            position: Vec2::new(e.x, e.y),
                            pressure: e.pressure,
                        });
                        let Some(&(entity, _)) = gamepads.entities.get(&e.which.0) else {
                            continue;
                        };
                        let Ok(mut pads) = touchpads.get_mut(entity) else {
                            continue;
                        };
                        let slot = pads
                            .0
                            .get_mut(e.touchpad as usize)
                            .and_then(|pad| pad.fingers.get_mut(e.finger as usize));
                        if let Some(slot) = slot {
                            *slot = finger;
                        }
                    }
                    SDL_EVENT_GAMEPAD_SENSOR_UPDATE => {
                        let e = event.gsensor;
                        let Some(&(entity, _)) = gamepads.entities.get(&e.which.0) else {
                            continue;
                        };
                        let mut last = gamepads.last_gyro.get(&e.which.0).copied();
                        let motion = motions.get_mut(entity).ok();
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
    entity_commands.insert(SdlGamepad {
        instance_id: id.0,
    });
    if let Some(touchpads) = touchpads(gamepad) {
        entity_commands.insert(touchpads);
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
    for button in (0..SDL_GAMEPAD_BUTTON_COUNT.0).map(SDL_GamepadButton) {
        // SAFETY: `gamepad` is open.
        if unsafe { SDL_GetGamepadButton(gamepad, button) } {
            out.button(entity, convert::button(button), 1.0);
        }
    }
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

/// The touchpad layout and any fingers already down, if the gamepad has touchpads.
fn touchpads(gamepad: NonNull<SDL_Gamepad>) -> Option<GamepadTouchpads> {
    let gamepad = gamepad.as_ptr();
    // SAFETY: `gamepad` is open; indices stay within the counts SDL reports.
    unsafe {
        let pads = (0..SDL_GetNumGamepadTouchpads(gamepad)).map(|pad| {
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
            }
        });
        let pads: Vec<_> = pads.collect();
        (!pads.is_empty()).then_some(GamepadTouchpads(pads))
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
    gamepads.disconnected.insert(key, entity);
    if let Ok(mut entity) = commands.get_entity(entity) {
        entity.remove::<(SdlGamepad, GamepadTouchpads, GamepadMotionSensors, GamepadMotion)>();
    }
    out.connection(entity, GamepadConnection::Disconnected);
}
