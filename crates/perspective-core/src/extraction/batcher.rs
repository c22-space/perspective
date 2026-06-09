use std::collections::VecDeque;
use std::time::Instant;

use crate::config::ExtractionConfig;

/// A smart batcher that accumulates (tenant_id, text) pairs and flushes
/// when either the batch size limit is reached or a time interval has elapsed.
pub struct ExtractionBatcher {
    buffer: VecDeque<(String, String)>,
    batch_size: usize,
    interval: std::time::Duration,
    last_flush: Instant,
}

impl ExtractionBatcher {
    /// Create a new batcher. Reads `batch_size` and `batch_interval_secs`
    /// from the pipeline's configuration.
    pub fn new(config: &ExtractionConfig) -> Self {
        Self {
            buffer: VecDeque::new(),
            batch_size: config.batch_size,
            interval: std::time::Duration::from_secs(config.batch_interval_secs),
            last_flush: Instant::now(),
        }
    }

    /// Create a batcher with explicit parameters (useful for testing).
    pub fn with_params(batch_size: usize, interval: std::time::Duration) -> Self {
        Self {
            buffer: VecDeque::new(),
            batch_size,
            interval,
            last_flush: Instant::now(),
        }
    }

    /// Add a text to the buffer, tagged with its tenant_id.
    pub fn buffer(&mut self, tenant_id: &str, text: &str) {
        self.buffer
            .push_back((tenant_id.to_string(), text.to_string()));
    }

    /// Returns `true` when the batch should be flushed.
    pub fn should_flush(&self) -> bool {
        self.buffer.len() >= self.batch_size
            || (!self.buffer.is_empty() && self.last_flush.elapsed() >= self.interval)
    }

    /// Drain all buffered items. Returns Vec of (tenant_id, text).
    pub fn drain(&mut self) -> Vec<(String, String)> {
        let items: Vec<(String, String)> = self.buffer.drain(..).collect();
        self.last_flush = Instant::now();
        items
    }

    /// Returns the current number of buffered items.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns `true` if the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_and_drain() {
        let mut batcher = ExtractionBatcher::with_params(
            3,
            std::time::Duration::from_secs(3600),
        );

        assert!(batcher.is_empty());
        assert!(!batcher.should_flush());

        batcher.buffer("tenant_a", "first");
        assert_eq!(batcher.len(), 1);
        assert!(!batcher.should_flush());

        batcher.buffer("tenant_b", "second");
        assert_eq!(batcher.len(), 2);
        assert!(!batcher.should_flush());

        batcher.buffer("tenant_a", "third");
        assert_eq!(batcher.len(), 3);
        assert!(batcher.should_flush());

        let items = batcher.drain();
        assert_eq!(items.len(), 3);
        assert_eq!(items[0], ("tenant_a".into(), "first".into()));
        assert_eq!(items[1], ("tenant_b".into(), "second".into()));
        assert_eq!(items[2], ("tenant_a".into(), "third".into()));
        assert!(batcher.is_empty());
    }

    #[test]
    fn test_flush_on_interval() {
        let mut batcher = ExtractionBatcher::with_params(
            100,
            std::time::Duration::from_millis(0),
        );

        batcher.buffer("t", "hello");
        assert!(batcher.should_flush());
        let items = batcher.drain();
        assert_eq!(items, vec![("t".into(), "hello".into())]);
    }

    #[test]
    fn test_drain_resets_timer() {
        let mut batcher =
            ExtractionBatcher::with_params(2, std::time::Duration::from_millis(0));

        batcher.buffer("t", "a");
        assert!(batcher.should_flush());
        let _ = batcher.drain();

        assert!(!batcher.should_flush());
    }
}
