//! A controller test screen laid out like a Steam Controller, showing the most recently used
//! gamepad.
//!
//! Keys 1–3 play test rumbles and 4 stops them. R recentres the gyro aim and G toggles the
//! motion sensors.

use core::time::Duration;

use bevy::{
    feathers::{
        FeathersPlugins,
        dark_theme::create_dark_theme,
        display::{label, label_dim},
        theme::{ThemeBackgroundColor, ThemeBorderColor, ThemeToken, UiTheme},
        tokens,
    },
    input::gamepad::{GamepadRumbleIntensity, GamepadRumbleRequest},
    prelude::*,
    window::{PrimaryWindow, WindowResized},
};
use bevy_sdl_gamepad::ext::{
    GamepadMotion, GamepadMotionConfig, GamepadMotionSample, GamepadTouchSensor,
    GamepadTouchSensors, GamepadTouchpadEvent, GamepadTouchpads, MotionSensor,
};
use bevy_sdl_gamepad::{SdlGamepad, SdlGamepadPlugin, button};

const WIDTH: f32 = 1120.0;
const HEIGHT: f32 = 700.0;
const STICK: f32 = 120.0;
const KNOB: f32 = 32.0;
const PAD: f32 = 220.0;
const FINGER: f32 = 18.0;
const AIM_WIDTH: f32 = 220.0;
const AIM_HEIGHT: f32 = 140.0;
const AIM_DOT: f32 = 14.0;
/// How far the aim dot can travel from the centre, in radians of controller rotation.
const AIM_RANGE: f32 = 0.6;

const IDLE: ThemeToken = tokens::BUTTON_BG;
const LIT: ThemeToken = tokens::BUTTON_PRIMARY_BG;

/// SDL's mapping for the Steam Controller puts its quick access button here.
const QAM: GamepadButton = button::MISC1;

/// Buttons with a place on screen, besides touchpad clicks; any others held are listed as text.
const SHOWN: &[GamepadButton] = &[
    GamepadButton::South,
    GamepadButton::East,
    GamepadButton::West,
    GamepadButton::North,
    GamepadButton::Select,
    GamepadButton::Mode,
    GamepadButton::Start,
    GamepadButton::LeftThumb,
    GamepadButton::RightThumb,
    GamepadButton::LeftTrigger,
    GamepadButton::RightTrigger,
    GamepadButton::LeftTrigger2,
    GamepadButton::RightTrigger2,
    GamepadButton::DPadUp,
    GamepadButton::DPadDown,
    GamepadButton::DPadLeft,
    GamepadButton::DPadRight,
    QAM,
    button::LEFT_PADDLE1,
    button::LEFT_PADDLE2,
    button::RIGHT_PADDLE1,
    button::RIGHT_PADDLE2,
];

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Controller test".into(),
                    resolution: (WIDTH as u32, HEIGHT as u32).into(),
                    ..default()
                }),
                ..default()
            }),
            FeathersPlugins,
            SdlGamepadPlugin {
                motion: GamepadMotionConfig {
                    gyro: true,
                    accel: true,
                },
                ..default()
            },
        ))
        .insert_resource(UiTheme(create_dark_theme()))
        .init_resource::<ActivePad>()
        .init_resource::<GyroAim>()
        .init_resource::<SampleRates>()
        .add_systems(Startup, scene.spawn())
        .add_systems(
            Update,
            (
                pick_active_pad,
                (
                    show_header,
                    show_lights,
                    show_triggers,
                    show_sticks,
                    show_touchpads,
                    show_other,
                    (count_events, show_motion).chain(),
                    rumble,
                    toggle_motion,
                    scale_to_window,
                ),
            )
                .chain(),
        )
        .run();
}

/// The gamepad being shown: the last one with a button press.
#[derive(Resource, Default)]
struct ActivePad(Option<Entity>);

/// Lights up part of a node while a button is held.
#[derive(Component, Clone)]
struct Light {
    source: LightSource,
    border: bool,
    lit: bool,
}

