use crate::config::ExtractionConfig;
use crate::error::{PerspectiveError, Result};
use crate::llm::BundledLlm;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use super::entities::extract_entities;
use super::relations::extract_relations;
/// A single structured fact extracted from text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedFact {
    /// The original source text this fact was extracted from.
    pub source_text: String,
    /// A concise statement of the fact.
    pub fact: String,
    /// Confidence in the extraction (0.0-1.0).
    pub confidence: f32,
    /// Extracted entities referenced by this fact.
    pub entities: Vec<String>,
    /// Extracted subject-predicate-object triples.
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
///
/// Supports two modes:
/// - **Bundled model**: Loads a local GGUF on demand, runs inference, unloads.
/// - **External endpoint**: Uses an OpenAI-compatible HTTP API (fallback).
pub struct ExtractionPipeline {
    config: ExtractionConfig,
    /// HTTP client for external endpoint mode.
    client: reqwest::Client,
}

impl ExtractionPipeline {
    /// Create a new pipeline from configuration.
    ///
    /// If `config.endpoint` is empty, uses bundled model mode (loaded on demand).
    /// If `config.endpoint` is set, uses HTTP mode (external LLM server).
    pub fn new(config: ExtractionConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .expect("Failed to build HTTP client");

        if config.endpoint.is_empty() {
            tracing::info!(
                "Extraction pipeline: bundled model mode (model_path={})",
                config.model_path
            );
        } else {
            tracing::info!(
                "Extraction pipeline: external endpoint mode ({})",
                config.endpoint
            );
        }

        Self { config, client }
    }

    /// Returns true if the bundled model is available (exists on disk).
    pub fn has_bundled_model(&self) -> bool {
        if !self.config.endpoint.is_empty() {
            return false;
        }
        let path = self.resolved_model_path();
        path.exists()
    }

    /// Resolve the model path relative to CWD.
    fn resolved_model_path(&self) -> std::path::PathBuf {
        let p = std::path::Path::new(&self.config.model_path);
        if p.is_absolute() {
            p.to_path_buf()
        } else {
            std::env::current_dir()
                .unwrap_or_default()
                .join(p)
        }
    }

    /// Extract structured facts from a batch of texts via the LLM.
    ///
    /// Loads the bundled model (if available), processes all texts, then unloads.
    /// For external endpoints, uses HTTP without model lifecycle management.
    pub async fn extract_batch(&self, texts: &[&str]) -> Result<Vec<ExtractedFact>> {
        if texts.is_empty() {
            return Ok(vec![]);
        }

        // For bundled mode: load model, process batch, unload
        if self.config.endpoint.is_empty() {
            return self.extract_batch_bundled(texts).await;
        }

        // For external endpoint: process via HTTP
        let mut facts = Vec::with_capacity(texts.len());
        for text in texts {
            match self.extract_single_http(text).await {
                Ok(fact) => facts.push(fact),
                Err(e) => {
                    warn!("Extraction failed for text '{}': {}", truncate(text, 80), e);
                    facts.push(fallback_fact(text));
                }
            }
        }
        Ok(facts)
    }

    /// Extract using the bundled model: load, process, unload.
    async fn extract_batch_bundled(&self, texts: &[&str]) -> Result<Vec<ExtractedFact>> {
        let model_path = self.resolved_model_path();
        if !model_path.exists() {
            warn!(
                "Bundled model not found at {}. Falling back to local extraction.",
                model_path.display()
            );
            return Ok(texts.iter().map(|t| fallback_fact(t)).collect());
        }

        // Load model
        let llm = BundledLlm::load(&model_path, self.config.max_tokens, self.config.n_ctx)
            .map_err(|e| {
                PerspectiveError::LlmApi(format!("Failed to load bundled model: {e}"))
            })?;

        // Process all texts
        let mut facts = Vec::with_capacity(texts.len());
        for text in texts {
            // NuExtract uses a specific prompt format: <|input|> with ### Template: and ### Text:
            // The model is trained to fill in empty string fields in the JSON template.
            let template = serde_json::json!({"fact": "", "confidence": ""}).to_string();
            let prompt = format!(
                "<|input|>\n### Template:\n{}\n### Text:\n{}\n\n<|output|>",
                template, text
            );

            match llm.complete(&prompt) {
                Ok(raw_content) => {
                    let (fact_text, confidence) =
                        parse_llm_json(&raw_content).unwrap_or_else(|| {
                            debug!("LLM response was not valid JSON, using raw content");
                            (raw_content.to_string(), 0.5)
                        });

                    let entities = extract_entities(text);
                    let relations = extract_relations(text, &entities);

                    facts.push(ExtractedFact {
                        source_text: text.to_string(),
                        fact: fact_text,
                        confidence,
                        entities: entities.into_iter().map(|e| e.name).collect(),
                        relations,
                    });
                }
                Err(e) => {
                    warn!("Extraction failed for text '{}': {}", truncate(text, 80), e);
                    facts.push(fallback_fact(text));
                }
            }
        }

        // Model is dropped here (unloaded from memory)
        drop(llm);

        Ok(facts)
    }

    /// Heuristic importance gate - returns `true` when the text is worth
    /// extracting memories from.
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

