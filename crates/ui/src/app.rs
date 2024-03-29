use std::sync::Arc;

use camino::Utf8PathBuf;
use flume::{Receiver, Sender};
use iced::keyboard::KeyCode;
use iced::widget::{
    button, column, container, horizontal_space, row, scrollable, slider, text, Column,
    Container, Image, Row, Space,
};
use iced::{
    alignment, executor, Alignment, Application, Command, ContentFit, Element, Event,
    Length, Subscription, Theme,
};
use iced_native::keyboard::Event as KeyboardEvent;
use log::error;

use clef_audio::player::{AudioAction, AudioMessage, PlayerDisplay, ProgressTimes};
use clef_db::queries::*;
use clef_db::SqlitePool;

mod audio_subscription;
pub(crate) mod crawler;
mod custom_style;
mod effect;
mod hoverable;
mod icons;
mod music_cache;
mod old_unfold;
mod resizer;
mod rgba;

use audio_subscription::audio_subscription;
use crawler::*;
use custom_style::no_background;
use effect::Effect;
use hoverable::*;
use music_cache::*;
use resizer::*;
use rgba::*;

use clef_shared::WINDOW_TITLE;

#[derive(Debug)]
pub struct App {
    config: Arc<Config>,
    db: SqlitePool,
    inbox: Receiver<AudioMessage>,
    to_audio: Sender<AudioAction>,
    to_resizer: Sender<ResizeRequest>,
    resizer_inbox: Receiver<ResizeRequest>,
    ui: Ui,
}

#[derive(Debug)]
struct Ui {
    crawling_music: bool,
    current_song: Option<CurrentSong>,
    progress: Option<ProgressDisplay>,
    hovered_song_id: Option<SongId>,
    music_cache: MusicCache,
}

impl Ui {
    fn new() -> Self {
        Self {
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
            to_resizer: to_resizer_tx,
            resizer_inbox: to_resizer_rx,
            ui: Ui::new(),
        }
    }

