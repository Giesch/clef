use std::sync::Arc;

use flume::{Receiver, Sender};
use iced::executor;
use iced::widget::{button, column, slider, vertical_space};
use iced::{Alignment, Application, Command, Length, Subscription, Theme};
use log::error;
use parking_lot::Mutex;

use crate::channels::{self, ProgressTimes, ToAudio, ToUi};

mod icons;

#[derive(Debug)]
pub struct Ui {
    player_state: PlayerStateView,
    inbox: Arc<Mutex<Receiver<channels::ToUi>>>,
    to_audio: Arc<Mutex<Sender<channels::ToAudio>>>,
    should_exit: bool,
    progress: Option<ProgressTimes>,
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
    Seek(f32),
}

impl Ui {
    fn send_to_audio(&mut self, to_audio: ToAudio) {
        self.to_audio
            .lock()
            .send(to_audio)
            .unwrap_or_else(|e| error!("failed to send to audio thread: {e}"));
    }
}

const BENNY_HILL: &str = "/home/giesch/Music/Benny Hill/Benny Hill - Theme Song.mp3";

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
            progress: None,
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
        match message {
            Message::PlayClicked => {
                match self.player_state {
                    PlayerStateView::Playing => {}

                    PlayerStateView::Paused => {
                        self.player_state = PlayerStateView::Playing;
                        self.send_to_audio(ToAudio::PlayPaused);
                    }

                    PlayerStateView::Stopped => {
                        let file_name = String::from(BENNY_HILL);
                        self.player_state = PlayerStateView::Playing;
                        self.send_to_audio(ToAudio::PlayFilename(file_name));
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

            Message::Seek(_) => {
                // TODO emit seek to audio
                Command::none()
            }

            Message::FromAudio(ToUi::Progress(times)) => {
                self.progress = Some(times);
                Command::none()
            }

            Message::FromAudio(ToUi::Stopped) => {
                self.player_state = PlayerStateView::Stopped;
                self.progress = None;
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

        let slide = match &self.progress {
            Some(times) => {
                let progress = 100.0 * (times.elapsed.seconds as f32 / times.total.seconds as f32);
                slider(0.0..=100.0, progress, Message::Seek).step(0.01)
            }
            None => slider(0.0..=100.0, 0.0, Message::Seek).step(0.01),
        };

        column![
            vertical_space(Length::Fill),
            play_pause_button,
            vertical_space(Length::Units(10)),
            slide
        ]
        .padding(20)
        .width(Length::Fill)
        .align_items(Alignment::Center)
        .into()
    }
}
