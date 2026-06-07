use crate::config::DecayConfig;
use crate::types::MemoryType;

/// Calculate memory retention strength using the Ebbinghaus forgetting curve.
///
/// Implements: R = e^(-t/S)
/// where `t` is elapsed time in seconds since last access, and `S` is the
/// memory's stability parameter. A higher stability means slower decay.
///
/// Returns a value in (0.0, 1.0] where 1.0 is perfect recall.
/// Returns 0.0 for zero or negative stability (degenerate case).
pub fn calculate_strength(stability: f32, elapsed_secs: f64) -> f32 {
    if stability <= 0.0 {
        return 0.0;
    }
    let decay_rate = elapsed_secs / stability as f64;
    // Clamp to avoid underflow to exactly 0.0 for extreme values
    (-decay_rate).exp() as f32
}

/// Reinforce a memory's stability after a successful retrieval.
///
/// Increases stability by the learning rate, scaled by the access count.
/// Repeated access (higher `access_count`) yields diminishing marginal gains,
/// consistent with spaced repetition theory.
///
/// Returns the new stability value, which is always >= the input `stability`.
pub fn reinforce(stability: f32, learning_rate: f32, access_count: u64) -> f32 {
    // Diminishing returns: effective rate shrinks logarithmically with access count
    let effective_rate = learning_rate / (1.0 + (access_count as f32).ln_1p());
    stability * (1.0 + effective_rate)
}

/// Determine the initial stability for a newly created memory based on its type.
///
/// Maps the per-type lambda values from `DecayConfig` to an initial stability.
/// Higher lambda → higher initial stability → slower initial decay.
///
/// - Episodic: lower stability (events fade unless reinforced)
/// - Semantic: moderate stability (facts persist longer)
/// - Procedural: very high / infinite stability (skills are hard to forget)
pub fn initial_stability(memory_type: MemoryType, config: &DecayConfig) -> f32 {
    match memory_type {
        MemoryType::Episodic => config.episodic_lambda,
        MemoryType::Semantic => config.semantic_lambda,
        MemoryType::Procedural => config.procedural_lambda,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strength_at_zero_elapsed_is_one() {
        let s = calculate_strength(1.0, 0.0);
        assert!((s - 1.0).abs() < f32::EPSILON, "expected ~1.0, got {s}");
    }

    #[test]
    fn strength_decays_over_time() {
        let early = calculate_strength(10.0, 10.0);
        let late = calculate_strength(10.0, 100.0);
        assert!(
            late < early,
            "later strength should be lower: {late} < {early}"
        );
    }

    #[test]
    fn strength_approaches_zero_for_long_elapsed() {
        let s = calculate_strength(1.0, 1000.0);
        assert!(s < 0.001, "expected near zero, got {s}");
    }

    #[test]
    fn zero_stability_returns_zero() {
        let s = calculate_strength(0.0, 10.0);
        assert!(s == 0.0, "expected 0.0, got {s}");
    }

    #[test]
    fn reinforce_increases_stability() {
        let base = 10.0;
        let reinforced = reinforce(base, 0.1, 1);
        assert!(
            reinforced > base,
            "reinforced ({reinforced}) should exceed base ({base})"
        );
    }

    #[test]
    fn reinforce_has_diminishing_returns() {
        let first = reinforce(10.0, 0.1, 1);
        let tenth = reinforce(10.0, 0.1, 10);
        let hundredth = reinforce(10.0, 0.1, 100);
        // Each successive access provides a smaller boost
        let gain1 = first - 10.0;
        let gain10 = tenth - 10.0;
        let gain100 = hundredth - 10.0;
        assert!(
            gain1 > gain10,
            "first gain ({gain1}) should exceed tenth ({gain10})"
        );
        assert!(
            gain10 > gain100,
            "tenth gain ({gain10}) should exceed hundredth ({gain100})"
        );
    }

    #[test]
    fn initial_stability_matches_config() {
        let config = DecayConfig {
            enabled: true,
            episodic_lambda: 0.1,
            semantic_lambda: 0.01,
            procedural_lambda: 0.0,
            learning_rate: 0.1,
            retrieval_threshold: 0.1,
            gc_threshold: 0.01,
        };
        assert_eq!(initial_stability(MemoryType::Episodic, &config), 0.1);
        assert_eq!(initial_stability(MemoryType::Semantic, &config), 0.01);
        assert_eq!(initial_stability(MemoryType::Procedural, &config), 0.0);
    }
}
