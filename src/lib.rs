#![warn(rust_2018_idioms)]
#![warn(clippy::pedantic)]
#![deny(missing_debug_implementations)]
#![forbid(unsafe_code)]

pub mod app;
pub mod audio;
pub mod channels;
pub mod db;

#[cfg(test)]
pub mod test_util;
