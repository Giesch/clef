use std::collections::VecDeque;
use std::fmt::Debug;
use std::thread;
use std::{sync::Arc, thread::JoinHandle};

use camino::Utf8PathBuf;
use flume::{Receiver, Sender, TryRecvError};
use parking_lot::Mutex;
use symphonia::core::units::Time;

use crate::audio::player::Player;
use crate::db::queries::SongId;

/// An mpsc message to the audio thread from the ui
#[derive(Debug, Clone, PartialEq)]
pub enum AudioAction {
    /// Begin playing the file (0) immediately,
    /// and continue playing files from the queue (1) when it ends
    PlayQueue(Queue<(SongId, Utf8PathBuf)>),
    /// Pause the currently playing song, if any
    Pause,
    /// Play the currently paused song, if any
    PlayPaused,
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
pub struct Queue<T>
where
    T: Debug + Clone + PartialEq + Eq,
{
    pub previous: Vec<T>,
    pub current: T,
    pub next: VecDeque<T>,
}

/// An mpsc message to the main/ui thread from audio
#[derive(Debug, Clone, PartialEq)]
pub enum AudioMessage {
    /// A change that affects ui state; None = player stopped
    DisplayUpdate(Option<PlayerDisplay>),

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

pub fn spawn_player(
    inbox: Receiver<AudioAction>,
    to_ui: Sender<AudioMessage>,
) -> std::result::Result<JoinHandle<()>, std::io::Error> {
    thread::Builder::new()
        .name("ClefAudioPlayer".to_string())
        .spawn(move || {
            let player = Player::new(inbox, to_ui.clone());
            if let Err(err) = player.run_loop() {
                to_ui.send(AudioMessage::AudioDied).ok();

                panic!("unrecovered error: {:?}", err);
            }
        })
}

// Iced Integration

#[derive(Debug, PartialEq, Eq)]
enum AudioSubState {
    Ready,
    Disconnected,
}

pub fn audio_subscription(
    inbox: Arc<Mutex<Receiver<AudioMessage>>>,
) -> iced::Subscription<AudioMessage> {
    struct AudioSub;

    iced::subscription::unfold(
        std::any::TypeId::of::<AudioSub>(),
        AudioSubState::Ready,
        move |state| listen(state, inbox.clone()),
    )
}

async fn listen(
    state: AudioSubState,
    inbox: Arc<Mutex<Receiver<AudioMessage>>>,
) -> (Option<AudioMessage>, AudioSubState) {
    if state == AudioSubState::Disconnected {
        return (None, AudioSubState::Disconnected);
    }

    match inbox.lock().try_recv() {
        Ok(msg) => (Some(msg), AudioSubState::Ready),

        Err(TryRecvError::Empty) => (None, AudioSubState::Ready),

        Err(TryRecvError::Disconnected) => {
            (Some(AudioMessage::AudioDied), AudioSubState::Disconnected)
        }
    }
}
