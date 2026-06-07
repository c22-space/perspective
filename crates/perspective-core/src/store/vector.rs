use qdrant_edge::{
    EdgeShard, EdgeConfigBuilder, EdgeVectorParamsBuilder,
    PointStruct, ScoredPoint, Distance,
    PointOperations, PointInsertOperations,
    UpdateOperation, PointId, DEFAULT_VECTOR_NAME, SearchRequest,
};
use uuid::Uuid;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use crate::error::{PerspectiveError, Result};

pub struct QdrantVectorStore {
    shards: std::collections::HashMap<String, Arc<EdgeShard>>,
    data_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: Uuid,
    pub score: f32,
    pub payload: Option<serde_json::Value>,
}

fn uuid_to_point_id(id: Uuid) -> PointId {
    PointId::Uuid(id)
}

fn point_id_to_uuid(id: &PointId) -> Uuid {
    match id {
        PointId::NumId(n) => Uuid::from_u128(*n as u128),
        PointId::Uuid(u) => *u,
    }
}

impl QdrantVectorStore {
    pub fn new(data_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(data_dir).map_err(|e| PerspectiveError::Qdrant(e.to_string()))?;
        Ok(Self {
            shards: std::collections::HashMap::new(),
            data_dir: data_dir.to_path_buf(),
        })
    }

    fn shard_path(&self, tenant_id: &str) -> PathBuf {
        self.data_dir.join(format!("qdrant_{}", tenant_id))
    }

    fn get_or_create_shard(&mut self, tenant_id: &str, dimensions: usize) -> Result<Arc<EdgeShard>> {
        if let Some(shard) = self.shards.get(tenant_id) {
            return Ok(Arc::clone(shard));
        }

        let path = self.shard_path(tenant_id);
        std::fs::create_dir_all(&path)
            .map_err(|e| PerspectiveError::Qdrant(format!("Failed to create shard dir: {}", e)))?;

        let shard = if path.join("segments").exists() {
            EdgeShard::load(&path, None)
                .map_err(|e| PerspectiveError::Qdrant(format!("Failed to load shard: {}", e)))?
        } else {
            let config = EdgeConfigBuilder::new()
                .vector(
                    DEFAULT_VECTOR_NAME,
                    EdgeVectorParamsBuilder::new(dimensions, Distance::Cosine).build(),
                )
                .on_disk_payload(true)
                .build();

            EdgeShard::new(&path, config)
                .map_err(|e| PerspectiveError::Qdrant(format!("Failed to create shard: {}", e)))?
        };

        let shard = Arc::new(shard);
        self.shards.insert(tenant_id.to_string(), Arc::clone(&shard));
        Ok(shard)
    }

    pub fn upsert(
        &mut self,
        tenant_id: &str,
        id: Uuid,
        vector: Vec<f32>,
        payload: serde_json::Value,
        dimensions: usize,
    ) -> Result<()> {
        let shard = self.get_or_create_shard(tenant_id, dimensions)?;

        let point = PointStruct::new(uuid_to_point_id(id), qdrant_edge::Vectors::from(vector), payload);

        let op = PointOperations::UpsertPoints(PointInsertOperations::PointsList(vec![point.into()]));

        shard.update(UpdateOperation::PointOperation(op))
            .map_err(|e| PerspectiveError::Qdrant(format!("Upsert failed: {}", e)))?;

        Ok(())
    }

    pub fn search(
        &mut self,
        tenant_id: &str,
        query_vector: Vec<f32>,
        limit: usize,
        dimensions: usize,
    ) -> Result<Vec<SearchResult>> {
        let path = self.shard_path(tenant_id);
        if !path.join("segments").exists() {
            return Ok(vec![]);
        }

        let shard = self.get_or_create_shard(tenant_id, dimensions)?;

        let search_request = SearchRequest {
            query: query_vector.into(),
            filter: None,
            params: None,
            limit,
            offset: 0,
            with_payload: Some(true.into()),
            with_vector: None,
            score_threshold: None,
        };

        let results: Vec<ScoredPoint> = shard.search(search_request)
            .map_err(|e| PerspectiveError::Qdrant(format!("Search failed: {}", e)))?;

        Ok(results.into_iter().map(|r| {
            let id = point_id_to_uuid(&r.id);
            let payload = r.payload.map(|p| {
                let mut map = serde_json::Map::new();
                for (k, v) in p.0 {
                    map.insert(k.to_string(), v.into());
                }
                serde_json::Value::Object(map)
            });
            SearchResult { id, score: r.score, payload }
        }).collect())
    }

    pub fn delete(&mut self, tenant_id: &str, id: Uuid, dimensions: usize) -> Result<()> {
        let path = self.shard_path(tenant_id);
        if !path.join("segments").exists() {
            return Ok(());
        }

        let shard = self.get_or_create_shard(tenant_id, dimensions)?;

        let op = PointOperations::DeletePoints {
            ids: vec![uuid_to_point_id(id)],
        };

        shard.update(UpdateOperation::PointOperation(op))
            .map_err(|e| PerspectiveError::Qdrant(format!("Delete failed: {}", e)))?;

        Ok(())
    }

    pub fn collection_exists(&self, tenant_id: &str) -> bool {
        self.shard_path(tenant_id).join("segments").exists()
    }
}
