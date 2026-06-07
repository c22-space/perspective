use perspective_core::config::Config;
use perspective_core::engine::PerspectiveEngine;
use perspective_core::error::Result;
use perspective_core::types::MemoryType;
use perspective_core::engine::StoreRequest;
use std::collections::HashMap;
use tracing::{info, warn};

use crate::config::PluginConfig;

/// Health status returned by the provider.
#[derive(Debug, Clone)]
pub struct HealthStatus {
    /// Whether the engine is healthy.
    pub healthy: bool,
    /// Human-readable status message.
    pub message: String,
}

/// The Hermes memory provider backed by a Perspective engine.
pub struct PerspectiveProvider {
    engine: PerspectiveEngine,
    config: PluginConfig,
}

impl PerspectiveProvider {
    /// Create a new provider from a core Config (uses "default" tenant).
    pub async fn new(config: Config) -> Result<Self> {
        let plugin_config = PluginConfig::from(config);
        Self::with_config(plugin_config).await
    }

    /// Create a new provider from a full PluginConfig.
    pub async fn with_config(config: PluginConfig) -> Result<Self> {
        info!(
            "Initializing PerspectiveProvider (tenant={})",
            config.tenant_id
        );
        let engine = PerspectiveEngine::new(config.engine.clone()).await?;
        Ok(Self { engine, config })
    }

    /// Create a new provider from an existing engine (for testing/custom init).
    pub fn from_engine(engine: PerspectiveEngine, config: PluginConfig) -> Self {
        Self { engine, config }
    }

    /// Retain (store) a memory.
    ///
    /// Stores the given content as an episodic memory tagged with the session
    /// and any extra metadata. Returns the memory ID as a string.
    pub async fn retain(
        &self,
        content: &str,
        session_id: &str,
        metadata: serde_json::Value,
    ) -> Result<String> {
        let tenant_id = &self.config.tenant_id;

        // Convert metadata JSON object to HashMap
        let meta_map = match metadata.as_object() {
            Some(obj) => obj
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect::<HashMap<String, serde_json::Value>>(),
            None => HashMap::new(),
        };

        let mut tags = vec![];
        if let Some(t) = metadata.get("tags").and_then(|v| v.as_array()) {
            tags = t
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
        }

        let context = metadata
            .get("context")
            .and_then(|v| v.as_str())
            .map(String::from);

        let req = StoreRequest {
            tenant_id: tenant_id.clone(),
            content: content.to_string(),
            memory_type: MemoryType::Episodic,
            tags,
            metadata: meta_map,
            context,
            source_session: Some(session_id.to_string()),
        };

        let id = self.engine.store(req).await?;
        let id_str = id.to_string();
        info!("Retained memory {} from session {}", id_str, session_id);
        Ok(id_str)
    }

    /// Recall memories relevant to the query, formatted as a context string.
    ///
    /// `budget` is the maximum number of memories to retrieve.
    /// Returns a formatted string suitable for injection into an LLM context.
    pub async fn recall(&self, query: &str, budget: usize) -> Result<String> {
        let tenant_id = &self.config.tenant_id;
        let result = self.engine.recall(tenant_id, query, budget).await?;

        if result.memories.is_empty() {
            return Ok(String::new());
        }

        let mut context = String::from("## Relevant Memories\n\n");
        for (i, memory) in result.memories.iter().enumerate() {
            let score = result.scores.get(i).copied().unwrap_or(0.0);
            let type_label = match memory.memory_type() {
                MemoryType::Episodic => "episodic",
                MemoryType::Semantic => "semantic",
                MemoryType::Procedural => "procedural",
            };
            context.push_str(&format!(
                "[{}] ({}, score: {:.3}) {}\n\n",
                i + 1,
                type_label,
                score,
                memory.content()
            ));
        }

        Ok(context)
    }

    /// Reflect on a query: recall relevant memories and synthesize a summary.
    ///
    /// Performs a recall and then produces a structured synthesis string
    /// that an LLM can use as reflection context.
    pub async fn reflect(&self, query: &str) -> Result<String> {
        let tenant_id = &self.config.tenant_id;
        let default_budget = self.config.engine.retrieval.default_budget;
        let result = self.engine.recall(tenant_id, query, default_budget).await?;

        if result.memories.is_empty() {
            return Ok(format!(
                "No memories found related to: \"{}\"",
                query
            ));
        }

        let mut synthesis = format!(
            "## Reflection on: \"{}\"\n\nFound {} relevant memories:\n\n",
            query,
            result.memories.len()
        );

        // Group by memory type
        let mut episodic = Vec::new();
        let mut semantic = Vec::new();
        let mut procedural = Vec::new();

        for memory in &result.memories {
            match memory.memory_type() {
                MemoryType::Episodic => episodic.push(memory),
                MemoryType::Semantic => semantic.push(memory),
                MemoryType::Procedural => procedural.push(memory),
            }
        }

        if !episodic.is_empty() {
            synthesis.push_str(&format!("### Episodes ({}):\n", episodic.len()));
            for m in &episodic {
                synthesis.push_str(&format!("- {}\n", m.content()));
            }
            synthesis.push('\n');
        }

        if !semantic.is_empty() {
            synthesis.push_str(&format!("### Facts ({}):\n", semantic.len()));
            for m in &semantic {
                synthesis.push_str(&format!("- {}\n", m.content()));
            }
            synthesis.push('\n');
        }

        if !procedural.is_empty() {
            synthesis.push_str(&format!(
                "### Procedures ({}):\n",
                procedural.len()
            ));
            for m in &procedural {
                synthesis.push_str(&format!("- {}\n", m.content()));
            }
            synthesis.push('\n');
        }

        Ok(synthesis)
    }

    /// Check the health of the underlying engine.
    pub async fn health(&self) -> Result<HealthStatus> {
        // Try to access the engine config - if we got here the engine was created
        // successfully. We can verify it's still usable by checking we can
        // read the config.
        let _config = self.engine.config();

        // Attempt a lightweight operation: recall with empty query to verify stores work
        match self
            .engine
            .recall(&self.config.tenant_id, "health check", 1)
            .await
        {
            Ok(_) => Ok(HealthStatus {
                healthy: true,
                message: format!(
                    "Engine healthy, tenant={}",
                    self.config.tenant_id
                ),
            }),
            Err(e) => {
                warn!("Engine health check failed: {}", e);
                Ok(HealthStatus {
                    healthy: false,
                    message: format!("Engine unhealthy: {}", e),
                })
            }
        }
    }

    /// Access the underlying engine (e.g. for advanced operations).
    pub fn engine(&self) -> &PerspectiveEngine {
        &self.engine
    }

    /// Access the plugin config.
    pub fn config(&self) -> &PluginConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_config_default() {
        let cfg = PluginConfig::default();
        assert_eq!(cfg.tenant_id, "default");
        assert_eq!(cfg.name, "perspective");
    }

    #[test]
    fn test_health_status_struct() {
        let h = HealthStatus {
            healthy: true,
            message: "ok".into(),
        };
        assert!(h.healthy);
        assert_eq!(h.message, "ok");
    }
}
