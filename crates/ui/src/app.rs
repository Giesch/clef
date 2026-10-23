use std::sync::Arc;

use camino::Utf8PathBuf;
use cosmic::app::context_drawer::{self, ContextDrawer};
use cosmic::app::{Core, Task};
use cosmic::iced::keyboard::Event as KeyboardEvent;
use cosmic::iced::keyboard::Key;
use cosmic::iced::widget::{column, container, row, slider, text, Image, Space};
use cosmic::iced::{
    alignment, Alignment, ContentFit, Event, Length, Padding, Subscription,
};
use cosmic::widget::about::About;
use cosmic::widget::{button, mouse_area, scrollable};
use cosmic::{theme, Application, ApplicationExt, Element, Renderer, Theme};
use flume::{Receiver, Sender};
use log::error;

use clef_audio::player::{AudioAction, AudioMessage, PlayerDisplay, ProgressTimes};
use clef_db::queries::*;
use clef_db::SqlitePool;

mod audio_subscription;
pub(crate) mod crawler;
mod effect;

mod icons;
mod music_cache;
mod old_unfold;
#[cfg(test)]
mod regression_tests;
mod resizer;
mod rgba;

use audio_subscription::audio_subscription;
use crawler::*;
use effect::Effect;
use music_cache::*;
use resizer::*;
use rgba::*;

use crate::fl;

type Column<'a, Message> = cosmic::iced::widget::Column<'a, Message, Theme, Renderer>;
type Container<'a, Message> =
    cosmic::iced::widget::Container<'a, Message, Theme, Renderer>;
type Row<'a, Message> = cosmic::iced::widget::Row<'a, Message, Theme, Renderer>;

pub const APP_ID: &str = "io.github.giesch.clef";

pub struct App {
    core: Core,
    about: About,
    show_about: bool,
    config: Arc<Config>,
    db: SqlitePool,
    inbox: Receiver<AudioMessage>,
    to_audio: Sender<AudioAction>,
    to_resizer: Sender<ResizeRequest>,
    resizer_inbox: Receiver<ResizeRequest>,
    ui: Ui,
    source_generation: u64,
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

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("config", &self.config)
            .field("show_about", &self.show_about)
            .field("ui", &self.ui)
            .field("source_generation", &self.source_generation)
            .finish_non_exhaustive()
    }
}

