use core::{ptr::NonNull, time::Duration};

use bevy_ecs::{
    entity::Entity,
    lifecycle::RemovedComponents,
    query::{Added, Changed, Or},
    resource::Resource,
    system::{NonSendMarker, Query},
    world::Mut,
};
use bevy_math::Vec3;
use sdl3_sys::{
    events::SDL_GamepadSensorEvent,
    gamepad::*,
    sensor::{SDL_SENSOR_ACCEL, SDL_SENSOR_GYRO, SDL_SensorType},
};

use crate::{
    SdlGamepad,
    ext::{GamepadMotion, GamepadMotionConfig, GamepadMotionSample, GamepadMotionSensors, MotionSensor},
    sdl,
};

/// A gap between gyro samples this long means samples were lost or the sensor was off, so the
/// rotation across it is unknown.
const MAX_SAMPLE_GAP: Duration = Duration::from_millis(100);

/// The config given to gamepads that connect without one.
#[derive(Resource, Clone, Copy)]
pub(crate) struct DefaultMotionConfig(pub(crate) GamepadMotionConfig);

pub(crate) fn sensors(gamepad: NonNull<SDL_Gamepad>) -> Option<GamepadMotionSensors> {
    let rate = |sensor| {
        // SAFETY: `gamepad` is open.
        unsafe { SDL_GamepadHasSensor(gamepad.as_ptr(), sensor) }
            .then(|| unsafe { SDL_GetGamepadSensorDataRate(gamepad.as_ptr(), sensor) })
    };
    let sensors = GamepadMotionSensors {
        gyro: rate(SDL_SENSOR_GYRO),
        accel: rate(SDL_SENSOR_ACCEL),
    };
    (sensors.gyro.is_some() || sensors.accel.is_some()).then_some(sensors)
}

/// Gamepads that just connected, or whose config changed.
type NeedsConfig = Or<(Added<SdlGamepad>, Changed<GamepadMotionConfig>)>;

pub(crate) fn apply_config(
    _: NonSendMarker,
    changed: Query<(&SdlGamepad, Option<&GamepadMotionConfig>), NeedsConfig>,
    mut removed: RemovedComponents<GamepadMotionConfig>,
    gamepads: Query<&SdlGamepad>,
) {
    sdl::with(|sdl| {
        let removed = removed.read().filter_map(|entity| gamepads.get(entity).ok());
        let configs = changed
            .iter()
            .map(|(pad, config)| (pad, config.copied().unwrap_or_default()))
            .chain(removed.map(|pad| (pad, GamepadMotionConfig::default())));
        for (pad, config) in configs {
            let Some(gamepad) = sdl.gamepad(pad.instance_id) else {
                continue;
            };
            for (sensor, enabled) in [(SDL_SENSOR_GYRO, config.gyro), (SDL_SENSOR_ACCEL, config.accel)] {
                // SAFETY: `gamepad` is open. SDL ignores sensors the gamepad does not have.
                unsafe { SDL_SetGamepadSensorEnabled(gamepad.as_ptr(), sensor, enabled) };
            }
        }
    });
}

/// Turns an SDL sensor event into a sample, and folds it into the gamepad's motion state.
pub(crate) fn sample(
    gamepad: Entity,
    event: &SDL_GamepadSensorEvent,
    motion: Option<Mut<GamepadMotion>>,
    last_gyro: &mut Option<Duration>,
) -> Option<GamepadMotionSample> {
    let sensor = match SDL_SensorType(event.sensor) {
        SDL_SENSOR_GYRO => MotionSensor::Gyro,
        SDL_SENSOR_ACCEL => MotionSensor::Accel,
        _ => return None,
    };
    let value = Vec3::from_array(event.data);
    // For gamepads without their own clock, SDL fills in when it received the reading.
    let timestamp = Duration::from_nanos(event.sensor_timestamp);

    if let Some(mut motion) = motion {
        match sensor {
            MotionSensor::Gyro => {
                motion.angular_velocity = value;
                let gap = last_gyro.and_then(|last| timestamp.checked_sub(last));
                if let Some(gap) = gap.filter(|gap| *gap <= MAX_SAMPLE_GAP) {
                    motion.rotation_delta += value * gap.as_secs_f32();
                }
                *last_gyro = Some(timestamp);
            }
            MotionSensor::Accel => motion.acceleration = value,
        }
    }
    Some(GamepadMotionSample {
        gamepad,
        sensor,
        value,
        timestamp,
    })
}
