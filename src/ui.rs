use std::sync::Arc;

use flume::{Receiver, Sender};
use iced::executor;
use iced::widget::{button, column, slider, vertical_space};
use iced::{Alignment, Application, Command, Length, Subscription, Theme};
use log::error;
use parking_lot::Mutex;

use crate::channels::{self, ProgressTimes, ToAudio, ToUi};

mod icons;
mod startup;
use startup::*;

#[derive(Debug)]
pub struct Ui {
    player_state: PlayerStateView,
    inbox: Arc<Mutex<Receiver<channels::ToUi>>>,
    to_audio: Arc<Mutex<Sender<channels::ToAudio>>>,
    should_exit: bool,
    progress: Option<ProgressTimes>,
    music_dir: Option<MusicDir>,
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
    GotMusicDir(Result<MusicDir, MusicDirError>),
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

impl Application for Ui {
    type Flags = Flags;
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;

    fn new(flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        let initial_state = Self {
            player_state: PlayerStateView::Stopped,
            inbox: flags.inbox,
            to_audio: flags.to_audio,
            should_exit: false,
            progress: None,
            music_dir: None,
        };

        let initial_command = Command::perform(crawl_music_dir(), Message::GotMusicDir);

        (initial_state, initial_command)
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
            Message::GotMusicDir(Ok(music_dir)) => {
                self.music_dir = Some(music_dir);

                Command::none()
            }
            // this is logged in the task; could show an error toast or something
            Message::GotMusicDir(Err(_)) => Command::none(),

            Message::PlayClicked => {
                match self.player_state {
                    PlayerStateView::Playing => {}

                    PlayerStateView::Paused => {
                        self.player_state = PlayerStateView::Playing;
                        self.send_to_audio(ToAudio::PlayPaused);
                    }

                    PlayerStateView::Stopped => {
                        let example_file = String::from(THE_WIND_THAT_SHAKES_THE_LAND);
                        self.player_state = PlayerStateView::Playing;
                        self.send_to_audio(ToAudio::PlayFilename(example_file));
                    }
                }

                Command::none()
            }

            Message::PauseClicked => {
                self.player_state = PlayerStateView::Paused;
                self.send_to_audio(ToAudio::Pause);
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

        let progress_slider = match &self.progress {
            Some(times) => {
                let elapsed: f32 = times.elapsed.seconds as f32 + times.elapsed.frac as f32;
                let total: f32 = times.total.seconds as f32 + times.total.frac as f32;
                let progress = 100.0 * (elapsed / total);
                slider(0.0..=100.0, progress, Message::Seek).step(0.01)
            }
            None => slider(0.0..=100.0, 0.0, Message::Seek).step(0.01),
        };

        let content = match &self.music_dir {
            Some(_music_dir) => {
                // TODO
                vertical_space(Length::Fill)
            }
            None => vertical_space(Length::Fill),
        };

        column![
            content,
            play_pause_button,
            vertical_space(Length::Units(10)),
            progress_slider
        ]
        .padding(20)
        .width(Length::Fill)
        .align_items(Alignment::Center)
        .into()
    }
}

// TODO remove these
// #[allow(unused)]
// const BENNY_HILL: &str = "/home/giesch/Music/Benny Hill/Benny Hill - Theme Song.mp3";
#[allow(unused)]
const THE_WIND_THAT_SHAKES_THE_LAND: &str = "/home/giesch/Music/Unleash The Archers - Abyss/Unleash The Archers - Abyss - 08 The Wind that Shapes the Land.mp3";
