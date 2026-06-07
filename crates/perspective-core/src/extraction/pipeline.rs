use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use crate::config::ExtractionConfig;
use crate::error::{PerspectiveError, Result};

use super::entities::extract_entities;
use super::relations::extract_relations;

/// A single structured fact extracted from text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFact {
    /// The original source text this fact was extracted from.
    pub source_text: String,
    /// A concise statement of the fact.
    pub fact: String,
    /// Confidence in the extraction (0.0–1.0).
    pub confidence: f32,
    /// Extracted entities referenced by this fact.
    pub entities: Vec<String>,
    /// Extracted subject–predicate–object triples.
    pub relations: Vec<super::relations::ExtractedRelation>,
}

/// Request payload for the OpenAI-compatible chat completion API.
#[derive(Debug, Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    max_tokens: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct ChatMessage {
    role: String,
    content: String,
}

/// Expected response from the chat completion API.
#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatResponseMessage,
}

#[derive(Debug, Deserialize)]
struct ChatResponseMessage {
    content: String,
}

/// Main extraction pipeline that calls an LLM to extract structured facts.
pub struct ExtractionPipeline {
    config: ExtractionConfig,
    client: reqwest::Client,
}

impl ExtractionPipeline {
    /// Create a new pipeline from configuration.
    pub fn new(config: ExtractionConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to build HTTP client");
        Self { config, client }
    }

    /// Extract structured facts from a batch of texts via the LLM.
    ///
    /// Each text is sent to the chat completion endpoint and the response is
    /// parsed into [`ExtractedFact`] items.
    pub async fn extract_batch(&self, texts: &[&str]) -> Result<Vec<ExtractedFact>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        let mut facts = Vec::with_capacity(texts.len());

        for text in texts {
            match self.extract_single(text).await {
                Ok(fact) => facts.push(fact),
                Err(e) => {
                    warn!("Extraction failed for text '{}': {}", truncate(text, 80), e);
                    // Still produce a basic fact so downstream can handle it
                    facts.push(ExtractedFact {
                        source_text: text.to_string(),
                        fact: text.to_string(),
                        confidence: 0.0,
                        entities: extract_entities(text)
                            .into_iter()
                            .map(|e| e.name)
                            .collect(),
                        relations: extract_relations(
                            text,
                            &extract_entities(text),
                        ),
                    });
                }
            }
        }

        Ok(facts)
    }

    /// Heuristic importance gate – returns `true` when the text is worth
    /// extracting memories from.
    ///
    /// Skips:
    /// * Very short text (< 5 words)
    /// * Common acknowledgments / filler phrases
    /// * Pure whitespace / punctuation
    pub fn is_memorable(&self, text: &str) -> bool {
        if !self.config.importance_gate {
            return true;
        }

        let trimmed = text.trim();

        // Too short to carry meaningful information
        if trimmed.split_whitespace().count() < 5 {
            debug!("Skipping short text: '{}'", truncate(trimmed, 40));
            return false;
        }

        // Common filler / acknowledgment patterns
        let lower = trimmed.to_lowercase();
        let forgettable = [
            "ok",
            "okay",
            "ok.",
            "okay.",
            "got it",
            "got it.",
            "thanks",
            "thank you",
            "thank you.",
            "sounds good",
            "sounds good.",
            "sure",
            "sure.",
            "alright",
            "alright.",
            "right",
            "right.",
            "cool",
            "cool.",
            "nice",
            "nice.",
            "will do",
            "will do.",
            "understood",
            "understood.",
            "noted",
            "noted.",
            "yes",
            "yes.",
            "no",
            "no.",
            "hmm",
            "hm",
            "ok thanks",
            "ty",
            "thx",
        ];

        if forgettable.iter().any(|f| lower == *f) {
            debug!("Skipping acknowledgement: '{}'", truncate(trimmed, 40));
            return false;
        }

        true
    }

    /// Extract a fact from a single piece of text.
    async fn extract_single(&self, text: &str) -> Result<ExtractedFact> {
        let prompt = format!(
            "Extract a concise factual statement from the following text. \
             Return a JSON object with keys: \"fact\" (string), \"confidence\" (float 0-1).\n\n\
             Text: \"{}\"",
            text.replace('\\', "\\\\").replace('"', "\\\"")
        );

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: prompt,
            }],
            temperature: 0.1,
            max_tokens: 256,
        };

        let endpoint = if self.config.endpoint.ends_with('/') {
            format!("{}chat/completions", self.config.endpoint)
        } else {
            format!("{}/chat/completions", self.config.endpoint)
        };

        let mut builder = self
            .client
            .post(&endpoint)
            .header("Content-Type", "application/json");

        if let Some(ref api_key) = self.config.api_key {
            builder = builder.bearer_auth(api_key);
        }

        let response = builder
            .json(&request)
            .send()
            .await
            .map_err(|e| PerspectiveError::LlmApi(format!("HTTP request failed: {e}")))?;

        let status = response.status();
        if !status.is_success() {
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "<unreadable>".into());
            return Err(PerspectiveError::LlmApi(format!(
                "API returned status {status}: {body}"
            )));
        }

        let chat_response: ChatResponse = response
            .json()
            .await
            .map_err(|e| PerspectiveError::LlmApi(format!("Failed to parse response: {e}")))?;

        let raw_content = chat_response
            .choices
            .first()
            .map(|c| c.message.content.as_str())
            .unwrap_or("");

        // Try to parse the LLM response as JSON; fall back to using raw text
        let (fact_text, confidence) = parse_llm_json(raw_content).unwrap_or_else(|| {
            debug!("LLM response was not valid JSON, using raw content");
            (raw_content.to_string(), 0.5)
        });

        // Also extract entities and relations locally
        let entities = extract_entities(text);
        let relations = extract_relations(text, &entities);

        Ok(ExtractedFact {
            source_text: text.to_string(),
            fact: fact_text,
            confidence,
            entities: entities.into_iter().map(|e| e.name).collect(),
            relations,
        })
    }
}

