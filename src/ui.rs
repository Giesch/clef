use std::sync::Arc;

use flume::{Receiver, Sender};
use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, slider, text, Column,
    Container, Image, Row, Space,
};
use iced::{
    alignment, executor, Alignment, Application, Command, ContentFit, Element, Length,
    Subscription, Theme,
};
use log::error;
use parking_lot::Mutex;

use crate::channels::{audio_subscription, AudioAction, AudioMessage, ProgressTimes};
use crate::db::queries::*;
use crate::db::SqlitePool;

pub mod config;
pub mod crawler;
mod custom_style;
mod effect;
mod hoverable;
mod icons;
mod music_cache;
mod resizer;
mod rgba;

use config::Config;
use crawler::*;
use custom_style::no_background;
use effect::Effect;
use hoverable::*;
use music_cache::*;
use resizer::*;
use rgba::*;

#[derive(Debug)]
pub struct App {
    config: Arc<Config>,
    db: SqlitePool,
    inbox: Arc<Mutex<Receiver<AudioMessage>>>,
    to_audio: Arc<Mutex<Sender<AudioAction>>>,
    to_resizer: Arc<Mutex<Sender<ResizeRequest>>>,
    resizer_inbox: Arc<Mutex<Receiver<ResizeRequest>>>,
    ui: Ui,
}

#[derive(Debug)]
struct Ui {
    should_exit: bool,
    crawling_music: bool,
    current_song: Option<CurrentSong>,
    progress: Option<ProgressDisplay>,
    hovered_song_id: Option<SongId>,
    music_cache: MusicCache,
}

impl Ui {
    fn new() -> Self {
        Self {
            should_exit: false,
            current_song: None,
            progress: None,
            hovered_song_id: None,
            crawling_music: true,
            music_cache: MusicCache::new(),
        }
    }
}

impl App {
    fn new(flags: Flags) -> Self {
        let (to_resizer_tx, to_resizer_rx) = flume::unbounded::<ResizeRequest>();

        Self {
            config: Arc::new(flags.config),
            inbox: flags.inbox,
            to_audio: flags.to_audio,
            db: flags.db_pool,
            to_resizer: Arc::new(Mutex::new(to_resizer_tx)),
            resizer_inbox: Arc::new(Mutex::new(to_resizer_rx)),
            ui: Ui::new(),
        }
    }

    fn execute(&mut self, effect: Effect<Message>) -> Command<Message> {
        match effect {
            Effect::Command(cmd) => cmd,

            Effect::None => Command::none(),

            Effect::ToAudio(audio_action) => {
                self.to_audio
                    .lock()
                    .send(audio_action)
                    .unwrap_or_else(|e| error!("failed to send to audio thread: {e}"));

                Command::none()
            }

            Effect::ToResizer(resize_request) => {
                self.to_resizer
                    .lock()
                    .send(resize_request)
                    .unwrap_or_else(|e| error!("failed to send to resizer thread: {e}"));

                Command::none()
            }
        }
    }
}

#[derive(Debug)]
struct CurrentSong {
    id: SongId,
    album_id: AlbumId,
    title: String,
    album: Option<String>,
    artist: Option<String>,
    playing: bool,
}

