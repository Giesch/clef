use std::collections::HashMap;

use camino::{Utf8Path, Utf8PathBuf};
use iced::Element;
use log::error;
use symphonia::core::meta::StandardTagKey;

use super::bgra::BgraBytes;

#[derive(Debug, Clone)]
pub struct MusicDir {
    albums: Vec<AlbumDir>,
    songs_by_id: HashMap<SongId, TaggedSong>,
}

impl MusicDir {
    pub fn new(albums: Vec<AlbumDir>, songs_by_id: HashMap<SongId, TaggedSong>) -> Self {
        MusicDir {
            albums,
            songs_by_id,
        }
    }

    pub fn albums(&self) -> &[AlbumDir] {
        &self.albums
    }

    pub fn add_album_covers(&mut self, mut loaded_images_by_path: HashMap<Utf8PathBuf, BgraBytes>) {
        for mut album in &mut self.albums {
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

    pub fn get_song<'a>(&'a self, song_id: &SongId) -> &'a TaggedSong {
        &self.songs_by_id.get(song_id).expect("unexpected song id")
    }

    pub fn with_album_views<'a, F, M>(&'a self, f: F) -> Vec<Element<'a, M>>
    where
        F: Fn(&AlbumDirView<'a>) -> Element<'a, M>,
    {
        let mut album_views = Vec::new();

        for album in &self.albums {
            let mut songs: Vec<&'a TaggedSong> = Vec::new();

            for song_id in &album.song_ids {
                let song = self.songs_by_id.get(song_id).expect("unexpected song id");
                songs.push(song);
            }

            let view = AlbumDirView {
                directory: &album.directory,
                songs,
                covers: &album.covers,
                loaded_cover: &album.loaded_cover,
            };
            album_views.push(view);
        }

        let mut results: Vec<Element<'a, M>> = Vec::new();
        for album_view in album_views {
            let element = f(&album_view);
            results.push(element)
        }

        return results;
    }
}

#[derive(Debug, Clone)]
pub struct AlbumDir {
    pub directory: Utf8PathBuf,
    // sorted by track number
    pub song_ids: Vec<SongId>,
    // unsorted, should have only 1
    pub covers: Vec<Utf8PathBuf>,
    // added later when conversion finishes
    pub loaded_cover: Option<BgraBytes>,
}

#[derive(Debug, Clone)]
pub struct AlbumDirView<'a> {
    pub directory: &'a Utf8Path,
    // sorted by track number
    pub songs: Vec<&'a TaggedSong>,
    // unsorted, should have only 1
    pub covers: &'a [Utf8PathBuf],
    // added later when conversion finishes
    pub loaded_cover: &'a Option<BgraBytes>,
}

impl<'a> AlbumDirView<'a> {
    pub fn display_title(&self) -> &str {
        if let Some(first_tag) = self
            .songs
            .as_slice()
            .first()
            .and_then(|&song| song.album_title())
        {
            return first_tag;
        }

        self.directory.components().last().unwrap().as_str()
    }

    pub fn display_artist(&self) -> Option<&str> {
        self.songs
            .as_slice()
            .first()
            .and_then(|&song| song.artist())
    }
}

#[derive(Debug, Clone)]
pub struct TaggedSong {
    pub path: Utf8PathBuf,
    pub tags: HashMap<TagKey, String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct SongId(Utf8PathBuf);

impl TaggedSong {
    pub fn id(&self) -> SongId {
        // NOTE for now, this does a clone
        // in the future it should use a uuid or usize from sqlite
        SongId(self.path.clone())
    }

    pub fn display_title(&self) -> &str {
        if let Some(tag) = self.get_tag(TagKey::TrackTitle) {
            return tag;
        }

        self.path.components().last().unwrap().as_str()
    }

    pub fn track_number(&self) -> Option<usize> {
        self.tags
            .get(&TagKey::TrackNumber)
            .and_then(|s| s.parse().ok())
    }

    pub fn album_title(&self) -> Option<&str> {
        self.get_tag(TagKey::Album)
    }

    pub fn artist(&self) -> Option<&str> {
        self.get_tag(TagKey::Artist)
    }

    fn get_tag(&self, key: TagKey) -> Option<&str> {
        self.tags.get(&key).map(|s| &**s)
    }
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
