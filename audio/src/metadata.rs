use std::collections::HashMap;
use std::fs::File;

use camino::Utf8Path;
use log::error;
use symphonia::core::meta::{Metadata, StandardTagKey};
use symphonia::core::{
    formats::FormatOptions,
    io::MediaSourceStream,
    meta::{MetadataOptions, MetadataRevision},
    probe::Hint,
};
use symphonia::default::get_probe;

use crate::track_info::{first_supported_track, TrackInfo};

#[derive(Debug)]
pub struct DecodedMetadata {
    pub tags: HashMap<TagKey, String>,
    pub total_seconds: u64,
}

/// NOTE This includes an empty tag map if the tags are missing,
/// and None for file not found or unsupported format
pub fn decode_metadata(path: &Utf8Path) -> Option<DecodedMetadata> {
    let mut hint = Hint::new();
    let source = {
        // Provide the file extension as a hint.
        if let Some(extension) = path.extension() {
            hint.with_extension(extension);
        }

        let file = match File::open(path) {
            Ok(f) => f,
            Err(e) => {
                error!("unexpected file not found: {:?}", e);
                return None;
            }
        };

        Box::new(file)
    };
    let mss = MediaSourceStream::new(source, Default::default());
    let format_opts = FormatOptions {
        enable_gapless: true,
        ..Default::default()
    };
    let metadata_opts: MetadataOptions = Default::default();

    let mut probed = match get_probe().format(&hint, mss, &format_opts, &metadata_opts) {
        Ok(p) => p,
        Err(e) => {
            let path_str = path.as_str();
            error!("file in unsupported format: {path_str} {e}");
            return None;
        }
    };

    let Some(track) = first_supported_track(probed.format.tracks()) else {
        error!("no supported track");
        return None;
    };
    let track_info: TrackInfo = track.into();

    let Some(times) = track_info.progress_times(0) else {
        error!("missing time information for audio file: {path}");
        return None;
    };

    let tags = if let Some(metadata_rev) = probed.format.metadata().current() {
        Some(gather_tags(metadata_rev))
    } else {
        probed
            .metadata
            .get()
            .as_ref()
            .and_then(Metadata::current)
            .map(gather_tags)
    };
    let tags = tags.unwrap_or_default();

    let total_seconds = times.total.seconds;

    Some(DecodedMetadata { tags, total_seconds })
}

fn gather_tags(metadata_rev: &MetadataRevision) -> HashMap<TagKey, String> {
    let mut result = HashMap::new();

    for tag in metadata_rev.tags().iter() {
        if let Some(key) = tag.std_key.and_then(|key| TagKey::try_from(key).ok()) {
            result.insert(key, tag.value.to_string());
        }
    }

    result
}

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
