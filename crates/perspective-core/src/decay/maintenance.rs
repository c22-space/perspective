use chrono::Utc;

use crate::config::DecayConfig;
use crate::decay::ebbinghaus;
use crate::types::Memory;

/// Compute the current recall strength for a single memory.
///
/// Uses the Ebbinghaus forgetting curve with the time elapsed since the
/// memory was last accessed and its current stability.
pub fn memory_strength(memory: &Memory) -> f32 {
    let stability = memory.stability();
    let elapsed = Utc::now()
        .signed_duration_since(memory.last_accessed())
        .num_seconds()
        .max(0) as f64;
    ebbinghaus::calculate_strength(stability, elapsed)
}

/// Apply decay to a batch of memories, returning each memory alongside its
/// freshly computed strength.
///
/// This is the primary entry point for periodic background maintenance.
/// Memories are returned in the same order as the input.
pub fn apply_decay_to_memories(memories: &[Memory], _config: &DecayConfig) -> Vec<(Memory, f32)> {
    memories
        .iter()
        .map(|m| (m.clone(), memory_strength(m)))
        .collect()
}

/// Identify garbage-collection candidates: memories whose recall strength
/// has fallen below the configured `gc_threshold`.
///
/// These memories are candidates for removal from the system.
pub fn get_gc_candidates<'a>(memories: &'a [Memory], config: &DecayConfig) -> Vec<&'a Memory> {
    let now = Utc::now();
    memories
        .iter()
        .filter(|m| {
            let stability = m.stability();
            let elapsed = now
                .signed_duration_since(m.last_accessed())
                .num_seconds()
                .max(0) as f64;
            let strength = ebbinghaus::calculate_strength(stability, elapsed);
            strength < config.gc_threshold
        })
        .collect()
}

/// Identify retrieval candidates: memories whose recall strength is at or
/// above the configured `retrieval_threshold`.
///
/// These memories are strong enough to be considered for recall results.
pub fn get_retrieval_candidates<'a>(
    memories: &'a [Memory],
    config: &DecayConfig,
) -> Vec<&'a Memory> {
    let now = Utc::now();
    memories
        .iter()
        .filter(|m| {
            let stability = m.stability();
            let elapsed = now
                .signed_duration_since(m.last_accessed())
                .num_seconds()
                .max(0) as f64;
            let strength = ebbinghaus::calculate_strength(stability, elapsed);
            strength >= config.retrieval_threshold
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EpisodicMemory, MemoryBase, SemanticMemory};
    use chrono::Utc;
    use std::collections::HashMap;
    use uuid::Uuid;

    fn make_episodic(stability: f32, access_count: u64) -> Memory {
        Memory::Episodic(EpisodicMemory {
            base: MemoryBase {
                id: Uuid::new_v4(),
                tenant_id: "test".into(),
                content: "test content".into(),
                embedding: None,
                tags: vec![],
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            timestamp: Utc::now(),
            context: None,
            importance: 0.5,
            access_count,
            last_accessed: Utc::now(),
            stability,
            source_session: None,
        })
    }

    fn make_semantic(stability: f32, access_count: u64) -> Memory {
        Memory::Semantic(SemanticMemory {
            base: MemoryBase {
                id: Uuid::new_v4(),
                tenant_id: "test".into(),
                content: "test fact".into(),
                embedding: None,
                tags: vec![],
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            confidence: 0.8,
            source_ids: vec![],
            access_count,
            last_accessed: Utc::now(),
            stability,
            first_seen: Utc::now(),
            last_validated: None,
        })
    }

    fn default_config() -> DecayConfig {
        DecayConfig {
            enabled: true,
            episodic_lambda: 0.1,
            semantic_lambda: 0.01,
            procedural_lambda: 0.0,
            learning_rate: 0.1,
            retrieval_threshold: 0.1,
            gc_threshold: 0.01,
        }
    }

    #[test]
    fn apply_decay_returns_strength_for_each_memory() {
        let memories = vec![
            make_episodic(1.0, 0),
            make_semantic(10.0, 0),
        ];
        let config = default_config();
        let results = apply_decay_to_memories(&memories, &config);
        assert_eq!(results.len(), 2);
        // Recently created memories (elapsed ≈ 0) should have strength near 1.0
        for (_, strength) in &results {
            assert!(*strength > 0.99, "expected near 1.0, got {strength}");
        }
    }

    #[test]
    fn no_gc_candidates_when_strength_is_high() {
        let memories = vec![make_episodic(1.0, 0)];
        let config = default_config();
        let candidates = get_gc_candidates(&memories, &config);
        // Recently created memory with high stability should NOT be a GC candidate
        assert!(
            candidates.is_empty(),
            "fresh memory should not be a GC candidate"
        );
    }

    #[test]
    fn retrieval_candidates_when_strength_is_high() {
        let memories = vec![make_episodic(1.0, 0), make_semantic(10.0, 0)];
        let config = default_config();
        let candidates = get_retrieval_candidates(&memories, &config);
        // Both freshly created memories should be retrieval candidates
        assert_eq!(candidates.len(), 2);
    }

    #[test]
    fn very_low_stability_is_gc_candidate() {
        // Stability of 0.001 with even a second elapsed → strength near 0
        let mut mem = make_episodic(0.001, 0);
        // Set last_accessed to the past
        if let Memory::Episodic(ref mut e) = mem {
            e.last_accessed = Utc::now() - chrono::Duration::seconds(100);
        }
        let config = default_config();
        let memories = vec![mem];
        let candidates = get_gc_candidates(&memories, &config);
        assert_eq!(
            candidates.len(),
            1,
            "very old low-stability memory should be GC candidate"
        );
    }
}
