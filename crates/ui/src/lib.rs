#![warn(rust_2018_idioms)]
#![deny(missing_debug_implementations)]
#![forbid(unsafe_code)]

pub mod app;
pub mod icon;
pub mod setup;

pub use app::Config;
pub use app::Flags;
