use flume::Receiver;

use crate::app::old_unfold::old_unfold;
use clef_audio::player::AudioMessage;

pub fn audio_subscription(
    inbox: Receiver<AudioMessage>,
    generation: u64,
) -> cosmic::iced::Subscription<AudioMessage> {
    struct AudioSub;

    old_unfold(
        (std::any::TypeId::of::<AudioSub>(), generation),
        || false,
        move |disconnected| {
            let inbox = inbox.clone();

            async move {
                if disconnected {
                    return cosmic::iced::futures::future::pending().await;
                }

                match inbox.recv_async().await {
                    Ok(message) => (Some(message), false),
                    Err(_) => (Some(AudioMessage::AudioDied), true),
                }
            }
        },
    )
}
