use std::{
    cmp::Ordering,
    collections::{HashMap, VecDeque},
};

use camino::Utf8PathBuf;

use crate::{
    channels::Queue,
    db::queries::{Album, AlbumId, Song, SongId},
};

use super::crawler::CrawledAlbum;

#[derive(Default, Debug)]
pub struct MusicCache {
    album_display_order: Vec<(AlbumId, AlbumSortKey)>,
    songs_by_id: HashMap<SongId, Song>,
    albums_by_id: HashMap<AlbumId, CachedAlbum>,
}

// this is intended include image data
#[derive(Debug)]
pub struct CachedAlbum {
    pub album: Album,
    pub songs: Vec<Song>,
}

/// Artist, Display Title
type AlbumSortKey = (Option<String>, String);

impl MusicCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn albums(&self) -> Vec<&CachedAlbum> {
        let mut albums = Vec::new();

        for (album_id, _sort_key) in self.album_display_order.iter() {
            if let Some(album) = self.albums_by_id.get(album_id) {
                albums.push(album);
            }
        }

        albums
    }

    pub fn add_crawled_album(&mut self, crawled: CrawledAlbum) {
        for song in &crawled.songs {
            self.songs_by_id.insert(song.id, song.clone());
        }

        let sort_key = (
            crawled.album.artist.clone(),
            crawled.album.display_title().to_string(),
        );
        self.album_display_order.push((crawled.album.id, sort_key));
        self.album_display_order
            .sort_by(|a, b| artist_then_title_with_nones_last(&a.1, &b.1));

        let album_id = crawled.album.id;
        let album = crawled.album;
        let songs = crawled.songs;
        let cached_album = CachedAlbum { album, songs };

        self.albums_by_id.insert(album_id, cached_album);
    }

    pub fn get_song(&self, song_id: &SongId) -> &Song {
        self.songs_by_id.get(song_id).expect("unexpected song id")
    }

    pub fn get_album_queue(&self, song: &Song) -> Queue<(SongId, Utf8PathBuf)> {
        let album = self.albums_by_id.get(&song.album_id).unwrap();

        let mut previous = Vec::new();
        let mut next = VecDeque::new();
        let mut current = None;

        for album_song in &album.songs {
            let path = album_song.file.clone();

            if current.is_none() {
                if album_song.id == song.id {
                    current = Some((song.id, path));
                } else {
                    previous.push((song.id, path));
                }
            } else {
                next.push_back((song.id, path));
            }
        }

        let current = current.expect("failed to find matching song for album queue");

        Queue { previous, current, next }
    }
}

// default lexicographic sort puts None first
fn artist_then_title_with_nones_last(
    (a_artist, a_title): &AlbumSortKey,
    (b_artist, b_title): &AlbumSortKey,
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
