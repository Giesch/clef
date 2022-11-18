use symphonia::core::codecs::{CodecParameters, CODEC_TYPE_NULL};
use symphonia::core::formats::Track;
use symphonia::core::units::TimeBase;

use crate::audio::player::ProgressTimes;

pub fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
}

#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub id: u32,
    pub time_base: Option<TimeBase>,
    pub duration: Option<u64>,
}

impl TrackInfo {
    /// Given a packet timestamp, returns the progress times to display for the track
    /// None = either time base or duration is missing from the track info
    pub fn progress_times(&self, timestamp: u64) -> Option<ProgressTimes> {
        match (self.time_base, self.duration) {
            (Some(time_base), Some(duration)) => Some(ProgressTimes {
                elapsed: time_base.calc_time(timestamp),
                remaining: time_base.calc_time(duration.saturating_sub(timestamp)),
                total: time_base.calc_time(duration),
            }),

            _ => None,
        }
    }
}

impl From<&Track> for TrackInfo {
    fn from(track: &Track) -> Self {
        let CodecParameters { time_base, n_frames, start_ts, .. } = track.codec_params;

        Self {
            id: track.id,
            time_base,
            duration: n_frames.map(|frames| start_ts + frames),
        }
    }
}
