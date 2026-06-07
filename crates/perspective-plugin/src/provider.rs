use perspective_core::engine::PerspectiveEngine;
use perspective_core::config::Config;

pub struct PerspectiveProvider {
    engine: PerspectiveEngine,
}

impl PerspectiveProvider {
    pub async fn new(config: Config) -> perspective_core::error::Result<Self> {
        let engine = PerspectiveEngine::new(config).await?;
        Ok(Self { engine })
    }

    pub async fn retain(&self, content: &str, session_id: &str, metadata: serde_json::Value) -> perspective_core::error::Result<String> {
        Ok("retained".into())
    }

    pub async fn recall(&self, query: &str, budget: usize) -> perspective_core::error::Result<String> {
        Ok("".into())
    }

    pub async fn reflect(&self, query: &str) -> perspective_core::error::Result<String> {
        Ok("".into())
    }
}
