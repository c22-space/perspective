//! Bundled LLM inference via candle for fact extraction.
//!
//! Uses `candle-core` quantized inference with `candle-transformers` Qwen2 model
//! to run local fact extraction without any C++ dependencies.

use crate::error::{PerspectiveError, Result};
use candle_core::quantized::gguf_file;
use candle_core::quantized::tokenizer::TokenizerFromGguf;
use candle_core::{Device, Tensor};
use candle_transformers::models::quantized_qwen2::ModelWeights;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokenizers::Tokenizer;
use tracing::info;

/// A thin wrapper around candle for local GGUF inference.
pub struct BundledLlm {
    inner: Arc<Mutex<LlmInner>>,
}

struct LlmInner {
    model: ModelWeights,
    tokenizer: Tokenizer,
    /// Max tokens to generate per extraction call.
    max_tokens: u32,
}

impl BundledLlm {
    /// Load a GGUF model from disk.
    ///
    /// `model_path` should point to the .gguf file.
    /// `max_tokens` controls how many tokens the model can generate per call.
    /// `n_ctx` is the context window size (unused by candle, kept for API compat).
    pub fn load(model_path: &Path, max_tokens: u32, _n_ctx: u32) -> Result<Self> {
        if !model_path.exists() {
            return Err(PerspectiveError::LlmApi(format!(
                "Model file not found: {}",
                model_path.display()
            )));
        }

        info!(
            "Loading bundled LLM from {} (max_tokens={})",
            model_path.display(),
            max_tokens,
        );

        let device = Device::Cpu;

        // Read the GGUF file
        let mut file = std::fs::File::open(model_path)
            .map_err(|e| PerspectiveError::LlmApi(format!("Failed to open model file: {e}")))?;

        let content = gguf_file::Content::read(&mut file)
            .map_err(|e| PerspectiveError::LlmApi(format!("Failed to parse GGUF file: {e}")))?;

        // Extract tokenizer from GGUF metadata
        let tokenizer = Tokenizer::from_gguf(&content)
            .map_err(|e| PerspectiveError::LlmApi(format!("Failed to load tokenizer from GGUF: {e}")))?;

        // Load model weights
        let model = ModelWeights::from_gguf(content, &mut file, &device)
            .map_err(|e| PerspectiveError::LlmApi(format!("Failed to load model weights: {e}")))?;

        info!(
            "Bundled LLM loaded (candle, {})",
            model_path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default()
        );

        Ok(Self {
            inner: Arc::new(Mutex::new(LlmInner {
                model,
                tokenizer,
                max_tokens,
            })),
        })
    }

    /// Run a single completion: send the prompt to the model, return the raw response text.
    ///
    /// This is synchronous and holds the mutex for the duration of inference.
    pub fn complete(&self, prompt: &str) -> Result<String> {
        let mut inner = self.inner.lock().map_err(|e| {
            PerspectiveError::LlmApi(format!("Failed to acquire LLM lock: {e}"))
        })?;

        // Tokenize the prompt
        let encoding = inner
            .tokenizer
            .encode(prompt, true)
            .map_err(|e| PerspectiveError::LlmApi(format!("Failed to tokenize prompt: {e}")))?;

        let tokens = encoding.get_ids();
        let n_prompt = tokens.len();

        // Convert tokens to tensor
        let tokens_tensor = Tensor::new(tokens, &Device::Cpu)
            .map_err(|e| PerspectiveError::LlmApi(format!("Failed to create token tensor: {e}")))?;
        let tokens_tensor = tokens_tensor
            .unsqueeze(0)
            .map_err(|e| PerspectiveError::LlmApi(format!("Failed to unsqueeze token tensor: {e}")))?;

        // Run the model on the prompt tokens to build up the KV cache
        let _logits = inner
            .model
            .forward(&tokens_tensor, 0)
            .map_err(|e| PerspectiveError::LlmApi(format!("Model forward pass failed: {e}")))?;

        // Generate tokens one at a time (greedy decoding)
        let mut generated_tokens = Vec::new();
        let mut prev_token = *tokens.last().unwrap_or(&0u32);

        for _step in 0..inner.max_tokens {
            let input_tensor = Tensor::new(&[prev_token], &Device::Cpu)
                .map_err(|e| PerspectiveError::LlmApi(format!("Failed to create input tensor: {e}")))?;
            let input_tensor = input_tensor
                .unsqueeze(0)
                .map_err(|e| PerspectiveError::LlmApi(format!("Failed to unsqueeze input tensor: {e}")))?;

            let logits = inner
                .model
                .forward(&input_tensor, n_prompt + generated_tokens.len())
                .map_err(|e| PerspectiveError::LlmApi(format!("Model forward pass failed: {e}")))?;

            // Get the token with highest probability (greedy)
            // logits shape: (batch=1, seq_len=1, vocab_size), argmax over vocab dim
            let next_token = logits
                .argmax(2)
                .map_err(|e| PerspectiveError::LlmApi(format!("Argmax failed: {e}")))?
                .to_scalar::<u32>()
                .map_err(|e| PerspectiveError::LlmApi(format!("Failed to extract token: {e}")))?;

            // Stop at EOS
            if next_token == 0 {
                break;
            }

            generated_tokens.push(next_token);
            prev_token = next_token;
        }

        // Decode generated tokens
        let output = inner
            .tokenizer
            .decode(&generated_tokens, true)
            .map_err(|e| PerspectiveError::LlmApi(format!("Failed to decode output tokens: {e}")))?;

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_bundled_llm_load_nonexistent() {
        let result = BundledLlm::load(&PathBuf::from("nonexistent-model.gguf"), 256, 2048);
        assert!(result.is_err());
    }
}
