use std::collections::HashMap;
use std::fs::File;

use camino::{Utf8Path, Utf8PathBuf};
use log::{debug, error, info};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub struct MusicDir {
    pub songs_by_directory: HashMap<Utf8PathBuf, Vec<TaggedSong>>,
    pub covers_by_directory: HashMap<Utf8PathBuf, Vec<Utf8PathBuf>>,
}

#[derive(thiserror::Error, Debug, Clone)]
pub enum MusicDirError {
    #[error("error walking directory")]
    WalkError,
    #[error("unsupported format")]
    UnsupportedFormat,
}

pub async fn crawl_music_dir() -> Result<MusicDir, MusicDirError> {
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
            let song = match TaggedSong::decode_file(path) {
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

    let covers_by_directory: HashMap<Utf8PathBuf, Vec<Utf8PathBuf>> = covers
        .into_iter()
        .map(|path| (path.with_file_name(""), path))
        .into_group_map();

    Ok(MusicDir {
        songs_by_directory,
        covers_by_directory,
    })
}

#[derive(Debug, Clone)]
pub struct TaggedSong {
    pub path: Utf8PathBuf,
    pub tags: HashMap<TagKey, String>,
}

/// A limited set of standard tag keys used by the application
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
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

#[derive(thiserror::Error, Debug)]
pub enum IgnoredTagError {
    #[error("ignored tag key")]
    Ignored,
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

impl TaggedSong {
    pub fn new(path: Utf8PathBuf, tags: HashMap<TagKey, String>) -> Self {
        Self { path, tags }
    }

    // NOTE This returns an empty tag map if they're missing, and None for file not found
    pub fn decode_file(path: &Utf8Path) -> Option<Self> {
        let tags = Self::decode_tags(path)?;
        Some(Self::new(path.to_owned(), tags))
    }

    // NOTE This returns an empty tag map if they're missing, and None for file not found
    fn decode_tags(path: &Utf8Path) -> Option<HashMap<TagKey, String>> {
        let mut hint = Hint::new();
        let source = {
            // Provide the file extension as a hint.
            if let Some(extension) = path.extension() {
                hint.with_extension(extension);
            }

            // TODO use async-std file? should this be async?
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

        let mut probed = match symphonia::default::get_probe().format(
            &hint,
            mss,
            &format_opts,
            &metadata_opts,
        ) {
            Ok(p) => p,
            Err(e) => {
                let path_str = path.as_str();
                debug!("file in unsupported format: {path_str} {e}");
                return Some(HashMap::new());
            }
        };

        let tags = if let Some(metadata_rev) = probed.format.metadata().current() {
            Some(gather_tags(&metadata_rev))
        } else if let Some(metadata_rev) = probed.metadata.get().as_ref().and_then(|m| m.current())
        {
            Some(gather_tags(&metadata_rev))
        } else {
            None
        };

        Some(tags.unwrap_or_default())
    }
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

fn is_cover_art(path: &Utf8Path) -> bool {
    path.extension() == Some("jpeg")
}

fn is_music(path: &Utf8Path) -> bool {
    path.extension() == Some("mp3")
}