impl App {
    fn new(core: Core, flags: Flags) -> Self {
        let (to_resizer_tx, to_resizer_rx) = flume::unbounded::<ResizeRequest>();

        let about = About::default()
            .name(fl!("app-title"))
            .icon(cosmic::widget::icon::from_name(APP_ID))
            .version(flags.version)
            .license("MPL-2.0")
            .license_url("https://www.mozilla.org/en-US/MPL/2.0/")
            .developers([("Dan Knutson", "dan.knutson@gmail.com")])
            .links([
                (fl!("repository"), "https://github.com/Giesch/clef"),
                (fl!("support"), "https://github.com/Giesch/clef/issues"),
            ]);

        Self {
            core,
            about,
            show_about: false,
            config: Arc::new(flags.config),
            inbox: flags.inbox,
            to_audio: flags.to_audio,
            db: flags.db_pool,
            to_resizer: to_resizer_tx,
            resizer_inbox: to_resizer_rx,
            ui: Ui::new(),
            source_generation: {
                static NEXT: std::sync::atomic::AtomicU64 =
                    std::sync::atomic::AtomicU64::new(0);
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
        }
    }

    fn execute(&mut self, effect: Effect<Message>) -> Task<Message> {
        match effect {
            Effect::None => Task::none(),
            Effect::Command(task) => task.map(cosmic::Action::App),
            Effect::ToAudio(action) => {
                self.to_audio
                    .send(action)
                    .unwrap_or_else(|e| error!("failed to send to audio thread: {e}"));
                Task::none()
            }
            Effect::ToResizer(request) => {
                self.to_resizer
                    .send(request)
                    .unwrap_or_else(|e| error!("failed to send to resizer thread: {e}"));
                Task::none()
            }
            Effect::CloseWindow => {
                cosmic::iced::window::latest().and_then(cosmic::iced::window::close)
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

#[derive(Debug, Clone)]
pub struct Config {
    pub local_data_directory: Utf8PathBuf,
    pub audio_directory: Utf8PathBuf,
    pub db_path: Utf8PathBuf,
    pub resized_images_directory: Utf8PathBuf,
}

#[derive(Debug, Clone)]
pub struct Flags {
    pub inbox: Receiver<AudioMessage>,
    pub to_audio: Sender<AudioAction>,
    pub db_pool: SqlitePool,
    pub config: Config,
    pub version: &'static str,
}

#[derive(Debug, Clone)]
pub enum Message {
    ToggleAbout,
    OpenUrl(String),
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
    LeftLibrary,
}

impl Application for App {
    type Executor = cosmic::executor::Default;
    type Flags = Flags;
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn init(core: Core, flags: Flags) -> (Self, Task<Message>) {
        let mut app = Self::new(core, flags);

        let title = fl!("app-title");
        app.set_header_title(title.clone());
        let task = match app.core.main_window_id() {
            Some(id) => app.set_window_title(title, id),
            None => Task::none(),
        };

        (app, task)
    }

    fn header_end(&self) -> Vec<Element<'_, Message>> {
        vec![button::text(fl!("about"))
            .on_press(Message::ToggleAbout)
            .into()]
    }

    fn context_drawer(&self) -> Option<ContextDrawer<'_, Message>> {
        self.show_about.then(|| {
            context_drawer::about(
                &self.about,
                |url| Message::OpenUrl(url.to_owned()),
                Message::ToggleAbout,
            )
        })
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::ToggleAbout => {
                self.show_about = !self.show_about;
                self.set_show_context(self.show_about);
                Task::none()
            }

            Message::OpenUrl(url) => {
                if let Err(e) = open::that_detached(&url) {
                    error!("failed to open {url}: {e}");
                }
                Task::none()
            }

            message => {
                let effect = update(&mut self.ui, message);
                self.execute(effect)
            }
        }
    }

    fn subscription(&self) -> Subscription<Message> {
        let crawler = if self.ui.crawling_music {
            crawler_subcription(
                self.config.clone(),
                self.db.clone(),
                self.source_generation,
            )
            .map(Message::FromCrawler)
        } else {
            Subscription::none()
        };

        let resizer = resizer_subscription(
            self.config.clone(),
            self.db.clone(),
            self.resizer_inbox.clone(),
            self.source_generation,
        )
        .map(Message::FromResizer);

        let audio = audio_subscription(self.inbox.clone(), self.source_generation)
            .map(Message::FromAudio);

        let native = cosmic::iced::event::listen().map(Message::Native);

        Subscription::batch([crawler, resizer, audio, native])
    }

    fn view(&self) -> Element<'_, Message> {
        view(&self.ui)
    }
}

// Update

fn update(ui: &mut Ui, message: Message) -> Effect<Message> {
    match message {
        Message::ToggleAbout | Message::OpenUrl(_) => Effect::none(),

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
            key: Key::Character(character),
            ..
        })) if character == " " => toggle(ui),

        Message::Native(Event::Mouse(cosmic::iced::mouse::Event::CursorLeft))
        | Message::LeftLibrary => {
            ui.hovered_song_id = None;
            Effect::none()
        }

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

    let library = container(content).padding(Padding {
        top: 20.0,
        right: 20.0,
        bottom: 0.0,
        left: 20.0,
    });

    // Song rows inside the scrollable get no cursor once it leaves the scrollable,
    // so the controls below it end the hover instead.
    let controls = mouse_area(
        column![
            view_bottom_row(&ui.current_song, &ui.progress),
            container(progress_slider).id("playback-seek")
        ]
        .spacing(10)
        .padding(Padding {
            top: 0.0,
            right: 20.0,
            bottom: 20.0,
            left: 20.0,
        })
        .align_x(Alignment::Center),
    )
    .on_enter(Message::LeftLibrary);

    let library = scrollable(library).width(Length::Fill).height(Length::Fill);

    let main_column = column![fill_container(library), controls]
        .spacing(10)
        .width(Length::Fill)
        .height(Length::Fill)
        .align_x(Alignment::Center);

    main_column.into()
}