impl CurrentSong {
    pub fn new(song: &Song, album: &Album, playing: bool) -> Self {
        Self {
            id: song.id,
            album_id: album.id,
            title: song.display_title().unwrap_or_default().to_owned(),
            album: album.title.clone(),
            artist: song.artist.clone(),
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
    pub inbox: Arc<Mutex<Receiver<AudioMessage>>>,
    pub to_audio: Arc<Mutex<Sender<AudioAction>>>,
    pub db_pool: SqlitePool,
    pub config: Config,
}

#[derive(Debug, Clone)]
pub enum Message {
    FromCrawler(CrawlerMessage),
    FromResizer(ResizerMessage),
    FromAudio(AudioMessage),
    PlayClicked,
    PlaySongClicked(SongId),
    PauseClicked,
    ForwardClicked,
    BackClicked,
    SeekDrag(f32),
    SeekRelease,
    SeekWithoutSong(f32),
    HoveredSong(SongId),
    UnhoveredSong(SongId),
}

impl Application for App {
    type Flags = Flags;
    type Message = Message;
    type Theme = Theme;
    type Executor = executor::Default;

    fn new(flags: Self::Flags) -> (Self, iced::Command<Self::Message>) {
        let initial_state = Self::new(flags);
        let initial_command = Command::none();

        (initial_state, initial_command)
    }

    fn title(&self) -> String {
        match &self.ui.current_song {
            Some(CurrentSong { title, .. }) => format!("Clef - {title}"),
            None => "Clef".to_string(),
        }
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn should_exit(&self) -> bool {
        self.ui.should_exit
    }

    fn update(&mut self, message: Self::Message) -> iced::Command<Self::Message> {
        let effect = update(&mut self.ui, message);
        self.execute(effect)
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        let crawler = if self.ui.crawling_music {
            crawler_subcription(self.config.clone(), self.db.clone())
                .map(Message::FromCrawler)
        } else {
            Subscription::none()
        };

        let resizer = resizer_subscription(
            self.config.clone(),
            self.db.clone(),
            self.resizer_inbox.clone(),
        )
        .map(Message::FromResizer);

        let audio = audio_subscription(self.inbox.clone()).map(Message::FromAudio);

        Subscription::batch([crawler, resizer, audio])
    }

    fn view(&self) -> iced::Element<'_, Self::Message, iced::Renderer<Self::Theme>> {
        const MAX: f32 = 1.0;
        const STEP: f32 = 0.01;

        let ui = &self.ui;

        let progress_slider = match &ui.progress {
            Some(progress) => {
                let proportion = progress.display_proportion();

                slider(0.0..=MAX, proportion, Message::SeekDrag)
                    .step(STEP)
                    .on_release(Message::SeekRelease)
            }

            // disabled
            None => slider(0.0..=MAX, 0.0, Message::SeekWithoutSong).step(STEP),
        };

        let content =
            view_album_list(&ui.music_cache, &ui.hovered_song_id, &ui.current_song);

        let content = fill_container(scrollable(content));
        let bottom_row = view_bottom_row(&ui.current_song);

        let main_column = column![content, bottom_row, progress_slider]
            .spacing(10)
            .padding(20)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_items(Alignment::Center);

        main_column.into()
    }
}

fn update(ui: &mut Ui, message: Message) -> Effect<Message> {
    match message {
        Message::FromCrawler(CrawlerMessage::NoAudioDirectory) => {
            error!("failed to crawl audio directory");
            ui.crawling_music = false;
            Effect::none()
        }
        Message::FromCrawler(CrawlerMessage::DbError) => {
            error!("crawler database error");
            ui.crawling_music = false;
            Effect::none()
        }
        Message::FromCrawler(CrawlerMessage::Done) => {
            ui.crawling_music = false;
            Effect::none()
        }
        Message::FromCrawler(CrawlerMessage::CrawledAlbum(crawled)) => {
            let resize =
                if crawled.cached_art.is_none() {
                    crawled.album.original_art.as_ref().map(|original_art| {
                        ResizeRequest {
                            album_id: crawled.album.id,
                            album_title: crawled
                                .album
                                .display_title()
                                .unwrap_or_default()
                                .to_string(),
                            source_path: original_art.clone(),
                        }
                    })
                } else {
                    None
                };

            ui.music_cache.add_crawled_album(*crawled);

            resize.into()
        }

        Message::FromResizer(ResizerMessage::ResizedImage(resized)) => {
            ui.music_cache
                .load_album_art(resized.album_id, resized.bytes);
            Effect::none()
        }

        Message::PlayClicked => {
            let audio_action = match &mut ui.current_song {
                None => None,
                Some(CurrentSong { playing, .. }) if *playing => None,
                Some(current_song) => {
                    current_song.playing = true;
                    Some(AudioAction::PlayPaused)
                }
            };

            audio_action.into()
        }

        Message::PlaySongClicked(song_id) => {
            let Some(current) = get_current_song(&ui.music_cache, song_id, true) else {
                return Effect::none();
            };

            let queue = ui.music_cache.get_album_queue(current.id, current.album_id);
            let Some(queue) = queue else {
                error!("unable to build album queue");
                return Effect::none();
            };

            ui.current_song = Some(current);

            AudioAction::PlayQueue(queue).into()
        }

        Message::PauseClicked => {
            if let Some(current_song) = &mut ui.current_song {
                current_song.playing = false;
            }

            AudioAction::Pause.into()
        }

        Message::ForwardClicked => AudioAction::Forward.into(),

        Message::BackClicked => {
            ui.progress = Some(ProgressDisplay::Optimistic(
                0.0,
                ProgressDisplay::OPTIMISTIC_THRESHOLD,
            ));

            AudioAction::Back.into()
        }

        Message::SeekDrag(proportion) => {
            ui.progress = Some(ProgressDisplay::Dragging(proportion));
            Effect::none()
        }

        Message::SeekRelease => {
            let proportion = match &ui.progress {
                Some(ProgressDisplay::Dragging(proportion)) => *proportion,
                _ => {
                    error!("seek release without drag");
                    return Effect::none();
                }
            };

            ui.progress = Some(ProgressDisplay::Optimistic(proportion, 0));

            AudioAction::Seek(proportion).into()
        }

        Message::SeekWithoutSong(_) => Effect::none(),

        Message::HoveredSong(song_id) => {
            ui.hovered_song_id = Some(song_id);
            Effect::none()
        }
        Message::UnhoveredSong(song_id) => {
            if ui.hovered_song_id == Some(song_id) {
                ui.hovered_song_id = None;
            }
            Effect::none()
        }

        Message::FromAudio(AudioMessage::DisplayUpdate(Some(display))) => {
            // update current song
            match &mut ui.current_song {
                Some(current_song) if current_song.id == display.song_id => {
                    current_song.playing = display.playing;
                }

                _ => {
                    if let Some(current_song) = get_current_song(
                        &ui.music_cache,
                        display.song_id,
                        display.playing,
                    ) {
                        ui.current_song = Some(current_song);
                    }
                }
            }

            // update progress bar if necessary
            match &ui.progress {
                Some(ProgressDisplay::Dragging(_)) => {
                    // ignore update to preserve drag state
                }
                Some(ProgressDisplay::Optimistic(_, _)) if !display.playing => {
                    // this is after dragging while paused
                    // ignore update to preserve the dropped state until next play
                }
                Some(ProgressDisplay::Optimistic(proportion, skips))
                    if display.playing
                        && *skips < ProgressDisplay::OPTIMISTIC_THRESHOLD =>
                {
                    // this is after dragging & releasing while playing,
                    // or when pressing play after a drop
                    // ignores the first updates to avoid flicker
                    ui.progress =
                        Some(ProgressDisplay::Optimistic(*proportion, skips + 1));
                }
                _ => {
                    ui.progress = Some(ProgressDisplay::FromAudio(display.times));
                }
            }

            Effect::none()
        }

        Message::FromAudio(AudioMessage::DisplayUpdate(None)) => {
            ui.current_song = None;
            ui.progress = None;
            Effect::none()
        }

        Message::FromAudio(AudioMessage::AudioDied) => {
            ui.should_exit = true;
            Effect::none()
        }
    }
}

fn get_current_song(
    music_cache: &MusicCache,
    song_id: SongId,
    playing: bool,
) -> Option<CurrentSong> {
    let Some(song) = music_cache.get_song(&song_id) else {
        error!("unexpected song id: {song_id:?}");
        return None;
    };
    let Some(album) = music_cache.get_album(&song.album_id) else {
        error!("unexpected album id: {:?}", &song.album_id);
        return None;
    };

    Some(CurrentSong::new(song, album, playing))
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
    music: &'a MusicCache,
    hovered_song_id: &'a Option<SongId>,
    current_song: &'a Option<CurrentSong>,
) -> Column<'a, Message> {
    let rows: Vec<_> = music
        .albums()
        .iter()
        .map(|a| view_album(a, hovered_song_id, current_song))
        .collect();

    Column::with_children(rows)
        .spacing(10)
        .width(Length::Fill)
        .align_items(Alignment::Center)
}

fn view_album<'a>(
    album: &'a CachedAlbum,
    hovered_song_id: &'a Option<SongId>,
    current_song: &'a Option<CurrentSong>,
) -> Element<'a, Message> {
    let album_image = view_album_image(album.art.as_ref());

    let album_info = column![
        text(album.album.display_title().unwrap_or_default()),
        text(album.album.artist.as_deref().unwrap_or_default()),
        text(album.album.release_date.as_deref().unwrap_or_default()),
    ]
    .width(Length::FillPortion(1));

    let song_rows: Vec<_> = album
        .songs
        .iter()
        .map(|song| {
            let status = song_row_status(current_song, hovered_song_id, song.id);
            view_song_row(song, status)
        })
        .collect();
    let songs_list = Column::with_children(song_rows).width(Length::FillPortion(2));

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
fn view_song_row(song: &Song, status: SongRowStatus) -> Element<'_, Message> {
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

        SongRowStatus::None => {
            text(song.track_number.map(|n| n.to_string()).unwrap_or_default())
                .width(MAGIC_SVG_SIZE)
                .height(MAGIC_SVG_SIZE)
                .horizontal_alignment(alignment::Horizontal::Center)
                .vertical_alignment(alignment::Vertical::Center)
                .into()
        }
    };

    let hoverable = Hoverable::new(
        row![
            button_slot,
            text(song.display_title().unwrap_or_default()).width(Length::Fill)
        ]
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use camino::Utf8PathBuf;

    use super::*;
    use crate::test_util::*;

    #[test]
    fn crawled_album_with_no_original_art_sends_no_resize_request() {
        let mut ui = Ui::new();

        let mut crawled = fake_album();
        crawled.cached_art = None;
        crawled.album.original_art = None;

        let message =
            Message::FromCrawler(CrawlerMessage::CrawledAlbum(Box::new(crawled.clone())));

        let effect = update(&mut ui, message);

        assert!(matches!(effect, Effect::None))
    }

    #[test]
    fn crawled_album_with_cached_resized_art_sends_no_resize_request() {
        let mut ui = Ui::new();

        let mut crawled = fake_album();
        crawled.cached_art = Some(RgbaBytes::empty());
        crawled.album.original_art = Some(Utf8PathBuf::from_str("original").unwrap());

        let message =
            Message::FromCrawler(CrawlerMessage::CrawledAlbum(Box::new(crawled.clone())));

        let effect = update(&mut ui, message);

        assert!(matches!(effect, Effect::None))
    }

    #[test]
    fn crawled_album_with_no_cached_resized_art_sends_resize_request() {
        let mut ui = Ui::new();
        let mut crawled = fake_album();
        crawled.cached_art = None;
        crawled.album.original_art = Some(Utf8PathBuf::from_str("original").unwrap());

        let message =
            Message::FromCrawler(CrawlerMessage::CrawledAlbum(Box::new(crawled.clone())));

        let effect = update(&mut ui, message);

        assert!(matches!(
            effect,
            Effect::ToResizer(ResizeRequest { album_id, source_path, .. })
                if album_id == crawled.album.id &&
                   source_path == crawled.album.original_art.unwrap()
        ))
    }
}
