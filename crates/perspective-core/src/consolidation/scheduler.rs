use std::time::{Duration, Instant};

use crate::config::ConsolidationConfig;

/// Scheduler that tracks when the last consolidation run occurred
/// and determines whether the next run is due.
pub struct ConsolidationScheduler {
    interval: Duration,
    last_run: Instant,
}

impl ConsolidationScheduler {
    /// Create a new scheduler from the consolidation config.
    pub fn new(config: ConsolidationConfig) -> Self {
        Self {
            interval: Duration::from_secs(config.interval_secs),
            last_run: Instant::now(),
        }
    }

    /// Returns `true` if at least `interval` has elapsed since the last run.
    pub fn should_run(&self) -> bool {
        self.last_run.elapsed() >= self.interval
    }

    /// Record that a consolidation run just completed.
    pub fn mark_run(&mut self) {
        self.last_run = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scheduler_new() {
        let config = ConsolidationConfig {
            enabled: true,
            interval_secs: 3600,
            dedup_similarity_threshold: 0.95,
            promotion_access_count: 5,
            staleness_days: 30,
        };
        let scheduler = ConsolidationScheduler::new(config);
        // Just created, so should not need to run yet
        assert!(!scheduler.should_run());
    }

    #[test]
    fn test_mark_run() {
        let config = ConsolidationConfig {
            enabled: true,
            interval_secs: 0, // zero interval => always ready
            dedup_similarity_threshold: 0.95,
            promotion_access_count: 5,
            staleness_days: 30,
        };
        let mut scheduler = ConsolidationScheduler::new(config);
        // With 0-second interval, should_run returns true
        // (elapsed >= Duration::ZERO is always true for non-zero elapsed)
        // But right after creation elapsed might be 0ns which is == so still true
        assert!(scheduler.should_run());
        scheduler.mark_run();
        // Immediately after mark_run, elapsed is tiny, but interval is 0
        assert!(scheduler.should_run());
    }
}
