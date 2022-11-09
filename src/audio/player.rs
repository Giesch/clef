use std::collections::VecDeque;
use std::fs::File;

use anyhow::{bail, Context};
use camino::Utf8PathBuf;
use flume::{Receiver, Sender, TryRecvError};
use log::{error, warn};
use symphonia::core::codecs::{CodecParameters, Decoder, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo, Track};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::{Time, TimeBase};

use super::output::{self, AudioOutput};
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
    seek_ts: Option<u64>,
    track_info: TrackInfo,
    up_next: VecDeque<Utf8PathBuf>,
}

#[derive(Debug, Clone)]
struct TrackInfo {
    id: u32,
    time_base: Option<TimeBase>,
    duration: Option<u64>,
}

impl TrackInfo {
    /// Given a packet timestamp, returns the progress times to display for the track
    /// None = either time base or duration is missing from the track info
    fn progress_times(&self, timestamp: u64) -> Option<ProgressTimes> {
        match (self.time_base, self.duration) {
            (Some(time_base), Some(duration)) => Some(ProgressTimes {
                elapsed: time_base.calc_time(timestamp),
                remaining: time_base.calc_time(duration.saturating_sub(timestamp)),
                total: time_base.calc_time(duration),
            }),

            _ => None,
        }
    }
}

impl From<&Track> for TrackInfo {
    fn from(track: &Track) -> Self {
        let CodecParameters { time_base, n_frames, start_ts, .. } = track.codec_params;

        Self {
            id: track.id,
            time_base,
            duration: n_frames.map(|frames| start_ts + frames),
        }
    }
}

impl std::fmt::Debug for PlayingState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let audio_output = match self.audio_output {
            Some(_) => "Some",
            None => "None",
        };

        f.debug_struct("PlayingState")
            .field("audio_output", &audio_output)
            .field("seek_ts", &self.seek_ts)
            .field("track_info", &self.track_info)
            .field("up_next", &self.up_next)
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
        let Player { state, inbox, to_ui } = self;

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

    fn step(state: PlayerState, msg: Option<ToAudio>) -> StepResult {
        use ToAudio::*;

        match (msg, state) {
            (Some(PlayQueue((to_play, up_next))), _any_state) => {
                // NOTE keep this in sync with the EOF section of continue_playing below
                let playing_state = PlayingState::play_queue(to_play, up_next)?;
                Ok((PlayerState::Playing(playing_state), None))
            }

            (Some(Pause), PlayerState::Playing(playing_state)) => {
                Ok((PlayerState::Paused(playing_state), None))
            }
            (Some(Pause), state) => Ok((state, None)),

            (Some(PlayPaused), PlayerState::Paused(playing_state)) => {
                Ok((PlayerState::Playing(playing_state), None))
            }
            (Some(PlayPaused), state) => Ok((state, None)),

            (Some(Seek(target)), PlayerState::Playing(playing_state)) => {
                let state = PlayerState::Playing(playing_state.seek_to(target));
                Ok((state, None))
            }
            (Some(Seek(target)), PlayerState::Paused(playing_state)) => {
                let state = PlayerState::Paused(playing_state.seek_to(target));
                Ok((state, None))
            }
            (Some(Seek(_)), state @ PlayerState::Stopped) => Ok((state, None)),

            (None, PlayerState::Playing(playing_state)) => {
                playing_state.continue_playing()
            }
            (None, state) => Ok((state, None)),
        }
    }
}

type StepResult = anyhow::Result<(PlayerState, Option<ToUi>)>;

impl PlayingState {
    // This is based on the main loop in the symphonia-play example
    fn play_queue(
        path: Utf8PathBuf,
        up_next: VecDeque<Utf8PathBuf>,
    ) -> anyhow::Result<Self> {
        let mut hint = Hint::new();

        // Provide the file extension as a hint.
        if let Some(extension) = path.extension() {
            hint.with_extension(extension);
        }

        let file =
            File::open(&path).with_context(|| format!("file not found: {path}"))?;

        let source = Box::new(file);

        let mss = MediaSourceStream::new(source, Default::default());

        let format_opts = FormatOptions {
            enable_gapless: true,
            ..Default::default()
        };

        let metadata_opts: MetadataOptions = Default::default();

        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &format_opts, &metadata_opts)
            .context("The input was not supported by any format reader")?;

