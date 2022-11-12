create table albums (
  id integer primary key not null,
  directory text not null,

  title text,
  artist text,
  release_date text,
  original_art text,
  resized_art text
);
