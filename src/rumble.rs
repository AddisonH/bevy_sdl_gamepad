use core::time::Duration;

use bevy_ecs::{
    entity::EntityHashMap,
    message::MessageReader,
    resource::Resource,
    system::{NonSendMarker, Res, ResMut},
};
use bevy_input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest};
use bevy_time::{Real, Time};
use sdl3_sys::gamepad::SDL_RumbleGamepad;

use crate::{events::Gamepads, sdl};

/// The longest rumble SDL plays from one call.
const MAX_SDL_DURATION: Duration = Duration::from_millis(0xFFFF);
/// How long before SDL's rumble runs out to renew a longer one.
const RENEW_MARGIN: Duration = Duration::from_secs(1);

#[derive(Resource, Default)]
pub(crate) struct Rumbles {
    gamepads: EntityHashMap<GamepadRumble>,
}

#[derive(Default)]
struct GamepadRumble {
    /// Running rumbles and when each ends; `Add` requests stack.
    running: Vec<(Duration, GamepadRumbleIntensity)>,
    sent: Option<Sent>,
}

/// What SDL was last asked to play.
#[derive(Clone, Copy)]
struct Sent {
    /// A reconnected gamepad has a new instance ID and plays nothing yet.
    instance_id: u32,
    level: (u16, u16),
    until: Duration,
}

pub(crate) fn play(
    _: NonSendMarker,
    time: Res<Time<Real>>,
    mut requests: MessageReader<GamepadRumbleRequest>,
    gamepads: Res<Gamepads>,
    mut rumbles: ResMut<Rumbles>,
) {
    let now = time.elapsed();
    for request in requests.read() {
        let rumble = rumbles.gamepads.entry(request.gamepad()).or_default();
        match *request {
            GamepadRumbleRequest::Stop { .. } => rumble.running.clear(),
            GamepadRumbleRequest::Add {
                duration,
                intensity,
                ..
            } => rumble.running.push((now + duration, intensity)),
        }
    }

    sdl::with(|sdl| {
        rumbles.gamepads.retain(|entity, rumble| {
            rumble.running.retain(|&(end, _)| end > now);
            let open = gamepads
                .ids
                .get(entity)
                .and_then(|&id| Some((id, sdl.gamepad(id)?)));
            let Some((instance_id, gamepad)) = open else {
                return !rumble.running.is_empty();
            };
            let (strong, weak) = rumble.running.iter().fold((0.0, 0.0), |(s, w), (_, i)| {
                (s + i.strong_motor, w + i.weak_motor)
            });
            let level = (magnitude(strong), magnitude(weak));
            let until = rumble
                .running
                .iter()
                .map(|&(end, _)| end)
                .max()
                .unwrap_or(now);
            let sent = rumble
                .sent
                .filter(|sent| sent.instance_id == instance_id)
                .unwrap_or(Sent {
                    instance_id,
                    level: (0, 0),
                    until: now,
                });
            let renew = until > sent.until && sent.until < now + RENEW_MARGIN;
            if level != sent.level || renew {
                let duration = (until - now).min(MAX_SDL_DURATION);
                // SDL plays a non-zero level with a 0 ms duration forever.
                let ms = (duration.as_millis() as u32).max(1);
                // SAFETY: `gamepad` is open. A gamepad without rumble returns false.
                unsafe { SDL_RumbleGamepad(gamepad.as_ptr(), level.0, level.1, ms) };
                rumble.sent = Some(Sent {
                    instance_id,
                    level,
                    until: now + duration,
                });
            }
            !rumble.running.is_empty()
        });
    });
}

fn magnitude(intensity: f32) -> u16 {
    (intensity.clamp(0.0, 1.0) * f32::from(u16::MAX)) as u16
}