    /// Extract via external HTTP endpoint (OpenAI-compatible).
    async fn extract_single_http(&self, text: &str) -> Result<ExtractedFact> {
        let template = serde_json::json!({"fact": "", "confidence": ""}).to_string();
        let prompt = format!(
            "<|input|>\n### Template:\n{}\n### Text:\n{}\n\n<|output|>",
            template, text
        );

        let request = ChatRequest {
            model: self.config.model.clone(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: prompt,
            }],
            temperature: 0.1,
            max_tokens: self.config.max_tokens,
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

        let (fact_text, confidence) = parse_llm_json(raw_content).unwrap_or_else(|| {
            debug!("LLM response was not valid JSON, using raw content");
            (raw_content.to_string(), 0.5)
        });

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

/// Create a fallback fact when LLM extraction fails.
fn fallback_fact(text: &str) -> ExtractedFact {
    let entities = extract_entities(text);
    let relations = extract_relations(text, &entities);
    ExtractedFact {
        source_text: text.to_string(),
        fact: text.to_string(),
        confidence: 0.0,
        entities: entities.into_iter().map(|e| e.name).collect(),
        relations,
    }
}

/// Attempt to parse the LLM output as a JSON object with `fact` and `confidence`.
fn parse_llm_json(raw: &str) -> Option<(String, f32)> {
    // Try direct parse
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) {
        let fact = val.get("fact")?.as_str()?.to_string();
        let confidence = parse_confidence(val.get("confidence"));
        return Some((fact, confidence));
    }

    // Try to find a JSON object inside a longer string
    let start = raw.find('{')?;
    let end = raw.rfind('}')?;
    let slice = &raw[start..=end];
    if let Ok(val) = serde_json::from_str::<serde_json::Value>(slice) {
        let fact = val.get("fact")?.as_str()?.to_string();
        let confidence = parse_confidence(val.get("confidence"));
        return Some((fact, confidence));
    }

    None
}

/// Parse confidence from a JSON value. Handles:
/// - Numeric: 0.8, 0.9, 1.0 → parsed as f32
/// - Text: "high" → 0.9, "medium" → 0.5, "low" → 0.2
/// - Empty/null → 0.3 (model uncertain)
fn parse_confidence(val: Option<&serde_json::Value>) -> f32 {
    match val {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.3) as f32,
        Some(serde_json::Value::String(s)) => {
            let lower = s.to_lowercase();
            match lower.as_str() {
                "high" | "very high" => 0.9,
                "medium" | "moderate" => 0.5,
                "low" | "very low" => 0.2,
                "" => 0.3,
                _ => {
                    // Try parsing as f32 (handles "0.8", "0.9", etc.)
                    s.parse::<f32>().unwrap_or(0.3)
                }
            }
        }
        _ => 0.3, // null or missing
    }
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
            endpoint: String::new(),
            model: "test".into(),
            api_key: None,
            batch_size: 10,
            batch_interval_secs: 30,
            importance_gate: true,
            model_path: String::new(),
            max_tokens: 256,
            n_ctx: 2048,
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
            endpoint: String::new(),
            model: "test".into(),
            api_key: None,
            batch_size: 10,
            batch_interval_secs: 30,
            importance_gate: true,
            model_path: String::new(),
            max_tokens: 256,
            n_ctx: 2048,
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
            endpoint: String::new(),
            model: "test".into(),
            api_key: None,
            batch_size: 10,
            batch_interval_secs: 30,
            importance_gate: false,
            model_path: String::new(),
            max_tokens: 256,
            n_ctx: 2048,
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

    #[test]
    fn test_parse_confidence_numeric() {
        let val = serde_json::from_str::<serde_json::Value>("0.8").unwrap();
        assert!((parse_confidence(Some(&val)) - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_parse_confidence_text_high() {
        let val = serde_json::Value::String("high".to_string());
        assert!((parse_confidence(Some(&val)) - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_parse_confidence_text_medium() {
        let val = serde_json::Value::String("medium".to_string());
        assert!((parse_confidence(Some(&val)) - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_parse_confidence_text_low() {
        let val = serde_json::Value::String("low".to_string());
        assert!((parse_confidence(Some(&val)) - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_parse_confidence_empty_string() {
        let val = serde_json::Value::String("".to_string());
        assert!((parse_confidence(Some(&val)) - 0.3).abs() < 0.01);
    }

    #[test]
    fn test_parse_confidence_null() {
        assert!((parse_confidence(None) - 0.3).abs() < 0.01);
    }

    #[test]
    fn test_parse_llm_json_with_text_confidence() {
        let raw = r#"{"fact": "Alice prefers dark mode", "confidence": "high"}"#;
        let result = parse_llm_json(raw);
        assert!(result.is_some());
        let (fact, conf) = result.unwrap();
        assert_eq!(fact, "Alice prefers dark mode");
        assert!((conf - 0.9).abs() < 0.01);
    }

    #[test]
    fn test_fallback_fact() {
        let fact = fallback_fact("Alice mentioned the project deadline");
        assert_eq!(fact.fact, "Alice mentioned the project deadline");
        assert_eq!(fact.confidence, 0.0);
        assert!(!fact.entities.is_empty());
    }
}
