use std::collections::HashMap;
use std::fs::File;
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use diesel::result::Error as DieselError;
use log::{error, info};
use symphonia::core::meta::StandardTagKey;
use symphonia::core::{
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::{MetadataOptions, MetadataRevision},
    probe::Hint,
};
use symphonia::default::get_probe;

use super::config::Config;
use super::rgba::{load_cached_rgba_bmp, RgbaBytes};
use crate::db::{
    queries::{self, Album, NewAlbum, NewSong, Song},
    SqlitePool, SqlitePoolConn,
};

#[derive(Clone, Debug)]
pub enum CrawlerMessage {
    NoAudioDirectory,
    DbError,
    CrawledAlbum(Box<CrawledAlbum>),
    Done,
}

#[derive(Clone, Debug)]
pub struct CrawledAlbum {
    pub album: Album,
    pub songs: Vec<Song>,
    pub cached_art: Option<RgbaBytes>,
}

#[derive(Clone, Debug)]
pub struct CrawledSong {
    pub path: Utf8PathBuf,
    pub tags: HashMap<TagKey, String>,
}

pub fn crawler_subcription(
    config: Arc<Config>,
    db: SqlitePool,
) -> iced::Subscription<CrawlerMessage> {
    struct CrawlerSub;

    iced::subscription::unfold(
        std::any::TypeId::of::<CrawlerSub>(),
        CrawlerState::Initial,
        move |state| step(state, config.clone(), db.clone()),
    )
}

enum CrawlerState {
    Initial,
    AlbumDirectories(Vec<Utf8PathBuf>, SqlitePoolConn),
    Final,
}

async fn step(
    state: CrawlerState,
    config: Arc<Config>,
    db: SqlitePool,
) -> (Option<CrawlerMessage>, CrawlerState) {
    match state {
        CrawlerState::Initial => match collect_album_dirs(&config.audio_directory) {
            Err(message) => (Some(message), CrawlerState::Final),
            Ok(mut album_dirs) => {
                let conn = match db.get() {
                    Ok(conn) => conn,
                    Err(e) => {
                        error!("failed to check out db connection: {e}");
                        return (Some(CrawlerMessage::DbError), CrawlerState::Final);
                    }
                };

                album_dirs.sort_by_key(|d| d.components().last().unwrap().to_string());
                album_dirs.reverse();

                (None, CrawlerState::AlbumDirectories(album_dirs, conn))
            }
        },

        CrawlerState::AlbumDirectories(mut directories, mut conn) => {
            let Some(album_dir) = directories.pop() else {
                return (Some(CrawlerMessage::Done), CrawlerState::Final);
            };

            let crawled_album = match collect_single_album(album_dir, &mut conn) {
                Ok(crawled_album) => Box::new(crawled_album),
                Err(maybe_message) => {
                    return (
                        maybe_message,
                        CrawlerState::AlbumDirectories(directories, conn),
                    );
                }
            };

            (
                Some(CrawlerMessage::CrawledAlbum(crawled_album)),
                CrawlerState::AlbumDirectories(directories, conn),
            )
        }

        CrawlerState::Final => (None, CrawlerState::Final),
    }
}

fn collect_album_dirs(audio_dir: &Utf8Path) -> Result<Vec<Utf8PathBuf>, CrawlerMessage> {
    let mut album_dirs = Vec::new();
    let entries = audio_dir.read_dir().map_err(|e| {
        error!("error reading audio directory entries: {e}");
        CrawlerMessage::NoAudioDirectory
    })?;

    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path: Utf8PathBuf = match entry.path().to_owned().try_into() {
            Ok(utf8) => utf8,
            Err(_) => {
                continue;
            }
        };

        if path.is_dir() {
            album_dirs.push(path);
        }
    }

    Ok(album_dirs)
}

fn collect_single_album(
    album_dir: Utf8PathBuf,
    conn: &mut SqlitePoolConn,
) -> Result<CrawledAlbum, Option<CrawlerMessage>> {
    let mut songs = Vec::new();
    let mut covers = Vec::new();
    let entries = album_dir.read_dir().map_err(|_| None)?;

    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };

        let path: Utf8PathBuf = match entry.path().try_into() {
            Ok(utf8) => utf8,
            Err(e) => {
                info!("skipping file with invalid utf8: {e}");
                continue;
            }
        };

        if is_music(&path) {
            if let Some(tags) = decode_tags(&path) {
                songs.push(CrawledSong { path, tags });
            } else {
                continue;
            }
        } else if is_cover_art(&path) {
            covers.push(path.to_owned());
        }
    }

    let (saved_album, mut saved_songs) = conn
        .immediate_transaction(|tx| {
            let saved_album = {
                let (album_title, album_artist, album_date) = songs
                    .first()
                    .map(|s| {
                        (
                            s.tags.get(&TagKey::Album),
                            s.tags.get(&TagKey::Artist),
                            s.tags.get(&TagKey::Date),
                        )
                    })
                    .unwrap_or_default();

                let new_album = NewAlbum {
                    directory: album_dir.clone(),
                    title: album_title.cloned(),
                    artist: album_artist.cloned(),
                    release_date: album_date.cloned(),
                    original_art: covers.first().cloned(),
                    resized_art: None,
                };

                queries::find_or_insert_album(tx, new_album)?
            };

            let mut saved_songs = Vec::new();
            for song in &songs {
                let new_song = NewSong {
                    album_id: saved_album.id,
                    file: song.path.clone(),
                    title: song.tags.get(&TagKey::TrackTitle).cloned(),
                    artist: song.tags.get(&TagKey::Artist).cloned(),
                    track_number: song
                        .tags
                        .get(&TagKey::TrackNumber)
                        .and_then(|s| s.parse().ok()),
                };

                let saved_song = queries::find_or_insert_song(tx, new_song)?;
                saved_songs.push(saved_song);
            }

            Ok((saved_album, saved_songs))
        })
        .map_err(|e: DieselError| {
            error!("failed to insert album: {e}");
            Some(CrawlerMessage::DbError)
        })?;

    saved_songs.sort_by_key(|s| s.track_number);

    let cached_art = saved_album.resized_art.as_ref().and_then(|path| {
        load_cached_rgba_bmp(path)
            .map_err(|e| {
                info!("error loading cached resized image: {e}");
                e
            })
            .ok()
    });

    Ok(CrawledAlbum {
        album: saved_album,
        songs: saved_songs,
        cached_art,
    })
}

