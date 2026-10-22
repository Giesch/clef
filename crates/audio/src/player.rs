use std::collections::VecDeque;
use std::fs::File;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{anyhow, bail, Context};
use camino::Utf8PathBuf;
use flume::{Receiver, Sender, TryRecvError};
use log::{error, info, trace, warn};
use souvlaki::{MediaPlayback, MediaPosition};
use symphonia::core::audio::{AsGenericAudioBufferRef, GenericAudioBufferRef};
use symphonia::core::codecs::audio::AudioDecoder as Decoder;
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::probe::Hint;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::units::{Time, Timestamp};

use clef_db::queries::SongId;
use clef_shared::queue::Queue;

use self::preloader::{
    AnyAudioBuffer, PredecodedPacket, PreloadedContent, Preloader, PreloaderAction,
    PreloaderEffect,
};

use super::track_info::{first_supported_track, TrackInfo};

mod media_controls;
use media_controls::*;
mod output;
use output::AudioOutput;
mod preloader;

#[allow(unused)]
#[cfg(not(target_os = "linux"))]
mod device_config;

#[allow(unused)]
#[cfg(not(target_os = "linux"))]
use device_config::CpalDeviceConfig;

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
pub struct ProgressTime {
    pub seconds: u64,
    pub frac: f64,
}

