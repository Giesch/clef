use diesel::prelude::*;

use super::schema::albums;
use super::schema::songs;

#[derive(Queryable, Debug)]
pub(super) struct AlbumRow {
    pub id: i32,
    pub directory: String,

    pub title: Option<String>,
    pub artist: Option<String>,
    pub release_date: Option<String>,
    pub original_art: Option<String>,
    pub resized_art: Option<String>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = albums)]
pub(super) struct NewAlbumRow {
    pub directory: String,

    pub title: Option<String>,
    pub artist: Option<String>,
    pub release_date: Option<String>,
    pub original_art: Option<String>,
    pub resized_art: Option<String>,
}

#[derive(Queryable, Debug)]
pub(super) struct SongRow {
    pub id: i32,
    pub album_id: i32,
    pub file: String,

    pub title: Option<String>,
    pub artist: Option<String>,
    pub track_number: Option<i32>,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = songs)]
pub(super) struct NewSongRow {
    pub album_id: i32,
    pub file: String,

    pub title: Option<String>,
    pub artist: Option<String>,
    pub track_number: Option<i32>,
}