#[derive(Clone, Copy)]
enum LightSource {
    Button(GamepadButton),
    /// The click button of this touchpad, whichever button that is on the gamepad.
    PadClick(usize),
    TouchSensor(GamepadTouchSensor),
}

impl Light {
    fn background(button: GamepadButton) -> Self {
        Self {
            source: LightSource::Button(button),
            border: false,
            lit: false,
        }
    }

    fn border(button: GamepadButton) -> Self {
        Self {
            border: true,
            ..Self::background(button)
        }
    }

    fn border_from(source: LightSource) -> Self {
        Self {
            source,
            border: true,
            lit: false,
        }
    }
}

#[derive(Component, Clone)]
struct TriggerFill(GamepadButton);

#[derive(Component, Clone)]
struct TriggerText(&'static str, GamepadButton);

#[derive(Component, Clone)]
struct StickKnob(GamepadAxis, GamepadAxis);

#[derive(Component, Clone)]
struct StickText(GamepadAxis, GamepadAxis);

#[derive(Component, Clone)]
struct FingerDot {
    pad: usize,
    finger: usize,
}

#[derive(Component, Clone)]
struct PadText(&'static str, usize);

#[derive(Component, Default, Clone)]
struct HeaderText;

#[derive(Component, Default, Clone)]
struct OtherText;

#[derive(Component, Default, Clone)]
struct AimDot;

#[derive(Component, Default, Clone)]
struct MotionText;

/// Where the controller points, in radians of yaw (x) and pitch (y) from where it was recentred.
#[derive(Resource, Default)]
struct GyroAim(Vec2);

/// Events counted over the current second, and the counts from the last full one.
#[derive(Resource, Default)]
struct SampleRates {
    counting: EventCounts,
    shown: EventCounts,
    second: f32,
}

#[derive(Default, Clone, Copy)]
struct EventCounts {
    gyro: u32,
    accel: u32,
    touch: [u32; 2],
}

/// Mirrors a left-side x position onto the right side.
fn mirror(x: f32, width: f32) -> f32 {
    WIDTH - x - width
}

fn place(x: f32, y: f32, width: f32, height: f32) -> impl Scene {
    bsn! {
        Node {
            position_type: PositionType::Absolute,
            left: px(x),
            top: px(y),
            width: px(width),
            height: px(height),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
    }
}

fn pill(text: &'static str, button: GamepadButton, x: f32, y: f32, w: f32, h: f32) -> impl Scene {
    bsn! {
        @place(x, y, w, h)
        Node { border_radius: BorderRadius::all(px(w.min(h) / 2.0)) }
        ThemeBackgroundColor(IDLE)
        ~{ Light::background(button) }
        Children [ @label(text) ]
    }
}

/// A diamond of four buttons, as on a D-pad or face buttons.
fn cluster(x: f32, y: f32, buttons: [(&'static str, GamepadButton); 4]) -> impl Scene {
    let [up, down, left, right] = buttons;
    bsn! {
        @place(x, y, 132.0, 132.0)
        Children [
            @pill(up.0, up.1, 44.0, 0.0, 44.0, 44.0)
            --
            @pill(down.0, down.1, 44.0, 88.0, 44.0, 44.0)
            --
            @pill(left.0, left.1, 0.0, 44.0, 44.0, 44.0)
            --
            @pill(right.0, right.1, 88.0, 44.0, 44.0, 44.0)
        ]
    }
}

fn trigger(text: &'static str, button: GamepadButton, x: f32, y: f32) -> impl Scene {
    bsn! {
        @place(x, y, 200.0, 26.0)
        Node {
            justify_content: JustifyContent::Start,
            border: UiRect::all(px(2)),
            border_radius: BorderRadius::all(px(6)),
            overflow: Overflow::clip(),
        }
        ThemeBackgroundColor(tokens::SLIDER_BG)
        ThemeBorderColor(IDLE)
        ~{ Light::border(button) }
        Children [
            Node { width: percent(0), height: percent(100) }
            ThemeBackgroundColor(tokens::SLIDER_BAR)
            ~{ TriggerFill(button) }
            --
            Node { position_type: PositionType::Absolute, left: px(8) }
            Children [ @label(text) ~{ TriggerText(text, button) } ]
        ]
    }
}

fn stick(
    x: f32,
    y: f32,
    axes: (GamepadAxis, GamepadAxis),
    click: GamepadButton,
    touch: GamepadTouchSensor,
) -> impl Scene {
    let offset = (STICK - KNOB) / 2.0;
    bsn! {
        @place(x, y, STICK, STICK)
        Node {
            border: UiRect::all(px(3)),
            border_radius: BorderRadius::all(px(STICK / 2.0)),
        }
        ThemeBackgroundColor(tokens::PANE_BODY_BG)
        ThemeBorderColor(IDLE)
        ~{ Light::border_from(LightSource::TouchSensor(touch)) }
        Children [
            @pill("", click, offset, offset, KNOB, KNOB)
            ~{ StickKnob(axes.0, axes.1) }
            --
            Node { position_type: PositionType::Absolute, top: px(STICK + 6.0) }
            Children [ @label_dim("") ~{ StickText(axes.0, axes.1) } ]
        ]
    }
}

fn trackpad(name: &'static str, pad: usize, x: f32, y: f32) -> impl Scene {
    let dot = |finger| {
        bsn! {
            Node {
                position_type: PositionType::Absolute,
                width: px(FINGER),
                height: px(FINGER),
                display: Display::None,
                border_radius: BorderRadius::all(px(FINGER / 2.0)),
            }
            ThemeBackgroundColor(LIT)
            ~{ FingerDot { pad, finger } }
        }
    };
    bsn! {
        @place(x, y, PAD, PAD)
        Node {
            border: UiRect::all(px(3)),
            border_radius: BorderRadius::all(px(18)),
        }
        ThemeBackgroundColor(tokens::PANE_BODY_BG)
        ThemeBorderColor(IDLE)
        ~{ Light::border_from(LightSource::PadClick(pad)) }
        Children [
            @{ dot(0) }
            --
            @{ dot(1) }
            --
            Node { position_type: PositionType::Absolute, top: px(PAD + 6.0) }
            Children [ @label_dim(name) ~{ PadText(name, pad) } ]
        ]
    }
}

/// A back paddle and the grip touch sensor beside it.
fn grip(
    paddles: [(&'static str, GamepadButton); 2],
    touch: GamepadTouchSensor,
    x: f32,
) -> impl Scene {
    bsn! {
        @place(x, 330.0, 110.0, 150.0)
        Node {
            flex_direction: FlexDirection::Column,
            row_gap: px(8),
            border: UiRect::all(px(3)),
            border_radius: BorderRadius::all(px(14)),
            padding: UiRect::all(px(8)),
        }
        ThemeBorderColor(IDLE)
        ~{ Light::border_from(LightSource::TouchSensor(touch)) }
        Children [
            @pill(paddles[0].0, paddles[0].1, 0.0, 0.0, 86.0, 32.0)
            Node { position_type: PositionType::Relative }
            --
            @pill(paddles[1].0, paddles[1].1, 0.0, 0.0, 86.0, 32.0)
            Node { position_type: PositionType::Relative }
            --
            @label_dim("grip")
        ]
    }
}

fn gyro_panel(x: f32, y: f32) -> impl Scene {
    let dot_x = (AIM_WIDTH - AIM_DOT) / 2.0 - 3.0;
    let dot_y = (AIM_HEIGHT - AIM_DOT) / 2.0 - 3.0;
    bsn! {
        @place(x, y, AIM_WIDTH, AIM_HEIGHT + 70.0)
        Node { flex_direction: FlexDirection::Column, justify_content: JustifyContent::Start, row_gap: px(6) }
        Children [
            Node {
                width: px(AIM_WIDTH),
                height: px(AIM_HEIGHT),
                border: UiRect::all(px(3)),
                border_radius: BorderRadius::all(px(12)),
            }
            ThemeBackgroundColor(tokens::PANE_BODY_BG)
            ThemeBorderColor(IDLE)
            Children [
                @place(0.0, (AIM_HEIGHT - 6.0) / 2.0 - 0.5, AIM_WIDTH - 6.0, 1.0)
                ThemeBackgroundColor(IDLE)
                --
                @place((AIM_WIDTH - 6.0) / 2.0 - 0.5, 0.0, 1.0, AIM_HEIGHT - 6.0)
                ThemeBackgroundColor(IDLE)
                --
                @place(dot_x, dot_y, AIM_DOT, AIM_DOT)
                Node { border_radius: BorderRadius::all(px(AIM_DOT / 2.0)) }
                ThemeBackgroundColor(LIT)
                AimDot
                --
                Node { position_type: PositionType::Absolute, left: px(8), top: px(4) }
                Children [ @label_dim("Gyro aim") ]
            ]
            --
            @label_dim("") MotionText
        ]
    }
}

fn scene() -> impl SceneList {
    bsn_list! {
        Camera2d
        --
        @root()
    }
}

fn root() -> impl Scene {
    use GamepadAxis::*;
    use GamepadButton::*;
    bsn! {
        Node {
            width: percent(100),
            height: percent(100),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
        }
        ThemeBackgroundColor(tokens::WINDOW_BG)
        Children [
        Node { width: px(WIDTH), height: px(HEIGHT) }
        Children [
            Node { position_type: PositionType::Absolute, left: px(16), top: px(10) }
            Children [ @label("") HeaderText ]
            --
            @trigger("LT", LeftTrigger2, 70.0, 44.0)
            --
            @trigger("RT", RightTrigger2, mirror(70.0, 200.0), 44.0)
            --
            @pill("LB", LeftTrigger, 70.0, 80.0, 200.0, 30.0)
            --
            @pill("RB", RightTrigger, mirror(70.0, 200.0), 80.0, 200.0, 30.0)
            --
            @cluster(100.0, 140.0, [("↑", DPadUp), ("↓", DPadDown), ("←", DPadLeft), ("→", DPadRight)])
            --
            @cluster(mirror(100.0, 132.0), 140.0, [("Y", North), ("A", South), ("X", West), ("B", East)])
            --
            @stick(290.0, 146.0, (LeftStickX, LeftStickY), LeftThumb, GamepadTouchSensor::LeftStick)
            --
            @stick(mirror(290.0, STICK), 146.0, (RightStickX, RightStickY), RightThumb, GamepadTouchSensor::RightStick)
            --
            @pill("View", Select, 452.0, 150.0, 80.0, 30.0)
            --
            @pill("Menu", Start, mirror(452.0, 80.0), 150.0, 80.0, 30.0)
            --
            @pill("Steam", Mode, mirror(0.0, 90.0) / 2.0, 200.0, 90.0, 40.0)
            --
            @pill("…", QAM, mirror(0.0, 60.0) / 2.0, 256.0, 60.0, 30.0)
            --
            @trackpad("Left pad · 0", 0, 220.0, 330.0)
            --
            @trackpad("Right pad · 1", 1, mirror(220.0, PAD), 330.0)
            --
            @gyro_panel(mirror(0.0, AIM_WIDTH) / 2.0, 330.0)
            --
            @grip([("L4", button::LEFT_PADDLE1), ("L5", button::LEFT_PADDLE2)], GamepadTouchSensor::LeftGrip, 60.0)
            --
            @grip([("R4", button::RIGHT_PADDLE1), ("R5", button::RIGHT_PADDLE2)], GamepadTouchSensor::RightGrip, mirror(60.0, 110.0))
            --
            Node {
                position_type: PositionType::Absolute,
                left: px(16),
                bottom: px(12),
                flex_direction: FlexDirection::Column,
                row_gap: px(4),
            }
            Children [
                @label("") OtherText
                --
                @label_dim("Rumble: 1 strong 0.5 s · 2 weak 0.5 s · 3 both stacked, 2 s · 4 stop · R recentre aim · G motion on/off")
            ]
        ]
        ]
    }
}

fn pick_active_pad(mut active: ResMut<ActivePad>, gamepads: Query<(Entity, &Gamepad)>) {
    if let Some((entity, _)) = gamepads
        .iter()
        .find(|(_, gamepad)| gamepad.get_just_pressed().next().is_some())
    {
        active.0 = Some(entity);
    }
    if active.0.is_none_or(|entity| !gamepads.contains(entity)) {
        active.0 = gamepads.iter().next().map(|(entity, _)| entity);
    }
}

fn active<'a, D: bevy::ecs::query::QueryData>(
    active: &ActivePad,
    query: &'a Query<D>,
) -> Option<bevy::ecs::query::ROQueryItem<'a, 'a, D>> {
    active.0.and_then(|entity| query.get(entity).ok())
}

fn show_header(
    active: Res<ActivePad>,
    gamepads: Query<(Entity, &Gamepad, &Name, Option<&SdlGamepad>)>,
    mut header: Single<&mut Text, With<HeaderText>>,
) {
    let id = |v: Option<u16>| v.map_or("----".into(), |v| format!("{v:04x}"));
    let lines: Vec<_> = gamepads
        .iter()
        .map(|(entity, gamepad, name, sdl)| {
            let marker = if active.0 == Some(entity) {
                "→"
            } else {
                "   "
            };
            let sdl = sdl.map_or(String::new(), |sdl| {
                format!(" · SDL #{}", sdl.instance_id())
            });
            format!(
                "{marker} {name} · {}:{} · {entity}{sdl}",
                id(gamepad.vendor_id()),
                id(gamepad.product_id())
            )
        })
        .collect();
    header.0 = if lines.is_empty() {
        "Connect a controller".into()
    } else {
        lines.join("\n")
    };
}

fn show_lights(
    mut commands: Commands,
    active_pad: Res<ActivePad>,
    gamepads: Query<(
        &Gamepad,
        Option<&GamepadTouchpads>,
        Option<&GamepadTouchSensors>,
    )>,
    mut lights: Query<(Entity, &mut Light)>,
) {
    let gamepad = active(&active_pad, &gamepads);
    for (entity, mut light) in &mut lights {
        let lit = gamepad.is_some_and(|(gamepad, pads, sensors)| {
            let button = match light.source {
                LightSource::Button(button) => Some(button),
                LightSource::PadClick(pad) => pads.and_then(|pads| pads.0.get(pad)?.click),
                LightSource::TouchSensor(sensor) => {
                    return sensors.is_some_and(|sensors| sensors.touched.pressed(sensor));
                }
            };
            button.is_some_and(|button| gamepad.pressed(button))
        });
        if lit == light.lit {
            continue;
        }
        light.lit = lit;
        let token = if lit { LIT } else { IDLE };
        if light.border {
            commands.entity(entity).insert(ThemeBorderColor(token));
        } else {
            commands.entity(entity).insert(ThemeBackgroundColor(token));
        }
    }
}

fn show_triggers(
    active_pad: Res<ActivePad>,
    gamepads: Query<&Gamepad>,
    mut fills: Query<(&TriggerFill, &mut Node)>,
    mut texts: Query<(&TriggerText, &mut Text)>,
) {
    let gamepad = active(&active_pad, &gamepads);
    let value = |button| {
        gamepad
            .and_then(|gamepad| gamepad.get(button))
            .unwrap_or(0.0)
    };
    for (fill, mut node) in &mut fills {
        node.width = percent(value(fill.0) * 100.0);
    }
    for (TriggerText(name, button), mut text) in &mut texts {
        text.0 = format!("{name} {:.3}", value(*button));
    }
}

fn show_sticks(
    active_pad: Res<ActivePad>,
    gamepads: Query<&Gamepad>,
    mut knobs: Query<(&StickKnob, &mut Node)>,
    mut texts: Query<(&StickText, &mut Text)>,
) {
    let gamepad = active(&active_pad, &gamepads);
    let value = |axis| gamepad.and_then(|gamepad| gamepad.get(axis)).unwrap_or(0.0);
    let travel = (STICK - KNOB) / 2.0 - 3.0;
    for (StickKnob(x, y), mut node) in &mut knobs {
        node.left = px(travel + value(*x) * travel);
        node.top = px(travel - value(*y) * travel);
    }
    for (StickText(x, y), mut text) in &mut texts {
        text.0 = format!("{:+.3}, {:+.3}", value(*x), value(*y));
    }
}

fn show_touchpads(
    active_pad: Res<ActivePad>,
    rates: Res<SampleRates>,
    touchpads: Query<&GamepadTouchpads>,
    mut dots: Query<(&FingerDot, &mut Node)>,
    mut texts: Query<(&PadText, &mut Text)>,
) {
    let pads = active(&active_pad, &touchpads);
    let finger = |pad: usize, finger: usize| {
        pads.and_then(|pads| pads.0.get(pad))
            .and_then(|pad| pad.fingers.get(finger).copied().flatten())
    };
    let travel = PAD - 6.0 - FINGER;
    for (dot, mut node) in &mut dots {
        match finger(dot.pad, dot.finger) {
            Some(finger) => {
                node.display = Display::Flex;
                node.left = px(finger.position.x * travel);
                node.top = px(finger.position.y * travel);
            }
            None => node.display = Display::None,
        }
    }
    for (PadText(name, pad), mut text) in &mut texts {
        let rate = rates.shown.touch[*pad];
        text.0 = match finger(*pad, 0) {
            Some(f) => format!(
                "{name} · {:.2}, {:.2} · {rate} events/s",
                f.position.x, f.position.y
            ),
            None => (*name).into(),
        };
    }
}

fn show_other(
    active_pad: Res<ActivePad>,
    gamepads: Query<(&Gamepad, Option<&GamepadTouchpads>)>,
    mut text: Single<&mut Text, With<OtherText>>,
) {
    let Some((gamepad, pads)) = active(&active_pad, &gamepads) else {
        text.0 = String::new();
        return;
    };
    let pad_clicks: Vec<_> = pads
        .iter()
        .flat_map(|pads| &pads.0)
        .filter_map(|pad| pad.click)
        .collect();
    let others: Vec<_> = gamepad
        .get_pressed()
        .filter(|button| !SHOWN.contains(button) && !pad_clicks.contains(button))
        .map(|button| format!("{button:?}"))
        .collect();
    text.0 = if others.is_empty() {
        String::new()
    } else {
        format!("Other buttons held: {}", others.join(", "))
    };
}

fn rumble(
    active_pad: Res<ActivePad>,
    keys: Res<ButtonInput<KeyCode>>,
    mut requests: MessageWriter<GamepadRumbleRequest>,
) {
    let Some(gamepad) = active_pad.0 else { return };
    let add = |intensity, seconds| GamepadRumbleRequest::Add {
        gamepad,
        intensity,
        duration: Duration::from_secs_f32(seconds),
    };
    if keys.just_pressed(KeyCode::Digit1) {
        requests.write(add(GamepadRumbleIntensity::STRONG_MAX, 0.5));
    }
    if keys.just_pressed(KeyCode::Digit2) {
        requests.write(add(GamepadRumbleIntensity::WEAK_MAX, 0.5));
    }
    if keys.just_pressed(KeyCode::Digit3) {
        requests.write(add(GamepadRumbleIntensity::strong_motor(0.5), 2.0));
        requests.write(add(GamepadRumbleIntensity::weak_motor(0.5), 1.0));
    }
    if keys.just_pressed(KeyCode::Digit4) {
        requests.write(GamepadRumbleRequest::Stop { gamepad });
    }
}

fn count_events(
    time: Res<Time>,
    active_pad: Res<ActivePad>,
    mut samples: MessageReader<GamepadMotionSample>,
    mut touches: MessageReader<GamepadTouchpadEvent>,
    mut rates: ResMut<SampleRates>,
) {
    for sample in samples.read().filter(|s| Some(s.gamepad) == active_pad.0) {
        match sample.sensor {
            MotionSensor::Gyro => rates.counting.gyro += 1,
            MotionSensor::Accel => rates.counting.accel += 1,
        }
    }
    for touch in touches.read().filter(|t| Some(t.gamepad) == active_pad.0) {
        if let Some(count) = rates.counting.touch.get_mut(touch.touchpad) {
            *count += 1;
        }
    }
    rates.second += time.delta_secs();
    if rates.second >= 1.0 {
        rates.second -= 1.0;
        rates.shown = core::mem::take(&mut rates.counting);
    }
}

fn show_motion(
    active_pad: Res<ActivePad>,
    motions: Query<(&GamepadMotion, Option<&GamepadMotionConfig>)>,
    keys: Res<ButtonInput<KeyCode>>,
    rates: Res<SampleRates>,
    mut aim: ResMut<GyroAim>,
    mut dot: Single<&mut Node, With<AimDot>>,
    mut text: Single<&mut Text, With<MotionText>>,
) {
    let Some((motion, config)) = active(&active_pad, &motions) else {
        text.0 = "No motion sensors".into();
        return;
    };
    // Turning left is positive rotation around Y; tilting the top towards you is positive
    // rotation around X.
    aim.0 += Vec2::new(-motion.rotation_delta.y, motion.rotation_delta.x);
    aim.0 = aim.0.clamp(Vec2::splat(-AIM_RANGE), Vec2::splat(AIM_RANGE));
    if keys.just_pressed(KeyCode::KeyR) {
        aim.0 = Vec2::ZERO;
    }
    let travel = Vec2::new(AIM_WIDTH - AIM_DOT, AIM_HEIGHT - AIM_DOT) / 2.0 - 3.0;
    let at = travel + aim.0 / AIM_RANGE * Vec2::new(travel.x, -travel.y);
    dot.left = px(at.x);
    dot.top = px(at.y);

    let on = config.is_some_and(|config| config.gyro || config.accel);
    let [gx, gy, gz] = motion.angular_velocity.to_array();
    let [ax, ay, az] = motion.acceleration.to_array();
    text.0 = format!(
        "gyro {gx:+.2} {gy:+.2} {gz:+.2} rad/s\naccel {ax:+.1} {ay:+.1} {az:+.1} m/s²\n{} · {} gyro, {} accel samples/s",
        if on { "sensors on" } else { "sensors off" },
        rates.shown.gyro,
        rates.shown.accel,
    );
}

fn toggle_motion(
    mut commands: Commands,
    active_pad: Res<ActivePad>,
    keys: Res<ButtonInput<KeyCode>>,
    configs: Query<Option<&GamepadMotionConfig>, With<GamepadMotion>>,
) {
    if !keys.just_pressed(KeyCode::KeyG) {
        return;
    }
    let Some(entity) = active_pad.0 else { return };
    let Ok(config) = configs.get(entity) else {
        return;
    };
    let on = !config.is_some_and(|config| config.gyro);
    commands.entity(entity).insert(GamepadMotionConfig {
        gyro: on,
        accel: on,
    });
}

/// Scales the whole screen to fit the window, keeping its proportions.
fn scale_to_window(
    mut resized: MessageReader<WindowResized>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut scale: ResMut<UiScale>,
) {
    if resized.read().last().is_some() {
        scale.0 = (window.width() / WIDTH).min(window.height() / HEIGHT);
    }
}
