use super::entities::ExtractedEntity;

/// A subject–predicate–object triple extracted from text.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractedRelation {
    /// The subject entity.
    pub subject: String,
    /// The predicate / relationship label.
    pub predicate: String,
    /// The object entity.
    pub object: String,
    /// Extraction confidence (0.0–1.0).
    pub confidence: f32,
}

/// Extract relationship triples from `text` given pre-extracted entities.
///
/// This uses a simple dependency-pattern heuristic:
/// * Look for patterns like "<Entity> <verb> <Entity>"
/// * Look for patterns like "<Entity> is a <Entity>"
/// * Look for preposition-mediated links ("works at", "lives in", etc.)
///
/// This is intentionally lightweight – a production system would use a
/// dependency parser or an LLM.
pub fn extract_relations(text: &str, entities: &[ExtractedEntity]) -> Vec<ExtractedRelation> {
    if entities.is_empty() {
        return vec![];
    }

    let mut relations = Vec::new();

    // Build a set of entity names for quick lookup
    let entity_names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();

    // Patterns: preposition-mediated relationships
    let relationship_patterns: &[(&str, &str)] = &[
        ("works at", "works_at"),
        ("works for", "works_for"),
        ("lives in", "lives_in"),
        ("located in", "located_in"),
        ("based in", "based_in"),
        ("founded by", "founded_by"),
        ("owns", "owns"),
        ("manages", "manages"),
        ("leads", "leads"),
        ("created", "created"),
        ("built", "built"),
        ("uses", "uses"),
        ("prefers", "prefers"),
        ("likes", "likes"),
        ("dislikes", "dislikes"),
        ("mentioned", "mentioned"),
        ("discussed", "discussed"),
        ("plans to", "plans_to"),
        ("wants to", "wants_to"),
        ("needs", "needs"),
        ("sent to", "sent_to"),
        ("received from", "received_from"),
        ("part of", "part_of"),
        ("member of", "member_of"),
    ];

    // Pattern 1: Find pairs of entities connected by known relationship phrases
    for &(pattern, predicate) in relationship_patterns {
        if let Some(idx) = text.to_lowercase().find(pattern) {
            // Look for entity before the pattern
            let before = &text[..idx].trim();
            let after = &text[idx + pattern.len()..].trim();

            let subject = find_nearest_entity(before, &entity_names);
            let object = find_nearest_entity(after, &entity_names);

            if let (Some(subj), Some(obj)) = (subject, object) {
                relations.push(ExtractedRelation {
                    subject: subj.to_string(),
                    predicate: predicate.to_string(),
                    object: obj.to_string(),
                    confidence: 0.7,
                });
            }
        }
    }

    // Pattern 2: "X is a Y" / "X is the Y of Z"
    extract_is_a_patterns(text, &entity_names, &mut relations);

    // Pattern 3: Co-occurrence in the same sentence (weak relationship)
    extract_cooccurrence_relations(text, entities, &mut relations);

    deduplicate_relations(relations)
}

/// Find the entity name that appears closest to the end of `text`.
fn find_nearest_entity<'a>(text: &str, entity_names: &[&'a str]) -> Option<&'a str> {
    let lower = text.to_lowercase();
    let mut best: Option<(&'a str, usize)> = None;

    for &name in entity_names {
        if let Some(pos) = lower.find(&name.to_lowercase()) {
            if best.is_none() || pos > best.unwrap().1 {
                best = Some((name, pos));
            }
        }
    }

    best.map(|(name, _)| name)
}

