use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginConfig {
    pub engine_endpoint: Option<String>,
    pub tenant_id: String,
}
