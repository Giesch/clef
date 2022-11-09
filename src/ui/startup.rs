use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs::File;

use camino::{Utf8Path, Utf8PathBuf};
use log::{error, info};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision};
use symphonia::core::probe::Hint;
use symphonia::default::get_probe;
use walkdir::WalkDir;

use super::data::*;
use super::rgba::{load_rgba, RgbaBytes};

#[derive(thiserror::Error, Debug, Clone)]
pub enum MusicDirError {
    #[error("no audio directory found")]
    NoAudioDirectory,
    #[error("error walking audio directory")]
    WalkError,
}

// Gather decoded songs and recognized art paths
pub async fn load_music() -> Result<MusicDir, MusicDirError> {
    let (songs, covers) = walk_audio_directory()?;
    let music_dir = prepare_for_display(songs, covers);

    Ok(music_dir)
}

fn walk_audio_directory() -> Result<(Vec<TaggedSong>, Vec<Utf8PathBuf>), MusicDirError> {
    let audio_dir = dirs::audio_dir().ok_or(MusicDirError::NoAudioDirectory)?;

    let mut songs: Vec<TaggedSong> = Vec::new();
    let mut covers: Vec<Utf8PathBuf> = Vec::new();
    for dir_entry in WalkDir::new(audio_dir).into_iter() {
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

        if is_music(path) {
            let song = match decode_tags(path) {
                Some(tags) => TaggedSong::new(path.to_owned(), None, tags),
                None => {
                    continue;
                }
            };

            songs.push(song);
        } else if is_cover_art(path) {
            covers.push(path.to_owned());
        }
    }

    Ok((songs, covers))
}

fn prepare_for_display(songs: Vec<TaggedSong>, covers: Vec<Utf8PathBuf>) -> MusicDir {
    use itertools::Itertools;

    let song_ids_by_directory: HashMap<Utf8PathBuf, Vec<SongId>> = songs
        .iter()
        .map(|song| (song.path.with_file_name(""), song.id))
        .into_group_map();

    let mut songs_by_id: HashMap<SongId, TaggedSong> = songs
        .into_iter()
        .map(|song| (song.id, song))
        .into_grouping_map()
        .fold_first(|acc, _key, _val| acc);

    let mut covers_by_directory: HashMap<Utf8PathBuf, Vec<Utf8PathBuf>> = covers
        .into_iter()
        .map(|path| (path.with_file_name(""), path))
        .into_group_map();

    // sort AlbumIds by album title, if available, or source directory name otherwise
    // sort SongIds within albums by track number
    // associate art with albums by directory
    let mut albums_by_id = HashMap::new();
    let sorted_albums: Vec<_> = song_ids_by_directory
        .into_iter()
        .map(|(directory, mut song_ids)| {
            song_ids.sort_by_key(|song_id| {
                let song = songs_by_id.get(song_id).expect("unexpected song id");
                song.track_number()
            });

            let covers = covers_by_directory.remove(&directory).unwrap_or_default();

            let (album_artist, album_title) = song_ids
                .iter()
                .next()
                .and_then(|id| songs_by_id.get(&id))
                .map(|song| (song.artist(), song.album_title()))
                .unwrap_or((None, None));

            let album = AlbumDir::new(directory, song_ids, covers);
            let album_id = album.id;

            // let dir_name = album.directory.to_string();
            let album_sort_title = album_title
                .map(|s| s.to_string())
                .unwrap_or_else(|| album.directory.components().last().unwrap().to_string());

            let album_sort_key = (album_artist, album_sort_title);

            albums_by_id.insert(album_id, album);

            (album_id, album_sort_key)
        })
        .sorted_by(|(_a_id, a_sort_key), (_b_id, b_sort_key)| {
            compare_tuples_with_nones_last(a_sort_key, b_sort_key)
        })
        .map(|(id, _)| id)
        .collect();

    // Add AlbumIds to songs that are in albums
    for (album_id, album_dir) in &albums_by_id {
        for song_id in &album_dir.song_ids {
            if let Some(song) = songs_by_id.get_mut(&song_id) {
                song.album_id = Some(*album_id);
            }
        }
    }

    MusicDir::new(sorted_albums, songs_by_id, albums_by_id)
}

// itertools' sorted_by_key puts None first
fn compare_tuples_with_nones_last(
    (a_artist, a_title): &(Option<&str>, String),
    (b_artist, b_title): &(Option<&str>, String),
) -> Ordering {
    match (a_artist, b_artist) {
        (None, Some(_)) => {
            return Ordering::Greater;
        }
        (Some(_), None) => {
            return Ordering::Less;
        }
        (Some(a_artist), Some(b_artist)) => match a_artist.cmp(b_artist) {
            Ordering::Less => {
                return Ordering::Less;
            }
            Ordering::Greater => return Ordering::Greater,
            Ordering::Equal => {}
        },
        (None, None) => {}
    }

    a_title.cmp(b_title)
}

fn is_cover_art(path: &Utf8Path) -> bool {
    path.extension() == Some("jpg") || path.extension() == Some("png")
}

fn is_music(path: &Utf8Path) -> bool {
    path.extension() == Some("mp3")
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

    let tags = metadata_rev.tags();
    for tag in tags.iter() {
        if let Some(key) = tag.std_key.and_then(|key| TagKey::try_from(key).ok()) {
            result.insert(key, tag.value.to_string());
        }
    }

    result
}

pub async fn load_images(paths: Vec<Utf8PathBuf>) -> Option<HashMap<Utf8PathBuf, RgbaBytes>> {
    use iced::futures::future::join_all;

    let results = join_all(paths.into_iter().map(load_image)).await;
    let pairs: Option<Vec<(Utf8PathBuf, RgbaBytes)>> = results.into_iter().collect();
    let bytes_by_path: HashMap<_, _> = pairs?.into_iter().collect();

    Some(bytes_by_path)
}

async fn load_image(utf8_path: Utf8PathBuf) -> Option<(Utf8PathBuf, RgbaBytes)> {
    let bytes = load_rgba(&utf8_path)?;
    Some((utf8_path, bytes))
}
