use std::collections::HashMap;
use std::sync::atomic::{self, AtomicUsize};

use camino::{Utf8Path, Utf8PathBuf};
use iced::Element;
use symphonia::core::meta::StandardTagKey;

use super::rgba::RgbaBytes;

// TODO remove the unwraps here when moving to sqlite

#[derive(Debug, Clone)]
pub struct MusicDir {
    sorted_albums: Vec<AlbumId>,
    songs_by_id: HashMap<SongId, TaggedSong>,
    albums_by_id: HashMap<AlbumId, AlbumDir>,
}

impl MusicDir {
    pub fn new(
        sorted_albums: Vec<AlbumId>,
        songs_by_id: HashMap<SongId, TaggedSong>,
        albums_by_id: HashMap<AlbumId, AlbumDir>,
    ) -> Self {
        MusicDir {
            sorted_albums,
            songs_by_id,
            albums_by_id,
        }
    }

    pub fn albums(&self) -> Vec<&AlbumDir> {
        self.sorted_albums
            .iter()
            .map(|id| self.albums_by_id.get(id).unwrap())
            .collect()
    }

    pub fn add_album_covers(&mut self, mut loaded_images_by_path: HashMap<Utf8PathBuf, RgbaBytes>) {
        for (_id, album) in self.albums_by_id.iter_mut() {
            // TODO this needs a better way of matching loaded images up to albums
            // ie, by sql id
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
        self.songs_by_id.get(song_id).expect("unexpected song id")
    }

    pub fn get_song_by_path(&self, song_path: Utf8PathBuf) -> &TaggedSong {
        let (_id, song) = self
            .songs_by_id
            .iter()
            .find(|(_id, song)| song.path == song_path)
            .expect("no matching song path found");

        song
    }

    pub fn with_joined_song_data<'a, F, M>(&'a self, view_fn: F) -> Vec<Element<'a, M>>
    where
        F: Fn(&AlbumDirView<'a>) -> Element<'a, M>,
    {
        let mut elements: Vec<Element<'a, M>> = Vec::new();

        for album_id in &self.sorted_albums {
            let album = self.albums_by_id.get(album_id).unwrap();

            let mut songs: Vec<&'a TaggedSong> = Vec::new();

            for song_id in &album.song_ids {
                let song = self.songs_by_id.get(song_id).expect("unexpected song id");
                songs.push(song);
            }

            let album_view = AlbumDirView {
                directory: &album.directory,
                loaded_cover: &album.loaded_cover,
                songs,
            };

            let element = view_fn(&album_view);
            elements.push(element)
        }

        elements
    }

    pub fn get_album(&self, album_id: &AlbumId) -> &AlbumDir {
        self.albums_by_id.get(album_id).unwrap()
    }
}

#[derive(Debug, Clone)]
pub struct AlbumDir {
    pub id: AlbumId,
    pub directory: Utf8PathBuf,
    // sorted by track number
    pub song_ids: Vec<SongId>,
    // unsorted, should have only 1
    pub covers: Vec<Utf8PathBuf>,
    // added after metadata when conversion finishes
    pub loaded_cover: Option<RgbaBytes>,
}

impl AlbumDir {
    pub fn new(directory: Utf8PathBuf, song_ids: Vec<SongId>, covers: Vec<Utf8PathBuf>) -> Self {
        let id = AlbumId(directory.clone());

        Self {
            id,
            directory,
            song_ids,
            covers,
            loaded_cover: None,
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct AlbumId(Utf8PathBuf);

#[derive(Debug, Clone)]
pub struct AlbumDirView<'a> {
    pub directory: &'a Utf8Path,
    // sorted by track number
    pub songs: Vec<&'a TaggedSong>,
    // added after metadata when conversion finishes
    pub loaded_cover: &'a Option<RgbaBytes>,
}

impl<'a> AlbumDirView<'a> {
    pub fn display_title(&self) -> &str {
        if let Some(first_tag) = self.songs.first().and_then(|&song| song.album_title()) {
            return first_tag;
        }

        self.directory.components().last().unwrap().as_str()
    }

    pub fn display_artist(&self) -> Option<&str> {
        self.songs.first().and_then(|&song| song.artist())
    }
}

#[derive(Debug, Clone)]
pub struct TaggedSong {
    pub id: SongId,
    pub path: Utf8PathBuf,
    pub album_id: Option<AlbumId>,
    pub tags: HashMap<TagKey, String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct SongId(usize);

// Id counter impl taken from iced_native::widget
static NEXT_SONG_ID: AtomicUsize = AtomicUsize::new(0);

impl SongId {
    pub fn unique() -> Self {
        let id = NEXT_SONG_ID.fetch_add(1, atomic::Ordering::Relaxed);
        Self(id)
    }
}

impl TaggedSong {
    pub fn new(
        path: Utf8PathBuf,
        album_id: Option<AlbumId>,
        tags: HashMap<TagKey, String>,
    ) -> Self {
        let id = SongId::unique();

        Self {
            id,
            path,
            album_id,
            tags,
        }
    }

    pub fn album_id(&self) -> AlbumId {
        // NOTE this has to stay in sync
        // with how album ids are generated
        AlbumId(self.path.with_file_name(""))
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
