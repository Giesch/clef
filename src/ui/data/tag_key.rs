use symphonia::core::meta::StandardTagKey;

/// A limited set of standard tag keys used by the application
#[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
pub enum TagKey {
    Album,
    AlbumArtist,
    Artist,
    Composer,
    Conductor,
    Date,
    Description,
    Genre,
    Label,
    Language,
    Lyrics,
    Mood,
    MovementName,
    MovementNumber,
    Part,
    PartTotal,
    Producer,
    ReleaseDate,
    Remixer,
    TrackNumber,
    TrackSubtitle,
    TrackTitle,
    TrackTotal,
}

impl TryFrom<StandardTagKey> for TagKey {
    type Error = IgnoredTagError;

    fn try_from(value: StandardTagKey) -> Result<Self, Self::Error> {
        match value {
            StandardTagKey::Album => Ok(TagKey::Album),
            StandardTagKey::AlbumArtist => Ok(TagKey::AlbumArtist),
            StandardTagKey::Artist => Ok(TagKey::Artist),
            StandardTagKey::Composer => Ok(TagKey::Composer),
            StandardTagKey::Conductor => Ok(TagKey::Conductor),
            StandardTagKey::Date => Ok(TagKey::Date),
            StandardTagKey::Description => Ok(TagKey::Description),
            StandardTagKey::Genre => Ok(TagKey::Genre),
            StandardTagKey::Label => Ok(TagKey::Label),
            StandardTagKey::Language => Ok(TagKey::Language),
            StandardTagKey::Lyrics => Ok(TagKey::Lyrics),
            StandardTagKey::Mood => Ok(TagKey::Mood),
            StandardTagKey::MovementName => Ok(TagKey::MovementName),
            StandardTagKey::MovementNumber => Ok(TagKey::MovementNumber),
            StandardTagKey::Part => Ok(TagKey::Part),
            StandardTagKey::PartTotal => Ok(TagKey::PartTotal),
            StandardTagKey::Producer => Ok(TagKey::Producer),
            StandardTagKey::ReleaseDate => Ok(TagKey::ReleaseDate),
            StandardTagKey::Remixer => Ok(TagKey::Remixer),
            StandardTagKey::TrackNumber => Ok(TagKey::TrackNumber),
            StandardTagKey::TrackSubtitle => Ok(TagKey::TrackSubtitle),
            StandardTagKey::TrackTitle => Ok(TagKey::TrackTitle),
            StandardTagKey::TrackTotal => Ok(TagKey::TrackTotal),

            _ => Err(IgnoredTagError::Ignored),
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum IgnoredTagError {
    #[error("ignored tag key")]
    Ignored,
}
