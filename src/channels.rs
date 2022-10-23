use std::sync::Arc;

use flume::{Receiver, Sender, TryRecvError};
use parking_lot::Mutex;
use symphonia::core::units::Time;

use crate::audio::player::Player;

// A message to the audio thread
#[derive(Debug, Clone)]
pub enum ToAudio {
    PlayFilename(String),
}

// A message to the main/ui thread
#[derive(Debug, Clone)]
pub enum ToUi {
    ProgressPercentage { elapsed: Time, remaining: Time },
    AudioDied,
}

pub fn spawn_player(inbox: Receiver<ToAudio>, to_ui: Sender<ToUi>) {
    std::thread::spawn(move || {
        let mut player = Player::new(inbox, to_ui.clone());
        // TODO need better traces for this
        if let Err(err) = player.run() {
            player.flush();

            // TODO try to restart the player instead of dying

            to_ui.send(ToUi::AudioDied).ok();

            panic!("unrecovered error from audio thread: {err}");
        }
    });
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
