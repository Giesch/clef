// @generated automatically by Diesel CLI.

diesel::table! {
    albums (id) {
        id -> Integer,
        directory -> Text,
        title -> Nullable<Text>,
        artist -> Nullable<Text>,
        release_date -> Nullable<Text>,
        original_art -> Nullable<Text>,
        resized_art -> Nullable<Text>,
    }
}

diesel::table! {
    songs (id) {
        id -> Integer,
        album_id -> Integer,
        file -> Text,
        title -> Nullable<Text>,
        artist -> Nullable<Text>,
        track_number -> Nullable<Integer>,
    }
}

diesel::joinable!(songs -> albums (album_id));

diesel::allow_tables_to_appear_in_same_query!(
    albums,
    songs,
);