/// Extract "X is a Y" style relationships.
fn extract_is_a_patterns(
    text: &str,
    entity_names: &[&str],
    relations: &mut Vec<ExtractedRelation>,
) {
    let lower = text.to_lowercase();

    // "X is a <concept>"
    let is_a_patterns = ["is a", "is an", "is the", "are a", "are an", "are the"];

    for pattern in &is_a_patterns {
        if let Some(pos) = lower.find(pattern) {
            let before = text[..pos].trim();
            let after = text[pos + pattern.len()..].trim();

            if let Some(subject) = find_nearest_entity(before, entity_names) {
                // The object could be a concept word after "is a"
                let object_text = extract_concept_after_pattern(after);
                if !object_text.is_empty() {
                    relations.push(ExtractedRelation {
                        subject: subject.to_string(),
                        predicate: "is_a".to_string(),
                        object: object_text,
                        confidence: 0.6,
                    });
                }
            }
        }
    }
}

/// Extract the first meaningful noun phrase after a pattern.
fn extract_concept_after_pattern(text: &str) -> String {
    // Take up to the next punctuation or end of sentence
    let end = text
        .find(|c: char| c == '.' || c == ',' || c == '!' || c == '?')
        .unwrap_or(text.len());
    let phrase = text[..end].trim();

    // Take first 1-3 words as the concept
    let words: Vec<&str> = phrase.split_whitespace().take(3).collect();
    words.join(" ")
}

/// Extract weak co-occurrence relations when two entities appear in the same sentence.
fn extract_cooccurrence_relations(
    text: &str,
    entities: &[ExtractedEntity],
    relations: &mut Vec<ExtractedRelation>,
) {
    let sentences: Vec<&str> = text
        .split(|c: char| c == '.' || c == '!' || c == '?')
        .collect();

    for sentence in &sentences {
        let lower = sentence.to_lowercase();
        let mut sentence_entities: Vec<&ExtractedEntity> = Vec::new();

        for entity in entities {
            if lower.contains(&entity.name.to_lowercase()) {
                sentence_entities.push(entity);
            }
        }

        // Generate pairwise co-occurrence relations
        for i in 0..sentence_entities.len() {
            for j in (i + 1)..sentence_entities.len() {
                let a = sentence_entities[i];
                let b = sentence_entities[j];
                relations.push(ExtractedRelation {
                    subject: a.name.clone(),
                    predicate: "co_occurs_with".to_string(),
                    object: b.name.clone(),
                    confidence: 0.4,
                });
            }
        }
    }
}

/// Remove duplicate relations.
fn deduplicate_relations(mut relations: Vec<ExtractedRelation>) -> Vec<ExtractedRelation> {
    let mut seen = std::collections::HashSet::new();
    relations.retain(|r| {
        let key = (
            r.subject.to_lowercase(),
            r.predicate.to_lowercase(),
            r.object.to_lowercase(),
        );
        seen.insert(key)
    });
    relations
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EntityType;

    fn make_entity(name: &str) -> ExtractedEntity {
        ExtractedEntity {
            name: name.to_string(),
            entity_type: EntityType::Person,
            confidence: 0.8,
        }
    }

    #[test]
    fn test_cooccurrence() {
        let entities = vec![make_entity("Alice"), make_entity("Bob")];
        let relations = extract_relations("Alice and Bob went to the store.", &entities);
        assert!(!relations.is_empty());
        let has_co = relations.iter().any(|r| r.predicate == "co_occurs_with");
        assert!(has_co, "expected co_occurs_with relation");
    }

    #[test]
    fn test_preposition_pattern() {
        let entities = vec![make_entity("Alice"), make_entity("Acme Inc")];
        let relations = extract_relations("Alice works at Acme Inc.", &entities);
        let has_works = relations
            .iter()
            .any(|r| r.predicate == "works_at");
        assert!(has_works, "expected works_at relation in {relations:?}");
    }

    #[test]
    fn test_is_a_pattern() {
        let entities = vec![make_entity("Rust")];
        let relations = extract_relations("Rust is a systems programming language.", &entities);
        let has_is_a = relations.iter().any(|r| r.predicate == "is_a");
        assert!(has_is_a, "expected is_a relation in {relations:?}");
    }

    #[test]
    fn test_empty_entities() {
        let relations = extract_relations("Some text.", &[]);
        assert!(relations.is_empty());
    }
}
