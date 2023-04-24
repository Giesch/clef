use std::str::FromStr;

use camino::Utf8PathBuf;

use clef_db::queries::*;

use crate::app::crawler::CrawledAlbum;

pub fn fake_album() -> CrawledAlbum {
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

    CrawledAlbum { album, songs, cached_art: None }
}

pub fn fake_song(number: i32, title: &str, album_id: AlbumId) -> Song {
    Song {
        id: SongId::new(number),
        album_id,
        file: Utf8PathBuf::from_str(title).unwrap(),
        title: Some(title.to_string()),
        artist: Some("Fake Artist".to_string()),
        track_number: Some(number),
        total_seconds: 100,
    }
}
