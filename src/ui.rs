use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

use camino::Utf8PathBuf;
use flume::{Receiver, Sender};
use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, slider, text, vertical_space,
    Column, Container, Image, Row, Space,
};
use iced::{alignment, executor};
use iced::{Alignment, Application, Command, ContentFit, Element, Length, Subscription, Theme};
use log::error;
use parking_lot::Mutex;

use crate::channels::{self, ProgressTimes, ToAudio, ToUi};

mod icons;
mod startup;
use startup::*;
mod rgba;
use rgba::*;
mod data;
use data::*;
mod hoverable;
use hoverable::*;

#[derive(Debug)]
pub struct Ui {
    player_state: PlayerStateView,
    inbox: Arc<Mutex<Receiver<channels::ToUi>>>,
    to_audio: Arc<Mutex<Sender<channels::ToAudio>>>,
    should_exit: bool,
    progress: Option<ProgressTimes>,
    music_dir: Option<MusicDir>,
    hovered_song_id: Option<SongId>,
}

#[derive(Debug)]
enum PlayerStateView {
    Stopped,
    Started(CurrentSongState),
}

#[derive(Debug)]
struct CurrentSongState {
    playing: bool, // false means paused
    song: CurrentSongView,
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
    LoadedImages(Option<HashMap<Utf8PathBuf, RgbaBytes>>),
    HoveredSong(SongId),
    UnhoveredSong(SongId),
}

impl Ui {
    fn from_flags(flags: Flags) -> Self {
        Self {
            player_state: PlayerStateView::Stopped,
            inbox: flags.inbox,
            to_audio: flags.to_audio,
            should_exit: false,
            progress: None,
            music_dir: None,
            hovered_song_id: None,
        }
    }

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
        let initial_state = Self::from_flags(flags);
        let initial_command = Command::perform(load_music(), Message::GotMusicDir);

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
                        self.player_state = PlayerStateView::Started(CurrentSongState {
                            playing: true,
                            song: CurrentSongView::from_song(song),
                        });

                        let up_next: VecDeque<_> = {
                            if let Some(album_id) = &song.album_id {
                                let album = music_dir.get_album(album_id);
                                let mut remaining_songs =
                                    album.song_ids.iter().skip_while(|&s| s != &song.id);

                                // remove new current track
                                remaining_songs.next();

                                remaining_songs
                                    .map(|song_id| music_dir.get_song(song_id).path.clone())
                                    .collect()
                            } else {
                                Default::default()
                            }
                        };

                        self.send_to_audio(ToAudio::PlayQueue((song.path.clone(), up_next)));
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

            Message::HoveredSong(song_id) => {
                self.hovered_song_id = Some(song_id);
                Command::none()
            }

            Message::UnhoveredSong(song_id) => {
                if self.hovered_song_id == Some(song_id) {
                    self.hovered_song_id = None;
                }
                Command::none()
            }

            Message::FromAudio(ToUi::Progress(times)) => {
                self.progress = Some(times);
                Command::none()
            }

            Message::FromAudio(ToUi::NextSong(song_path)) => {
                let music_dir = match &self.music_dir {
                    Some(m) => m,
                    None => {
                        error!("recieved NextSong before loading music");
                        return Command::none();
                    }
                };

                let song = music_dir.get_song_by_path(song_path);
                self.player_state = PlayerStateView::Started(CurrentSongState {
                    playing: true,
                    song: CurrentSongView::from_song(song),
                });

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
            Some(music_dir) => view_album_list(music_dir, &self.hovered_song_id).into(),
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

fn view_album_list<'a>(
    music_dir: &'a MusicDir,
    hovered_song_id: &'a Option<SongId>,
) -> Column<'a, Message> {
    let rows: Vec<_> = music_dir
        .with_joined_song_data(|album_dir| view_album(album_dir, hovered_song_id))
        .into_iter()
        .collect();

    Column::with_children(rows)
        .spacing(10)
        .width(Length::Fill)
        .align_items(Alignment::Center)
}

