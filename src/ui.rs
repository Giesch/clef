use std::sync::Arc;

use flume::{Receiver, Sender};
use iced::executor;
use iced::widget::{button, column, vertical_space};
use iced::{Alignment, Application, Command, Length, Subscription, Theme};
use log::{debug, error};
use parking_lot::Mutex;

use crate::channels::{self, ToAudio, ToUi};

mod icons;

#[derive(Debug)]
pub struct Ui {
    player_state: PlayerStateView,
    inbox: Arc<Mutex<Receiver<channels::ToUi>>>,
    to_audio: Arc<Mutex<Sender<channels::ToAudio>>>,
    should_exit: bool,
}

#[derive(Debug)]
enum PlayerStateView {
    Playing,
    Paused,
    Stopped,
}

#[derive(Debug)]
pub struct Flags {
    pub inbox: Arc<Mutex<Receiver<channels::ToUi>>>,
    pub to_audio: Arc<Mutex<Sender<channels::ToAudio>>>,
}

#[derive(Debug, Clone)]
pub enum Message {
    PlayClicked,
    PauseClicked,
    FromAudio(ToUi),
}

impl Application for Ui {
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;
    type Flags = Flags;

    fn new(flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        let initial_state = Self {
            player_state: PlayerStateView::Stopped,
            inbox: flags.inbox,
            to_audio: flags.to_audio,
            should_exit: false,
        };

        (initial_state, Command::none())
    }

    fn title(&self) -> String {
        String::from("Clef")
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn should_exit(&self) -> bool {
        self.should_exit
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        debug!("in update; message: {:?}", message);

        match message {
            Message::PlayClicked => {
                match self.player_state {
                    PlayerStateView::Playing => {}

                    PlayerStateView::Paused => {
                        self.player_state = PlayerStateView::Playing;

                        self.to_audio
                            .lock()
                            .send(ToAudio::PlayPaused)
                            .unwrap_or_else(|e| error!("failed to send to audio: {e}"));
                    }

                    PlayerStateView::Stopped => {
                        let file_name = String::from(BENNY_HILL);
                        self.player_state = PlayerStateView::Playing;

                        self.to_audio
                            .lock()
                            .send(ToAudio::PlayFilename(file_name))
                            .unwrap_or_else(|e| error!("failed to send to audio: {e}"));
                    }
                }

                Command::none()
            }

            Message::PauseClicked => {
                self.player_state = PlayerStateView::Paused;

                self.to_audio
                    .lock()
                    .send(ToAudio::Pause)
                    .unwrap_or_else(|e| error!("failed to send to audio: {e}"));

                Command::none()
            }

            Message::FromAudio(ToUi::ProgressPercentage { .. }) => {
                // TODO this message should have a percentage instead of a remaining
                Command::none()
            }

            Message::FromAudio(ToUi::AudioDied) => {
                self.should_exit = true;

                Command::none()
            }
        }
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        channels::audio_subscription(self.inbox.clone()).map(Message::FromAudio)
    }

    fn view(&self) -> iced::Element<'_, Self::Message, iced::Renderer<Self::Theme>> {
        let play_pause_button = match self.player_state {
            PlayerStateView::Playing => button(icons::pause()).on_press(Message::PauseClicked),
            PlayerStateView::Paused => button(icons::play()).on_press(Message::PlayClicked),
            PlayerStateView::Stopped => button(icons::play()).on_press(Message::PlayClicked),
        };

        column![vertical_space(Length::Fill), play_pause_button]
            .padding(20)
            .width(Length::Fill)
            .align_items(Alignment::Center)
            .into()
    }
}

const BENNY_HILL: &str = "/home/giesch/Music/Benny Hill/Benny Hill - Theme Song.mp3";
