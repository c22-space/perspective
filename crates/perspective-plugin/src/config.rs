use perspective_core::config::Config;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Plugin-level configuration that wraps the core engine Config.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    /// The tenant ID to use for all operations.
    pub tenant_id: String,
    /// The underlying core engine configuration.
    pub engine: Config,
    /// Human-readable plugin name.
    pub name: String,
}

impl PluginConfig {
    /// Create a new PluginConfig with a tenant ID and default engine config.
    pub fn new(tenant_id: impl Into<String>) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            engine: Config::default(),
            name: "perspective".to_string(),
        }
    }

    /// Create from an explicit core Config.
    pub fn with_engine(tenant_id: impl Into<String>, engine: Config) -> Self {
        Self {
            tenant_id: tenant_id.into(),
            engine,
            name: "perspective".to_string(),
        }
    }

    /// Set the data directory on the inner engine config.
    pub fn with_data_dir(mut self, dir: PathBuf) -> Self {
        self.engine.storage.data_dir = dir;
        self
    }
}

impl Default for PluginConfig {
    fn default() -> Self {
        Self {
            tenant_id: "default".to_string(),
            engine: Config::default(),
            name: "perspective".to_string(),
        }
    }
}

impl From<Config> for PluginConfig {
    fn from(engine: Config) -> Self {
        Self {
            tenant_id: "default".to_string(),
            engine,
            name: "perspective".to_string(),
        }
    }
}
