use std::collections::{HashMap, VecDeque};

use camino::{Utf8Path, Utf8PathBuf};
use iced::Element;

use crate::channels::Queue;

use super::rgba::RgbaBytes;

pub mod song_id;
pub use song_id::*;

pub mod album_id;
pub use album_id::*;

pub mod tag_key;
pub use tag_key::*;

#[derive(thiserror::Error, Debug, Clone)]
pub enum LoadMusicError {
    #[error("no audio directory found")]
    NoAudioDirectory,
    #[error("error walking audio directory")]
    WalkError,
    #[error("no user config dir available")]
    NoConfigDir,
}

#[derive(Debug, Clone)]
pub struct Music {
    sorted_albums: Vec<AlbumId>,
    songs_by_id: HashMap<SongId, TaggedSong>,
    albums_by_id: HashMap<AlbumId, AlbumDir>,
    /// Application subdirectory of the user's config dir
    config_dir: Utf8PathBuf,
}

impl Music {
    pub fn new(
        sorted_albums: Vec<AlbumId>,
        songs_by_id: HashMap<SongId, TaggedSong>,
        albums_by_id: HashMap<AlbumId, AlbumDir>,
        config_dir: Utf8PathBuf,
    ) -> Self {
        Music {
            sorted_albums,
            songs_by_id,
            albums_by_id,
            config_dir,
        }
    }

    pub fn albums(&self) -> Vec<&AlbumDir> {
        self.sorted_albums
            .iter()
            .map(|id| self.albums_by_id.get(id).unwrap())
            .collect()
    }

    pub fn add_album_covers(
        &mut self,
        mut loaded_images_by_path: HashMap<Utf8PathBuf, RgbaBytes>,
    ) {
        for (_id, album) in self.albums_by_id.iter_mut() {
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

    pub fn get_album_queue(
        &self,
        song: &TaggedSong,
    ) -> Option<Queue<(SongId, Utf8PathBuf)>> {
        if let Some(album_id) = song.album_id {
            let album = self.get_album(&album_id);

            let mut previous = Vec::new();
            let mut next = VecDeque::new();
            let mut current = None;

            for song_id in &album.song_ids {
                let album_song = self.get_song(song_id);
                let path = album_song.path.clone();

                if current.is_none() {
                    if album_song.id == song.id {
                        current = Some((*song_id, path));
                    } else {
                        previous.push((*song_id, path));
                    }
                } else {
                    next.push_back((*song_id, path));
                }
            }

            if let Some(current) = current {
                Some(Queue { previous, current, next })
            } else {
                None
            }
        } else {
            Some(Queue {
                previous: Default::default(),
                current: (song.id, song.path.clone()),
                next: Default::default(),
            })
        }
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
    pub fn new(
        directory: Utf8PathBuf,
        song_ids: Vec<SongId>,
        covers: Vec<Utf8PathBuf>,
    ) -> Self {
        let id = AlbumId::unique();

        Self {
            id,
            directory,
            song_ids,
            covers,
            loaded_cover: None,
        }
    }
}

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
        if let Some(first_tag) = self.get_tag(TagKey::Album) {
            return first_tag;
        }

        self.directory.components().last().unwrap().as_str()
    }

    pub fn display_artist(&self) -> Option<&str> {
        if let Some(album_artist) = self.get_tag(TagKey::AlbumArtist) {
            return Some(album_artist);
        }

        self.get_tag(TagKey::Artist)
    }

    pub fn date(&self) -> Option<&str> {
        self.get_tag(TagKey::Date)
    }

    fn get_tag(&self, tag: TagKey) -> Option<&str> {
        self.songs.first().and_then(|&song| song.get_tag(tag))
    }
}

#[derive(Debug, Clone)]
pub struct TaggedSong {
    pub id: SongId,
    pub path: Utf8PathBuf,
    pub album_id: Option<AlbumId>,
    pub tags: HashMap<TagKey, String>,
}

impl TaggedSong {
    pub fn new(
        path: Utf8PathBuf,
        album_id: Option<AlbumId>,
        tags: HashMap<TagKey, String>,
    ) -> Self {
        let id = SongId::unique();

        Self { id, path, album_id, tags }
    }

    pub fn display_title(&self) -> &str {
        if let Some(tag) = self.get_tag(TagKey::TrackTitle) {
            return tag;
        }

        self.path.file_stem().unwrap_or_default()
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