    fn execute(&mut self, effect: Effect<Message>) -> Command<Message> {
        match effect {
            Effect::None => Command::none(),

            Effect::Command(cmd) => cmd,

            Effect::ToAudio(audio_action) => {
                self.to_audio
                    .send(audio_action)
                    .unwrap_or_else(|e| error!("failed to send to audio thread: {e}"));

                Command::none()
            }

            Effect::ToResizer(resize_request) => {
                self.to_resizer
                    .send(resize_request)
                    .unwrap_or_else(|e| error!("failed to send to resizer thread: {e}"));

                Command::none()
            }

            Effect::CloseWindow => iced::window::close(),
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
    total_seconds: i64,
}

impl CurrentSong {
    pub fn new(song: &Song, album: &Album, playing: bool) -> Self {
        Self {
            id: song.id,
            album_id: album.id,
            title: song.display_title().unwrap_or_default().to_owned(),
            album: album.title.clone(),
            artist: song.artist.clone(),
            total_seconds: song.total_seconds,
            playing,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProgressDisplay {
    Dragging(f32),
    FromAudio(ProgressTimes),
}

impl ProgressDisplay {
    fn display_proportion(&self) -> f32 {
        match self {
            ProgressDisplay::Dragging(proportion) => *proportion,
            ProgressDisplay::FromAudio(times) => {
                let elapsed = times.elapsed.seconds as f32 + times.elapsed.frac as f32;
                let total = times.total.seconds as f32 + times.total.frac as f32;
                elapsed / total
            }
        }
    }
}

#[derive(Debug)]
pub struct Config {
    pub local_data_directory: Utf8PathBuf,
    pub audio_directory: Utf8PathBuf,
    pub db_path: Utf8PathBuf,
    pub resized_images_directory: Utf8PathBuf,
}

#[derive(Debug)]
pub struct Flags {
    pub inbox: Receiver<AudioMessage>,
    pub to_audio: Sender<AudioAction>,
    pub db_pool: SqlitePool,
    pub config: Config,
}

#[derive(Debug, Clone)]
pub enum Message {
    GotHwnd,
    FromCrawler(CrawlerMessage),
    FromResizer(ResizerMessage),
    FromAudio(AudioMessage),
    Native(Event),
    PlayPausedClicked,
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

        #[cfg(not(target_os = "windows"))]
        let initial_command = Command::none();

        #[cfg(target_os = "windows")]
        let initial_command = Command::perform(
            async move { clef_shared::window_handle_hack::set_hwnd() },
            |_| Message::GotHwnd,
        );

        (initial_state, initial_command)
    }

    fn title(&self) -> String {
        // NOTE This is used to look up our own window handle on startup.
        // So on windows, it should not be modified until after recieving GotHwnd.
        WINDOW_TITLE.to_string()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
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

        let native = iced_native::subscription::events().map(Message::Native);

        Subscription::batch([crawler, resizer, audio, native])
    }

    fn view(&self) -> iced::Element<'_, Self::Message, iced::Renderer<Self::Theme>> {
        view(&self.ui)
    }
}

// Update

fn update(ui: &mut Ui, message: Message) -> Effect<Message> {
    match message {
        Message::GotHwnd => Effect::none(),

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

        Message::Native(Event::Keyboard(KeyboardEvent::KeyReleased {
            key_code: KeyCode::Space,
            ..
        })) => toggle(ui),

        Message::Native(_) => Effect::none(),

        Message::PlayPausedClicked => AudioAction::PlayPaused.into(),

        Message::PlaySongClicked(song_id) => {
            let Some(current) = get_current_song(&ui.music_cache, song_id, true) else {
                return Effect::none();
            };

            let queue = ui.music_cache.get_album_queue(current.id, current.album_id);
            let Some(queue) = queue else {
                error!("unable to build album queue");
                return Effect::none();
            };

            AudioAction::PlayQueue(Box::new(queue)).into()
        }

        Message::PauseClicked => AudioAction::Pause.into(),
        Message::ForwardClicked => AudioAction::Forward.into(),
        Message::BackClicked => AudioAction::Back.into(),

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
            update_current_song(ui, &display);

            match &ui.progress {
                Some(ProgressDisplay::Dragging(_)) => {
                    // ignore update to preserve draging slider state
                }

                Some(ProgressDisplay::FromAudio(_)) | None => {
                    ui.progress = Some(ProgressDisplay::FromAudio(display.times));
                }
            }

            Effect::none()
        }

        Message::FromAudio(AudioMessage::SeekComplete(display)) => {
            update_current_song(ui, &display);

            // deliberately overwrite the dragging state
            ui.progress = Some(ProgressDisplay::FromAudio(display.times));

            Effect::none()
        }

        Message::FromAudio(AudioMessage::DisplayUpdate(None)) => {
            ui.current_song = None;
            ui.progress = None;
            Effect::none()
        }

        Message::FromAudio(AudioMessage::AudioDied) => Effect::CloseWindow,
    }
}

fn toggle(ui: &Ui) -> Effect<Message> {
    let playing = ui.current_song.as_ref().map(|c| c.playing);

    match playing {
        Some(true) => AudioAction::Pause.into(),
        Some(false) => AudioAction::PlayPaused.into(),
        None => Effect::none(),
    }
}

fn update_current_song(ui: &mut Ui, display: &PlayerDisplay) {
    match &mut ui.current_song {
        Some(current_song) if current_song.id == display.song_id => {
            current_song.playing = display.playing;
        }

        _ => {
            if let Some(current_song) =
                get_current_song(&ui.music_cache, display.song_id, display.playing)
            {
                ui.current_song = Some(current_song);
            }
        }
    };
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

// View

fn view(ui: &Ui) -> Element<'_, Message> {
    const MAX: f32 = 1.0;
    const STEP: f32 = 0.01;

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

    let content = view_album_list(&ui.music_cache, ui.hovered_song_id, &ui.current_song);

    let content = fill_container(scrollable(content));
    let bottom_row = view_bottom_row(&ui.current_song, &ui.progress);

    let main_column = column![content, bottom_row, progress_slider]
        .spacing(10)
        .padding(20)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_items(Alignment::Center);

    main_column.into()
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
    hovered_song_id: Option<SongId>,
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
    hovered_song_id: Option<SongId>,
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

fn view_album_image(image_bytes: Option<&RgbaBytes>) -> Element<'_, Message> {
    let length = Length::Fixed(IMAGE_SIZE as f32);

    let Some(image_bytes) = image_bytes else {
        return Space::new(length, length).into();
    };

    Image::new(image_bytes)
        .width(length)
        .height(length)
        .content_fit(ContentFit::ScaleDown)
        .into()
}

fn song_row_status(
    current_song: &Option<CurrentSong>,
    hovered_song_id: Option<SongId>,
    song_row_id: SongId,
) -> SongRowStatus {
    match current_song {
        Some(song) if song.id == song_row_id && song.playing => SongRowStatus::Playing,
        Some(song) if song.id == song_row_id => SongRowStatus::Paused,
        _ if hovered_song_id == Some(song_row_id) => SongRowStatus::Hovered,
        _ => SongRowStatus::Blank,
    }
}

#[derive(Debug, Eq, PartialEq)]
enum SongRowStatus {
    /// currently playing - show pause button
    Playing,
    /// currently paused - show play_paused button
    Paused,
    /// Both hovered and not currently playing - show play from start button
    Hovered,
    /// Both not playing and not hovered - show nothing
    Blank,
}

/// A song in the album table
fn view_song_row(song: &Song, status: SongRowStatus) -> Element<'_, Message> {
    let button_slot: Element<'_, Message> = match status {
        SongRowStatus::Playing => button(icons::pause())
            .on_press(Message::PauseClicked)
            .style(no_background())
            .into(),

        SongRowStatus::Paused => button(icons::play())
            .on_press(Message::PlayPausedClicked)
            .style(no_background())
            .into(),

        SongRowStatus::Hovered => button(icons::play())
            .on_press(Message::PlaySongClicked(song.id))
            .style(no_background())
            .into(),

        SongRowStatus::Blank => {
            text(song.track_number.map(|n| n.to_string()).unwrap_or_default())
                .width(MAGIC_SVG_SIZE)
                .height(MAGIC_SVG_SIZE)
                .horizontal_alignment(alignment::Horizontal::Center)
                .vertical_alignment(alignment::Vertical::Center)
                .into()
        }
    };

