use crate::types::EntityType;

/// A named entity extracted from text.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ExtractedEntity {
    /// The surface form of the entity as it appears in the text.
    pub name: String,
    /// Classified entity type.
    pub entity_type: EntityType,
    /// Extraction confidence (0.0–1.0).
    pub confidence: f32,
}

/// Extract named entities from `text` using a capitalisation heuristic.
///
/// The heuristic looks for:
/// * Capitalised words that are not at the start of a sentence (heuristic: words
///   after the first word of a sentence that begin with an uppercase letter).
/// * Two-or-more-word runs where each word is capitalised (likely proper nouns).
///
/// This is intentionally simple – a production system would use an NER model.
pub fn extract_entities(text: &str) -> Vec<ExtractedEntity> {
    let mut entities = Vec::new();

    // Split into sentences first
    let sentences = split_sentences(text);

    for sentence in &sentences {
        let words: Vec<&str> = sentence.split_whitespace().collect();
        if words.is_empty() {
            continue;
        }

        // Track runs of capitalised words (multi-word proper nouns)
        let mut current_run: Vec<&str> = Vec::new();
        for (i, word) in words.iter().enumerate() {
            let cleaned = word.trim_matches(|c: char| c.is_ascii_punctuation());
            if cleaned.is_empty() {
                // Flush any current run
                if !current_run.is_empty() {
                    let name = current_run.join(" ");
                    let entity_type = classify_entity(&name);
                    entities.push(ExtractedEntity {
                        name,
                        entity_type,
                        confidence: entity_confidence(current_run.len()),
                    });
                    current_run.clear();
                }
                continue;
            }

            let is_capitalised = cleaned
                .chars()
                .next()
                .map(|c| c.is_ascii_uppercase())
                .unwrap_or(false);

            let is_all_caps = cleaned.len() > 1
                && cleaned
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || !c.is_alphabetic());

            if is_capitalised || is_all_caps {
                // Skip common sentence-initial words that are capitalised but not entities
                if i == 0
                    && current_run.is_empty()
                    && is_sentence_initial_word(&lower_cleaned(cleaned))
                {
                    continue;
                }
                current_run.push(cleaned);
            } else {
                // End of a capitalised run
                if !current_run.is_empty() {
                    let name = current_run.join(" ");
                    let entity_type = classify_entity(&name);
                    entities.push(ExtractedEntity {
                        name,
                        entity_type,
                        confidence: entity_confidence(current_run.len()),
                    });
                    current_run.clear();
                }
            }

            // Prevent the run from growing too large (likely false positive)
            if current_run.len() > 6 {
                let name = current_run.join(" ");
                let entity_type = classify_entity(&name);
                entities.push(ExtractedEntity {
                    name,
                    entity_type,
                    confidence: entity_confidence(current_run.len()),
                });
                current_run.clear();
            }

            // Also capture single capitalised words that look like names
            if current_run.is_empty() && is_capitalised && i > 0 {
                // single word that got flushed already or standalone
            }
        }

        // Flush any remaining run
        if !current_run.is_empty() {
            let name = current_run.join(" ");
            let entity_type = classify_entity(&name);
            entities.push(ExtractedEntity {
                name,
                entity_type,
                confidence: entity_confidence(current_run.len()),
            });
        }
    }

    // Also extract patterns in quotes and after common markers
    entities.extend(extract_quoted_entities(text));
    entities.extend(extract_at_mentions(text));

    deduplicate_entities(entities)
}

/// Split text into rough sentences.
fn split_sentences(text: &str) -> Vec<&str> {
    let mut sentences = Vec::new();
    let mut start = 0;

    for (i, ch) in text.char_indices() {
        if ch == '.' || ch == '!' || ch == '?' {
            let end = i + ch.len_utf8();
            if end > start {
                sentences.push(&text[start..end]);
            }
            start = end;
        }
    }

    // Capture the remainder
    if start < text.len() {
        sentences.push(&text[start..]);
    }

    if sentences.is_empty() {
        sentences.push(text);
    }

    sentences
}

/// Extract entities that appear inside quotation marks.
fn extract_quoted_entities(text: &str) -> Vec<ExtractedEntity> {
    let mut entities = Vec::new();
    let mut chars = text.char_indices().peekable();

    while let Some((i, ch)) = chars.next() {
        if ch == '"' || ch == '\'' || ch == '\u{201c}' || ch == '\u{2018}' {
            let quote_start = i + ch.len_utf8();
            let close_char = match ch {
                '"' => '"',
                '\'' => '\'',
                '\u{201c}' => '\u{201d}',
                '\u{2018}' => '\u{2019}',
                _ => '"',
            };

            // Find closing quote
            while let Some(&(j, c)) = chars.peek() {
                if c == close_char {
                    let _ = chars.next();
                    let content = &text[quote_start..j];
                    let trimmed = content.trim();
                    if trimmed.split_whitespace().count() >= 1
                        && trimmed.split_whitespace().count() <= 5
                        && trimmed
                            .chars()
                            .next()
                            .map(|c| c.is_ascii_uppercase())
                            .unwrap_or(false)
                    {
                        let entity_type = classify_entity(trimmed);
                        entities.push(ExtractedEntity {
                            name: trimmed.to_string(),
                            entity_type,
                            confidence: 0.6, // lower confidence for quoted text
                        });
                    }
                    break;
                } else {
                    let _ = chars.next();
                }
            }
        }
    }

    entities
}