impl ProgressTime {
    pub(crate) fn from_seconds(seconds: f64) -> Self {
        Self {
            seconds: seconds as u64,
            frac: seconds.fract(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProgressTimes {
    pub elapsed: ProgressTime,
    pub remaining: ProgressTime,
    pub total: ProgressTime,
}

impl ProgressTimes {
    pub const ZERO: Self = Self {
        elapsed: ProgressTime { seconds: 0, frac: 0.0 },
        remaining: ProgressTime { seconds: 0, frac: 0.0 },
        total: ProgressTime { seconds: 0, frac: 0.0 },
    };
}

#[derive(Debug)]
pub struct Player {
    /// Audio state for the current song; None = stopped
    state: Option<PlayerState>,
    inbox: Receiver<AudioAction>,
    to_ui: Sender<AudioMessage>,
    media_controls: WrappedControls,
    to_preloader: Sender<PreloaderAction>,
    from_preloader: Receiver<PreloaderEffect>,

    #[allow(unused)]
    #[cfg(not(target_os = "linux"))]
    device_config: CpalDeviceConfig,
}

struct PlayerState {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn Decoder>,
    audio_output: Option<Box<dyn output::AudioOutput>>,
    playing: bool, // false = paused
    seek_ts: Option<Timestamp>,
    track_info: TrackInfo,
    queue: Queue<QueuedSong>,
    timestamp: Timestamp,
    /// pre-decoded data about the next song in the queue
    preloaded_content: Option<PreloadedContent>,
    /// pre-decoded packets for the currrently playing song
    predecoded_packets: VecDeque<PredecodedPacket>,
}

impl std::fmt::Debug for PlayerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let audio_output = match self.audio_output {
            Some(_) => "Some(..)",
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
        let song_id = player_state.queue.current.id;
        let playing = player_state.playing;

        let timestamp = player_state.optimistic_timestamp();
        let times = player_state
            .track_info
            .progress_times(timestamp)
            .unwrap_or_else(|| {
                error!("missing track time info");
                ProgressTimes::ZERO
            });

        Self { song_id, playing, times }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum AudioThreadError {
    #[error("disconnected from main ui thread")]
    Disconnected,

    #[error("unhandled audio error: {0}")]
    Other(#[from] anyhow::Error),
}

impl Player {
    pub fn spawn(
        inbox: Receiver<AudioAction>,
        to_ui: Sender<AudioMessage>,
        to_self: Sender<AudioAction>,
    ) -> anyhow::Result<JoinHandle<()>> {
        let (to_preloader, preloader_inbox) =
            flume::unbounded::<preloader::PreloaderAction>();
        let (to_player, from_preloader) =
            flume::unbounded::<preloader::PreloaderEffect>();

        Preloader::spawn(preloader_inbox, to_player)
            .context("failed to spawn preloader")?;

        let join_handle = std::thread::Builder::new()
            .name("ClefAudioPlayer".to_string())
            .spawn(move || {
                #[allow(unused)]
                #[cfg(not(target_os = "linux"))]
                let device_config = CpalDeviceConfig::get_default()
                    .expect("failed to get default device config");

                let player = Player::new(
                    inbox,
                    to_ui.clone(),
                    to_self,
                    to_preloader,
                    from_preloader,
                    #[allow(unused)]
                    #[cfg(not(target_os = "linux"))]
                    device_config,
                )
                .expect("failed to create player");

                if let Err(err) = player.run_loop() {
                    to_ui.send(AudioMessage::AudioDied).ok();

                    match err {
                        AudioThreadError::Disconnected => {
                            // This can happen both during startup and shutdown,
                            // before the the ui exists or after its closed.
                            // In both cases we just wait for the app.
                        }

                        AudioThreadError::Other(e) => {
                            panic!("unrecovered error: {e}");
                        }
                    }
                }
            })?;

        Ok(join_handle)
    }

    pub fn new(
        inbox: Receiver<AudioAction>,
        to_ui: Sender<AudioMessage>,
        to_self: Sender<AudioAction>,
        to_preloader: Sender<PreloaderAction>,
        from_preloader: Receiver<PreloaderEffect>,

        #[allow(unused)]
        #[cfg(not(target_os = "linux"))]
        device_config: CpalDeviceConfig,
    ) -> anyhow::Result<Self> {
        let media_controls = WrappedControls::new(to_self);

        Ok(Self {
            state: None,
            inbox,
            to_ui,
            media_controls,
            to_preloader,
            from_preloader,

            #[allow(unused)]
            #[cfg(not(target_os = "linux"))]
            device_config,
        })
    }

    pub fn run_loop(self) -> Result<(), AudioThreadError> {
        #[allow(unused)]
        #[cfg(target_os = "linux")]
        let Player {
            mut state,
            inbox,
            to_ui,
            mut media_controls,
            to_preloader,
            from_preloader,
        } = self;

        #[allow(unused)]
        #[cfg(not(target_os = "linux"))]
        let Player {
            mut state,
            inbox,
            to_ui,
            mut media_controls,
            to_preloader,
            from_preloader,
            device_config,
        } = self;

        loop {
            let preloaded = match from_preloader.try_recv() {
                Ok(action) => Some(action),
                Err(TryRecvError::Empty) => None,
                // NOTE the player can continue functioning without the preloader,
                // it'll just have audible gaps;
                // the disconnect should be logged elsewhere
                Err(TryRecvError::Disconnected) => None,
            };

            match preloaded {
                Some(PreloaderEffect::Loaded(content)) => {
                    let path = content.path.clone();
                    trace!("Got preloaded decoder: {path}");

                    if let Some(state) = &mut state {
                        state.preloaded_content = Some(content);
                    }
                }
                Some(PreloaderEffect::PreloaderDied) => {
                    // this error is logged in Preloader::spawn
                    // the player can continue working with gaps
                    #[cfg(debug_assertions)]
                    panic!("preloader thread died")
                }
                None => {}
            }

            let action = match inbox.try_recv() {
                Ok(action) => Some(action),
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    return Err(AudioThreadError::Disconnected);
                }
            };

            let was_playing = state.is_some();

            let effects =
                Self::step(state, action).context("error during player step")?;

            if let Some(message) = effects.audio_message {
                to_ui.send(message).ok();
            }

            if let Some(metadata) = &effects.metadata {
                media_controls.set_metadata(metadata.into());
            }

            if let Some(playback) = effects.playback {
                media_controls.set_playback(playback);
            }

            if effects.player_state.is_none() && was_playing {
                media_controls.deinit();
            }

            if let Some(preload) = effects.preload {
                to_preloader.send(preload).ok();
            }

            state = effects.player_state;
        }
    }

    fn step(state: Option<PlayerState>, msg: Option<AudioAction>) -> StepResult {
        use AudioAction::*;

        match (msg, state) {
            (Some(PlayQueue(queue)), _any_state) => {
                let player_state = PlayerState::play_queue(*queue)?;
                let mut effects = publish_display_update(player_state);
                effects.preload_next();

                Ok(effects)
            }

            (Some(Pause), Some(mut player_state)) if player_state.playing => {
                player_state.playing = false;
                Ok(publish_display_update(player_state))
            }
            (Some(Pause), state) => Ok(AudioEffects::none(state)),

            (Some(PlayPaused), Some(mut player_state)) if !player_state.playing => {
                player_state.playing = true;
                Ok(publish_display_update(player_state))
            }
            (Some(PlayPaused), state) => Ok(AudioEffects::none(state)),

            (Some(Toggle), Some(mut player_state)) => {
                player_state.playing = !player_state.playing;
                Ok(publish_display_update(player_state))
            }
            (Some(Toggle), None) => Ok(AudioEffects::none(None)),

            (Some(Forward), Some(player_state)) => {
                let mut effects = player_state.forward()?;
                effects.preload_next();

                Ok(effects)
            }
            (Some(Forward), None) => Ok(AudioEffects::none(None)),

            (Some(Back), Some(player_state)) => {
                let mut effects = player_state.back()?;

                effects.preload = effects
                    .player_state
                    .as_ref()
                    .and_then(|state| state.up_next())
                    .map(|up_next| PreloaderAction::Load(up_next.path.clone()));

                Ok(effects)
            }
            (Some(Back), None) => Ok(AudioEffects::none(None)),

            (Some(Seek(proportion)), Some(player_state)) => {
                let Some(ProgressTimes { total, .. }) = player_state
                    .track_info
                    .progress_times(player_state.timestamp)
                else {
                    error!("missing track info: {:#?}", player_state.track_info);
                    return Ok(publish_seek_complete(player_state));
                };

                let mut seek_seconds = total.seconds as f32 * proportion;
                seek_seconds += total.frac as f32 * proportion;

                let player_state = player_state.seek_to(seek_seconds);

                Ok(publish_seek_complete(player_state))
            }
            (Some(Seek(_)), None) => Ok(AudioEffects::none(None)),

            (None, Some(player_state)) if player_state.playing => {
                let before = player_state.queue.current.id;

                let mut effects = player_state.continue_playing()?;

                let after = effects
                    .player_state
                    .as_ref()
                    .map(|state| state.queue.current.id);

                // if we started playing a new song,
                // then try to preload the one after that
                if matches!(after, Some(after) if before != after) {
                    effects.preload_next();
                }

                Ok(effects)
            }
            (None, state) => Ok(AudioEffects::none(state)),
        }
    }
}

type StepResult = anyhow::Result<AudioEffects>;

#[derive(Debug)]
struct AudioEffects {
    /// The new player state; this is 'required'; None = stopped
    player_state: Option<PlayerState>,
    /// ui message to send
    audio_message: Option<AudioMessage>,
    /// metadata to publish to media controls
    metadata: Option<ControlsMetadata>,
    /// playback & progress to publish to media controls
    playback: Option<MediaPlayback>,
    preload: Option<PreloaderAction>,
}

impl AudioEffects {
    /// preserve the player state, doing nothing else
    fn none(player_state: Option<PlayerState>) -> Self {
        Self {
            player_state,
            audio_message: None,
            metadata: None,
            playback: None,
            preload: None,
        }
    }

    /// add a preload action for the next track if there is one
    fn preload_next(&mut self) {
        if let Some(up_next) = self.player_state.as_ref().and_then(PlayerState::up_next) {
            self.preload = Some(PreloaderAction::Load(up_next.path.clone()));
        }
    }
}

impl PlayerState {
    fn play_preloaded(queue: Queue<QueuedSong>, preloaded: PreloadedContent) -> Self {
        Self {
            queue,
            reader: preloaded.reader,
            decoder: preloaded.decoder,
            track_info: preloaded.track_info,
            seek_ts: None,
            audio_output: None,
            playing: true,
            timestamp: Timestamp::ZERO,
            preloaded_content: None,
            predecoded_packets: preloaded.predecoded_packets,
        }
    }

    // This is based on the main loop in the symphonia-play example
    fn play_queue(queue: Queue<QueuedSong>) -> anyhow::Result<Self> {
        let mut hint = Hint::new();

        // Provide the file extension as a hint.
        if let Some(extension) = queue.current.path.extension() {
            hint.with_extension(extension);
        }

        let file = File::open(&queue.current.path)
            .with_context(|| format!("file not found: {}", queue.current.path))?;

        let source = Box::new(file);

        let mss = MediaSourceStream::new(source, Default::default());

        let format_opts = FormatOptions::default();

        let metadata_opts: MetadataOptions = Default::default();

        let reader = symphonia::default::get_probe()
            .probe(&hint, mss, format_opts, metadata_opts)
            .context("The input was not supported by any format reader")?;

        let track =
            first_supported_track(reader.tracks()).context("no playable track")?;
        let track_info: TrackInfo = track.into();
        let params = track
            .codec_params
            .as_ref()
            .and_then(|params| params.audio())
            .context("missing audio codec parameters")?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &Default::default())
            .context("making decoder")?;

        Ok(Self {
            reader,
            seek_ts: None,
            audio_output: None,
            playing: true,
            timestamp: Timestamp::ZERO,
            decoder,
            track_info,
            queue,
            preloaded_content: None,
            predecoded_packets: Default::default(),
        })
    }

    fn seek_to(mut self, target: f32) -> Self {
        let seek_to = SeekTo::Time {
            time: Time::try_from_secs_f64(f64::from(target)).unwrap_or(Time::ZERO),
            track_id: Some(self.track_info.id),
        };

        self.seek_ts = match self.reader.seek(SeekMode::Accurate, seek_to) {
            Ok(seeked_to) => {
                self.decoder.reset();
                self.predecoded_packets.clear();
                Some(seeked_to.required_ts)
            }
            Err(e) => {
                error!("seek error: {e}");
                None
            }
        };

        self
    }

    fn forward(mut self) -> StepResult {
        match self.queue.try_forward() {
            Ok(new_queue) => {
                let mut new_state = match self.preloaded_content {
                    // hit preload
                    Some(preloaded) if preloaded.path == new_queue.current.path => {
                        trace!("hit preload");
                        Self::play_preloaded(new_queue, preloaded)
                    }

                    // missed preload
                    _ => {
                        info!("missed preload");
                        Self::play_queue(new_queue)?
                    }
                };

                new_state.playing = self.playing;

                Ok(publish_display_update(new_state))
            }

            Err(_old_queue) => {
                if let Some(output) = &mut self.audio_output {
                    output.flush();
                }

                Ok(publish_stop())
            }
        }
    }

    fn back(mut self) -> StepResult {
        let past_two_seconds = self
            .track_info
            .progress_times(self.timestamp)
            .map(|p| p.elapsed.seconds >= 2)
            .unwrap_or_default();

        if !past_two_seconds {
            match self.queue.try_back() {
                Ok(new_queue) => {
                    let mut new_state = Self::play_queue(new_queue)?;
                    new_state.playing = self.playing;

                    return Ok(publish_display_update(new_state));
                }
                Err(old_queue) => {
                    self.queue = old_queue;
                }
            }
        }

        let mut new_state = self.seek_to(0.0);
        new_state.timestamp = Timestamp::ZERO;

        Ok(publish_display_update(new_state))
    }

    // This is based on the main loop in the symphonia-play example
    fn continue_playing(self) -> StepResult {
        let mut player_state = self;

        let (timestamp, decoded) = {
            if let Some(PredecodedPacket { timestamp, decoded }) =
                player_state.predecoded_packets.pop_front()
            {
                (timestamp, DecodedPacket::Preloaded((timestamp, decoded)))
            } else {
                // Get the next packet from the format reader.
                let packet = match player_state.reader.next_packet() {
                    Ok(Some(packet)) => packet,
                    Ok(None) => {
                        if let Some(mut output) = player_state.audio_output.take() {
                            output.flush();
                        }
                        return player_state.forward();
                    }

                    Err(error) => {
                        bail!("error reading next packet: {error}");
                    }
                };

                if packet.track_id != player_state.track_info.id {
                    return Ok(AudioEffects::none(Some(player_state)));
                }
                let decoded = match player_state.decoder.decode(&packet) {
                    Ok(decoded) => DecodedPacket::JustDecoded(decoded),

                    Err(SymphoniaError::DecodeError(err)) => {
                        // Decode errors are not fatal.
                        // Print the error message and try to decode the next packet as usual.
                        warn!("decode error: {}", err);
                        return Ok(AudioEffects::none(Some(player_state)));
                    }

                    Err(err) => bail!("failed to read packet: {err}"),
                };

                (packet.pts, decoded)
            }
        };

        // If the audio output is not open, try to open it.
        if player_state.audio_output.is_none() {
            // Get the audio buffer specification. This is a description of the decoded
            // audio buffer's sample format and sample rate.
            let spec = decoded.spec().clone();

            // Get the capacity of the decoded buffer. Note that this is capacity, not
            // length! The capacity of the decoded buffer is constant for the life of the
            // decoder, but the length is not.
            let duration = decoded.capacity() as u64;

            // FIXME this needs to happen much earlier,
            //   but that means we need to be able to swap out the spec,
            //   and reallocate  based on changing duration?
            // Try to open the audio output.
            let new_audio_output =
                output::try_open(spec, duration).context("opening audio device")?;
            player_state.audio_output.replace(new_audio_output);
        }

        // Write the decoded audio samples to the audio output
        // If the timestamp for the packet is >= a seek position,
        // then continue 'playing' until seek is reached.
        player_state.timestamp = timestamp;
        let seeking = player_state
            .seek_ts
            .map(|seek_ts| timestamp < seek_ts)
            .unwrap_or_default();
        if seeking {
            return Ok(AudioEffects::none(Some(player_state)));
        } else {
            // when a seek is complete, return to publishing the real timestamp
            player_state.seek_ts = None;
        }

        let audio_output: &mut dyn AudioOutput = player_state
            .audio_output
            .as_deref_mut()
            .ok_or_else(|| anyhow!("no audio device"))?;

        match decoded {
            DecodedPacket::Preloaded((_ts, buf)) => audio_output
                .write(buf.as_generic_audio_buffer_ref())
                .context("writing preloaded audio")?,

            DecodedPacket::JustDecoded(buf) => {
                audio_output.write(buf).context("writing audio")?
            }
        }

        Ok(publish_display_update(player_state))
    }

    // NOTE This is to avoid flashing the 'old' timestamp while seeking
    // to the new timestamp; we publish the timestamp where we're going to.
    // This relies on resetting seek_ts to None in continue_playing
    // when the seek is complete.
    fn optimistic_timestamp(&self) -> Timestamp {
        self.seek_ts.unwrap_or(self.timestamp)
    }

    fn up_next(&self) -> Option<&QueuedSong> {
        self.queue.next.iter().next()
    }
}

enum DecodedPacket<'a> {
    Preloaded((Timestamp, AnyAudioBuffer)),
    JustDecoded(GenericAudioBufferRef<'a>),
}

impl<'a> DecodedPacket<'a> {
    fn spec(&self) -> &symphonia::core::audio::AudioSpec {
        match self {
            DecodedPacket::Preloaded((_ts, buf)) => buf.spec(),
            DecodedPacket::JustDecoded(buf) => buf.spec(),
        }
    }

    fn capacity(&self) -> usize {
        match self {
            DecodedPacket::Preloaded((_ts, buf)) => buf.capacity(),
            DecodedPacket::JustDecoded(buf) => buf.capacity(),
        }
    }
}

fn publish_display_update(new_state: PlayerState) -> AudioEffects {
    let (display, metadata, playback) = prepare_publish(&new_state);

    AudioEffects {
        player_state: Some(new_state),
        audio_message: Some(AudioMessage::DisplayUpdate(Some(display))),
        metadata: Some(metadata),
        playback: Some(playback),
        preload: None,
    }
}

fn publish_seek_complete(new_state: PlayerState) -> AudioEffects {
    let (display, metadata, playback) = prepare_publish(&new_state);

    AudioEffects {
        player_state: Some(new_state),
        audio_message: Some(AudioMessage::SeekComplete(display)),
        metadata: Some(metadata),
        playback: Some(playback),
        preload: None,
    }
}

fn publish_stop() -> AudioEffects {
    AudioEffects {
        audio_message: Some(AudioMessage::DisplayUpdate(None)),
        player_state: None,
        metadata: None,
        playback: None,
        preload: None,
    }
}

fn prepare_publish(
    new_state: &PlayerState,
) -> (PlayerDisplay, ControlsMetadata, MediaPlayback) {
    let current = &new_state.queue.current;

    let cover_url = current
        .resized_art
        .as_ref()
        .map(|path| format!("file://{path}"));

    let metadata = ControlsMetadata {
        title: current.title.clone(),
        album: current.album_title.clone(),
        artist: current.artist.clone(),
        duration: current.duration,
        cover_url,
    };

    let timestamp = new_state.optimistic_timestamp();
    let progress = new_state
        .track_info
        .progress_times(timestamp)
        .map(|progress| MediaPosition(Duration::from_secs(progress.elapsed.seconds)));

    let playback = if new_state.playing {
        MediaPlayback::Playing { progress }
    } else {
        MediaPlayback::Paused { progress }
    };

    let display: PlayerDisplay = new_state.into();

    (display, metadata, playback)
}

#[cfg(test)]
#[path = "player/regression_tests.rs"]
mod regression_tests;
