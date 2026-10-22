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
    #[error("audio stream closed")]
    StreamClosedError,
}

pub type Result<T> = std::result::Result<T, AudioOutputError>;

#[path = "output/pulse.rs"]
mod pulse;

pub fn try_open(spec: AudioSpec, capacity: u64) -> Result<Box<dyn AudioOutput>> {
    pulse::try_open(spec, capacity)
}
