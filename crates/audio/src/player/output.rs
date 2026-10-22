//! Platform-dependent audio output.

use symphonia::core::audio::{AudioSpec, GenericAudioBufferRef};

pub trait AudioOutput {
    fn write(&mut self, decoded: GenericAudioBufferRef<'_>) -> Result<()>;
    fn flush(&mut self);
}

#[allow(clippy::enum_variant_names)]
#[derive(Debug, thiserror::Error)]
pub enum AudioOutputError {
    #[error("cannot open audio stream")]
    OpenStreamError,
    #[cfg(not(target_os = "linux"))]
    #[error("cannot start audio stream")]
    PlayStreamError,
    #[cfg(not(target_os = "linux"))]
    #[error("unsupported audio output: {0}")]
    Unsupported(&'static str),
    #[error("audio stream closed")]
    StreamClosedError,
}

pub type Result<T> = std::result::Result<T, AudioOutputError>;

#[cfg(not(target_os = "linux"))]
#[path = "output/cpal.rs"]
mod cpal;
#[cfg(any(test, not(target_os = "linux")))]
#[path = "output/drain.rs"]
mod drain;

#[cfg(target_os = "linux")]
#[path = "output/pulse.rs"]
mod pulse;

pub fn try_open(spec: AudioSpec, capacity: u64) -> Result<Box<dyn AudioOutput>> {
    #[cfg(target_os = "linux")]
    {
        pulse::try_open(spec, capacity)
    }
    #[cfg(not(target_os = "linux"))]
    {
        cpal::try_open(spec, capacity)
    }
}
