use qdrant_client::Qdrant;
use qdrant_client::Payload;
use qdrant_client::qdrant::{
    CreateCollectionBuilder, Distance, PointStruct, VectorParamsBuilder,
    SearchPointsBuilder, DeletePointsBuilder, UpsertPointsBuilder,
    PointsIdsList, point_id,
};
use uuid::Uuid;
use crate::error::{PerspectiveError, Result};

pub struct QdrantVectorStore {
    client: Qdrant,
    collection_prefix: String,
}

#[derive(Debug, Clone)]
pub struct SearchResult {
    pub id: Uuid,
    pub score: f32,
    pub payload: Option<serde_json::Value>,
}

impl QdrantVectorStore {
    pub async fn new(url: &str, api_key: Option<&str>) -> Result<Self> {
        let client = Qdrant::from_url(url)
            .api_key(api_key.unwrap_or(""))
            .build()
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?;
        Ok(Self {
            client,
            collection_prefix: "perspective".into(),
        })
    }

    fn collection_name(&self, tenant_id: &str) -> String {
        format!("{}_{}", self.collection_prefix, tenant_id)
    }

    pub async fn ensure_collection(
        &self,
        tenant_id: &str,
        dimensions: u64,
    ) -> Result<()> {
        let name = self.collection_name(tenant_id);
        self.client
            .create_collection(
                CreateCollectionBuilder::new(&name)
                    .vectors_config(VectorParamsBuilder::new(dimensions, Distance::Cosine))
                    .on_disk_payload(true),
            )
            .await
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?;
        Ok(())
    }

    pub async fn upsert(
        &self,
        tenant_id: &str,
        id: Uuid,
        vector: Vec<f32>,
        payload: serde_json::Value,
    ) -> Result<()> {
        let name = self.collection_name(tenant_id);
        let qdrant_payload: Payload = payload.try_into()
            .map_err(|e: qdrant_client::QdrantError| PerspectiveError::Qdrant(e.to_string()))?;
        let point = PointStruct::new(id.to_string(), vector, qdrant_payload);
        self.client
            .upsert_points(
                UpsertPointsBuilder::new(name, vec![point]),
            )
            .await
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?;
        Ok(())
    }

    pub async fn search(
        &self,
        tenant_id: &str,
        query_vector: Vec<f32>,
        limit: u64,
    ) -> Result<Vec<SearchResult>> {
        let name = self.collection_name(tenant_id);
        let results = self.client
            .search_points(
                SearchPointsBuilder::new(&name, query_vector, limit)
                    .with_payload(true),
            )
            .await
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?;

        Ok(results
            .result
            .into_iter()
            .map(|r| {
                let id = match &r.id {
                    Some(point_id) => match &point_id.point_id_options {
                        Some(point_id::PointIdOptions::Uuid(uuid)) => {
                            Uuid::parse_str(uuid).unwrap_or_default()
                        }
                        Some(point_id::PointIdOptions::Num(num)) => {
                            Uuid::from_u128(*num as u128)
                        }
                        None => Uuid::nil(),
                    },
                    None => Uuid::nil(),
                };
                SearchResult {
                    id,
                    score: r.score,
                    payload: if r.payload.is_empty() {
                        None
                    } else {
                        Some(serde_json::to_value(r.payload).unwrap_or_default())
                    },
                }
            })
            .collect())
    }

    pub async fn delete(&self, tenant_id: &str, id: Uuid) -> Result<()> {
        let name = self.collection_name(tenant_id);
        self.client
            .delete_points(
                DeletePointsBuilder::new(name)
                    .points(PointsIdsList {
                        ids: vec![id.to_string().into()],
                    }),
            )
            .await
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?;
        Ok(())
    }

    pub async fn collection_exists(&self, tenant_id: &str) -> Result<bool> {
        let name = self.collection_name(tenant_id);
        let result = self.client.collection_exists(&name)
            .await
            .map_err(|e| PerspectiveError::Qdrant(e.to_string()))?;
        Ok(result)
    }
}
