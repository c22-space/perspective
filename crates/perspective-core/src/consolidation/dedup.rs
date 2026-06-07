use uuid::Uuid;

use crate::types::Memory;

/// Compute cosine similarity between two embedding vectors.
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

/// Extract the embedding from a Memory, if present.
fn memory_embedding(memory: &Memory) -> Option<&Vec<f32>> {
    match memory {
        Memory::Episodic(m) => m.base.embedding.as_ref(),
        Memory::Semantic(m) => m.base.embedding.as_ref(),
        Memory::Procedural(m) => m.base.embedding.as_ref(),
    }
}

/// Find pairs of memories whose embedding cosine similarity exceeds `threshold`.
///
/// Returns a list of `(id_a, id_b)` pairs where `id_a < id_b` (lexicographic
/// by UUID) to avoid duplicates and self-pairs.
pub fn find_duplicates(memories: &[Memory], threshold: f32) -> Vec<(Uuid, Uuid)> {
    let mut duplicates = Vec::new();

    for i in 0..memories.len() {
        let emb_i = match memory_embedding(&memories[i]) {
            Some(e) => e,
            None => continue,
        };
        let id_i = memories[i].id();

        for memory_j in &memories[i + 1..] {
            let emb_j = match memory_embedding(memory_j) {
                Some(e) => e,
                None => continue,
            };
            let id_j = memory_j.id();

            let sim = cosine_similarity(emb_i, emb_j);
            if sim >= threshold {
                // Canonical ordering: smaller UUID first
                if id_i <= id_j {
                    duplicates.push((id_i, id_j));
                } else {
                    duplicates.push((id_j, id_i));
                }
            }
        }
    }

    duplicates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{EpisodicMemory, MemoryBase};
    use chrono::Utc;
    use std::collections::HashMap;

    fn make_memory(id: Uuid, embedding: Option<Vec<f32>>) -> Memory {
        Memory::Episodic(EpisodicMemory {
            base: MemoryBase {
                id,
                tenant_id: "test".into(),
                content: "test content".into(),
                embedding,
                tags: vec![],
                metadata: HashMap::new(),
                created_at: Utc::now(),
                updated_at: Utc::now(),
            },
            timestamp: Utc::now(),
            context: None,
            importance: 0.5,
            access_count: 0,
            last_accessed: Utc::now(),
            stability: 1.0,
            source_session: None,
        })
    }

    #[test]
    fn test_identical_embeddings_detected() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let emb = vec![1.0, 0.0, 0.0];
        let memories = vec![
            make_memory(id1, Some(emb.clone())),
            make_memory(id2, Some(emb)),
        ];
        let dupes = find_duplicates(&memories, 0.95);
        assert_eq!(dupes.len(), 1);
    }

    #[test]
    fn test_no_duplicates_below_threshold() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let memories = vec![
            make_memory(id1, Some(vec![1.0, 0.0])),
            make_memory(id2, Some(vec![0.0, 1.0])),
        ];
        let dupes = find_duplicates(&memories, 0.95);
        assert!(dupes.is_empty());
    }

    #[test]
    fn test_missing_embeddings_skipped() {
        let id1 = Uuid::new_v4();
        let id2 = Uuid::new_v4();
        let memories = vec![
            make_memory(id1, None),
            make_memory(id2, Some(vec![1.0, 0.0])),
        ];
        let dupes = find_duplicates(&memories, 0.5);
        assert!(dupes.is_empty());
    }
}
