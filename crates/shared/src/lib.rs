#![warn(rust_2018_idioms)]
#![deny(missing_debug_implementations)]
#![deny(unsafe_code)]

/// NOTE This is used for looking up the window handle on windows.
pub const WINDOW_TITLE: &str = "Clef";

pub mod queue;

#[cfg(target_os = "windows")]
pub mod window_handle_hack;
