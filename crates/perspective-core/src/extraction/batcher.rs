use std::collections::VecDeque;
use std::time::Instant;

use crate::config::ExtractionConfig;

/// Estimate the number of tokens in a text string.
/// Uses a simple heuristic: ~4 characters per token (works for English text).
fn estimate_tokens(text: &str) -> usize {
    // Count non-whitespace characters and divide by 4
    let chars = text.chars().filter(|c| !c.is_whitespace()).count();
    chars / 4
}

/// A smart batcher that accumulates (tenant_id, text) pairs and flushes
/// when the estimated token count reaches the configured limit.
pub struct ExtractionBatcher {
    buffer: VecDeque<(String, String)>,
    buffer_tokens: usize,
    max_tokens: usize,
    interval: std::time::Duration,
    last_flush: Instant,
}

impl ExtractionBatcher {
    /// Create a new batcher. Reads `max_tokens` and `batch_interval_secs`
    /// from the pipeline's configuration.
    pub fn new(config: &ExtractionConfig) -> Self {
        Self {
            buffer: VecDeque::new(),
            buffer_tokens: 0,
            max_tokens: config.batch_size, // batch_size now means max_tokens
            interval: std::time::Duration::from_secs(config.batch_interval_secs),
            last_flush: Instant::now(),
        }
    }

    /// Create a batcher with explicit parameters (useful for testing).
    pub fn with_params(max_tokens: usize, interval: std::time::Duration) -> Self {
        Self {
            buffer: VecDeque::new(),
            buffer_tokens: 0,
            max_tokens,
            interval,
            last_flush: Instant::now(),
        }
    }

    /// Add a text to the buffer, tagged with its tenant_id.
    pub fn buffer(&mut self, tenant_id: &str, text: &str) {
        let tokens = estimate_tokens(text);
        self.buffer_tokens += tokens;
        self.buffer.push_back((tenant_id.to_string(), text.to_string()));
    }

    /// Returns `true` when the batch should be flushed.
    /// Only flushes when estimated tokens reach max_tokens.
    pub fn should_flush(&self) -> bool {
        self.buffer_tokens >= self.max_tokens
    }

    /// Drain all buffered items. Returns Vec of (tenant_id, text).
    pub fn drain(&mut self) -> Vec<(String, String)> {
        let items: Vec<(String, String)> = self.buffer.drain(..).collect();
        self.buffer_tokens = 0;
        self.last_flush = Instant::now();
        items
    }

    /// Returns the current estimated token count in the buffer.
    pub fn current_tokens(&self) -> usize {
        self.buffer_tokens
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
    fn test_token_estimation() {
        // "hello world" has 10 non-whitespace chars, 10/4 = 2 tokens
        assert_eq!(estimate_tokens("hello world"), 2);
        // Empty string
        assert_eq!(estimate_tokens(""), 0);
        // "test" has 4 chars, 4/4 = 1 token
        assert_eq!(estimate_tokens("test"), 1);
    }

    #[test]
    fn test_buffer_and_drain_by_tokens() {
        let mut batcher = ExtractionBatcher::with_params(
            2, // max 2 tokens
            std::time::Duration::from_secs(3600),
        );

        assert!(batcher.is_empty());
        assert_eq!(batcher.current_tokens(), 0);
        assert!(!batcher.should_flush());

        // "test" = 1 token
        batcher.buffer("tenant_a", "test");
        assert_eq!(batcher.len(), 1);
        assert_eq!(batcher.current_tokens(), 1);
        assert!(!batcher.should_flush());

        // "test" = 1 more token, total = 2
        batcher.buffer("tenant_b", "test");
        assert_eq!(batcher.len(), 2);
        assert_eq!(batcher.current_tokens(), 2);
        assert!(batcher.should_flush());

        let items = batcher.drain();
        assert_eq!(items.len(), 2);
        assert!(batcher.is_empty());
        assert_eq!(batcher.current_tokens(), 0);
    }

    #[test]
    fn test_flush_on_interval() {
        let mut batcher = ExtractionBatcher::with_params(
            1000, // high token limit
            std::time::Duration::from_millis(0), // immediate timeout
        );

        batcher.buffer("t", "hello");
        assert!(batcher.should_flush()); // flushes due to interval
        let items = batcher.drain();
        assert_eq!(items, vec![("t".into(), "hello".into())]);
    }

    #[test]
    fn test_drain_resets_state() {
        let mut batcher = ExtractionBatcher::with_params(
            1000,
            std::time::Duration::from_millis(0),
        );

        batcher.buffer("t", "a");
        assert!(batcher.should_flush());
        let _ = batcher.drain();
        assert!(!batcher.should_flush());
        assert_eq!(batcher.current_tokens(), 0);
    }

    #[test]
    fn test_large_text_flushes() {
        let mut batcher = ExtractionBatcher::with_params(
            10, // 10 tokens max
            std::time::Duration::from_secs(3600),
        );

        // 40 chars = ~10 tokens
        let long_text = "a".repeat(40);
        batcher.buffer("t", &long_text);
        assert!(batcher.should_flush());
        assert_eq!(batcher.current_tokens(), 10);
    }
}
