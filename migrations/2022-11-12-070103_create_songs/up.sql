create table songs (
  id integer primary key not null,
  album_id integer references albums (id) not null,
  file text not null,
  total_seconds bigint not null,

  title text,
  artist text,
  track_number integer
);
