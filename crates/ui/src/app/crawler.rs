use std::collections::HashMap;
use std::sync::Arc;

use camino::{Utf8Path, Utf8PathBuf};
use clef_db::queries::DbError;
use log::{error, info};

use super::config::Config;
use super::rgba::{load_cached_rgba_bmp, RgbaBytes};
use crate::app::old_unfold::old_unfold;
use clef_audio::metadata::{decode_metadata, TagKey};
use clef_db::{
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
    pub total_seconds: u64,
}

pub fn crawler_subcription(
    config: Arc<Config>,
    db: SqlitePool,
) -> iced::Subscription<CrawlerMessage> {
    struct CrawlerSub;

    old_unfold(
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

            let crawled_album = match collect_single_album(&album_dir, &mut conn) {
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
        let path: Utf8PathBuf = match entry.path().clone().try_into() {
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
    album_dir: &Utf8Path,
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
            if let Some(decoded) = decode_metadata(&path) {
                songs.push(CrawledSong {
                    path,
                    tags: decoded.tags,
                    total_seconds: decoded.total_seconds,
                });
            } else {
                info!("skipping file with invalid music metadata: {path}");
                continue;
            }
        } else if is_cover_art(&path) {
            let file_meta = match entry.metadata() {
                Ok(meta) => meta,
                Err(e) => {
                    info!("skipping file with invalid file metadata: {path} {e}");
                    continue;
                }
            };
            covers.push((path.clone(), file_meta.len()));
        }
    }

    covers.sort_by_key(|(_path, file_size)| *file_size);
    let original_art = covers.last().map(|(path, _file_size)| path).cloned();

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
                    directory: album_dir.to_owned(),
                    title: album_title.cloned(),
                    artist: album_artist.cloned(),
                    release_date: album_date.cloned(),
                    original_art,
                    resized_art: None,
                };

                queries::find_or_insert_album(tx, new_album)?
            };

            let mut saved_songs = Vec::new();
            for crawled in &songs {
                let new_song = NewSong {
                    album_id: saved_album.id,
                    file: crawled.path.clone(),
                    total_seconds: crawled.total_seconds as i64,
                    title: crawled.tags.get(&TagKey::TrackTitle).cloned(),
                    artist: crawled.tags.get(&TagKey::Artist).cloned(),
                    track_number: crawled
                        .tags
                        .get(&TagKey::TrackNumber)
                        .and_then(|s| s.parse().ok()),
                };

                let saved_song = queries::find_or_insert_song(tx, new_song)?;
                saved_songs.push(saved_song);
            }

            Ok((saved_album, saved_songs))
        })
        .map_err(|e: DbError| {
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
