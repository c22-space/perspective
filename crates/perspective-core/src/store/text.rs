use crate::error::{PerspectiveError, Result};
use std::path::Path;
use tantivy::query::QueryParser;
use tantivy::schema::*;
use tantivy::{doc, Index, IndexReader, IndexWriter};
use uuid::Uuid;

pub struct TextStore {
    index: Index,
    reader: IndexReader,
    content_field: Field,
    id_field: Field,
    tenant_field: Field,
}

#[derive(Debug)]
pub struct TextSearchResult {
    pub id: Uuid,
    pub score: f32,
}

impl TextStore {
    pub fn new(path: &Path) -> Result<Self> {
        std::fs::create_dir_all(path).map_err(|e| PerspectiveError::Storage(e.to_string()))?;

        let mut schema_builder = Schema::builder();
        let content_field = schema_builder.add_text_field("content", TEXT | STORED);
        let id_field = schema_builder.add_text_field("id", STRING | STORED);
        let tenant_field = schema_builder.add_text_field("tenant", STRING | STORED);
        let schema = schema_builder.build();

        // Try opening existing index first, create if it doesn't exist
        let index = match Index::open_in_dir(path) {
            Ok(idx) => idx,
            Err(_) => Index::create_in_dir(path, schema.clone())
                .map_err(|e| PerspectiveError::Storage(e.to_string()))?,
        };

        let reader = index
            .reader_builder()
            .reload_policy(tantivy::ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;

        Ok(Self {
            index,
            reader,
            content_field,
            id_field,
            tenant_field,
        })
    }

    pub fn add_document(&self, tenant_id: &str, id: Uuid, content: &str) -> Result<()> {
        let mut writer: IndexWriter = self
            .index
            .writer(50_000_000)
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;

        let doc = doc!(
            self.content_field => content,
            self.id_field => id.to_string().as_str(),
            self.tenant_field => tenant_id,
        );

        writer
            .add_document(doc)
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;
        writer
            .commit()
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn search(
        &self,
        tenant_id: &str,
        query_str: &str,
        limit: usize,
    ) -> Result<Vec<TextSearchResult>> {
        let searcher = self.reader.searcher();

        let query_parser = QueryParser::for_index(&self.index, vec![self.content_field]);

        let parsed_query = query_parser
            .parse_query(query_str)
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;

        let top_docs = searcher
            .search(
                &parsed_query,
                &tantivy::collector::TopDocs::with_limit(limit),
            )
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;

        let mut results = Vec::new();
        for (score, doc_addr) in top_docs {
            if let Ok(doc) = searcher.doc::<tantivy::TantivyDocument>(doc_addr) {
                let is_tenant = doc
                    .get_first(self.tenant_field)
                    .and_then(|v| v.as_str())
                    .map(|s| s == tenant_id)
                    .unwrap_or(false);

                if is_tenant {
                    if let Some(id_val) = doc.get_first(self.id_field) {
                        if let Some(id_str) = id_val.as_str() {
                            if let Ok(id) = Uuid::parse_str(id_str) {
                                results.push(TextSearchResult { id, score });
                            }
                        }
                    }
                }
            }
        }

        Ok(results)
    }

    pub fn delete_document(&self, _tenant_id: &str, id: Uuid) -> Result<()> {
        let mut writer: IndexWriter = self
            .index
            .writer(50_000_000)
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;

        let query = tantivy::query::TermQuery::new(
            tantivy::Term::from_field_text(self.id_field, &id.to_string()),
            tantivy::schema::IndexRecordOption::Basic,
        );

        writer
            .delete_query(Box::new(query))
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;
        writer
            .commit()
            .map_err(|e| PerspectiveError::Storage(e.to_string()))?;
        Ok(())
    }
}
