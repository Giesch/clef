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

use crate::audio::output::{self, AudioOutput};
use crate::channels::{PlayerDisplay, ProgressTimes, Queue, ToAudio, ToUi};
use crate::ui::SongId;

#[derive(Debug)]
pub struct Player {
    state: Option<PlayerState>, // None = stopped
    inbox: Receiver<ToAudio>,
    to_ui: Sender<ToUi>,
}

struct PlayerState {
    reader: Box<dyn FormatReader>,
    audio_output: Option<Box<dyn output::AudioOutput>>,
    decoder: Box<dyn Decoder>,
    playing: bool, // false = paused
    seek_ts: Option<u64>,
    track_info: TrackInfo,
    queue: Queue<(SongId, Utf8PathBuf)>,
    timestamp: u64,
}

impl std::fmt::Debug for PlayerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let audio_output = match self.audio_output {
            Some(_) => "Some",
            None => "None",
        };

        f.debug_struct("PlayerState")
            .field("audio_output", &audio_output)
            .field("playing", &self.playing)
            .field("seek_ts", &self.seek_ts)
            .field("track_info", &self.track_info)
            .field("queue", &self.queue)
            .field("timestamp", &self.timestamp)
            .finish()
    }
}

impl From<&PlayerState> for PlayerDisplay {
    fn from(player_state: &PlayerState) -> Self {
        let song_id = player_state.queue.current.0;
        let playing = player_state.playing;

        let times = player_state
            .track_info
            .progress_times(player_state.timestamp)
            .unwrap_or_else(|| {
                error!("missing track time info");
                ProgressTimes::ZERO
            });

        Self { song_id, playing, times }
    }
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

impl Player {
    pub fn new(inbox: Receiver<ToAudio>, to_ui: Sender<ToUi>) -> Self {
        Self { state: None, inbox, to_ui }
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

    fn step(state: Option<PlayerState>, msg: Option<ToAudio>) -> StepResult {
        use ToAudio::*;

        match (msg, state) {
            (Some(PlayQueue(queue)), _any_state) => {
                // NOTE keep this in sync with the EOF section of continue_playing below
                let player_state = PlayerState::play_queue(queue)?;
                let ui_message = ToUi::DisplayUpdate(Some((&player_state).into()));
                Ok((Some(player_state), Some(ui_message)))
            }

            (Some(Pause), Some(mut player_state)) if player_state.playing => {
                player_state.playing = false;
                let ui_message = ToUi::DisplayUpdate(Some((&player_state).into()));
                Ok((Some(player_state), Some(ui_message)))
            }
            (Some(Pause), state) => Ok((state, None)),

            (Some(PlayPaused), Some(mut player_state)) if !player_state.playing => {
                player_state.playing = true;
                let ui_message = ToUi::DisplayUpdate(Some((&player_state).into()));
                Ok((Some(player_state), Some(ui_message)))
            }
            (Some(PlayPaused), state) => Ok((state, None)),

            (Some(Forward), Some(player_state)) => player_state.forward(),
            (Some(Forward), None) => Ok((None, None)),

            (Some(Back), Some(player_state)) => player_state.back(),
            (Some(Back), None) => Ok((None, None)),

            (Some(Seek(proportion)), Some(player_state)) => {
                if let Some(total) = player_state
                    .track_info
                    .progress_times(player_state.timestamp)
                    .map(|times| times.total)
                {
                    let mut seek_seconds = total.seconds as f32 * proportion;
                    seek_seconds += total.frac as f32 * proportion;

                    let player_state = player_state.seek_to(seek_seconds);
                    let ui_message = ToUi::DisplayUpdate(Some((&player_state).into()));
                    Ok((Some(player_state), Some(ui_message)))
                } else {
                    error!("missing track info: {:#?}", player_state.track_info);
                    let ui_message = ToUi::DisplayUpdate(Some((&player_state).into()));
                    Ok((Some(player_state), Some(ui_message)))
                }
            }
            (Some(Seek(_)), None) => Ok((None, None)),

            (None, Some(player_state)) if player_state.playing => {
                player_state.continue_playing()
            }
            (None, state) => Ok((state, None)),
        }
    }
}

type StepResult = anyhow::Result<(Option<PlayerState>, Option<ToUi>)>;

impl PlayerState {
    // This is based on the main loop in the symphonia-play example
    fn play_queue(queue: Queue<(SongId, Utf8PathBuf)>) -> anyhow::Result<Self> {
        let mut hint = Hint::new();

        // Provide the file extension as a hint.
        if let Some(extension) = queue.current.1.extension() {
            hint.with_extension(extension);
        }

        let file = File::open(&queue.current.1)
            .with_context(|| format!("file not found: {}", &queue.current.1))?;

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
            playing: true,
            timestamp: 0,
            decoder,
            track_info,
            queue,
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

    fn forward(mut self) -> StepResult {
        match self.queue.next.pop_front() {
            Some(new_current) => {
                let new_queue = Queue {
                    previous: {
                        self.queue.previous.push(self.queue.current);
                        self.queue.previous
                    },
                    current: new_current,
                    next: self.queue.next,
                };

                let mut new_state = Self::play_queue(new_queue)?;
                new_state.playing = self.playing;

                let ui_message = ToUi::DisplayUpdate(Some((&new_state).into()));

                Ok((Some(new_state), Some(ui_message)))
            }

            None => Ok((None, Some(ToUi::DisplayUpdate(None)))),
        }
    }

    fn back(mut self) -> StepResult {
        let past_two_seconds = self
            .track_info
            .progress_times(self.timestamp)
            .map(|p| p.elapsed.seconds > 1)
            .unwrap_or_default();

        if !past_two_seconds {
            if let Some(new_current) = self.queue.previous.pop() {
                let new_queue = Queue {
                    previous: self.queue.previous,
                    current: new_current,
                    next: {
                        self.queue.next.push_front(self.queue.current);
                        self.queue.next
                    },
                };

                let mut new_state = Self::play_queue(new_queue)?;
                new_state.playing = self.playing;

                let ui_message = ToUi::DisplayUpdate(Some((&new_state).into()));

                return Ok((Some(new_state), Some(ui_message)));
            }
        }

        let mut new_state = self.seek_to(0.0);
        new_state.timestamp = 0;
        let display: PlayerDisplay = (&new_state).into();

        Ok((Some(new_state), Some(ToUi::DisplayUpdate(Some(display)))))
    }

    // This is based on the main loop in the symphonia-play example
    fn continue_playing(self) -> StepResult {
        let mut player_state = self;

        // Get the next packet from the format reader.
        let packet = match player_state.reader.next_packet() {
            Ok(packet) => packet,

            // NOTE this is the normal way to reach the end of the file,
            // not really an error
            // https://github.com/pdeljanov/Symphonia/issues/134
            Err(SymphoniaError::IoError(io_error))
                if io_error.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                if let Some(output) = &mut player_state.audio_output {
                    output.flush();
                }

                return player_state.forward();
            }

            Err(error) => {
                bail!("error reading next packet: {error}");
            }
        };

        if packet.track_id() != player_state.track_info.id {
            return Ok((Some(player_state), None));
        }

        let decoded = match player_state.decoder.decode(&packet) {
            Ok(decoded) => decoded,

            Err(SymphoniaError::DecodeError(err)) => {
                // Decode errors are not fatal.
                // Print the error message and try to decode the next packet as usual.
                warn!("decode error: {}", err);
                return Ok((Some(player_state), None));
            }

            Err(err) => bail!("failed to read packet: {err}"),
        };

        // If the audio output is not open, try to open it.
        if player_state.audio_output.is_none() {
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
            player_state.audio_output.replace(new_audio_output);
        }

        // Write the decoded audio samples to the audio output if the presentation
        // timestamp for the packet is >= the seeked position (if any).
        let timestamp = packet.ts();
        player_state.timestamp = timestamp;

        let seeking = player_state
            .seek_ts
            .map(|seek_ts| timestamp < seek_ts)
            .unwrap_or_default();
        if seeking {
            return Ok((Some(player_state), None));
        }

        let audio_output: &mut dyn AudioOutput = player_state
            .audio_output
            .as_deref_mut()
            .ok_or_else(|| anyhow::anyhow!("no audio device"))?;

        audio_output.write(decoded).context("writing audio")?;

        let display: PlayerDisplay = (&player_state).into();
        let ui_message = ToUi::DisplayUpdate(Some(display));

        Ok((Some(player_state), Some(ui_message)))
    }
}

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

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

        let queue = Queue {
            previous: Default::default(),
            current: (SongId::unique(), Utf8PathBuf::from_str("fake").unwrap()),
            next: Default::default(),
        };

        let player_state = PlayerState {
            audio_output: Some(Box::new(output)),
            reader: Box::new(reader),
            decoder: Box::new(decoder),
            playing: true,
            seek_ts: None,
            track_info,
            timestamp: 0,
            queue,
        };

        let (new_state, to_ui) = player_state.continue_playing().unwrap();

        assert!(matches!(new_state, None));
        assert!(matches!(to_ui, Some(ToUi::DisplayUpdate(None))));
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