/// Extract entities from @mentions.
fn extract_at_mentions(text: &str) -> Vec<ExtractedEntity> {
    let mut entities = Vec::new();
    for word in text.split_whitespace() {
        // Check for @ prefix first, before stripping punctuation
        let stripped = word.trim_end_matches(|c: char| c.is_ascii_punctuation());
        if let Some(name) = stripped.strip_prefix('@') {
            if !name.is_empty() {
                entities.push(ExtractedEntity {
                    name: name.to_string(),
                    entity_type: EntityType::Person,
                    confidence: 0.7,
                });
            }
        }
    }
    entities
}

/// Simple heuristic classification of an entity name to an EntityType.
fn classify_entity(name: &str) -> EntityType {
    let lower = name.to_lowercase();

    // Common organisation suffixes
    let org_suffixes = [
        "inc",
        "inc.",
        "llc",
        "ltd",
        "ltd.",
        "corp",
        "corp.",
        "co",
        "co.",
        "company",
        "foundation",
        "institute",
        "association",
        "university",
        "laboratory",
        "lab",
        "studio",
    ];
    for suffix in &org_suffixes {
        if lower.ends_with(suffix) {
            return EntityType::Organization;
        }
    }

    // Common tool / project names
    let tool_keywords = [
        "rust",
        "python",
        "javascript",
        "typescript",
        "docker",
        "kubernetes",
        "postgres",
        "redis",
        "qdrant",
        "tantivy",
        "redb",
        "tauri",
        "react",
        "vue",
        "angular",
        "node",
        "cargo",
        "git",
        "linux",
        "windows",
        "macos",
        "openai",
        "anthropic",
        "claude",
        "gpt",
        "llama",
    ];
    for kw in &tool_keywords {
        if lower.contains(kw) {
            return EntityType::Tool;
        }
    }

    // Location hints
    let location_hints = [
        "street",
        "road",
        "avenue",
        "boulevard",
        "city",
        "state",
        "country",
    ];
    for hint in &location_hints {
        if lower.contains(hint) {
            return EntityType::Location;
        }
    }

    // Default to Person for single/multi-word capitalised names
    EntityType::Person
}

/// Confidence heuristic: longer runs are more likely to be real entities.
fn entity_confidence(run_length: usize) -> f32 {
    match run_length {
        1 => 0.55,
        2 => 0.75,
        3 => 0.80,
        _ => 0.85,
    }
}

/// Remove duplicate entities (by name), keeping the first occurrence.
fn deduplicate_entities(mut entities: Vec<ExtractedEntity>) -> Vec<ExtractedEntity> {
    let mut seen = std::collections::HashSet::new();
    entities.retain(|e| {
        let key = e.name.to_lowercase();
        seen.insert(key)
    });
    entities
}

/// Check if a word is a common English sentence-initial word (not an entity).
fn is_sentence_initial_word(word: &str) -> bool {
    matches!(
        word,
        "the"
            | "a"
            | "an"
            | "i"
            | "in"
            | "on"
            | "at"
            | "for"
            | "it"
            | "he"
            | "she"
            | "we"
            | "they"
            | "my"
            | "his"
            | "her"
            | "our"
            | "this"
            | "that"
            | "these"
            | "those"
            | "if"
            | "when"
            | "as"
            | "but"
            | "and"
            | "or"
            | "so"
            | "yet"
            | "then"
            | "there"
            | "here"
            | "what"
            | "which"
            | "who"
            | "how"
            | "can"
            | "could"
            | "would"
            | "should"
            | "will"
            | "may"
            | "might"
            | "shall"
            | "must"
            | "do"
            | "does"
            | "did"
            | "is"
            | "are"
            | "was"
            | "were"
            | "has"
            | "have"
            | "had"
            | "not"
            | "no"
            | "all"
            | "each"
            | "every"
            | "both"
            | "few"
            | "more"
            | "most"
            | "other"
            | "some"
            | "such"
            | "than"
            | "too"
            | "very"
            | "just"
            | "because"
            | "since"
            | "while"
            | "before"
            | "after"
            | "above"
            | "below"
            | "between"
            | "during"
            | "without"
            | "about"
            | "into"
            | "through"
            | "to"
            | "from"
            | "by"
            | "with"
            | "of"
            | "over"
            | "under"
            | "down"
            | "up"
            | "out"
            | "off"
            | "also"
            | "however"
            | "only"
            | "well"
            | "now"
            | "still"
            | "much"
            | "many"
            | "any"
    )
}

/// Lowercase a cleaned word helper.
fn lower_cleaned(word: &str) -> String {
    word.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_single_capitalised() {
        let entities = extract_entities("Alice went to the store.");
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"Alice"), "expected Alice in {names:?}");
    }

    #[test]
    fn test_extract_multi_word_entity() {
        let entities = extract_entities("I visited New York last summer.");
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"New York"),
            "expected New York in {names:?}"
        );
    }

    #[test]
    fn test_extract_quoted() {
        let entities = extract_entities("She mentioned \"Project Alpha\" in the meeting.");
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(
            names.contains(&"Project Alpha"),
            "expected Project Alpha in {names:?}"
        );
    }

    #[test]
    fn test_extract_at_mention() {
        let entities = extract_entities("I talked to @alice about the design.");
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"alice"), "expected alice in {names:?}");
    }

    #[test]
    fn test_classify_entity() {
        assert_eq!(classify_entity("Acme Inc"), EntityType::Organization);
        assert_eq!(classify_entity("Docker"), EntityType::Tool);
        assert_eq!(classify_entity("Alice"), EntityType::Person);
    }

    #[test]
    fn test_deduplication() {
        let entities = extract_entities("Alice and Alice went to New York and New York.");
        let mut names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        names.sort();
        names.dedup();
        // Each unique name should appear only once
        assert_eq!(names.len(), entities.len());
    }
}
