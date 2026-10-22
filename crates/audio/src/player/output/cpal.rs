use super::{drain::DrainProgress, AudioOutput, AudioOutputError, Result};
use crate::resampler::Resampler;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rb::{RbConsumer, RbProducer, RB};
use std::sync::Arc;
use std::time::{Duration, Instant};
use symphonia::core::audio::{AudioSpec, GenericAudioBufferRef};

pub(super) fn try_open(spec: AudioSpec, capacity: u64) -> Result<Box<dyn AudioOutput>> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or(AudioOutputError::OpenStreamError)?;
    let supported = device
        .default_output_config()
        .map_err(|_| AudioOutputError::OpenStreamError)?;
    #[allow(unused_mut)]
    let mut config = supported.config();
    #[cfg(not(target_os = "windows"))]
    {
        config.channels = spec.channels().count() as u16;
        config.sample_rate = spec.rate();
    }
    let channels_match = usize::from(config.channels) == spec.channels().count();
    if !channels_match {
        return Err(AudioOutputError::Unsupported(
            "source and device channel counts differ",
        ));
    }

    match supported.sample_format() {
        cpal::SampleFormat::F32 => open::<f32>(spec, capacity, &device, config),
        cpal::SampleFormat::I16 => open::<i16>(spec, capacity, &device, config),
        cpal::SampleFormat::U16 => open::<u16>(spec, capacity, &device, config),
        _ => Err(AudioOutputError::Unsupported("device sample format")),
    }
}

fn open<T>(
    spec: AudioSpec,
    capacity: u64,
    device: &cpal::Device,
    config: cpal::StreamConfig,
) -> Result<Box<dyn AudioOutput>>
where
    T: cpal::SizedSample + cpal::FromSample<f32> + Send + 'static,
{
    let ring = rb::SpscRb::<T>::new(
        config.sample_rate as usize / 5 * usize::from(config.channels),
    );
    let producer = ring.producer();
    let consumer = ring.consumer();
    let clock_origin = Instant::now();
    let progress = Arc::new(DrainProgress::default());
    let callback_progress = Arc::clone(&progress);
    let stream = device
        .build_output_stream(
            config,
            move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
                let read = consumer.read(data).unwrap_or(0);
                let timestamp = info.timestamp();
                let latency = timestamp
                    .playback
                    .saturating_duration_since(timestamp.callback);
                callback_progress.publish(
                    read,
                    config.channels,
                    config.sample_rate,
                    clock_origin.elapsed(),
                    latency,
                );
                data[read..].fill(T::EQUILIBRIUM);
            },
            |error| log::error!("audio output error: {error}"),
            None,
        )
        .map_err(|_| AudioOutputError::OpenStreamError)?;
    stream
        .play()
        .map_err(|_| AudioOutputError::PlayStreamError)?;
    let needs_resampling = spec.rate() != config.sample_rate;
    let resampler = needs_resampling
        .then(|| Resampler::new(&spec, config.sample_rate as usize, capacity as usize));

    Ok(Box::new(CpalOutput {
        _ring: ring,
        producer,
        stream,
        resampler,
        samples: Vec::new(),
        clock_origin,
        progress,
        produced: 0,
    }))
}

struct CpalOutput<T: Copy> {
    _ring: rb::SpscRb<T>,
    producer: rb::Producer<T>,
    stream: cpal::Stream,
    resampler: Option<Resampler>,
    samples: Vec<f32>,
    clock_origin: Instant,
    progress: Arc<DrainProgress>,
    produced: u64,
}

fn write_samples<T: cpal::Sample + cpal::FromSample<f32>>(
    producer: &mut rb::Producer<T>,
    produced: &mut u64,
    samples: &[f32],
) -> Result<()> {
    let converted: Vec<T> = samples
        .iter()
        .map(|sample| T::from_sample(*sample))
        .collect();
    let mut remaining = converted.as_slice();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !remaining.is_empty() {
        let timeout = deadline.saturating_duration_since(Instant::now());
        if timeout.is_zero() {
            return Err(AudioOutputError::StreamClosedError);
        }

        match producer.write_blocking_timeout(remaining, timeout) {
            Some(written) if written > 0 => {
                *produced += written as u64;
                remaining = &remaining[written..];
            }
            _ => return Err(AudioOutputError::StreamClosedError),
        }
    }

    Ok(())
}

impl<T> AudioOutput for CpalOutput<T>
where
    T: cpal::SizedSample + cpal::FromSample<f32> + Send + 'static,
{
    fn write(&mut self, decoded: GenericAudioBufferRef<'_>) -> Result<()> {
        let samples = if let Some(resampler) = &mut self.resampler {
            resampler.resample(decoded)
        } else {
            decoded.copy_to_vec_interleaved(&mut self.samples);
            &self.samples
        };

        write_samples(&mut self.producer, &mut self.produced, samples)
    }

    fn flush(&mut self) {
        if let Some(resampler) = &mut self.resampler {
            let _ =
                write_samples(&mut self.producer, &mut self.produced, resampler.flush());
        }

        let timeout = self
            .clock_origin
            .elapsed()
            .saturating_add(Duration::from_secs(2));
        while !self
            .progress
            .finished(self.produced, self.clock_origin.elapsed(), timeout)
        {
            std::thread::park_timeout(Duration::from_millis(1));
        }

        let _ = self.stream.pause();
    }
}
