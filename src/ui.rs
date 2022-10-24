use std::collections::HashMap;
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use flume::{Receiver, Sender};
use iced::executor;
use iced::widget::{button, column, slider, vertical_space};
use iced::{Alignment, Application, Command, Length, Subscription, Theme};
use log::{error, info};
use parking_lot::Mutex;
use walkdir::WalkDir;

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

const BENNY_HILL: &str = "/home/giesch/Music/Benny Hill/Benny Hill - Theme Song.mp3";

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
            Message::GotMusicDir(music_dir) => Command::none(),

            Message::PlayClicked => {
                match self.player_state {
                    PlayerStateView::Playing => {}

                    PlayerStateView::Paused => {
                        self.player_state = PlayerStateView::Playing;
                        self.send_to_audio(ToAudio::PlayPaused);
                    }

                    PlayerStateView::Stopped => {
                        self.player_state = PlayerStateView::Playing;
                        self.send_to_audio(ToAudio::PlayFilename(String::from(BENNY_HILL)));
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

        column![
            vertical_space(Length::Fill),
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

#[derive(Debug, Clone)]
pub struct MusicDir(Vec<AlbumFolder>);

#[derive(thiserror::Error, Debug, Clone)]
pub enum MusicDirError {
    #[error("error walking directory")]
    WalkError,
}

async fn crawl_music_dir() -> Result<MusicDir, MusicDirError> {
    let mut file_names_by_dir: HashMap<Utf8PathBuf, Vec<String>> = HashMap::new();
    for dir_entry in WalkDir::new("/home/giesch/Music").into_iter() {
        let dir_entry = match dir_entry {
            Ok(dir_entry) => dir_entry,
            Err(e) => {
                error!("error walking music directory: {e}");
                return Err(MusicDirError::WalkError);
            }
        };

        let path: &Utf8Path = match dir_entry.path().try_into() {
            Ok(utf8) => utf8,
            Err(e) => {
                info!("skipping file with invalid utf8: {e}");
                continue;
            }
        };

        if let Some(file_name) = path.file_name() {
            let dir_name = path.with_file_name("");
            let file_names = file_names_by_dir.entry(dir_name).or_insert_with(Vec::new);
            file_names.push(file_name.to_string());
        }
    }

    let albums = parse_albums(file_names_by_dir);

    Ok(MusicDir(albums))
}

#[derive(Debug, Clone)]
struct AlbumFolder {
    directory: Utf8PathBuf,
    cover_paths: Vec<Utf8PathBuf>,
    tracks: Vec<AlbumFolderTrack>,
}

impl AlbumFolder {
    fn new(directory: Utf8PathBuf) -> Self {
        Self {
            directory,
            cover_paths: Vec::new(),
            tracks: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
struct AlbumFolderTrack {
    // TODO include decoded mp3 stuff
    path: Utf8PathBuf,
}

impl AlbumFolderTrack {
    fn new(path: Utf8PathBuf) -> Self {
        Self { path }
    }
}

fn parse_albums(file_names_by_dir: HashMap<Utf8PathBuf, Vec<String>>) -> Vec<AlbumFolder> {
    let mut albums: Vec<AlbumFolder> = Vec::new();

    for (directory, files) in file_names_by_dir {
        let mut album = AlbumFolder::new(directory.clone());
        for file in files {
            if is_music(&file) {
                let music_path = directory.with_file_name(&file);
                let track = AlbumFolderTrack::new(music_path);
                album.tracks.push(track);
            } else if is_cover_art(&file) {
                let cover_path = directory.with_file_name(&file);
                album.cover_paths.push(cover_path);
            }
        }

        albums.push(album);
    }

    albums
}

fn is_cover_art(file_name: &str) -> bool {
    file_name.ends_with(".jpeg")
}

fn is_music(file_name: &str) -> bool {
    file_name.ends_with(".mp3")
}
