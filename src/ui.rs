use std::collections::HashMap;
use std::sync::Arc;

use camino::Utf8PathBuf;
use flume::{Receiver, Sender};
use iced::executor;
use iced::widget::{
    button, column, container, row, scrollable, slider, text, vertical_space, Column, Container,
    Image, Space,
};
use iced::{Alignment, Application, Command, ContentFit, Element, Length, Subscription, Theme};
use log::error;
use parking_lot::Mutex;

use crate::channels::{self, ProgressTimes, ToAudio, ToUi};

mod icons;
mod startup;
use startup::*;
mod bgra;
use bgra::*;

#[derive(Debug)]
pub struct Ui {
    player_state: PlayerStateView,
    inbox: Arc<Mutex<Receiver<channels::ToUi>>>,
    to_audio: Arc<Mutex<Sender<channels::ToAudio>>>,
    should_exit: bool,
    progress: Option<ProgressTimes>,
    music_dir: Option<MusicDirView>,
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
    GotMusicDir(Result<MusicDirView, MusicDirError>),
    PlayClicked,
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
                    .iter()
                    .flat_map(|album| album.covers.first())
                    .cloned()
                    .collect();

                self.music_dir = Some(music_dir);

                Command::perform(load_images(image_paths), Message::LoadedImages)
            }
            Message::GotMusicDir(Err(_)) => Command::none(),

            Message::LoadedImages(Some(mut loaded_images_by_path)) => {
                match &mut self.music_dir {
                    Some(music_dir) => {
                        for mut album in music_dir {
                            // TODO this needs a better way of matching loaded images up to albums
                            match album.covers.first() {
                                Some(cover_path) => {
                                    if let Some(bytes) = loaded_images_by_path.remove(cover_path) {
                                        album.loaded_cover = Some(bytes);
                                    }
                                }
                                None => {
                                    continue;
                                }
                            }
                        }
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

        let content: Element<'_, Message> = match &self.music_dir {
            Some(music_dir) => view_album_list(music_dir).into(),
            None => vertical_space(Length::Fill).into(),
        };

        let content = fill_container(scrollable(content));

        let main_column = column![content, play_pause_button, progress_slider]
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

fn view_album_list(music_dir: &MusicDirView) -> Column<'_, Message> {
    let rows: Vec<_> = music_dir
        .iter()
        .map(view_album)
        .map(Element::from)
        .collect();

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

fn view_album<'a>(album_dir: &'a AlbumDirView) -> Element<'a, Message> {
    let album_image = view_album_image((&album_dir.loaded_cover).as_ref());

    let album_info = column![
        text(album_dir.display_title()),
        text(album_dir.display_artist().unwrap_or(""))
    ]
    .width(Length::FillPortion(1));

    let song_rows: Vec<_> = album_dir
        .songs
        .iter()
        .map(|song| Element::from(text(song.display_title())))
        .collect();

    let songs_list = Column::with_children(song_rows).width(Length::FillPortion(1));

    row![album_image, album_info, songs_list].spacing(10).into()
}

pub async fn load_images(paths: Vec<Utf8PathBuf>) -> Option<HashMap<Utf8PathBuf, BgraBytes>> {
    use iced::futures::future::join_all;

    let results = join_all(paths.into_iter().map(load_image)).await;
    let pairs: Option<Vec<(Utf8PathBuf, BgraBytes)>> = results.into_iter().collect();
    let bytes_by_path: HashMap<_, _> = pairs?.into_iter().collect();

    Some(bytes_by_path)
}

async fn load_image(utf8_path: Utf8PathBuf) -> Option<(Utf8PathBuf, BgraBytes)> {
    let bytes = load_bgra(&utf8_path)?;
    Some((utf8_path, bytes))
}

// TODO remove these
#[allow(unused)]
const BENNY_HILL: &str = "/home/giesch/Music/Benny Hill/Benny Hill - Theme Song.mp3";
#[allow(unused)]
const THE_WIND_THAT_SHAKES_THE_LAND: &str = "/home/giesch/Music/Unleash The Archers - Abyss/Unleash The Archers - Abyss - 08 The Wind that Shapes the Land.mp3";
