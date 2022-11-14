use std::cmp::Ordering;
use std::collections::{HashMap, VecDeque};

use camino::Utf8PathBuf;
use log::error;

use crate::channels::Queue;
use crate::db::queries::{Album, AlbumId, Song, SongId};
use crate::ui::{crawler::CrawledAlbum, rgba::RgbaBytes};

#[derive(Default, Debug)]
pub struct MusicCache {
    album_display_order: Vec<(AlbumId, AlbumSortKey)>,
    songs_by_id: HashMap<SongId, Song>,
    albums_by_id: HashMap<AlbumId, CachedAlbum>,
}

#[derive(Debug)]
pub struct CachedAlbum {
    pub album: Album,
    pub songs: Vec<Song>,
    pub art: Option<RgbaBytes>,
}

/// Artist, Display Title
type AlbumSortKey = (Option<String>, Option<String>);

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
            crawled.album.display_title().map(str::to_string),
        );
        self.album_display_order.push((crawled.album.id, sort_key));
        self.album_display_order
            .sort_by(|a, b| artist_then_title_with_nones_last(&a.1, &b.1));

        let album_id = crawled.album.id;
        let cached_album = CachedAlbum {
            album: crawled.album,
            songs: crawled.songs,
            art: crawled.cached_art,
        };

        self.albums_by_id.insert(album_id, cached_album);
    }

    pub fn load_album_art(&mut self, album_id: AlbumId, image_bytes: RgbaBytes) {
        if let Some(album) = self.albums_by_id.get_mut(&album_id) {
            album.art = Some(image_bytes);
        } else {
            error!("loaded art for unknown album: {album_id:#?}");
        }
    }

    pub fn get_song(&self, song_id: &SongId) -> Option<&Song> {
        self.songs_by_id.get(song_id)
    }

    pub fn get_album(&self, album_id: &AlbumId) -> Option<&Album> {
        self.albums_by_id.get(album_id).map(|ca| &ca.album)
    }

    pub fn get_album_queue(
        &self,
        clicked_song_id: SongId,
        clicked_album_id: AlbumId,
    ) -> Option<Queue<(SongId, Utf8PathBuf)>> {
        let album = self.albums_by_id.get(&clicked_album_id)?;

        let mut previous = Vec::new();
        let mut next = VecDeque::new();
        let mut current = None;

        for album_song in &album.songs {
            let id_path = (album_song.id, album_song.file.clone());

            if current.is_none() {
                if album_song.id == clicked_song_id {
                    current = Some(id_path);
                } else {
                    previous.push(id_path);
                }
            } else {
                next.push_back(id_path);
            }
        }

        current.map(|current| Queue { previous, current, next })
    }
}

fn artist_then_title_with_nones_last(
    (a_artist, a_title): &AlbumSortKey,
    (b_artist, b_title): &AlbumSortKey,
) -> Ordering {
    match with_nones_last(a_artist, b_artist) {
        Ordering::Equal => {}
        comparison => return comparison,
    }

    with_nones_last(a_title, b_title)
}

// default lexicographic sort puts None first
fn with_nones_last<T: Ord>(a: &Option<T>, b: &Option<T>) -> Ordering {
    match (a, b) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(a), Some(b)) => a.cmp(b),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn get_album_queue_smoke() {
        let mut music_cache = MusicCache::default();
        let album = fake_album();
        music_cache.add_crawled_album(album.clone());

        let clicked = &album.songs[2];
        let queue = music_cache
            .get_album_queue(clicked.id, clicked.album_id)
            .unwrap();

        assert_eq!(queue.current.0, SongId::new(3));

        let previous_ids: Vec<SongId> =
            queue.previous.into_iter().map(|pair| pair.0).collect();
        assert_eq!(previous_ids, vec![SongId::new(1), SongId::new(2)]);

        let next_ids: Vec<SongId> = queue.next.into_iter().map(|pair| pair.0).collect();
        assert_eq!(next_ids, vec![SongId::new(4), SongId::new(5)]);
    }

    fn fake_album() -> CrawledAlbum {
        let album_id = AlbumId::new(1);
        let album = Album {
            id: album_id,
            directory: Utf8PathBuf::from_str("Album Dir").unwrap(),
            title: Some("Album Title".to_string()),
            artist: Some("Fake Artist".to_string()),
            release_date: None,
            original_art: None,
            resized_art: None,
        };

        let songs = vec![
            fake_song(1, "First", album_id),
            fake_song(2, "Second", album_id),
            fake_song(3, "Third", album_id),
            fake_song(4, "Fourth", album_id),
            fake_song(5, "Fifth", album_id),
        ];

        CrawledAlbum {
            album,
            songs,
            cached_art: None,
            covers: Default::default(),
        }
    }

    fn fake_song(number: i32, title: &str, album_id: AlbumId) -> Song {
        Song {
            id: SongId::new(number),
            album_id,
            file: Utf8PathBuf::from_str(title).unwrap(),
            title: Some(title.to_string()),
            artist: Some("Fake Artist".to_string()),
            track_number: Some(number),
        }
    }
}
