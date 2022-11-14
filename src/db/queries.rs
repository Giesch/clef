use camino::{Utf8Path, Utf8PathBuf};
use diesel::result::Error as DieselError;
use diesel::SqliteConnection;

use super::models::{AlbumRow, NewAlbumRow, NewSongRow, SongRow};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AlbumId(i32);

impl AlbumId {
    #[cfg(test)]
    pub fn new(id: i32) -> Self {
        Self(id)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SongId(i32);

impl SongId {
    #[cfg(test)]
    pub fn new(id: i32) -> Self {
        Self(id)
    }
}

#[derive(Debug, Clone)]
pub struct Album {
    pub id: AlbumId,
    pub directory: Utf8PathBuf,

    pub title: Option<String>,
    pub artist: Option<String>,
    pub release_date: Option<String>,
    pub original_art: Option<Utf8PathBuf>,
    pub resized_art: Option<Utf8PathBuf>,
}

impl From<AlbumRow> for Album {
    fn from(row: AlbumRow) -> Self {
        Self {
            id: AlbumId(row.id),
            directory: row.directory.into(),
            title: row.title,
            artist: row.artist,
            release_date: row.release_date,
            original_art: row.original_art.map(Into::into),
            resized_art: row.resized_art.map(Into::into),
        }
    }
}

impl Album {
    pub fn display_title(&self) -> Option<&str> {
        if self.title.is_some() {
            return self.title.as_deref();
        }

        let directory_name = self.directory.components().last();

        directory_name.map(|c| c.as_str())
    }
}

#[derive(Debug, Clone)]
pub struct Song {
    pub id: SongId,
    pub album_id: AlbumId,
    pub file: Utf8PathBuf,

    pub title: Option<String>,
    pub artist: Option<String>,
    pub track_number: Option<i32>,
}

impl From<SongRow> for Song {
    fn from(row: SongRow) -> Self {
        Song {
            id: SongId(row.id),
            album_id: AlbumId(row.album_id),
            file: row.file.into(),
            title: row.title,
            artist: row.artist,
            track_number: row.track_number,
        }
    }
}

impl Song {
    pub fn display_title(&self) -> Option<&str> {
        if self.title.is_some() {
            return self.title.as_deref();
        }

        self.file.file_stem()
    }
}

#[derive(Debug, Clone)]
pub struct NewAlbum {
    pub directory: Utf8PathBuf,

    pub title: Option<String>,
    pub artist: Option<String>,
    pub release_date: Option<String>,
    pub original_art: Option<Utf8PathBuf>,
    pub resized_art: Option<Utf8PathBuf>,
}

impl From<NewAlbum> for NewAlbumRow {
    fn from(album: NewAlbum) -> Self {
        Self {
            directory: album.directory.into(),
            original_art: album.original_art.map(Into::into),
            resized_art: album.resized_art.map(Into::into),

            title: album.title,
            artist: album.artist,
            release_date: album.release_date,
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewSong {
    pub album_id: AlbumId,
    pub file: Utf8PathBuf,

    pub title: Option<String>,
    pub artist: Option<String>,
    pub track_number: Option<i32>,
}

impl From<NewSong> for NewSongRow {
    fn from(song: NewSong) -> Self {
        Self {
            album_id: song.album_id.0,
            file: song.file.into(),
            title: song.title,
            artist: song.artist,
            track_number: song.track_number,
        }
    }
}

pub fn find_or_insert_album(
    tx: &mut SqliteConnection,
    new_album: NewAlbum,
) -> Result<Album, DieselError> {
    use super::schema::albums;
    use albums::dsl::*;
    use diesel::prelude::*;

    let new_row: NewAlbumRow = new_album.into();

    let existing_row: Option<AlbumRow> = albums
        .filter(directory.eq(&new_row.directory))
        .first(tx)
        .optional()?;

    if let Some(existing_row) = existing_row {
        return Ok(existing_row.into());
    }

    let created_row: AlbumRow = diesel::insert_into(albums::table)
        .values(&new_row)
        .get_result(tx)?;

    Ok(created_row.into())
}

pub fn find_or_insert_song(
    tx: &mut SqliteConnection,
    new_song: NewSong,
) -> Result<Song, DieselError> {
    use super::schema::songs;
    use diesel::prelude::*;
    use songs::dsl::*;

    let new_row: NewSongRow = new_song.into();
    let existing_row: Option<SongRow> =
        songs.filter(file.eq(&new_row.file)).first(tx).optional()?;

    if let Some(existing_row) = existing_row {
        return Ok(existing_row.into());
    }

    let created_row: SongRow = diesel::insert_into(songs::table)
        .values(&new_row)
        .get_result(tx)?;

    Ok(created_row.into())
}

pub fn add_resized_image_location(
    tx: &mut SqliteConnection,
    AlbumId(album_id): AlbumId,
    location: &Utf8Path,
) -> Result<(), DieselError> {
    use super::schema::albums;
    use albums::dsl::*;
    use diesel::prelude::*;

    diesel::update(albums)
        .filter(id.eq(&album_id))
        .set(resized_art.eq(location.as_str()))
        .execute(tx)?;

    Ok(())
}
