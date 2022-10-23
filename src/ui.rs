use flume::{Receiver, Sender};
use iced::executor;
use iced::widget::{button, column, vertical_space};
use iced::{Alignment, Application, Command, Length, Theme};

use crate::channels::{self, ToAudio};

pub struct Ui {
    inbox: Receiver<channels::ToUi>,
    to_audio: Sender<channels::ToAudio>,
    playing: bool,
}

pub struct Flags {
    pub inbox: Receiver<channels::ToUi>,
    pub to_audio: Sender<channels::ToAudio>,
}

#[derive(Debug, Clone, Copy)]
pub enum Message {
    PlayClicked,
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
            playing: false,
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
                    .send(ToAudio::PlayFilename(file_name))
                    .unwrap();

                Command::none()
            }
        }
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