const AUDIO_EXTENSIONS: [&str; 2] = ["mp3", "flac"];

fn is_music(path: &Utf8Path) -> bool {
    path.extension()
        .map(|ext| AUDIO_EXTENSIONS.contains(&ext))
        .unwrap_or_default()
}

const IMAGE_EXTENSIONS: [&str; 2] = ["jpg", "png"];

fn is_cover_art(path: &Utf8Path) -> bool {
    path.extension()
        .map(|ext| IMAGE_EXTENSIONS.contains(&ext))
        .unwrap_or_default()
}

/// NOTE This returns an empty tag map if they're missing,
/// and None for file not found or unsupported format
fn decode_tags(path: &Utf8Path) -> Option<HashMap<TagKey, String>> {
    let mut hint = Hint::new();
    let source = {
        // Provide the file extension as a hint.
        if let Some(extension) = path.extension() {
            hint.with_extension(extension);
        }

        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                error!("unexpected file not found: {:?}", e);
                return None;
            }
        };

        Box::new(file)
    };
    let mss = MediaSourceStream::new(source, Default::default());
    let format_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let metadata_opts: MetadataOptions = Default::default();

    let mut probed = match get_probe().format(&hint, mss, &format_opts, &metadata_opts) {
        Ok(p) => p,
        Err(e) => {
            let path_str = path.as_str();
            error!("file in unsupported format: {path_str} {e}");
            return None;
        }
    };

    let tags = if let Some(metadata_rev) = probed.format.metadata().current() {
        Some(gather_tags(metadata_rev))
    } else {
        probed
            .metadata
            .get()
            .as_ref()
            .and_then(|m| m.current())
            .map(gather_tags)
    };

    Some(tags.unwrap_or_default())
}

fn gather_tags(metadata_rev: &MetadataRevision) -> HashMap<TagKey, String> {
    let mut result = HashMap::new();

    for tag in metadata_rev.tags().iter() {
        if let Some(key) = tag.std_key.and_then(|key| TagKey::try_from(key).ok()) {
            result.insert(key, tag.value.to_string());
        }
    }

    result
}

/// A limited set of standard tag keys used by the application
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum TagKey {
    Album,
    AlbumArtist,
    Artist,
    Composer,
    Conductor,
    Date,
    Description,
    Genre,
    Label,
    Language,
    Lyrics,
    Mood,
    MovementName,
    MovementNumber,
    Part,
    PartTotal,
    Producer,
    ReleaseDate,
    Remixer,
    TrackNumber,
    TrackSubtitle,
    TrackTitle,
    TrackTotal,
}

impl TryFrom<StandardTagKey> for TagKey {
    type Error = IgnoredTagError;

    fn try_from(value: StandardTagKey) -> Result<Self, Self::Error> {
        match value {
            StandardTagKey::Album => Ok(TagKey::Album),
            StandardTagKey::AlbumArtist => Ok(TagKey::AlbumArtist),
            StandardTagKey::Artist => Ok(TagKey::Artist),
            StandardTagKey::Composer => Ok(TagKey::Composer),
            StandardTagKey::Conductor => Ok(TagKey::Conductor),
            StandardTagKey::Date => Ok(TagKey::Date),
            StandardTagKey::Description => Ok(TagKey::Description),
            StandardTagKey::Genre => Ok(TagKey::Genre),
            StandardTagKey::Label => Ok(TagKey::Label),
            StandardTagKey::Language => Ok(TagKey::Language),
            StandardTagKey::Lyrics => Ok(TagKey::Lyrics),
            StandardTagKey::Mood => Ok(TagKey::Mood),
            StandardTagKey::MovementName => Ok(TagKey::MovementName),
            StandardTagKey::MovementNumber => Ok(TagKey::MovementNumber),
            StandardTagKey::Part => Ok(TagKey::Part),
            StandardTagKey::PartTotal => Ok(TagKey::PartTotal),
            StandardTagKey::Producer => Ok(TagKey::Producer),
            StandardTagKey::ReleaseDate => Ok(TagKey::ReleaseDate),
            StandardTagKey::Remixer => Ok(TagKey::Remixer),
            StandardTagKey::TrackNumber => Ok(TagKey::TrackNumber),
            StandardTagKey::TrackSubtitle => Ok(TagKey::TrackSubtitle),
            StandardTagKey::TrackTitle => Ok(TagKey::TrackTitle),
            StandardTagKey::TrackTotal => Ok(TagKey::TrackTotal),

            _ => Err(IgnoredTagError::Ignored),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum IgnoredTagError {
    #[error("ignored tag key")]
    Ignored,
}
