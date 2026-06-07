use uuid::Uuid;
use crate::error::Result;
use crate::store::vector::QdrantVectorStore;

/// Query the Qdrant vector store for the `limit` most similar embeddings
/// and return `(id, score)` pairs.
pub fn search_similar(
    store: &QdrantVectorStore,
    tenant_id: &str,
    query: Vec<f32>,
    limit: usize,
) -> Result<Vec<(Uuid, f32)>> {
    let results = store.search(tenant_id, query, limit)?;
    Ok(results.into_iter().map(|r| (r.id, r.score)).collect())
}
