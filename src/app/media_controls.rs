use std::sync::Arc;
use std::time::Duration;

use flume::{Receiver, TryRecvError};
use parking_lot::Mutex;
use souvlaki::{MediaControlEvent, MediaMetadata};

// an owned version of `souvlaki::MediaMetadata`
#[derive(Debug)]
pub struct ControlsMetadata {
    pub title: Option<String>,
    pub album: Option<String>,
    pub artist: Option<String>,
    pub cover_url: Option<String>,
    pub duration: Option<Duration>,
}

impl<'a> From<&'a ControlsMetadata> for MediaMetadata<'a> {
    fn from(metadata: &'a ControlsMetadata) -> Self {
        MediaMetadata {
            title: metadata.title.as_deref(),
            album: metadata.album.as_deref(),
            artist: metadata.artist.as_deref(),
            cover_url: metadata.cover_url.as_deref(),
            duration: metadata.duration,
        }
    }
}

// Subscription

pub fn media_controls_subscription(
    from_controls: Arc<Mutex<Receiver<MediaControlEvent>>>,
) -> iced::Subscription<MediaControlEvent> {
    struct MediaControlsSub;

    iced::subscription::unfold(
        std::any::TypeId::of::<MediaControlsSub>(),
        MediaControlsState::Ready,
        move |state| listen_for_media_controls(state, from_controls.clone()),
    )
}

#[derive(Debug, PartialEq, Eq)]
enum MediaControlsState {
    Ready,
    Disconnected,
}

async fn listen_for_media_controls(
    state: MediaControlsState,
    from_controls: Arc<Mutex<Receiver<MediaControlEvent>>>,
) -> (Option<MediaControlEvent>, MediaControlsState) {
    if state == MediaControlsState::Disconnected {
        return (None, MediaControlsState::Disconnected);
    }

    match from_controls.lock().try_recv() {
        Ok(msg) => (Some(msg), MediaControlsState::Ready),

        Err(TryRecvError::Empty) => (None, MediaControlsState::Ready),

        Err(TryRecvError::Disconnected) => (None, MediaControlsState::Disconnected),
    }
}