fn view_album_image(image_bytes: Option<&RgbaBytes>) -> Element<'_, Message> {
    let length = 256;

    // NOTE this doesn't distinguish between loading and no art to load
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

fn view_album<'a>(
    album_dir: &AlbumDirView<'a>,
    hovered_song_id: &'a Option<SongId>,
) -> Element<'a, Message> {
    let album_image = view_album_image(album_dir.loaded_cover.as_ref());

    let album_info = column![
        text(album_dir.display_title()),
        text(album_dir.display_artist().unwrap_or(""))
    ]
    .width(Length::FillPortion(1));

    let song_rows: Vec<_> = album_dir
        .songs
        .iter()
        .enumerate()
        .map(|(index, &song)| view_song_row(song, hovered_song_id, index))
        .collect();
    let songs_list = Column::with_children(song_rows).width(Length::FillPortion(1));

    let row = row![album_image, album_info, songs_list].spacing(10);

    Element::from(row)
}

/// A song in the album table
fn view_song_row<'a>(
    song: &'a TaggedSong,
    hovered_song_id: &'a Option<SongId>,
    index: usize,
) -> Element<'a, Message> {
    let hovered = *hovered_song_id == Some(song.id);

    let button_slot: Element<'_, Message> = if hovered {
        button(icons::play())
            .on_press(Message::PlaySongClicked(song.id))
            .into()
    } else {
        text(index + 1)
            .width(MAGIC_SVG_SIZE)
            .height(MAGIC_SVG_SIZE)
            .horizontal_alignment(alignment::Horizontal::Center)
            .vertical_alignment(alignment::Vertical::Center)
            .into()
    };

    let hoverable = Hoverable::new(
        row![button_slot, text(song.display_title()).width(Length::Fill)]
            .width(Length::Fill)
            .align_items(Alignment::Center)
            .spacing(10)
            .into(),
        Message::HoveredSong(song.id),
        Message::UnhoveredSong(song.id),
    )
    .padding(2);

    Element::from(hoverable)
}

// 24 (svg) + 5 + 5 (default button padding)
const MAGIC_SVG_SIZE: Length = Length::Units(34);

/// The bottom row with the play/pause button and current song info
fn view_bottom_row(player_state: &PlayerStateView) -> Element<'_, Message> {
    let row_content = match player_state {
        PlayerStateView::Started(current_song_state) => {
            let play_pause_button = if current_song_state.playing {
                button(icons::pause()).on_press(Message::PauseClicked)
            } else {
                button(icons::play()).on_press(Message::PlayClicked)
            };

            row![
                view_current_album_artist(&current_song_state.song)
                    .width(Length::Fill)
                    .height(MAGIC_SVG_SIZE)
                    .align_items(Alignment::Center),
                play_pause_button,
                text(&current_song_state.song.title)
                    .width(Length::Fill)
                    .height(MAGIC_SVG_SIZE)
                    .horizontal_alignment(alignment::Horizontal::Center)
                    .vertical_alignment(alignment::Vertical::Center)
            ]
        }

        PlayerStateView::Stopped => {
            row![
                Space::new(Length::Fill, MAGIC_SVG_SIZE),
                button(icons::play()),
                Space::new(Length::Fill, MAGIC_SVG_SIZE),
            ]
        }
    };

    row_content.width(Length::Fill).spacing(10).into()
}

fn view_current_album_artist(current: &CurrentSongView) -> Row<'_, Message> {
    let mut children: Vec<Element<'_, Message>> = Vec::new();

    children.push(horizontal_space(Length::Fill).into());

    if let Some(album) = &current.album {
        children.push(text(album).into());
    }

    if current.album.is_some() && current.artist.is_some() {
        children.push(text(" - ").into());
    }

    if let Some(artist) = &current.artist {
        children.push(text(artist).into());
    }

    children.push(horizontal_space(Length::Fill).into());

    Row::with_children(children).spacing(10)
}
