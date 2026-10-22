use super::{AudioOutput, AudioOutputError, Result};
use libpulse_binding as pulse;
use libpulse_simple_binding::Simple;
use symphonia::core::audio::{AudioSpec, Channels, GenericAudioBufferRef, Position};

pub(super) fn try_open(spec: AudioSpec, _capacity: u64) -> Result<Box<dyn AudioOutput>> {
    let pa_spec = pulse::sample::Spec {
        format: pulse::sample::Format::FLOAT32NE,
        channels: u8::try_from(spec.channels().count())
            .map_err(|_| AudioOutputError::OpenStreamError)?,
        rate: spec.rate(),
    };
    if !pa_spec.is_valid() {
        return Err(AudioOutputError::OpenStreamError);
    }
    let map = channel_map(spec.channels()).ok_or(AudioOutputError::OpenStreamError)?;
    let pa = Simple::new(
        None,
        "Clef",
        pulse::stream::Direction::Playback,
        None,
        "Music",
        &pa_spec,
        Some(&map),
        None,
    )
    .map_err(|err| {
        log::error!("audio stream open error: {err}");
        AudioOutputError::OpenStreamError
    })?;
    Ok(Box::new(PulseOutput { pa, samples: Vec::new() }))
}

struct PulseOutput {
    pa: Simple,
    samples: Vec<u8>,
}

impl AudioOutput for PulseOutput {
    fn write(&mut self, decoded: GenericAudioBufferRef<'_>) -> Result<()> {
        if decoded.frames() == 0 {
            return Ok(());
        }
        decoded.copy_bytes_to_vec_interleaved_as::<f32>(&mut self.samples);
        self.pa.write(&self.samples).map_err(|err| {
            log::error!("audio stream write error: {err}");
            AudioOutputError::StreamClosedError
        })
    }
    fn flush(&mut self) {
        let _ = self.pa.drain();
    }
}

fn channel_map(channels: &Channels) -> Option<pulse::channelmap::Map> {
    let Channels::Positioned(positions) = channels else {
        return None;
    };
    let mut map = pulse::channelmap::Map::default();
    map.init();
    map.set_len(u8::try_from(channels.count()).ok()?);
    for (index, position) in positions.iter().enumerate() {
        use pulse::channelmap::Position as Pa;
        map.get_mut()[index] = if channels.count() == 1 {
            Pa::Mono
        } else {
            match position {
                Position::FRONT_LEFT => Pa::FrontLeft,
                Position::FRONT_RIGHT => Pa::FrontRight,
                Position::FRONT_CENTER => Pa::FrontCenter,
                Position::LFE1 => Pa::Lfe,
                Position::REAR_LEFT => Pa::RearLeft,
                Position::REAR_RIGHT => Pa::RearRight,
                Position::FRONT_LEFT_CENTER => Pa::FrontLeftOfCenter,
                Position::FRONT_RIGHT_CENTER => Pa::FrontRightOfCenter,
                Position::REAR_CENTER => Pa::RearCenter,
                Position::SIDE_LEFT => Pa::SideLeft,
                Position::SIDE_RIGHT => Pa::SideRight,
                Position::TOP_CENTER => Pa::TopCenter,
                Position::TOP_FRONT_LEFT => Pa::TopFrontLeft,
                Position::TOP_FRONT_CENTER => Pa::TopFrontCenter,
                Position::TOP_FRONT_RIGHT => Pa::TopFrontRight,
                Position::TOP_REAR_LEFT => Pa::TopRearLeft,
                Position::TOP_REAR_CENTER => Pa::TopRearCenter,
                Position::TOP_REAR_RIGHT => Pa::TopRearRight,
                _ => return None,
            }
        };
    }
    Some(map)
}
