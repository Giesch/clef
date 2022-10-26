use std::collections::HashMap;
use std::fs::File;

use camino::{Utf8Path, Utf8PathBuf};
use log::{error, info};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision};
use symphonia::core::probe::Hint;
use walkdir::WalkDir;

use super::bgra::{load_bgra, BgraBytes};
use super::data::*;

#[derive(thiserror::Error, Debug, Clone)]
pub enum MusicDirError {
    #[error("error walking directory")]
    WalkError,
    #[error("unsupported format")]
    UnsupportedFormat,
}

pub async fn crawl_music_dir() -> Result<MusicDirView, MusicDirError> {
    let mut songs: Vec<TaggedSong> = Vec::new();
    let mut covers: Vec<Utf8PathBuf> = Vec::new();

    for dir_entry in WalkDir::new("/home/giesch/Music").into_iter() {
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

        if is_music(&path) {
            let song = match decode_file(path) {
                Some(decoded) => decoded,
                None => {
                    continue;
                }
            };

            songs.push(song);
        } else if is_cover_art(&path) {
            covers.push(path.to_owned());
        }
    }

    use itertools::Itertools;

    let songs_by_directory: HashMap<Utf8PathBuf, Vec<TaggedSong>> = songs
        .into_iter()
        .map(|song| (song.path.with_file_name(""), song))
        .into_group_map();

    let mut covers_by_directory: HashMap<Utf8PathBuf, Vec<Utf8PathBuf>> = covers
        .into_iter()
        .map(|path| (path.with_file_name(""), path))
        .into_group_map();

    let sorted_directories: Vec<_> = songs_by_directory
        .into_iter()
        .map(|(directory, mut songs)| {
            songs.sort_by_cached_key(|song| song.track_number());
            let covers = covers_by_directory.remove(&directory).unwrap_or_default();

            AlbumDirView {
                directory,
                songs,
                covers,
                loaded_cover: None,
            }
        })
        .sorted_by_key(|album_dir| album_dir.directory.to_string())
        .collect();

    Ok(sorted_directories)
}

fn is_cover_art(path: &Utf8Path) -> bool {
    path.extension() == Some("jpg")
}

fn is_music(path: &Utf8Path) -> bool {
    path.extension() == Some("mp3")
}

// NOTE This returns an empty tag map if they're missing, and None for file not found
fn decode_file(path: &Utf8Path) -> Option<TaggedSong> {
    let tags = decode_tags(path)?;
    let path = path.to_owned();
    Some(TaggedSong { path, tags })
}

// NOTE This returns an empty tag map if they're missing, and None for file not found
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

    let mut probed =
        match symphonia::default::get_probe().format(&hint, mss, &format_opts, &metadata_opts) {
            Ok(p) => p,
            Err(e) => {
                let path_str = path.as_str();
                error!("file in unsupported format: {path_str} {e}");
                return Some(HashMap::new());
            }
        };

    let tags = if let Some(metadata_rev) = probed.format.metadata().current() {
        Some(gather_tags(&metadata_rev))
    } else if let Some(metadata_rev) = probed.metadata.get().as_ref().and_then(|m| m.current()) {
        Some(gather_tags(&metadata_rev))
    } else {
        None
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
