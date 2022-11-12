use diesel::prelude::*;

use super::schema::songs;

#[derive(Queryable, Debug)]
pub struct Song {
    pub id: i32,
    pub file: String,
}

#[derive(Insertable, Debug)]
#[diesel(table_name = songs)]
pub struct NewSong {
    pub file: String,
}
