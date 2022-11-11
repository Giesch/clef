use std::collections::HashMap;
use std::sync::Arc;

use camino::Utf8PathBuf;
use flume::{Receiver, Sender};
use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, slider, text,
    vertical_space, Column, Container, Image, Row, Space,
};
use iced::{alignment, executor};
use iced::{
    Alignment, Application, Command, ContentFit, Element, Length, Subscription, Theme,
};
use log::error;
use parking_lot::Mutex;

use crate::channels::{self, ProgressTimes, ToAudio, ToUi};

mod icons;
mod startup;
use startup::*;
mod rgba;
use rgba::*;
mod data;
pub use data::*;
mod hoverable;
use hoverable::*;
mod custom_style;
use custom_style::no_background;

#[derive(Debug)]
pub struct Ui {
    inbox: Arc<Mutex<Receiver<channels::ToUi>>>,
    to_audio: Arc<Mutex<Sender<channels::ToAudio>>>,

    should_exit: bool,
    current_song: Option<CurrentSong>,
    progress: Option<ProgressDisplay>,
    music: Option<Music>,
    hovered_song_id: Option<SongId>,
}

impl Ui {
    fn new(flags: Flags) -> Self {
        Self {
            inbox: flags.inbox,
            to_audio: flags.to_audio,
            should_exit: false,
            current_song: None,
            progress: None,
            music: None,
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

#[derive(Debug)]
struct CurrentSong {
    id: SongId,
    title: String,
    album: Option<String>,
    artist: Option<String>,
    playing: bool,
}

impl CurrentSong {
    pub fn playing(song: &TaggedSong) -> Self {
        Self::from_song(song, true)
    }

    pub fn from_song(song: &TaggedSong, playing: bool) -> Self {
        Self {
            id: song.id,
            title: song.display_title().to_owned(),
            album: song.album_title().map(str::to_owned),
            artist: song.artist().map(str::to_owned),
            playing,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ProgressDisplay {
    Dragging(f32),
    Optimistic(f32, usize),
    FromAudio(ProgressTimes),
}

impl ProgressDisplay {
    /// The number of audio thread updates to skip after the user
    /// releases the mouse while seeking. This prevents a flicker of
    /// displaying the old play position.
    const OPTIMISTIC_THRESHOLD: usize = 2;

    fn display_proportion(&self) -> f32 {
        match self {
            ProgressDisplay::Dragging(proportion) => *proportion,
            ProgressDisplay::Optimistic(proportion, _) => *proportion,
            ProgressDisplay::FromAudio(times) => {
                let elapsed = times.elapsed.seconds as f32 + times.elapsed.frac as f32;
                let total = times.total.seconds as f32 + times.total.frac as f32;
                elapsed / total
            }
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
    GotMusic(Result<Music, LoadMusicError>),
    PlayClicked,
    PlaySongClicked(SongId),
    PauseClicked,
    ForwardClicked,
    BackClicked,
    FromAudio(ToUi),
    SeekDrag(f32),
    SeekRelease,
    SeekWithoutSong(f32),
    LoadedImages(Option<HashMap<Utf8PathBuf, RgbaBytes>>),
    HoveredSong(SongId),
    UnhoveredSong(SongId),
}

impl Application for Ui {
    type Flags = Flags;
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;

    fn new(flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        let initial_state = Self::new(flags);
        let initial_command = Command::perform(load_music(), Message::GotMusic);

        (initial_state, initial_command)
    }

    fn title(&self) -> String {
        match &self.current_song {
            Some(CurrentSong { title, .. }) => format!("Clef - {title}"),
            None => "Clef".to_string(),
        }
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn should_exit(&self) -> bool {
        self.should_exit
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        match message {
            Message::GotMusic(Ok(music)) => {
                let image_paths: Vec<_> = music
                    .albums()
                    .iter()
                    .flat_map(|album| album.covers.first())
                    .cloned()
                    .collect();

                self.music = Some(music);

                Command::perform(load_images(image_paths), Message::LoadedImages)
            }
            Message::GotMusic(Err(_)) => Command::none(),

            Message::LoadedImages(Some(loaded_images_by_path)) => {
                match &mut self.music {
                    Some(music) => {
                        music.add_album_covers(loaded_images_by_path);
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
                match &mut self.current_song {
                    None => {}
                    Some(CurrentSong { playing, .. }) if *playing => {}
                    Some(current_song) => {
                        current_song.playing = true;
                        self.send_to_audio(ToAudio::PlayPaused);
                    }
                }

                Command::none()
            }

            Message::PlaySongClicked(song_id) => {
                let Some(music) = &self.music else {
                    error!("play clicked before music loaded");
                    return Command::none();
                };

                let song = music.get_song(&song_id);
                self.current_song = Some(CurrentSong::playing(song));
                let Some(queue) = music.get_album_queue(song) else {
                    error!("failed to find album for song");
                    return Command::none();
                };

                self.send_to_audio(ToAudio::PlayQueue(queue));

                Command::none()
            }

            Message::PauseClicked => {
                if let Some(current_song) = &mut self.current_song {
                    current_song.playing = false;
                }

                self.send_to_audio(ToAudio::Pause);

                Command::none()
            }

            Message::ForwardClicked => {
                self.send_to_audio(ToAudio::Forward);
                Command::none()
            }

            Message::BackClicked => {
                self.progress = Some(ProgressDisplay::Optimistic(
                    0.0,
                    ProgressDisplay::OPTIMISTIC_THRESHOLD,
                ));
                self.send_to_audio(ToAudio::Back);
                Command::none()
            }

            Message::SeekDrag(proportion) => {
                self.progress = Some(ProgressDisplay::Dragging(proportion));
                Command::none()
            }

            Message::SeekRelease => {
                let proportion = match &self.progress {
                    Some(ProgressDisplay::Dragging(proportion)) => *proportion,
                    _ => {
                        error!("seek release without drag");
                        return Command::none();
                    }
                };

                self.progress = Some(ProgressDisplay::Optimistic(proportion, 0));
                self.send_to_audio(ToAudio::Seek(proportion));

                Command::none()
            }

            Message::SeekWithoutSong(_) => Command::none(),

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

            Message::FromAudio(ToUi::DisplayUpdate(Some(display))) => {
                // update current song
                match &mut self.current_song {
                    Some(current_song) if current_song.id == display.song_id => {
                        current_song.playing = display.playing;
                    }

                    _ => {
                        let Some(music) = &self.music else {
                            error!("recieved audio thread message before loading music");
                            return Command::none();
                        };

                        let song = music.get_song(&display.song_id);
                        self.current_song =
                            Some(CurrentSong::from_song(song, display.playing));
                    }
                }

                // update progress bar if necessary
                match &self.progress {
                    Some(ProgressDisplay::Dragging(_)) => {
                        // ignore update to preserve drag state
                    }
                    Some(ProgressDisplay::Optimistic(_, _)) if !display.playing => {
                        // this is after dragging while paused
                        // ignore update to preserve dropped state
                    }
                    Some(ProgressDisplay::Optimistic(proportion, skips))
                        if display.playing
                            && *skips < ProgressDisplay::OPTIMISTIC_THRESHOLD =>
                    {
                        // this is after dragging while playing
                        // ignores first updates after releasing to avoid flicker
                        self.progress =
                            Some(ProgressDisplay::Optimistic(*proportion, skips + 1));
                    }
                    _ => {
                        self.progress = Some(ProgressDisplay::FromAudio(display.times));
                    }
                }

                Command::none()
            }

            Message::FromAudio(ToUi::DisplayUpdate(None)) => {
                self.current_song = None;
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
        const MAX: f32 = 1.0;
        const STEP: f32 = 0.01;

        let progress_slider = match &self.progress {
            Some(progress) => {
                let proportion = progress.display_proportion();

                slider(0.0..=MAX, proportion, Message::SeekDrag)
                    .step(STEP)
                    .on_release(Message::SeekRelease)
            }

            // disabled
            None => slider(0.0..=MAX, 0.0, Message::SeekWithoutSong).step(STEP),
        };

        let content: Element<'_, Message> = match &self.music {
            Some(music) => {
                view_album_list(music, &self.hovered_song_id, &self.current_song).into()
            }

            None => vertical_space(Length::Fill).into(),
        };

        let content = fill_container(scrollable(content));
        let bottom_row = view_bottom_row(&self.current_song);

        let main_column = column![content, bottom_row, progress_slider]
            .spacing(10)
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_items(Alignment::Center);

        main_column.into()
    }
}

fn fill_container<'a>(
    content: impl Into<Element<'a, Message>>,
) -> Container<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x()
        .center_y()
}

fn view_album_list<'a>(
    music: &'a Music,
    hovered_song_id: &'a Option<SongId>,
    current_song: &'a Option<CurrentSong>,
) -> Column<'a, Message> {
    let rows: Vec<_> = music
        .with_joined_song_data(|album_dir| {
            view_album(album_dir, hovered_song_id, current_song)
        })
        .into_iter()
        .collect();

    Column::with_children(rows)
        .spacing(10)
        .width(Length::Fill)
        .align_items(Alignment::Center)
}

fn view_album<'a>(
    album_dir: &AlbumDirView<'a>,
    hovered_song_id: &'a Option<SongId>,
    current_song: &'a Option<CurrentSong>,
) -> Element<'a, Message> {
    let album_image = view_album_image(album_dir.loaded_cover.as_ref());

    let album_info = column![
        text(album_dir.display_title()),
        text(album_dir.display_artist().unwrap_or("")),
        text(album_dir.date().unwrap_or("")),
    ]
    .width(Length::FillPortion(1));

    let song_rows: Vec<_> = album_dir
        .songs
        .iter()
        .enumerate()
        .map(|(index, &song)| {
            let status = song_row_status(current_song, hovered_song_id, song.id);
            view_song_row(SongRowProps { song, status, index })
        })
        .collect();
    let songs_list = Column::with_children(song_rows).width(Length::FillPortion(1));

    let row = row![album_image, album_info, songs_list].spacing(10);

    Element::from(row)
}

fn song_row_status(
    current_song: &Option<CurrentSong>,
    hovered_song_id: &Option<SongId>,
    song_row_id: SongId,
) -> SongRowStatus {
    match current_song {
        Some(song) if song.id == song_row_id => {
            if song.playing {
                SongRowStatus::Playing
            } else {
                SongRowStatus::Paused
            }
        }

        _ => {
            if *hovered_song_id == Some(song_row_id) {
                SongRowStatus::Hovered
            } else {
                SongRowStatus::None
            }
        }
    }
}

fn view_album_image(image_bytes: Option<&RgbaBytes>) -> Element<'_, Message> {
    let length = Length::Units(IMAGE_SIZE);

    match image_bytes {
        Some(image_bytes) => Image::new(image_bytes.clone())
            .width(length)
            .height(length)
            .content_fit(ContentFit::ScaleDown)
            .into(),

        None => Space::new(length, length).into(),
    }
}

struct SongRowProps<'a> {
    song: &'a TaggedSong,
    // 0-based index of the song in the album table
    index: usize,
    status: SongRowStatus,
}

#[derive(Debug, Eq, PartialEq)]
enum SongRowStatus {
    /// currently playing - show pause button
    Playing,
    /// currently paused - show play/pause button
    Paused,
    /// Both hovered and not currently playing - show play from start button
    Hovered,
    /// Both not playing and not hovered - show nothing
    None,
}

/// A song in the album table
fn view_song_row(
    SongRowProps { song, status, index }: SongRowProps<'_>,
) -> Element<'_, Message> {
    let button_slot: Element<'_, Message> = match status {
        SongRowStatus::Playing => button(icons::pause())
            .on_press(Message::PauseClicked)
            .style(no_background())
            .into(),

        SongRowStatus::Paused => button(icons::play())
            .on_press(Message::PlayClicked)
            .style(no_background())
            .into(),

        SongRowStatus::Hovered => button(icons::play())
            .on_press(Message::PlaySongClicked(song.id))
            .style(no_background())
            .into(),

        SongRowStatus::None => text(index + 1)
            .width(MAGIC_SVG_SIZE)
            .height(MAGIC_SVG_SIZE)
            .horizontal_alignment(alignment::Horizontal::Center)
            .vertical_alignment(alignment::Vertical::Center)
            .into(),
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
fn view_bottom_row(current_song: &Option<CurrentSong>) -> Element<'_, Message> {
    let row_content = match current_song {
        Some(current_song) => {
            let play_pause_button = if current_song.playing {
                button(icons::pause())
                    .on_press(Message::PauseClicked)
                    .style(no_background())
            } else {
                button(icons::play())
                    .on_press(Message::PlayClicked)
                    .style(no_background())
            };

            row![
                text(&current_song.title)
                    .width(Length::Fill)
                    .height(MAGIC_SVG_SIZE)
                    .horizontal_alignment(alignment::Horizontal::Center)
                    .vertical_alignment(alignment::Vertical::Center),
                button(icons::back())
                    .on_press(Message::BackClicked)
                    .style(no_background()),
                play_pause_button,
                button(icons::forward())
                    .on_press(Message::ForwardClicked)
                    .style(no_background()),
                view_current_album_artist(current_song)
                    .width(Length::Fill)
                    .height(MAGIC_SVG_SIZE)
                    .align_items(Alignment::Center),
            ]
        }

        None => {
            row![
                Space::new(Length::Fill, MAGIC_SVG_SIZE),
                button(icons::play()).style(no_background()),
                Space::new(Length::Fill, MAGIC_SVG_SIZE),
            ]
        }
    };

    let bottom_row = row_content.width(Length::Fill).spacing(10);

    Element::from(bottom_row)
}

fn view_current_album_artist(current: &CurrentSong) -> Row<'_, Message> {
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