    let duration = format_seconds(song.total_seconds as f64);

    let hoverable = Hoverable::new(
        row![
            button_slot,
            text(song.display_title().unwrap_or_default()).width(Length::Fill),
            text(duration),
            horizontal_space(Length::Fixed(10f32))
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
const MAGIC_SVG_SIZE: Length = Length::Fixed(34f32);

/// The bottom row with the play/pause button and current song info
fn view_bottom_row<'a>(
    current_song: &'a Option<CurrentSong>,
    progress: &'a Option<ProgressDisplay>,
) -> Element<'a, Message> {
    let row_content = match (current_song, progress) {
        (Some(current_song), Some(progress)) => {
            let play_pause_button = if current_song.playing {
                button(icons::pause())
                    .on_press(Message::PauseClicked)
                    .style(no_background())
            } else {
                button(icons::play())
                    .on_press(Message::PlayPausedClicked)
                    .style(no_background())
            };

            let elapsed = match progress {
                ProgressDisplay::Dragging(proportion) => {
                    f64::from(*proportion) * current_song.total_seconds as f64
                }

                ProgressDisplay::FromAudio(times) => times.elapsed.seconds as f64,
            };

            let elapsed = format_seconds(elapsed);
            let total = format_seconds(current_song.total_seconds as f64);
            let duration = format!("{elapsed} / {total}");

            let left_side = row![
                text(&current_song.title)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .horizontal_alignment(alignment::Horizontal::Center)
                    .vertical_alignment(alignment::Vertical::Center),
                button(icons::back())
                    .on_press(Message::BackClicked)
                    .style(no_background()),
            ]
            .height(MAGIC_SVG_SIZE)
            .width(Length::FillPortion(1));

            let right_side = row![
                button(icons::forward())
                    .on_press(Message::ForwardClicked)
                    .style(no_background()),
                view_current_album_artist(current_song)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_items(Alignment::Center),
                text(duration)
                    .height(Length::Fill)
                    .horizontal_alignment(alignment::Horizontal::Center)
                    .vertical_alignment(alignment::Vertical::Center),
            ]
            .height(MAGIC_SVG_SIZE)
            .width(Length::FillPortion(1));

            row![left_side, play_pause_button, right_side,].height(MAGIC_SVG_SIZE)
        }

        _ => row![
            Space::new(Length::Fill, MAGIC_SVG_SIZE),
            button(icons::play()).style(no_background()),
            Space::new(Length::Fill, MAGIC_SVG_SIZE),
        ]
        .height(MAGIC_SVG_SIZE),
    };

    let bottom_row = row_content.width(Length::Fill).spacing(10);

    Element::from(bottom_row)
}

fn view_current_album_artist(current: &CurrentSong) -> Row<'_, Message> {
    let mut children: Vec<Element<'_, Message>> = Vec::new();

