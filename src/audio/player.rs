use std::fs::File;
use std::path::Path;

use flume::{Receiver, Sender, TryRecvError};
use log::{error, warn};
use symphonia::core::codecs::{Decoder, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, Track};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::TimeBase;

use super::output::{self, AudioOutputError};
use crate::channels::{ToAudio, ToUi};

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

    pub fn run(self) -> Result<(), PlayerError> {
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
                Err(TryRecvError::Disconnected) => return Err(PlayerError::Disconnected),
            };

            let (new_state, to_send) = Self::step(state, msg)?;
            state = new_state;

            if let Some(msg) = to_send {
                to_ui.send(msg).ok();
            }
        }
    }

    fn step(
        state: PlayerState,
        msg: Option<ToAudio>,
    ) -> Result<(PlayerState, Option<ToUi>), PlayerError> {
        match (msg, state) {
            (Some(ToAudio::PlayFilename(_)), state @ PlayerState::Playing(_)) => Ok((state, None)),
            (
                Some(ToAudio::PlayFilename(file_name)),
                PlayerState::Stopped | PlayerState::Paused(_),
            ) => {
                let playing_state = PlayingState::from_file(&file_name)?;
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
                let (playing_state, msg) = playing_state.continue_playing()?;
                Ok((PlayerState::Playing(playing_state), msg))
            }
        }
    }
}

impl PlayingState {
    // This is based on the main loop in the symphonia-play example
    fn from_file(file_name: &str) -> Result<PlayingState, PlayerError> {
        let mut hint = Hint::new();
        let source = {
            let path = Path::new(file_name);

            // Provide the file extension as a hint.
            if let Some(extension) = path.extension() {
                if let Some(extension_str) = extension.to_str() {
                    hint.with_extension(extension_str);
                }
            }
            let file = File::open(path)
                .map_err(|_e| PlayerError::Other(format!("file not found: {file_name}")))?;

            Box::new(file)
        };

        let mss = MediaSourceStream::new(source, Default::default());

        let format_opts = FormatOptions {
            enable_gapless: true,
            ..Default::default()
        };

        let metadata_opts: MetadataOptions = Default::default();

        match symphonia::default::get_probe().format(&hint, mss, &format_opts, &metadata_opts) {
            Err(err) => {
                let message = format!("The input was not supported by any format reader: {err}");
                Err(PlayerError::Other(message))
            }

            Ok(probed) => {
                let track = match first_supported_track(&probed.format.tracks()) {
                    Some(track) => track,
                    None => {
                        return Err(PlayerError::Other("no playable track".into()));
                    }
                };
                let track_info: TrackInfo = track.into();

                // default decode opts (no verify)
                let decoder = symphonia::default::get_codecs()
                    .make(&track.codec_params, &Default::default())?;

                Ok(PlayingState {
                    reader: probed.format,
                    seek_ts: 0,
                    audio_output: None,
                    decoder,
                    track_info,
                })
            }
        }
    }

    // This is based on the main loop in the symphonia-play example
    fn continue_playing(self) -> Result<(Self, Option<ToUi>), PlayerError> {
        let mut playing_state = self;

        // Get the next packet from the format reader.
        let packet = playing_state.reader.next_packet()?;

        if packet.track_id() != playing_state.track_info.id {
            return Ok((playing_state, None));
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
                    playing_state
                        .audio_output
                        .replace(output::try_open(spec, duration)?);
                } else {
                    // TODO: Check the audio spec. and duration hasn't changed.
                }

                // Write the decoded audio samples to the audio output if the presentation timestamp
                // for the packet is >= the seeked position (0 if not seeking).
                let timestamp = packet.ts();
                if timestamp >= playing_state.seek_ts {
                    if let Some(audio_output) = &mut playing_state.audio_output {
                        audio_output.write(decoded)?;

                        let msg = match (
                            &playing_state.track_info.time_base,
                            &playing_state.track_info.duration,
                        ) {
                            (Some(time_base), Some(duration)) => {
                                let elapsed = time_base.calc_time(timestamp);
                                let remaining =
                                    time_base.calc_time(duration.saturating_sub(timestamp));

                                Some(ToUi::ProgressPercentage { elapsed, remaining })
                            }

                            _ => {
                                log::debug!("missing time info in track");
                                None
                            }
                        };

                        Ok((playing_state, msg))
                    } else {
                        Err(PlayerError::Other("no audio device".into()))
                    }
                } else {
                    // seeking
                    Ok((playing_state, None))
                }
            }

            Err(SymphoniaError::DecodeError(err)) => {
                // Decode errors are not fatal.
                // Print the error message and try to decode the next packet as usual.
                warn!("decode error: {}", err);

                Ok((playing_state, None))
            }

            Err(err) => return Err(PlayerError::Symphonia(err)),
        }
    }
}

// TODO
// handle these errors at coordinator level
// send error to ui and restart the audio thread
//   this would lose queue state if we're keeping that here
//   but we want the current queue to be persisted anyway, so restart is ok
// going to want some kind of supervisor module
#[derive(thiserror::Error, Debug)]
pub enum PlayerError {
    #[error("disconnected from ui thread")]
    Disconnected,
    #[error("symphonia error: {0}")]
    Symphonia(#[from] SymphoniaError),
    #[error("audio output error: {0}")]
    AudioOutput(#[from] AudioOutputError),
    // TODO replace this with anyhow/eyre/whatever
    #[error("internal audio error: {0}")]
    Other(String),
}

fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}
