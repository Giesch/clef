use std::sync::Arc;

use flume::{Receiver, Sender};
use iced::executor;
use iced::widget::{button, column, vertical_space};
use iced::{Alignment, Application, Command, Length, Subscription, Theme};
use parking_lot::Mutex;

use crate::channels::{self, ToAudio, ToUi};

pub struct Ui {
    inbox: Arc<Mutex<Receiver<channels::ToUi>>>,
    to_audio: Arc<Mutex<Sender<channels::ToAudio>>>,
}

pub struct Flags {
    pub inbox: Arc<Mutex<Receiver<channels::ToUi>>>,
    pub to_audio: Arc<Mutex<Sender<channels::ToAudio>>>,
}

#[derive(Debug, Clone)]
pub enum Message {
    PlayClicked,
    FromAudio(ToUi),
}

impl Application for Ui {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = Flags;

    fn new(flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        let initial_state = Self {
            inbox: flags.inbox,
            to_audio: flags.to_audio,
        };

        (initial_state, Command::none())
    }

    fn title(&self) -> String {
        String::from("Clef")
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        match message {
            Message::PlayClicked => {
                let file_name = String::from(BENNY_HILL);
                self.to_audio
                    .lock()
                    .send(ToAudio::PlayFilename(file_name))
                    .unwrap_or_else(|e| log::error!("failed to send to audio: {e}"));

                Command::none()
            }

            Message::FromAudio(msg) => {
                log::info!("message from audio thread: {:?}", msg);
                Command::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        channels::audio_subscription(self.inbox.clone()).map(Message::FromAudio)
    }

    fn view(&self) -> iced::Element<'_, Self::Message, iced::Renderer<Self::Theme>> {
        column![
            vertical_space(Length::Fill),
            button("Play").on_press(Message::PlayClicked)
        ]
        .padding(20)
        .width(Length::Fill)
        .align_items(Alignment::Center)
        .into()
    }
}

const BENNY_HILL: &str = "/home/giesch/Music/Benny Hill/Benny Hill - Theme Song.mp3";