    children.push(horizontal_space(Length::Fill).into());

    let mut label = String::new();
    if let Some(album) = &current.album {
        label.push_str(album);
    }
    if current.album.is_some() && current.artist.is_some() {
        label.push_str(" - ");
    }
    if let Some(artist) = &current.artist {
        label.push_str(artist);
    }
    children.push(text(label).into());

    children.push(horizontal_space(Length::Fill).into());

    Row::with_children(children).spacing(10)
}

fn format_seconds(seconds: f64) -> String {
    let minutes = seconds / 60.0;
    let whole_minutes = minutes.floor();
    let seconds = (minutes - whole_minutes) * 60.0;
    let seconds = seconds.round();

    format!("{whole_minutes}:{seconds:02}")
}

#[cfg(test)]
mod tests {
    use std::{assert_eq, str::FromStr};

    use camino::Utf8PathBuf;

    use super::*;
    use crate::test_util::*;

    #[test]
    fn crawled_album_with_no_original_art_sends_no_resize_request() {
        let mut ui = Ui::new();

        let mut crawled = fake_album();
        crawled.cached_art = None;
        crawled.album.original_art = None;

        let message = crawled_album_message(&crawled);

        let effect = update(&mut ui, message);

        assert!(matches!(effect, Effect::None))
    }

    #[test]
    fn crawled_album_with_cached_resized_art_sends_no_resize_request() {
        let mut ui = Ui::new();

        let mut crawled = fake_album();
        crawled.cached_art = Some(RgbaBytes::empty());
        crawled.album.original_art = Some(Utf8PathBuf::from_str("original").unwrap());

        let message = crawled_album_message(&crawled);

        let effect = update(&mut ui, message);

        assert!(matches!(effect, Effect::None))
    }

    #[test]
    fn crawled_album_with_no_cached_resized_art_sends_resize_request() {
        let mut ui = Ui::new();
        let mut crawled = fake_album();
        crawled.cached_art = None;
        crawled.album.original_art = Some(Utf8PathBuf::from_str("original").unwrap());

        let message = crawled_album_message(&crawled);

        let effect = update(&mut ui, message);

        match effect {
            Effect::ToResizer(ResizeRequest { album_id, source_path, .. }) => {
                assert_eq!(album_id, crawled.album.id);
                assert_eq!(source_path, crawled.album.original_art.unwrap());
            }
            _ => panic!("expected resize request"),
        }
    }

    fn crawled_album_message(crawled: &CrawledAlbum) -> Message {
        let crawled = Box::new(crawled.clone());
        Message::FromCrawler(CrawlerMessage::CrawledAlbum(crawled))
    }
}
