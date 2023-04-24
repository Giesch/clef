pub mod player;
pub use player::track_info::{first_supported_track, TrackInfo};

#[cfg(not(target_os = "linux"))]
mod resampler;
