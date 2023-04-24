#![warn(rust_2018_idioms)]
#![deny(missing_debug_implementations)]
#![forbid(unsafe_code)]

pub mod metadata;
pub mod player;
pub mod track_info;

#[cfg(not(target_os = "linux"))]
mod resampler;
