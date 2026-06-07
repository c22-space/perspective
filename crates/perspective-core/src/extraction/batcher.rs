use std::collections::VecDeque;
use std::time::Instant;

use crate::config::ExtractionConfig;

/// A smart batcher that accumulates texts and flushes when either the batch
/// size limit is reached or a time interval has elapsed.
pub struct ExtractionBatcher {
    buffer: VecDeque<String>,
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

    /// Add a text to the buffer.
    pub fn buffer(&mut self, text: &str) {
        self.buffer.push_back(text.to_string());
    }

    /// Returns `true` when the batch should be flushed: either the buffer has
    /// reached `batch_size` or the interval since the last flush has elapsed.
    pub fn should_flush(&self) -> bool {
        if self.buffer.len() >= self.batch_size {
            return true;
        }
        if self.last_flush.elapsed() >= self.interval && !self.buffer.is_empty() {
            return true;
        }
        false
    }

    /// Drain all buffered items and return them as a `Vec<String>`. Resets the
    /// internal flush timer.
    pub fn drain(&mut self) -> Vec<String> {
        let items: Vec<String> = self.buffer.drain(..).collect();
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
            std::time::Duration::from_secs(3600), // very long interval
        );

        assert!(batcher.is_empty());
        assert!(!batcher.should_flush());

        batcher.buffer("first");
        assert_eq!(batcher.len(), 1);
        assert!(!batcher.should_flush());

        batcher.buffer("second");
        assert_eq!(batcher.len(), 2);
        assert!(!batcher.should_flush());

        batcher.buffer("third");
        assert_eq!(batcher.len(), 3);
        assert!(batcher.should_flush());

        let items = batcher.drain();
        assert_eq!(items, vec!["first", "second", "third"]);
        assert!(batcher.is_empty());
    }

    #[test]
    fn test_flush_on_interval() {
        let mut batcher = ExtractionBatcher::with_params(
            100,                                 // large batch size so it won't trigger
            std::time::Duration::from_millis(0), // immediate
        );

        batcher.buffer("hello");
        assert!(batcher.should_flush());
        let items = batcher.drain();
        assert_eq!(items, vec!["hello"]);
    }

    #[test]
    fn test_drain_resets_timer() {
        let mut batcher = ExtractionBatcher::with_params(2, std::time::Duration::from_millis(0));

        batcher.buffer("a");
        assert!(batcher.should_flush());
        let _ = batcher.drain();

        // After drain, timer is reset but buffer is empty
        assert!(!batcher.should_flush());
    }
}
