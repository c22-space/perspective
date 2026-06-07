use chrono::{DateTime, Utc};

/// Exponential time-decay score based on how long since `last_accessed`.
///
/// Returns `e^{-lambda * hours_elapsed}` where hours_elapsed is the
/// wall-clock hours between `last_accessed` and *now*.
pub fn score_recency(last_accessed: DateTime<Utc>, lambda: f32) -> f32 {
    let now = Utc::now();
    let elapsed_secs = (now - last_accessed).num_seconds().max(0) as f32;
    let elapsed_hours = elapsed_secs / 3600.0;
    (-lambda * elapsed_hours).exp()
}

/// Importance score that grows sub-logarithmically with access count.
///
/// Formula: `base * min(1.0, 0.5 + 0.1 * ln(access_count + 1))`
pub fn score_importance(base: f32, access_count: u64) -> f32 {
    let growth = 0.5 + 0.1 * ((access_count as f32) + 1.0).ln();
    base * growth.min(1.0)
}

/// Relevance score combining a vector similarity score with optional
/// graph-hopping proximity.
///
/// `max(vector_score, 1.0 / (1.0 + hops))` – when no graph hops are
/// provided the vector score is returned directly.
pub fn score_relevance(vector_score: f32, graph_hops: Option<u32>) -> f32 {
    match graph_hops {
        Some(hops) => vector_score.max(1.0 / (1.0 + hops as f32)),
        None => vector_score,
    }
}

/// Multiplicative combination of the three sub-scores into a single
/// final retrieval score.
pub fn final_score(recency: f32, importance: f32, relevance: f32) -> f32 {
    recency * importance * relevance
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn recency_fresh_is_high() {
        let now = Utc::now();
        let s = score_recency(now, 0.1);
        assert!(s > 0.99, "fresh access should yield ~1.0, got {s}");
    }

    #[test]
    fn recency_old_is_low() {
        let old = Utc::now() - Duration::hours(100);
        let s = score_recency(old, 0.1);
        assert!(s < 0.1, "old access should yield <0.1, got {s}");
    }

    #[test]
    fn importance_zero_access() {
        let s = score_importance(1.0, 0);
        // 0.5 + 0.1 * ln(1) = 0.5
        assert!((s - 0.5).abs() < 1e-5);
    }

    #[test]
    fn importance_caps_at_base() {
        let s = score_importance(1.0, 10_000);
        assert!(s <= 1.0 + 1e-5);
    }

    #[test]
    fn relevance_with_no_hops() {
        assert_eq!(score_relevance(0.8, None), 0.8);
    }

    #[test]
    fn relevance_with_hops() {
        // hops=0 -> 1.0/(1.0+0) = 1.0, max(0.3, 1.0) = 1.0
        assert!((score_relevance(0.3, Some(0)) - 1.0).abs() < 1e-5);
        // hops=4 -> 1.0/5.0 = 0.2, max(0.8, 0.2) = 0.8
        assert!((score_relevance(0.8, Some(4)) - 0.8).abs() < 1e-5);
    }

    #[test]
    fn final_score_multiplies() {
        let fs = final_score(0.5, 0.6, 0.7);
        assert!((fs - 0.21).abs() < 1e-5);
    }
}
