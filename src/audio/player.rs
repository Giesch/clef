use std::fs::File;

use anyhow::{bail, Context};
use camino::Utf8PathBuf;
use flume::{Receiver, Sender, TryRecvError};
use log::warn;
use symphonia::core::codecs::{Decoder, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, Track};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::TimeBase;

use super::output;
use crate::channels::{ProgressTimes, ToAudio, ToUi};

#[derive(Debug)]
pub struct Player {
    state: PlayerState,
    inbox: Receiver<ToAudio>,
    to_ui: Sender<ToUi>,
}

#[derive(Debug)]
enum PlayerState {
    Stopped,
    Playing(PlayingState),
    Paused(PlayingState),
}

struct PlayingState {
    reader: Box<dyn FormatReader>,
    audio_output: Option<Box<dyn output::AudioOutput>>,
    decoder: Box<dyn Decoder>,
    seek_ts: u64, // 0 = not seeking, play from beginning
    track_info: TrackInfo,
    path: Utf8PathBuf,
}

#[derive(Debug, Clone)]
struct TrackInfo {
    id: u32,
    time_base: Option<TimeBase>,
    duration: Option<u64>,
}

impl From<&Track> for TrackInfo {
    fn from(track: &Track) -> Self {
        Self {
            id: track.id,
            time_base: track.codec_params.time_base,
            duration: track
                .codec_params
                .n_frames
                .map(|frames| track.codec_params.start_ts + frames),
        }
    }
}

impl std::fmt::Debug for PlayingState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let audio_output = &match self.audio_output {
            Some(_) => "Some",
            None => "None",
        };

        f.debug_struct("PlayingState")
            .field("audio_output", audio_output)
            .field("seek_ts", &self.seek_ts)
            .field("track_info", &self.track_info)
            .field("path", &self.path)
            .finish()
    }
}

impl Player {
    pub fn new(inbox: Receiver<ToAudio>, to_ui: Sender<ToUi>) -> Self {
        Self {
            state: PlayerState::Stopped,
            inbox,
            to_ui,
        }
    }

    pub fn run_loop(self) -> anyhow::Result<()> {
        let Player {
            state,
            inbox,
            to_ui,
        } = self;

        let mut state = state;

        loop {
            let msg = match inbox.try_recv() {
                Ok(msg) => Some(msg),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => bail!("disconnected from ui thread"),
            };

            let (new_state, to_send) =
                Self::step(state, msg).context("error during player step")?;
            state = new_state;

            if let Some(msg) = to_send {
                to_ui.send(msg).ok();
            }
        }
    }

    fn step(
        state: PlayerState,
        msg: Option<ToAudio>,
    ) -> anyhow::Result<(PlayerState, Option<ToUi>)> {
        match (msg, state) {
            (Some(ToAudio::PlayFilename(file_name)), _any_state) => {
                let playing_state = PlayingState::from_file(file_name).context("playing file")?;
                Ok((PlayerState::Playing(playing_state), None))
            }

            (Some(ToAudio::Pause), state @ PlayerState::Stopped) => Ok((state, None)),
            (Some(ToAudio::Pause), state @ PlayerState::Paused(_)) => Ok((state, None)),
            (Some(ToAudio::Pause), PlayerState::Playing(playing_state)) => {
                Ok((PlayerState::Paused(playing_state), None))
            }

            (Some(ToAudio::PlayPaused), state @ PlayerState::Playing(_)) => Ok((state, None)),
            (Some(ToAudio::PlayPaused), state @ PlayerState::Stopped) => Ok((state, None)),
            (Some(ToAudio::PlayPaused), PlayerState::Paused(playing_state)) => {
                Ok((PlayerState::Playing(playing_state), None))
            }

            (None, state @ PlayerState::Stopped) => Ok((state, None)),
            (None, state @ PlayerState::Paused(_)) => Ok((state, None)),
            (None, PlayerState::Playing(playing_state)) => {
                let pair = playing_state
                    .continue_playing()
                    .context("continue playing")?;

                Ok(pair)
            }
        }
    }
}

