use std::collections::HashMap;
use std::sync::Arc;

use camino::Utf8PathBuf;
use flume::{Receiver, Sender};
use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, slider, text, vertical_space,
    Column, Container, Image, Space,
};
use iced::{alignment, executor};
use iced::{Alignment, Application, Command, ContentFit, Element, Length, Subscription, Theme};
use log::error;
use parking_lot::Mutex;

use crate::channels::{self, ProgressTimes, ToAudio, ToUi};

mod icons;
mod startup;
use startup::*;
mod bgra;
use bgra::*;
mod data;
use data::*;

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
    Stopped,
    Started(CurrentSongState),
}

#[derive(Debug)]
struct CurrentSongState {
    playing: bool, // false means paused
    current: CurrentSongView,
}

#[derive(Debug)]
struct CurrentSongView {
    title: String,
    album: Option<String>,
    artist: Option<String>,
}

impl CurrentSongView {
    pub fn from_song(song: &TaggedSong) -> Self {
        Self {
            title: song.display_title().to_owned(),
            album: song.album_title().map(str::to_owned),
            artist: song.artist().map(str::to_owned),
        }
    }
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
    PlaySongClicked(SongId),
    PauseClicked,
    FromAudio(ToUi),
    Seek(f32),
    LoadedImages(Option<HashMap<Utf8PathBuf, BgraBytes>>),
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
                let image_paths: Vec<_> = music_dir
                    .albums()
                    .iter()
                    .flat_map(|album| album.covers.first())
                    .cloned()
                    .collect();

                self.music_dir = Some(music_dir);

                Command::perform(load_images(image_paths), Message::LoadedImages)
            }
            Message::GotMusicDir(Err(_)) => Command::none(),

            Message::LoadedImages(Some(loaded_images_by_path)) => {
                match &mut self.music_dir {
                    Some(music_dir) => {
                        music_dir.add_album_covers(loaded_images_by_path);
                    }

                    None => {
                        error!("loaded images before music directory")
                    }
                }

                Command::none()
            }

            Message::LoadedImages(None) => {
                error!("failed to load images");
                Command::none()
            }

            Message::PlayClicked => {
                match &mut self.player_state {
                    PlayerStateView::Stopped => {}
                    PlayerStateView::Started(CurrentSongState { playing, .. }) if *playing => {}
                    PlayerStateView::Started(current_song_state) => {
                        current_song_state.playing = true;
                        self.send_to_audio(ToAudio::PlayPaused);
                    }
                }

                Command::none()
            }

            Message::PlaySongClicked(song_id) => {
                match &self.music_dir {
                    Some(music_dir) => {
                        let song = music_dir.get_song(&song_id);
                        let current = CurrentSongView::from_song(song);
                        let current_song_state = CurrentSongState {
                            playing: true,
                            current,
                        };
                        self.player_state = PlayerStateView::Started(current_song_state);

                        // NOTE this clone is necessary unless we want to have some kind of
                        // shared in-memory storage with the audio thread,
                        // or we let the audio thread have access to sqlite in the future
                        self.send_to_audio(ToAudio::PlayFilename(song.path.clone()));
                    }
                    None => {
                        error!("play clicked before music loaded");
                    }
                }
                Command::none()
            }

            Message::PauseClicked => {
                match &mut self.player_state {
                    PlayerStateView::Stopped => {}
                    PlayerStateView::Started(current_song_state) => {
                        current_song_state.playing = false;
                    }
                };

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
        let bottom_row = view_bottom_row(&self.player_state);

        let progress_slider = match &self.progress {
            Some(times) => {
                let elapsed: f32 = times.elapsed.seconds as f32 + times.elapsed.frac as f32;
                let total: f32 = times.total.seconds as f32 + times.total.frac as f32;
                let progress = 100.0 * (elapsed / total);
                slider(0.0..=100.0, progress, Message::Seek).step(0.01)
            }
            None => slider(0.0..=100.0, 0.0, Message::Seek).step(0.01),
        };

        let content: Element<'_, Message> = match &self.music_dir {
            Some(music_dir) => view_album_list(music_dir).into(),
            None => vertical_space(Length::Fill).into(),
        };

        let content = fill_container(scrollable(content));

        let main_column = column![content, bottom_row, progress_slider]
            .spacing(10)
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_items(Alignment::Center);

        main_column.into()
    }
}

fn fill_container<'a>(content: impl Into<Element<'a, Message>>) -> Container<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x()
        .center_y()
}

fn view_album_list(music_dir: &MusicDir) -> Column<'_, Message> {
    let rows: Vec<_> = music_dir.with_album_views(view_album).into_iter().collect();

    Column::with_children(rows)
        .spacing(10)
        .width(Length::Fill)
        .align_items(Alignment::Center)
}

fn view_album_image(image_bytes: Option<&BgraBytes>) -> Element<'_, Message> {
    let length = 256;

    match image_bytes {
        // NOTE this isn't cached, so the clone happens every time;
        // currently it's not a problem, it might be with more images
        Some(image_bytes) => Image::new(image_bytes.clone())
            .width(Length::Units(length))
            .height(Length::Units(length))
            .content_fit(ContentFit::ScaleDown)
            .into(),

        None => Space::new(Length::Units(length), Length::Units(length)).into(),
    }
}

fn view_album<'a>(album_dir: &AlbumDirView<'a>) -> Element<'a, Message> {
    let album_image = view_album_image((&album_dir.loaded_cover).as_ref());

    let album_info = column![
        text(album_dir.display_title()),
        text(album_dir.display_artist().unwrap_or(""))
    ]
    .width(Length::FillPortion(1));

    let song_rows: Vec<_> = album_dir
        .songs
        .iter()
        .map(|&song| view_song_row(song))
        .collect();
    let songs_list = Column::with_children(song_rows)
        .spacing(2)
        .width(Length::FillPortion(1));

    row![album_image, album_info, songs_list].spacing(10).into()
}

fn view_song_row(song: &TaggedSong) -> Element<'_, Message> {
    row![
        button(icons::play()).on_press(Message::PlaySongClicked(song.id())),
        text(song.display_title())
    ]
    .align_items(Alignment::Center)
    .spacing(4)
    .into()
}

fn view_bottom_row<'a>(player_state: &'a PlayerStateView) -> Element<'a, Message> {
    let row_content = match player_state {
        PlayerStateView::Started(current_song_state) => {
            let current = &current_song_state.current;
            let play_pause_button = if current_song_state.playing {
                button(icons::pause()).on_press(Message::PauseClicked)
            } else {
                button(icons::play()).on_press(Message::PlayClicked)
            };

            row![
                view_current_album_artist(current)
                    .width(Length::Fill)
                    .align_items(Alignment::Center),
                play_pause_button,
                text(&current.title)
                    .width(Length::Fill)
                    .horizontal_alignment(alignment::Horizontal::Center)
            ]
        }

        PlayerStateView::Stopped => {
            row![
                horizontal_space(Length::Fill),
                button(icons::play()),
                horizontal_space(Length::Fill)
            ]
        }
    };

    row_content.width(Length::Fill).spacing(10).into()
}

fn view_current_album_artist<'a>(current: &'a CurrentSongView) -> Column<'a, Message> {
    let mut children: Vec<Element<'a, Message>> = Vec::new();

    if let Some(album) = &current.album {
        children.push(text(album).into());
    }
    if let Some(artist) = &current.artist {
        children.push(text(artist).into());
    }

    Column::with_children(children)
}
