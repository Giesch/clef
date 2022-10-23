// This is based on the main loop in the symphonia-play example
//
// Symphonia
// Copyright (c) 2019-2022 The Project Symphonia Developers.
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

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

pub struct Player {
    state: PlayerState,
    inbox: Receiver<ToAudio>,
    to_ui: Sender<ToUi>,
}

#[derive(Debug)]
enum PlayerState {
    Stopped,
    Playing(PlayingState),
}

struct PlayingState {
    reader: Box<dyn FormatReader>,
    audio_output: Option<Box<dyn output::AudioOutput>>,
    decoder: Box<dyn Decoder>,
    seek_ts: u64, // 0 = not seeking, play from beginning
    track_info: TrackInfo,
}

#[derive(Debug)]
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

    pub fn flush(&mut self) {
        let Player { state, .. } = self;

        match state {
            PlayerState::Playing(playing_state) => {
                if let Some(audio_output) = &mut playing_state.audio_output {
                    audio_output.flush();
                }
            }
            _ => {}
        }
    }

    pub fn run(&mut self) -> Result<(), PlayerError> {
        loop {
            let Player {
                state,
                inbox,
                to_ui: _,
            } = self;

            let msg = match inbox.try_recv() {
                Ok(msg) => Some(msg),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => return Err(PlayerError::Disconnected),
            };

            match (state, msg) {
                (PlayerState::Stopped, None) => {}

                (PlayerState::Stopped, Some(ToAudio::PlayFilename(file_name))) => {
                    let playing_state = begin_playing(&file_name)?;
                    self.state = PlayerState::Playing(playing_state);
                }

                (PlayerState::Playing(playing_state), None) => {
                    // Get the next packet from the format reader.
                    let packet = playing_state.reader.next_packet()?;

                    if packet.track_id() != playing_state.track_info.id {
                        continue;
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

                                    match (
                                        &playing_state.track_info.time_base,
                                        &playing_state.track_info.duration,
                                    ) {
                                        (Some(time_base), Some(duration)) => {
                                            let elapsed = time_base.calc_time(timestamp);
                                            let remaining = time_base
                                                .calc_time(duration.saturating_sub(timestamp));

                                            let msg =
                                                ToUi::ProgressPercentage { elapsed, remaining };
                                            self.to_ui.send(msg).ok();
                                        }

                                        _ => {
                                            log::debug!("missing time info in track")
                                        }
                                    };
                                }
                            }
                        }

                        Err(SymphoniaError::DecodeError(err)) => {
                            // Decode errors are not fatal.
                            // Print the error message and try to decode the next packet as usual.
                            warn!("decode error: {}", err);
                        }

                        Err(err) => return Err(PlayerError::Symphonia(err)),
                    }
                }

                (PlayerState::Playing(_), Some(ToAudio::PlayFilename(file_name))) => {
                    let playing_state = begin_playing(&file_name)?;
                    self.state = PlayerState::Playing(playing_state);
                }
            }
        }
    }
}

fn begin_playing(file_name: &str) -> Result<PlayingState, PlayerError> {
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
            let decoder =
                symphonia::default::get_codecs().make(&track.codec_params, &Default::default())?;

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
