use uuid::Uuid;
use crate::error::Result;
use crate::store::vector::QdrantVectorStore;

/// Query the Qdrant vector store for the `limit` most similar embeddings
/// and return `(id, score)` pairs.
pub fn search_similar(
    store: &mut QdrantVectorStore,
    tenant_id: &str,
    query: Vec<f32>,
    limit: usize,
    dimensions: usize,
) -> Result<Vec<(Uuid, f32)>> {
    let results = store.search(tenant_id, query, limit, dimensions)?;
    Ok(results.into_iter().map(|r| (r.id, r.score)).collect())
}
