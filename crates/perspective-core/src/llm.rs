//! Bundled LLM inference via llama.cpp for fact extraction.
//!
//! Wraps `llama-cpp-2` to provide a simple synchronous inference interface
//! for the extraction pipeline. The model is loaded once and reused.

use crate::error::{PerspectiveError, Result};
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tracing::info;

/// A thin wrapper around llama.cpp for local inference.
///
/// Holds the loaded model and provides a thread-safe `complete()` method
/// that the extraction pipeline calls instead of making HTTP requests.
pub struct BundledLlm {
    inner: Arc<Mutex<LlmInner>>,
}

struct LlmInner {
    backend: LlamaBackend,
    model: LlamaModel,
    /// Max tokens to generate per extraction call.
    max_tokens: u32,
    /// Context window size.
    n_ctx: u32,
}

impl BundledLlm {
    /// Load a GGUF model from disk.
    ///
    /// `model_path` should point to the .gguf file.
    /// `max_tokens` controls how many tokens the model can generate per call.
    /// `n_ctx` is the context window size in tokens.
    pub fn load(model_path: &Path, max_tokens: u32, n_ctx: u32) -> Result<Self> {
        // Pre-check: llama-cpp-2 panics on missing files instead of returning Err
        if !model_path.exists() {
            return Err(PerspectiveError::LlmApi(format!(
                "Model file not found: {}",
                model_path.display()
            )));
        }

        info!(
            "Loading bundled LLM from {} (max_tokens={}, n_ctx={})",
            model_path.display(),
            max_tokens,
            n_ctx
        );

        let backend = LlamaBackend::init().map_err(|e| {
            PerspectiveError::LlmApi(format!("Failed to initialize llama.cpp backend: {e}"))
        })?;

        let model_params = LlamaModelParams::default();
        let model =
            LlamaModel::load_from_file(&backend, model_path, &model_params).map_err(|e| {
                PerspectiveError::LlmApi(format!(
                    "Failed to load model from {}: {e}",
                    model_path.display()
                ))
            })?;

        info!(
            "Bundled LLM loaded ({})",
            model_path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_default()
        );

        Ok(Self {
            inner: Arc::new(Mutex::new(LlmInner {
                backend,
                model,
                max_tokens,
                n_ctx,
            })),
        })
    }

    /// Run a single completion: send the prompt to the model, return the raw response text.
    ///
    /// This is synchronous and holds the mutex for the duration of inference.
    /// Since Ternary-Bonsai is 442MB, this completes in well under a second on CPU.
    pub fn complete(&self, prompt: &str) -> Result<String> {
        let inner = self.inner.lock().map_err(|e| {
            PerspectiveError::LlmApi(format!("Failed to acquire LLM lock: {e}"))
        })?;

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZeroU32::new(inner.n_ctx))
            .with_n_batch(inner.n_ctx);

        let mut ctx = inner
            .model
            .new_context(&inner.backend, ctx_params)
            .map_err(|e| {
                PerspectiveError::LlmApi(format!("Failed to create LLM context: {e}"))
            })?;

        // Tokenize the prompt
        let tokens_list = inner
            .model
            .str_to_token(prompt, AddBos::Always)
            .map_err(|e| {
                PerspectiveError::LlmApi(format!("Failed to tokenize prompt: {e}"))
            })?;

        // Create batch and feed prompt tokens
        let mut batch = LlamaBatch::new(inner.n_ctx as usize, 1);
        let last_index = tokens_list.len() as i32 - 1;
        for (i, token) in (0_i32..).zip(tokens_list) {
            let is_last = i == last_index;
            batch.add(token, i, &[0], is_last).map_err(|e| {
                PerspectiveError::LlmApi(format!("Failed to add token to batch: {e}"))
            })?;
        }
        ctx.decode(&mut batch)
            .map_err(|e| PerspectiveError::LlmApi(format!("Failed to decode prompt: {e}")))?;

        // Generate response tokens
        let mut n_cur = batch.n_tokens();
        let mut sampler = LlamaSampler::greedy();
        let mut decoder = encoding_rs::UTF_8.new_decoder();
        let mut output = String::new();

        while n_cur < inner.max_tokens as i32 {
            let token = sampler.sample(&ctx, batch.n_tokens() - 1);
            sampler.accept(token);

            if token == inner.model.token_eos() {
                break;
            }

            let piece = inner
                .model
                .token_to_piece(token, &mut decoder, true, None)
                .unwrap_or_default();
            output.push_str(&piece);

            batch.clear();
            batch.add(token, n_cur, &[0], true).map_err(|e| {
                PerspectiveError::LlmApi(format!("Failed to add generated token: {e}"))
            })?;
            ctx.decode(&mut batch).map_err(|e| {
                PerspectiveError::LlmApi(format!("Failed to decode generated token: {e}"))
            })?;

            n_cur += 1;
        }

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