impl PlayingState {
    // This is based on the main loop in the symphonia-play example
    fn from_file(path: Utf8PathBuf) -> anyhow::Result<PlayingState> {
        let mut hint = Hint::new();

        // Provide the file extension as a hint.
        if let Some(extension) = path.extension() {
            hint.with_extension(extension);
        }

        let file = File::open(&path).with_context(|| format!("file not found: {path}"))?;

        let source = Box::new(file);

        let mss = MediaSourceStream::new(source, Default::default());

        let format_opts = FormatOptions {
            enable_gapless: true,
            ..Default::default()
        };

        let metadata_opts: MetadataOptions = Default::default();

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &metadata_opts)
            .with_context(|| format!("The input was not supported by any format reader"))?;

        let track = first_supported_track(&probed.format.tracks()).context("no playable track")?;
        let track_info: TrackInfo = track.into();

        // default decode opts (no verify)
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &Default::default())
            .context("making decoder")?;

        Ok(PlayingState {
            reader: probed.format,
            seek_ts: 0,
            audio_output: None,
            decoder,
            track_info,
            path,
        })
    }

    // This is based on the main loop in the symphonia-play example
    fn continue_playing(self) -> anyhow::Result<(PlayerState, Option<ToUi>)> {
        let mut playing_state = self;

        // Get the next packet from the format reader.
        let packet = match playing_state.reader.next_packet() {
            Ok(packet) => packet,

            // this is an expected error
            // https://github.com/pdeljanov/Symphonia/issues/134
            Err(SymphoniaError::IoError(io_error))
                if io_error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                if let Some(output) = &mut playing_state.audio_output {
                    output.flush();
                }

                return Ok((PlayerState::Stopped, Some(ToUi::Stopped)));
            }

            Err(error) => {
                bail!("error reading next packet: {error}");
            }
        };

        if packet.track_id() != playing_state.track_info.id {
            let next_state = PlayerState::Playing(playing_state);
            return Ok((next_state, None));
        }

        match playing_state.decoder.decode(&packet) {
            Ok(decoded) => {
                // If the audio output is not open, try to open it.
                if playing_state.audio_output.is_none() {
                    // Get the audio buffer specification. This is a description of the decoded
                    // audio buffer's sample format and sample rate.
                    let spec = *decoded.spec();

                    // Get the capacity of the decoded buffer. Note that this is capacity, not
                    // length! The capacity of the decoded buffer is constant for the life of the
                    // decoder, but the length is not.
                    let duration = decoded.capacity() as u64;

                    // Try to open the audio output.
                    let new_audio_output =
                        output::try_open(spec, duration).context("opening audio device")?;
                    playing_state.audio_output.replace(new_audio_output);
                } else {
                    // TODO: Check the audio spec. and duration hasn't changed.
                }

                // Write the decoded audio samples to the audio output if the presentation timestamp
                // for the packet is >= the seeked position (0 if not seeking).
                let timestamp = packet.ts();
                if timestamp >= playing_state.seek_ts {
                    if let Some(audio_output) = &mut playing_state.audio_output {
                        audio_output.write(decoded).context("writing audio")?;

                        let msg = match (
                            &playing_state.track_info.time_base,
                            &playing_state.track_info.duration,
                        ) {
                            (Some(time_base), Some(duration)) => {
                                let elapsed = time_base.calc_time(timestamp);
                                let remaining =
                                    time_base.calc_time(duration.saturating_sub(timestamp));
                                let total = time_base.calc_time(*duration);

                                let times = ProgressTimes {
                                    elapsed,
                                    remaining,
                                    total,
                                };
                                Some(ToUi::Progress(times))
                            }

                            _ => {
                                log::debug!("missing time info in track");
                                None
                            }
                        };

                        let next_state = PlayerState::Playing(playing_state);
                        Ok((next_state, msg))
                    } else {
                        bail!("no audio device");
                    }
                } else {
                    // seeking
                    let next_state = PlayerState::Playing(playing_state);
                    Ok((next_state, None))
                }
            }

            Err(SymphoniaError::DecodeError(err)) => {
                // Decode errors are not fatal.
                // Print the error message and try to decode the next packet as usual.
                warn!("decode error: {}", err);

                let next_state = PlayerState::Playing(playing_state);
                Ok((next_state, None))
            }

            Err(err) => bail!("failed to read packet: {err}"),
        }
    }
}

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::output::AudioOutput;
    use mockall::mock;

    #[test]
    fn continue_playing_doesnt_crash_for_eof() {
        let track_info = TrackInfo {
            id: 0,
            time_base: None,
            duration: None,
        };

        let mut reader = MockReader::new();
        reader.expect_next_packet().times(1).returning(|| {
            let kind = std::io::ErrorKind::UnexpectedEof;
            let io_error = std::io::Error::new(kind, anyhow::anyhow!("EOF"));

            return Err(SymphoniaError::IoError(io_error));
        });

        let decoder = MockDecoder::new();

        let output = MockOutput::default();

        let playing_state = PlayingState {
            path: Utf8PathBuf::new(),
            audio_output: Some(Box::new(output)),
            reader: Box::new(reader),
            decoder: Box::new(decoder),
            seek_ts: 0,
            track_info,
        };

        let (new_state, to_ui) = playing_state.continue_playing().unwrap();

        assert!(matches!(new_state, PlayerState::Stopped));
        assert!(matches!(to_ui, Some(ToUi::Stopped)));
    }

    mock! {
        Reader {}

        impl FormatReader for Reader {
            fn next_packet(
                &mut self,
            ) -> symphonia::core::errors::Result<symphonia::core::formats::Packet>;


            fn try_new(
                source: MediaSourceStream,
                options: &FormatOptions,
            ) -> symphonia::core::errors::Result<Self>;

            fn cues(&self) -> &[symphonia::core::formats::Cue];

            fn metadata(&mut self) -> symphonia::core::meta::Metadata<'_>;

            fn seek(
                &mut self,
                mode: symphonia::core::formats::SeekMode,
                to: symphonia::core::formats::SeekTo,
            ) -> symphonia::core::errors::Result<symphonia::core::formats::SeekedTo>;

            fn tracks(&self) -> &[Track];

            fn into_inner(self: Box<Self>) -> MediaSourceStream;
        }
    }

    mock! {
        Decoder {}

        impl Decoder for Decoder {
            fn try_new(
                params: &symphonia::core::codecs::CodecParameters,
                options: &symphonia::core::codecs::DecoderOptions,
            ) -> symphonia::core::errors::Result<Self>;

            fn supported_codecs() -> &'static [symphonia::core::codecs::CodecDescriptor];

            fn reset(&mut self) ;

            fn codec_params(&self) -> &symphonia::core::codecs::CodecParameters;

            fn decode(
                &mut self,
                packet: &symphonia::core::formats::Packet,
            ) -> symphonia::core::errors::Result<symphonia::core::audio::AudioBufferRef<'static>>;

            fn finalize(&mut self) -> symphonia::core::codecs::FinalizeResult;

            fn last_decoded(&self) -> symphonia::core::audio::AudioBufferRef<'_>;
        }
    }

    /// A mock AudioOutput that asserts it was flushed on drop.
    ///
    /// NOTE
    /// mockall can't mock functions with non-static generic arguments,
    /// like the write method's 'decoded' argument
    #[derive(Default)]
    struct MockOutput {
        flushed: bool,
    }

    impl AudioOutput for MockOutput {
        fn flush(&mut self) {
            self.flushed = true;
        }

        fn write(
            &mut self,
            _decoded: symphonia::core::audio::AudioBufferRef<'_>,
        ) -> output::Result<()> {
            unimplemented!()
        }
    }

    impl Drop for MockOutput {
        fn drop(&mut self) {
            assert!(self.flushed);
        }
    }
}