/// Attempt to parse the LLM output as a JSON object with `fact` and `confidence`.
fn parse_llm_json(raw: &str) -> Option<(String, f32)> {
    // Try direct parse
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) {
        let fact = val.get("fact")?.as_str()?.to_string();
        let confidence = val.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
        return Some((fact, confidence));
    }

    // Try to find a JSON object inside a longer string
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    let slice = &raw[start..=end];
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(slice) {
        let fact = val.get("fact")?.as_str()?.to_string();
        let confidence = val.get("confidence").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32;
        return Some((fact, confidence));
    }

    None
}

/// Truncate a string for display purposes.
fn truncate(s: &str, max_len: usize) -> &str {
    if s.len() <= max_len {
        s
    } else {
        &s[..max_len]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_memorable_short_text() {
        let config = ExtractionConfig {
            enabled: true,
            endpoint: "http://localhost:11434/v1".into(),
            model: "llama3".into(),
            api_key: None,
            batch_size: 10,
            batch_interval_secs: 30,
            importance_gate: true,
        };
        let pipeline = ExtractionPipeline::new(config);
        assert!(!pipeline.is_memorable("hi"));
        assert!(!pipeline.is_memorable("ok"));
        assert!(!pipeline.is_memorable("got it"));
        assert!(!pipeline.is_memorable("Alice mentioned the project"));
    }

    #[test]
    fn test_is_memorable_longer_text() {
        let config = ExtractionConfig {
            enabled: true,
            endpoint: "http://localhost:11434/v1".into(),
            model: "llama3".into(),
            api_key: None,
            batch_size: 10,
            batch_interval_secs: 30,
            importance_gate: true,
        };
        let pipeline = ExtractionPipeline::new(config);
        assert!(pipeline.is_memorable(
            "Alice mentioned that the project deadline has been moved to next Friday"
        ));
    }

    #[test]
    fn test_is_memorable_gate_disabled() {
        let config = ExtractionConfig {
            enabled: true,
            endpoint: "http://localhost:11434/v1".into(),
            model: "llama3".into(),
            api_key: None,
            batch_size: 10,
            batch_interval_secs: 30,
            importance_gate: false,
        };
        let pipeline = ExtractionPipeline::new(config);
        assert!(pipeline.is_memorable("ok"));
    }

    #[test]
    fn test_parse_llm_json_valid() {
        let raw = r#"{"fact": "Alice prefers dark mode", "confidence": 0.9}"#;
        let result = parse_llm_json(raw);
        assert!(result.is_some());
        let (fact, conf) = result.unwrap();
        assert_eq!(fact, "Alice prefers dark mode");
        assert!((conf - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_parse_llm_json_embedded() {
        let raw = "Here is the fact: {\"fact\": \"Bob likes Rust\", \"confidence\": 0.8} done.";
        let result = parse_llm_json(raw);
        assert!(result.is_some());
        let (fact, _) = result.unwrap();
        assert_eq!(fact, "Bob likes Rust");
    }

    #[test]
    fn test_parse_llm_json_invalid() {
        let result = parse_llm_json("just plain text");
        assert!(result.is_none());
    }
}
