#![warn(rust_2018_idioms)]
#![deny(missing_debug_implementations)]
#![deny(unsafe_code)]

pub mod app;
pub mod audio;
pub mod db;
pub mod logging;
pub mod queue;

#[cfg(target_os = "windows")]
pub mod window_handle_hack;

#[cfg(test)]
pub mod test_util;
