use rubato::audioadapter_buffers::direct::InterleavedSlice;
use rubato::Resampler as _;
use symphonia::core::audio::{AudioSpec, GenericAudioBufferRef};

/// Streaming float resampling with encoder-independent frame accounting.
#[allow(missing_debug_implementations)]
pub struct Resampler {
    inner: rubato::Fft<f32>,
    channels: usize,
    input_rate: usize,
    output_rate: usize,
    pending: Vec<f32>,
    scratch: Vec<f32>,
    output: Vec<f32>,
    delay: usize,
    input_frames: usize,
    output_frames: usize,
    finished: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use symphonia::core::audio::{
        layouts, AsGenericAudioBufferRef, AudioBuffer, AudioMut,
    };
    #[test]
    fn resampler_channel_order_frame_counts_and_flush() {
        for (input_rate, output_rate) in [(44_100, 48_000), (48_000, 44_100)] {
            for total in [0, 1, 31, 5003] {
                let spec = AudioSpec::new(input_rate, layouts::CHANNEL_LAYOUT_STEREO);
                let mut resampler = Resampler::new(&spec, output_rate, 256);
                let mut result = Vec::new();
                let mut used = 0;
                for block in [0, 1, 17, 513, 3, 1000, 3469] {
                    let frames = block.min(total - used);
                    let mut buffer = AudioBuffer::<f32>::new(spec.clone(), frames);
                    buffer.resize_with_silence(frames);
                    buffer.plane_mut(0).unwrap().fill(0.25);
                    buffer.plane_mut(1).unwrap().fill(-0.125);
                    result.extend_from_slice(
                        resampler.resample(buffer.as_generic_audio_buffer_ref()),
                    );
                    used += frames;
                }
                assert_eq!(used, total);
                result.extend_from_slice(resampler.flush());
                assert!(resampler.flush().is_empty());
                assert_eq!(result.len(), total * output_rate / input_rate as usize * 2);
                if result.len() > 2000 {
                    for frame in result
                        .chunks_exact(2)
                        .skip(500)
                        .take(result.len() / 2 - 1000)
                    {
                        assert!((frame[0] - 0.25).abs() < 0.002);
                        assert!((frame[1] + 0.125).abs() < 0.002);
                    }
                }
            }
        }
    }
}

impl Resampler {
    pub fn new(spec: &AudioSpec, output_rate: usize, chunk: usize) -> Self {
        let inner = rubato::Fft::new(
            spec.rate() as usize,
            output_rate,
            chunk.max(1),
            spec.channels().count(),
            rubato::FixedSync::Input,
        )
        .expect("valid resampling configuration");
        let delay = inner.output_delay();
        Self {
            inner,
            channels: spec.channels().count(),
            input_rate: spec.rate() as usize,
            output_rate,
            pending: Vec::new(),
            scratch: Vec::new(),
            output: Vec::new(),
            delay,
            input_frames: 0,
            output_frames: 0,
            finished: false,
        }
    }

    pub fn resample(&mut self, input: GenericAudioBufferRef<'_>) -> &[f32] {
        assert!(!self.finished, "resampler already flushed");
        let mut samples: Vec<f32> = Vec::new();
        input.copy_to_vec_interleaved(&mut samples);
        self.input_frames += input.frames();
        self.pending.extend(samples);
        self.output.clear();
        while self.pending.len() / self.channels >= self.inner.input_frames_next() {
            self.process_chunk();
        }
        &self.output
    }

    fn process_chunk(&mut self) {
        let input_frames = self.inner.input_frames_next();
        let output_frames = self.inner.output_frames_max();
        self.scratch.resize(output_frames * self.channels, 0.0);
        let input = InterleavedSlice::new(&self.pending, self.channels, input_frames)
            .expect("complete input chunk");
        let mut output =
            InterleavedSlice::new_mut(&mut self.scratch, self.channels, output_frames)
                .expect("allocated output chunk");
        let (used, written) = self
            .inner
            .process_into_buffer(&input, &mut output, None)
            .expect("valid resampler buffers");
        self.pending.drain(..used * self.channels);
        let trim = self.delay.min(written);
        self.delay -= trim;
        self.output.extend_from_slice(
            &self.scratch[trim * self.channels..written * self.channels],
        );
        self.output_frames += written - trim;
    }

    pub fn flush(&mut self) -> &[f32] {
        self.output.clear();
        if self.finished {
            return &self.output;
        }
        self.finished = true;
        let expected = (self.input_frames as u128 * self.output_rate as u128
            / self.input_rate as u128) as usize;
        let already_written = self.output_frames;
        while self.output_frames < expected {
            self.pending
                .resize(self.inner.input_frames_next() * self.channels, 0.0);
            self.process_chunk();
        }
        self.output
            .truncate(expected.saturating_sub(already_written) * self.channels);
        self.output_frames = expected;
        self.pending.clear();
        &self.output
    }
}
