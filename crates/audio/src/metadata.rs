use std::collections::HashMap;
use std::fs::File;

use camino::Utf8Path;
use log::error;
use symphonia::core::{
    formats::{probe::Hint, FormatOptions},
    io::MediaSourceStream,
    meta::{MetadataRevision, StandardTag},
};

use crate::track_info::{first_supported_track, TrackInfo};

#[derive(Debug)]
pub struct DecodedMetadata {
    pub tags: HashMap<TagKey, String>,
    pub total_seconds: u64,
}

/// Includes an empty tag map when tags are absent. Returns None for unreadable media.
pub fn decode_metadata(path: &Utf8Path) -> Option<DecodedMetadata> {
    let result = (|| -> anyhow::Result<DecodedMetadata> {
        let mut hint = Hint::new();
        if let Some(extension) = path.extension() {
            hint.with_extension(extension);
        }
        let source =
            MediaSourceStream::new(Box::new(File::open(path)?), Default::default());
        let mut reader = symphonia::default::get_probe().probe(
            &hint,
            source,
            FormatOptions::default(),
            Default::default(),
        )?;
        let track = first_supported_track(reader.tracks())
            .ok_or_else(|| anyhow::anyhow!("no audio track"))?;
        let info = TrackInfo::from(track);
        let times = info
            .progress_times(symphonia::core::units::Timestamp::ZERO)
            .ok_or_else(|| anyhow::anyhow!("missing track timing"))?;
        let mut tags = HashMap::new();
        let mut metadata = reader.metadata();
        loop {
            if let Some(revision) = metadata.current() {
                gather_tags(revision, info.id, &mut tags);
            }
            if metadata.pop().is_none() {
                break;
            }
        }
        Ok(DecodedMetadata {
            tags,
            total_seconds: times.total.seconds,
        })
    })();
    match result {
        Ok(metadata) => Some(metadata),
        Err(err) => {
            error!("cannot read metadata for {path}: {err}");
            None
        }
    }
}

fn gather_tags(
    revision: &MetadataRevision,
    track_id: u32,
    result: &mut HashMap<TagKey, String>,
) {
    let containers = std::iter::once(&revision.media).chain(
        revision
            .per_track
            .iter()
            .filter(|track| track.track_id == u64::from(track_id))
            .map(|track| &track.metadata),
    );
    for container in containers {
        for tag in &container.tags {
            if let Some(key) = tag.std.as_ref().and_then(tag_key) {
                let value = match tag.std.as_ref() {
                    Some(
                        StandardTag::TrackNumber(value)
                        | StandardTag::TrackTotal(value)
                        | StandardTag::PartTotal(value)
                        | StandardTag::MovementNumber(value),
                    ) => value.to_string(),
                    _ => tag.raw.value.to_string(),
                };
                result.insert(key, value);
            }
        }
    }
}

/// The standard tags exposed to the application.
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

fn tag_key(tag: &StandardTag) -> Option<TagKey> {
    Some(match tag {
        StandardTag::Album(_) => TagKey::Album,
        StandardTag::AlbumArtist(_) => TagKey::AlbumArtist,
        StandardTag::Artist(_) => TagKey::Artist,
        StandardTag::Composer(_) => TagKey::Composer,
        StandardTag::Conductor(_) => TagKey::Conductor,
        StandardTag::RecordingDate(_) => TagKey::Date,
        StandardTag::Description(_) => TagKey::Description,
        StandardTag::Genre(_) => TagKey::Genre,
        StandardTag::Label(_) => TagKey::Label,
        StandardTag::Language(_) => TagKey::Language,
        StandardTag::Lyrics(_) => TagKey::Lyrics,
        StandardTag::Mood(_) => TagKey::Mood,
        StandardTag::MovementName(_) => TagKey::MovementName,
        StandardTag::MovementNumber(_) => TagKey::MovementNumber,
        StandardTag::Part(_) => TagKey::Part,
        StandardTag::PartTotal(_) => TagKey::PartTotal,
        StandardTag::Producer(_) => TagKey::Producer,
        StandardTag::ReleaseDate(_) => TagKey::ReleaseDate,
        StandardTag::Remixer(_) => TagKey::Remixer,
        StandardTag::TrackNumber(_) => TagKey::TrackNumber,
        StandardTag::TrackSubtitle(_) => TagKey::TrackSubtitle,
        StandardTag::TrackTitle(_) => TagKey::TrackTitle,
        StandardTag::TrackTotal(_) => TagKey::TrackTotal,
        _ => return None,
    })
}
