use std::collections::VecDeque;
use std::thread;
use std::{sync::Arc, thread::JoinHandle};

use camino::Utf8PathBuf;
use flume::{Receiver, Sender, TryRecvError};
use parking_lot::Mutex;
use symphonia::core::units::Time;

use crate::audio::player::Player;

/// An mpsc message to the audio thread
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToAudio {
    PlayFilename(Utf8PathBuf),
    PlayQueue((Utf8PathBuf, VecDeque<Utf8PathBuf>)),
    Pause,
    PlayPaused,
}

/// An mpsc message to the main/ui thread
#[derive(Debug, Clone, PartialEq)]
pub enum ToUi {
    /// The player has made progress on the current song
    Progress(ProgressTimes),
    /// The player has continued to the next song in the queue
    NextSong(Utf8PathBuf),
    /// The player reached the end of the queue
    Stopped,
    /// The player thread died
    AudioDied,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProgressTimes {
    pub elapsed: Time,
    pub remaining: Time,
    pub total: Time,
}

pub fn spawn_player(
    inbox: Receiver<ToAudio>,
    to_ui: Sender<ToUi>,
) -> std::result::Result<JoinHandle<()>, std::io::Error> {
    thread::Builder::new()
        .name("ClefAudioPlayer".to_string())
        .spawn(move || {
            let player = Player::new(inbox, to_ui.clone());
            if let Err(err) = player.run_loop() {
                to_ui.send(ToUi::AudioDied).ok();

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

pub fn audio_subscription(inbox: Arc<Mutex<Receiver<ToUi>>>) -> iced::Subscription<ToUi> {
    struct AudioSub;

    iced::subscription::unfold(
        std::any::TypeId::of::<AudioSub>(),
        AudioSubState::Ready,
        move |state| listen(state, inbox.clone()),
    )
}

async fn listen(
    state: AudioSubState,
    inbox: Arc<Mutex<Receiver<ToUi>>>,
) -> (Option<ToUi>, AudioSubState) {
    if state == AudioSubState::Disconnected {
        return (None, AudioSubState::Disconnected);
    }

    match inbox.lock().try_recv() {
        Ok(msg) => (Some(msg), AudioSubState::Ready),

        Err(TryRecvError::Empty) => (None, AudioSubState::Ready),

        Err(TryRecvError::Disconnected) => (None, AudioSubState::Disconnected),
    }
}