        let track =
            first_supported_track(probed.format.tracks()).context("no playable track")?;
        let track_info: TrackInfo = track.into();

        // default decode opts (no verify)
        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &Default::default())
            .context("making decoder")?;

        Ok(Self {
            reader: probed.format,
            seek_ts: None,
            audio_output: None,
            decoder,
            track_info,
            up_next,
        })
    }

    fn seek_to(mut self, target: f32) -> Self {
        let seek_to = SeekTo::Time {
            time: Time::from(target),
            track_id: Some(self.track_info.id),
        };

        self.seek_ts = match self.reader.seek(SeekMode::Accurate, seek_to) {
            Ok(seeked_to) => Some(seeked_to.required_ts),
            Err(e) => {
                error!("seek error: {e}");
                None
            }
        };

        self
    }

    // This is based on the main loop in the symphonia-play example
    fn continue_playing(self) -> StepResult {
        let mut playing_state = self;

        // Get the next packet from the format reader.
        let packet = match playing_state.reader.next_packet() {
            Ok(packet) => packet,

            // NOTE this is the normal way to reach the end of the file,
            // not really an error
            // https://github.com/pdeljanov/Symphonia/issues/134
            Err(SymphoniaError::IoError(io_error))
                if io_error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                return playing_state.flush_and_play_next();
            }

            Err(error) => {
                bail!("error reading next packet: {error}");
            }
        };

        if packet.track_id() != playing_state.track_info.id {
            return Ok((PlayerState::Playing(playing_state), None));
        }

        let decoded = match playing_state.decoder.decode(&packet) {
            Ok(decoded) => decoded,

            Err(SymphoniaError::DecodeError(err)) => {
                // Decode errors are not fatal.
                // Print the error message and try to decode the next packet as usual.
                warn!("decode error: {}", err);
                return Ok((PlayerState::Playing(playing_state), None));
            }

            Err(err) => bail!("failed to read packet: {err}"),
        };

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

        // Write the decoded audio samples to the audio output if the presentation
        // timestamp for the packet is >= the seeked position (if any).
        let timestamp = packet.ts();
        let seeking = playing_state
            .seek_ts
            .map(|seek_ts| timestamp < seek_ts)
            .unwrap_or_default();
        if seeking {
            return Ok((PlayerState::Playing(playing_state), None));
        }

        let audio_output: &mut dyn AudioOutput = playing_state
            .audio_output
            .as_deref_mut()
            .ok_or_else(|| anyhow::anyhow!("no audio device"))?;

        audio_output.write(decoded).context("writing audio")?;

        let progress_msg = playing_state
            .track_info
            .progress_times(timestamp)
            .map(ToUi::Progress);

        Ok((PlayerState::Playing(playing_state), progress_msg))
    }

    fn flush_and_play_next(mut self) -> StepResult {
        if let Some(output) = &mut self.audio_output {
            output.flush();
        }

        if let Some(next) = self.up_next.pop_front() {
            let playing_state =
                Self::play_queue(next.clone(), self.up_next).context("playing file")?;

            return Ok((
                PlayerState::Playing(playing_state),
                Some(ToUi::NextSong(next)),
            ));
        }

        Ok((PlayerState::Stopped, Some(ToUi::Stopped)))
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

        let decoder = MockDecoder::new();
        let output = MockOutput::default();

        let mut reader = MockReader::new();
        reader.expect_next_packet().times(1).returning(|| {
            let kind = std::io::ErrorKind::UnexpectedEof;
            let io_error = std::io::Error::new(kind, anyhow::anyhow!("EOF"));
            Err(SymphoniaError::IoError(io_error))
        });

        let playing_state = PlayingState {
            up_next: VecDeque::new(),
            audio_output: Some(Box::new(output)),
            reader: Box::new(reader),
            decoder: Box::new(decoder),
            seek_ts: None,
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
