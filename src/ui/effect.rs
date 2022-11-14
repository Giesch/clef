use iced::Command;

use crate::channels::AudioAction;
use crate::ui::resizer::ResizeRequest;

pub enum Effect<Message> {
    Command(Command<Message>),
    ToAudio(AudioAction),
    ToResizer(ResizeRequest),
}

impl<Message> Default for Effect<Message> {
    fn default() -> Self {
        Self::none()
    }
}

impl<Message> Effect<Message> {
    pub fn none() -> Self {
        Self::Command(Command::none())
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
