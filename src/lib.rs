#![warn(rust_2018_idioms)]
#![deny(missing_debug_implementations)]
#![forbid(unsafe_code)]

pub mod app;
pub mod audio;
pub mod db;
pub mod queue;

#[cfg(test)]
pub mod test_util;