fn fill_container<'a>(
    content: impl Into<Element<'a, Message>>,
) -> Container<'a, Message> {
    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .center_x(Length::Fill)
        .center_y(Length::Fill)
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
        .align_x(Alignment::Center)
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
        return Space::new().width(length).height(length).into();
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
        SongRowStatus::Playing => button::custom(icons::pause())
            .width(MAGIC_SVG_SIZE)
            .height(MAGIC_SVG_SIZE)
            .padding(5)
            .on_press(Message::PauseClicked)
            .class(theme::Button::Icon)
            .into(),

        SongRowStatus::Paused => button::custom(icons::play())
            .width(MAGIC_SVG_SIZE)
            .height(MAGIC_SVG_SIZE)
            .padding(5)
            .on_press(Message::PlayPausedClicked)
            .class(theme::Button::Icon)
            .into(),

        SongRowStatus::Hovered => button::custom(
            container(icons::play()).id(format!("song-play-{:?}", song.id)),
        )
        .width(MAGIC_SVG_SIZE)
        .height(MAGIC_SVG_SIZE)
        .padding(5)
        .on_press(Message::PlaySongClicked(song.id))
        .class(theme::Button::Icon)
        .into(),

        SongRowStatus::Blank => {
            text(song.track_number.map(|n| n.to_string()).unwrap_or_default())
                .width(MAGIC_SVG_SIZE)
                .height(MAGIC_SVG_SIZE)
                .align_x(alignment::Horizontal::Center)
                .align_y(alignment::Vertical::Center)
                .into()
        }
    };

    let duration = format_seconds(song.total_seconds as f64);

    let hoverable = mouse_area(
        container(
            row![
                container(button_slot).id(format!("song-slot-{:?}", song.id)),
                text(song.display_title().unwrap_or_default()).width(Length::Fill),
                text(duration),
                Space::new().width(10)
            ]
            .width(Length::Fill)
            .align_y(Alignment::Center)
            .spacing(10),
        )
        .padding(2)
        .id(format!("song-row-{:?}", song.id)),
    )
    .on_enter(Message::HoveredSong(song.id))
    .on_exit(Message::UnhoveredSong(song.id));

    Element::from(hoverable)
}

// Inline controls explicitly reserve 24px SVG + 5px padding on each side.
const MAGIC_SVG_SIZE: Length = Length::Fixed(34f32);

/// The bottom row with the play/pause button and current song info
fn view_bottom_row<'a>(
    current_song: &'a Option<CurrentSong>,
    progress: &'a Option<ProgressDisplay>,
) -> Element<'a, Message> {
    let row_content = match (current_song, progress) {
        (Some(current_song), Some(progress)) => {
            let play_pause_button = if current_song.playing {
                button::custom(container(icons::pause()).id("playback-pause"))
                    .on_press(Message::PauseClicked)
                    .class(theme::Button::Icon)
            } else {
                button::custom(container(icons::play()).id("playback-resume"))
                    .on_press(Message::PlayPausedClicked)
                    .class(theme::Button::Icon)
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
                    .align_x(alignment::Horizontal::Center)
                    .align_y(alignment::Vertical::Center),
                button::custom(container(icons::back()).id("playback-back"))
                    .on_press(Message::BackClicked)
                    .class(theme::Button::Icon),
            ]
            .height(MAGIC_SVG_SIZE)
            .width(Length::FillPortion(1));

            let right_side = row![
                button::custom(container(icons::forward()).id("playback-forward"))
                    .on_press(Message::ForwardClicked)
                    .class(theme::Button::Icon),
                view_current_album_artist(current_song)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .align_y(Alignment::Center),
                text(duration)
                    .height(Length::Fill)
                    .align_x(alignment::Horizontal::Center)
                    .align_y(alignment::Vertical::Center),
            ]
            .height(MAGIC_SVG_SIZE)
            .width(Length::FillPortion(1));

            row![left_side, play_pause_button, right_side,].height(MAGIC_SVG_SIZE)
        }

        _ => row![
            Space::new().width(Length::Fill).height(MAGIC_SVG_SIZE),
            button::custom(icons::play()).class(theme::Button::Icon),
            Space::new().width(Length::Fill).height(MAGIC_SVG_SIZE),
        ]
        .height(MAGIC_SVG_SIZE),
    };

    let bottom_row = row_content.width(Length::Fill).spacing(10);

    Element::from(bottom_row)
}

fn view_current_album_artist(current: &CurrentSong) -> Row<'_, Message> {
    let mut children: Vec<Element<'_, Message>> = Vec::new();

    children.push(Space::new().width(Length::Fill).into());

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

    children.push(Space::new().width(Length::Fill).into());

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
