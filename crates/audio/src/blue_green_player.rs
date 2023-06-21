use std::collections::VecDeque;
use std::fs::File;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use camino::Utf8PathBuf;
use flume::{Receiver, Sender, TryRecvError};
use log::{error, info, trace, warn};
use souvlaki::{MediaPlayback, MediaPosition};
use symphonia::core::audio::{AsAudioBufferRef, AudioBufferRef};
use symphonia::core::codecs::Decoder;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

use clef_db::queries::SongId;
use clef_shared::queue::Queue;

use super::track_info::{first_supported_track, TrackInfo};

// TODO implement a swapping player pair as described here:
// https://github.com/pdeljanov/Symphonia/discussions/169

/// An mpsc message to the audio thread from the ui
#[derive(Debug, Clone, PartialEq)]
pub enum AudioAction {
    /// Begin playing the file (0) immediately,
    /// and continue playing files from the queue (1) when it ends
    PlayQueue(Box<Queue<QueuedSong>>),
    /// Pause the currently playing song, if any
    Pause,
    /// Play the currently paused song, if any
    PlayPaused,
    /// Swap between play/pause based on current state
    Toggle,
    /// Seek to position (0) of the current song, if any
    /// Expected to be a proportion in range 0.0..=1.0
    Seek(f32),
    /// Play the next track, if any, or transition to stopped
    Forward,
    /// Seek to the beginning of the current song,
    /// or if near it already, go back a track in the queue, if possible
    Back,
    /// Begin decoding a song without playing it
    Prepare(QueuedSong),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueuedSong {
    pub id: SongId,
    pub path: Utf8PathBuf,
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album_title: Option<String>,
    pub resized_art: Option<Utf8PathBuf>,
    pub duration: Option<Duration>,
}

/// An mpsc message to the main/ui thread from audio
#[derive(Debug, Clone, PartialEq)]
pub enum AudioMessage {
    /// A change that affects ui state; None = player stopped
    DisplayUpdate(Option<PlayerDisplay>),

    /// The first update after a seek request from the UI
    SeekComplete(PlayerDisplay),

    /// The audio thread died
    AudioDied,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerDisplay {
    pub song_id: SongId,
    pub playing: bool,
    pub times: ProgressTimes,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProgressTimes {
    pub elapsed: Time,
    pub remaining: Time,
    pub total: Time,
}

impl ProgressTimes {
    pub const ZERO: Self = Self {
        elapsed: Time { seconds: 0, frac: 0.0 },
        remaining: Time { seconds: 0, frac: 0.0 },
        total: Time { seconds: 0, frac: 0.0 },
    };
}

#[derive(Debug)]
pub struct Player {
    // TODO this has to change; decoder can be present before playing
    // could also change 'playing: bool'
    /// Audio state for the current song; None = stopped
    state: Option<PlayerState>,
    inbox: Receiver<AudioAction>,
    to_ui: Sender<AudioMessage>,
    media_controls: WrappedControls,
}

struct PlayerState {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    audio_output: Option<Box<dyn output::AudioOutput>>,
    playing: bool, // false = paused
    seek_ts: Option<u64>,
    track_info: TrackInfo,
    queue: Queue<QueuedSong>,
    timestamp: u64,

    // TODO what's the right way to handle these?
    // is it better to just have the decoder and not this stuff?
    //
    // this one is handled by the sibling player thread
    // /// pre-decoded data about the next song in the queue
    // preloaded_content: Option<PreloadedContent>,
    //
    /// pre-decoded packets for the currrently playing song
    predecoded_packets: VecDeque<PredecodedPacket>,
}

pub struct PreloadedContent {
    pub path: Utf8PathBuf,
    pub reader: Box<dyn FormatReader>,
    pub decoder: Box<dyn Decoder>,
    pub track_info: TrackInfo,
    pub predecoded_packets: VecDeque<PredecodedPacket>,
}

pub struct PredecodedPacket {
    pub timestamp: u64,
    pub decoded: AnyAudioBuffer,
}
