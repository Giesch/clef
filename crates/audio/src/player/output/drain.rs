use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[derive(Default)]
pub(super) struct DrainProgress {
    acknowledged: AtomicU64,
    end_nanos: AtomicU64,
}

impl DrainProgress {
    pub(super) fn publish(
        &self,
        samples: usize,
        channels: u16,
        rate: u32,
        now: Duration,
        latency: Duration,
    ) {
        if samples == 0 {
            return;
        }

        let duration = Duration::from_secs_f64(
            samples as f64 / f64::from(channels) / f64::from(rate),
        );
        let end = now.saturating_add(latency).saturating_add(duration);
        self.end_nanos.store(
            end.as_nanos().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
        self.acknowledged
            .fetch_add(samples as u64, Ordering::Release);
    }

    pub(super) fn ready(&self, produced: u64, elapsed: Duration) -> bool {
        let acknowledged = self.acknowledged.load(Ordering::Acquire);
        let end = Duration::from_nanos(self.end_nanos.load(Ordering::Relaxed));

        acknowledged >= produced && elapsed >= end
    }

    pub(super) fn finished(
        &self,
        produced: u64,
        elapsed: Duration,
        timeout: Duration,
    ) -> bool {
        self.ready(produced, elapsed) || elapsed >= timeout
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drain_waits_for_acknowledged_playback_tail_and_bounds_wait() {
        let progress = DrainProgress::default();
        assert!(progress.ready(0, Duration::ZERO));
        assert!(!progress.ready(4, Duration::from_secs(1)));
        progress.publish(4, 2, 1000, Duration::ZERO, Duration::from_millis(10));
        assert!(!progress.ready(4, Duration::from_millis(11)));
        assert!(progress.ready(4, Duration::from_millis(12)));
        progress.publish(0, 2, 1000, Duration::from_secs(1), Duration::from_secs(1));
        assert!(progress.ready(4, Duration::from_millis(12)));
        assert!(
            !progress.ready(8, Duration::from_secs(1)),
            "unpublished samples cannot reuse a stale end time"
        );
        assert!(progress.finished(8, Duration::from_secs(2), Duration::from_secs(2)));
        progress.publish(2, 2, 1000, Duration::from_millis(20), Duration::ZERO);
        assert!(!progress.ready(6, Duration::from_millis(20)));
        assert!(progress.ready(6, Duration::from_millis(21)));
    }
}
