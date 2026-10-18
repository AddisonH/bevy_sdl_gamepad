use core::{
    ffi::{CStr, c_char},
    ptr::NonNull,
};
use std::{cell::RefCell, ffi::CString};

use bevy_app::AppExit;
use bevy_ecs::{message::MessageReader, system::NonSendMarker};
use bevy_log::{debug, warn};
use bevy_platform::collections::HashMap;
use sdl3_sys::{
    error::SDL_GetError,
    gamepad::{SDL_CloseGamepad, SDL_Gamepad, SDL_OpenGamepad},
    hints::SDL_SetHint,
    init::{SDL_INIT_GAMEPAD, SDL_InitSubSystem, SDL_QuitSubSystem},
    joystick::SDL_JoystickID,
};

pub(crate) const VALVE_VENDOR_ID: u16 = 0x28de;

thread_local! {
    // SDL's init and event functions must run on the main thread, so every system that reaches
    // this takes `NonSendMarker`.
    static SDL: RefCell<Option<Sdl>> = const { RefCell::new(None) };
}

/// The initialised SDL gamepad subsystem and the gamepads opened through it.
pub(crate) struct Sdl {
    gamepads: HashMap<u32, NonNull<SDL_Gamepad>>,
}

impl Sdl {
    pub(crate) fn open(&mut self, id: SDL_JoystickID) -> Option<NonNull<SDL_Gamepad>> {
        // SAFETY: SDL is initialised; an unknown ID makes SDL return null.
        let Some(gamepad) = NonNull::new(unsafe { SDL_OpenGamepad(id) }) else {
            warn!("Could not open SDL gamepad {}: {}", id.0, error());
            return None;
        };
        self.gamepads.insert(id.0, gamepad);
        Some(gamepad)
    }

    pub(crate) fn close(&mut self, id: SDL_JoystickID) {
        if let Some(gamepad) = self.gamepads.remove(&id.0) {
            // SAFETY: the handle was opened by `open` and is closed once.
            unsafe { SDL_CloseGamepad(gamepad.as_ptr()) };
        }
    }

    pub(crate) fn gamepad(&self, id: u32) -> Option<NonNull<SDL_Gamepad>> {
        self.gamepads.get(&id).copied()
    }
}

impl Drop for Sdl {
    fn drop(&mut self) {
        // Closing Valve controllers hands them back to their keyboard and mouse emulation.
        for gamepad in self.gamepads.drain().map(|(_, gamepad)| gamepad) {
            // SAFETY: each handle was opened by `open` and is closed once.
            unsafe { SDL_CloseGamepad(gamepad.as_ptr()) };
        }
        // SAFETY: balances the `SDL_InitSubSystem` in `init`.
        unsafe { SDL_QuitSubSystem(SDL_INIT_GAMEPAD) };
    }
}

/// Runs `f` with the SDL context, or does nothing if SDL failed to initialise.
pub(crate) fn with<R>(f: impl FnOnce(&mut Sdl) -> R) -> Option<R> {
    SDL.with_borrow_mut(|sdl| sdl.as_mut().map(f))
}

pub(crate) fn init(hints: &[(String, String)]) {
    for (name, value) in hints {
        let (Ok(name), Ok(value)) = (CString::new(name.as_str()), CString::new(value.as_str()))
        else {
            warn!("Ignoring SDL hint {name:?} containing a NUL byte");
            continue;
        };
        // SAFETY: both pointers are valid NUL-terminated strings for the duration of the call.
        unsafe { SDL_SetHint(name.as_ptr(), value.as_ptr()) };
    }
    // SAFETY: called on the main thread, before any other SDL call.
    if !unsafe { SDL_InitSubSystem(SDL_INIT_GAMEPAD) } {
        warn!("Could not initialise SDL gamepads: {}", error());
        return;
    }
    debug!("SDL gamepad subsystem initialised");
    SDL.set(Some(Sdl {
        gamepads: HashMap::default(),
    }));
}

pub(crate) fn shutdown_on_exit(_: NonSendMarker, mut exit: MessageReader<AppExit>) {
    if exit.read().next().is_some() {
        SDL.take();
    }
}

pub(crate) fn error() -> String {
    string(SDL_GetError()).unwrap_or_default()
}

/// Copies a string returned by SDL, which may be null.
pub(crate) fn string(ptr: *const c_char) -> Option<String> {
    // SAFETY: non-null strings from SDL are NUL-terminated.
    (!ptr.is_null()).then(|| {
        unsafe { CStr::from_ptr(ptr) }
            .to_string_lossy()
            .into_owned()
    })
}
