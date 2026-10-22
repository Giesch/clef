use symphonia::core::formats::Track;
use symphonia::core::units::{Duration, TimeBase, Timestamp};

use crate::player::{ProgressTime, ProgressTimes};

pub fn first_supported_track(tracks: &[Track]) -> Option<&Track> {
    tracks.iter().find(|track| {
        track
            .codec_params
            .as_ref()
            .and_then(|params| params.audio())
            .is_some()
    })
}

#[derive(Debug, Clone)]
pub struct TrackInfo {
    pub id: u32,
    pub time_base: Option<TimeBase>,
    pub duration: Option<Duration>,
}

impl TrackInfo {
    /// Convert presentation timestamps to nonnegative display times.
    pub fn progress_times(&self, timestamp: Timestamp) -> Option<ProgressTimes> {
        let time_base = self.time_base?;
        let duration = self.duration?;
        let total = time_base.calc_duration(duration)?;
        let elapsed = time_base
            .calc_time(timestamp)?
            .as_secs_f64()
            .clamp(0.0, total.as_secs_f64());
        Some(ProgressTimes {
            elapsed: ProgressTime::from_seconds(elapsed),
            remaining: ProgressTime::from_seconds(total.as_secs_f64() - elapsed),
            total: ProgressTime::from_seconds(total.as_secs_f64()),
        })
    }
}

impl From<&Track> for TrackInfo {
    fn from(track: &Track) -> Self {
        Self {
            id: track.id,
            time_base: track.time_base,
            duration: track.duration,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timing_missing_overflow_and_bounds() {
        let base = TimeBase::try_from_recip(44_100).unwrap();
        let mut info = TrackInfo {
            id: 0,
            time_base: Some(base),
            duration: Some(44_100u64.into()),
        };
        assert_eq!(
            info.progress_times((-100i64).into())
                .unwrap()
                .elapsed
                .seconds,
            0
        );
        assert_eq!(
            info.progress_times(88_200i64.into())
                .unwrap()
                .remaining
                .seconds,
            0
        );
        info.duration = None;
        assert!(info.progress_times(Timestamp::ZERO).is_none());
        info.duration = Some(u64::MAX.into());
        info.time_base = TimeBase::try_from_recip(1);
        assert!(info.progress_times(Timestamp::ZERO).is_none());
        info.time_base = None;
        assert!(info.progress_times(Timestamp::ZERO).is_none());
    }
}
