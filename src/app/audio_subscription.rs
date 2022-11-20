use flume::{Receiver, TryRecvError};

use crate::audio::player::AudioMessage;

#[derive(Debug, PartialEq, Eq)]
enum AudioSubState {
    Ready,
    Disconnected,
}

pub fn audio_subscription(
    inbox: Receiver<AudioMessage>,
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
    inbox: Receiver<AudioMessage>,
) -> (Option<AudioMessage>, AudioSubState) {
    if state == AudioSubState::Disconnected {
        return (None, AudioSubState::Disconnected);
    }

    match inbox.try_recv() {
        Ok(msg) => (Some(msg), AudioSubState::Ready),

        Err(TryRecvError::Empty) => (None, AudioSubState::Ready),

        Err(TryRecvError::Disconnected) => {
            (Some(AudioMessage::AudioDied), AudioSubState::Disconnected)
        }
    }
}
