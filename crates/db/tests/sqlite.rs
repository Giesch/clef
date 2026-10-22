use camino::{Utf8Path, Utf8PathBuf};
use clef_db::queries::{
    add_resized_image_location, find_or_insert_album, find_or_insert_song, NewAlbum,
    NewSong,
};
use clef_db::{create_pool, run_migrations, SqlitePool};
use diesel::prelude::*;
use diesel::sql_types::{Integer, Text};

fn temporary_pool() -> (tempfile::TempDir, SqlitePool) {
    let directory = tempfile::tempdir().expect("create isolated database directory");
    let path = Utf8PathBuf::from_path_buf(directory.path().join("test.sqlite"))
        .expect("temporary path is UTF-8");
    let pool = create_pool(&path).expect("create temporary SQLite pool");

    (directory, pool)
}

#[test]
fn sqlite_migrations_queries_roundtrip() {
    let (_directory, pool) = temporary_pool();
    run_migrations(&pool).expect("run migrations");
    run_migrations(&pool).expect("migrations are repeatable");
    let mut connection = pool.get().expect("get connection");

    let new_album = NewAlbum {
        directory: "test-library/album".into(),
        title: Some("Album".into()),
        artist: Some("Artist".into()),
        release_date: Some("2026".into()),
        original_art: Some("test-library/album/cover.png".into()),
        resized_art: None,
    };
    let album =
        find_or_insert_album(&mut connection, new_album.clone()).expect("insert album");
    let existing_album =
        find_or_insert_album(&mut connection, new_album).expect("read album");
    assert_eq!(existing_album.id, album.id);
    assert_eq!(
        existing_album.directory,
        Utf8Path::new("test-library/album")
    );
    assert_eq!(existing_album.title.as_deref(), Some("Album"));
    assert_eq!(existing_album.artist.as_deref(), Some("Artist"));
    assert_eq!(existing_album.release_date.as_deref(), Some("2026"));
    assert_eq!(existing_album.original_art, album.original_art);

    let new_song = NewSong {
        album_id: album.id,
        file: "test-library/album/song.flac".into(),
        total_seconds: 123,
        title: Some("Song".into()),
        artist: Some("Artist".into()),
        track_number: Some(2),
    };
    let song =
        find_or_insert_song(&mut connection, new_song.clone()).expect("insert song");
    let existing_song =
        find_or_insert_song(&mut connection, new_song).expect("read song");
    assert_eq!(existing_song.id, song.id);
    assert_eq!(existing_song.album_id, album.id);
    assert_eq!(
        existing_song.file,
        Utf8Path::new("test-library/album/song.flac")
    );
    assert_eq!(existing_song.total_seconds, 123);
    assert_eq!(existing_song.title.as_deref(), Some("Song"));
    assert_eq!(existing_song.artist.as_deref(), Some("Artist"));
    assert_eq!(existing_song.track_number, Some(2));

    let resized = Utf8Path::new("test-cache/cover.png");
    add_resized_image_location(&mut connection, album.id, resized)
        .expect("update album art");
    use clef_db::schema::{albums, songs};
    let stored_art: Option<String> = albums::table
        .find(album.id.unpack())
        .select(albums::resized_art)
        .first(&mut connection)
        .expect("read updated album art");
    assert_eq!(stored_art.as_deref(), Some(resized.as_str()));
    assert_eq!(
        albums::table
            .count()
            .get_result::<i64>(&mut connection)
            .unwrap(),
        1
    );
    assert_eq!(
        songs::table
            .count()
            .get_result::<i64>(&mut connection)
            .unwrap(),
        1
    );

    // Reopen a populated database with its existing schema and migration history.
    drop(connection);
    drop(pool);
    let database_path = Utf8PathBuf::from_path_buf(_directory.path().join("test.sqlite"))
        .expect("temporary path is UTF-8");
    let reopened = create_pool(&database_path).expect("reopen existing database");
    run_migrations(&reopened).expect("existing migrations remain applied");
    let mut connection = reopened.get().expect("get reopened connection");
    let stored_album: (Option<String>, Option<String>) = albums::table
        .find(album.id.unpack())
        .select((albums::title, albums::resized_art))
        .first(&mut connection)
        .expect("read preserved album");
    let stored_song: (Option<String>, String, i32) = songs::table
        .filter(songs::file.eq(song.file.as_str()))
        .select((songs::title, songs::file, songs::album_id))
        .first(&mut connection)
        .expect("read preserved song");
    assert_eq!(stored_album.0, album.title);
    assert_eq!(stored_album.1.as_deref(), Some(resized.as_str()));
    assert_eq!(stored_song.0, song.title);
    assert_eq!(stored_song.1, song.file.as_str());
    assert_eq!(stored_song.2, album.id.unpack());
}

#[derive(Debug, QueryableByName)]
struct JournalMode {
    #[diesel(sql_type = Text)]
    journal_mode: String,
}

#[derive(Debug, QueryableByName)]
struct ForeignKeys {
    #[diesel(sql_type = Integer)]
    foreign_keys: i32,
}

#[derive(Debug, QueryableByName)]
struct BusyTimeout {
    #[diesel(sql_type = Integer)]
    timeout: i32,
}

#[test]
fn sqlite_connection_pragmas() {
    let (_directory, pool) = temporary_pool();
    // Keep both connections checked out to verify connection-local settings too.
    let mut first = pool.get().expect("get first connection");
    let mut second = pool.get().expect("get second connection");
    for connection in [&mut *first, &mut *second] {
        let journal = diesel::sql_query("PRAGMA journal_mode")
            .get_result::<JournalMode>(connection)
            .expect("read journal mode");
        assert_eq!(journal.journal_mode, "wal");
        let foreign_keys = diesel::sql_query("PRAGMA foreign_keys")
            .get_result::<ForeignKeys>(connection)
            .expect("read foreign keys setting");
        assert_eq!(foreign_keys.foreign_keys, 1);
        let busy_timeout = diesel::sql_query("PRAGMA busy_timeout")
            .get_result::<BusyTimeout>(connection)
            .expect("read busy timeout");
        assert_eq!(busy_timeout.timeout, 5000);
    }
}
