# bevy_sdl_gamepad

An [SDL3](https://libsdl.org) gamepad backend for [Bevy](https://bevy.org), replacing
`bevy_gilrs`.

SDL supports far more controllers than gilrs, with drivers for PlayStation, Xbox, Switch and
Steam hardware. Gamepads still appear through Bevy's usual `Gamepad` component and rumble
requests, so existing code and input crates keep working. On top of that, the `ext` module
provides what `bevy_input` has no types for yet:

- gyroscope and accelerometer, with every sample and a per-frame rotation for gyro aiming
- touchpads, with every touch and each pad's click button
- touch sensors on sticks and grips
- paddles and other extra buttons, as `GamepadButton::Other` constants in `button`

**Status: experimental.** The `ext` API will change. Tested on Linux with the second-generation
Steam Controller; Windows and macOS are untested. It does nothing on the web, where `bevy_gilrs`
is still the backend to use.

| bevy_sdl_gamepad | Bevy | SDL  |
| ---------------- | ---- | ---- |
| 0.1              | 0.20 | 3.4  |

## Usage

```toml
[dependencies]
bevy_sdl_gamepad = "0.1"
```

Bevy's own gamepad backend must be off, or every controller appears twice:

```rust
use bevy::{gilrs::GilrsPlugin, prelude::*};
use bevy_sdl_gamepad::SdlGamepadPlugin;

fn main() {
    App::new()
        .add_plugins((
            DefaultPlugins.build().disable::<GilrsPlugin>(),
            SdlGamepadPlugin::default(),
        ))
        .run();
}
```

Turning off Bevy's `bevy_gilrs` feature works too. The plugin logs a warning if another backend
connects a gamepad.

Motion sensors are off until asked for, because they cost bandwidth and battery. Insert
`ext::GamepadMotionConfig` on a gamepad entity, or set `SdlGamepadPlugin::motion` to turn them on
for every gamepad.

The `controller_test` example shows every input of a connected gamepad:

```sh
cargo run --example controller_test
```

## Building

By default SDL is built from source and linked statically, which needs CMake and a C compiler.
To link the SDL 3 library installed on the system instead, found with pkg-config, turn off
default features:

```toml
bevy_sdl_gamepad = { version = "0.1", default-features = false }
```

## Platform notes

**Linux:** SDL talks to many controllers directly through `/dev/hidraw*`, which needs udev rules
granting access. Steam installs them for Valve controllers; distributions usually package them
as `steam-devices`.

**Steam:** while Steam is running with Steam Input on for a game, it takes over the controller
and the game sees a virtual Xbox controller without gyro or touchpads. Turn Steam Input off for
the game to get the controller itself. While the game has a Valve controller open, its keyboard
and mouse emulation is off.

## License

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or
[MIT license](LICENSE-MIT) at your option. SDL is under the
[zlib license](https://libsdl.org/license.php).
