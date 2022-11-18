use iced::Command;

use crate::app::resizer::ResizeRequest;
use crate::channels::AudioAction;

use super::media_controls::ControlsMetadata;

#[derive(Debug)]
pub enum Effect<Message> {
    None,
    #[allow(unused)] // will need this for one-off commands
    Command(Command<Message>),
    ToAudio(AudioAction),
    ToResizer(ResizeRequest),
    ControlsMetadata(ControlsMetadata),
    Batch(Vec<Effect<Message>>),
}

impl<Message> Default for Effect<Message> {
    fn default() -> Self {
        Self::none()
    }
}

impl<Message> Effect<Message> {
    pub fn none() -> Self {
        Self::None
    }

    pub fn batch(effects: impl IntoIterator<Item = Self>) -> Self {
        Self::Batch(effects.into_iter().collect())
    }
}

impl<Message> From<ResizeRequest> for Effect<Message> {
    fn from(resize: ResizeRequest) -> Self {
        Self::ToResizer(resize)
    }
}

impl<Message> From<Option<ResizeRequest>> for Effect<Message> {
    fn from(resize: Option<ResizeRequest>) -> Self {
        resize.map(Self::ToResizer).unwrap_or_default()
    }
}

impl<Message> From<AudioAction> for Effect<Message> {
    fn from(to_audio: AudioAction) -> Self {
        Self::ToAudio(to_audio)
    }
}

impl<Message> From<Option<AudioAction>> for Effect<Message> {
    fn from(to_audio: Option<AudioAction>) -> Self {
        to_audio.map(Self::ToAudio).unwrap_or_default()
    }
}

impl<Message> From<ControlsMetadata> for Effect<Message> {
    fn from(metadata: ControlsMetadata) -> Self {
        Self::ControlsMetadata(metadata)
    }
}
