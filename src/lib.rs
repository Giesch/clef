#![warn(rust_2018_idioms)]
#![deny(missing_debug_implementations)]
#![forbid(unsafe_code)]

pub mod audio;
pub mod channels;
pub mod db;
pub mod ui;

#[cfg(test)]
pub mod test_util;
