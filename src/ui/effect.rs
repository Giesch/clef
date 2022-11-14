use iced::Command;

use crate::channels::AudioAction;
use crate::ui::resizer::ResizeRequest;

#[derive(Debug)]
pub enum Effect<Message> {
    #[allow(unused)] // will need this for one-off commands
    Command(Command<Message>),
    ToAudio(AudioAction),
    ToResizer(ResizeRequest),
    None,
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
    fn from(resize: Option<AudioAction>) -> Self {
        resize.map(Self::ToAudio).unwrap_or_default()
    }
}
