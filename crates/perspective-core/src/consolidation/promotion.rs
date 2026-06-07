use uuid::Uuid;

use crate::types::Memory;

/// Find episodic memories that have been accessed more than `threshold` times
/// and are candidates for promotion to semantic memory.
///
/// Only memories of type `Episodic` are considered; other types are ignored.
pub fn find_promotable(memories: &[Memory], threshold: u64) -> Vec<Uuid> {
    memories
        .iter()
        .filter_map(|m| {
            if let Memory::Episodic(ep) = m {
                if ep.access_count > threshold {
                    return Some(ep.base.id);
                }
            }
            None
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EpisodicMemory, MemoryBase, SemanticMemory};
    use chrono::Utc;
    use std::collections::HashMap;

    fn make_episodic(id: Uuid, access_count: u64) -> Memory {
        Memory::Episodic(EpisodicMemory {
            base: MemoryBase {
                id,
                tenant_id: "test".into(),
                content: "test".into(),
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
            stability: 1.0,
            source_session: None,
        })
    }

    fn make_semantic(id: Uuid, access_count: u64) -> Memory {
        Memory::Semantic(SemanticMemory {
            base: MemoryBase {
                id,
                tenant_id: "test".into(),
                content: "test".into(),
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
            stability: 1.0,
            first_seen: Utc::now(),
            last_validated: None,
        })
    }

    #[test]
    fn test_promotable_above_threshold() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let memories = vec![make_episodic(id1, 10), make_episodic(id2, 3)];
        let result = find_promotable(&memories, 5);
        assert_eq!(result.len(), 1);
        assert!(result.contains(&id1));
    }

    #[test]
    fn test_semantic_not_promoted() {
        let id = Uuid::new_v4();
        let memories = vec![make_semantic(id, 100)];
        let result = find_promotable(&memories, 5);
        assert!(result.is_empty());
    }

    #[test]
    fn test_exact_threshold_not_promoted() {
        let id = Uuid::new_v4();
        let memories = vec![make_episodic(id, 5)];
        let result = find_promotable(&memories, 5);
        assert!(result.is_empty());
    }
}
