//! Unified search service using Tantivy for FTS and vector search
//!
//! Provides a comprehensive search solution for AdapterOS with:
//! - Full-text search (FTS) for chat messages and documents
//! - Vector similarity search for semantic retrieval
//! - Hybrid search combining both approaches with re-ranking
//! - Tenant isolation for multi-tenant environments
//! - Deterministic ordering for reproducible results

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Arc, Mutex};
use tantivy::schema::*;
use tantivy::{Index, IndexWriter, ReloadPolicy, TantivyDocument, Term};
use tracing::{debug, info, warn};

/// Error type for search operations
#[derive(Debug)]
pub enum SearchError {
    /// Tantivy index error
    IndexError(String),
    /// IO error
    Io(std::io::Error),
    /// Query error
    QueryError(String),
    /// Lock error
    LockError(String),
}

impl From<tantivy::TantivyError> for SearchError {
    fn from(err: tantivy::TantivyError) -> Self {
        SearchError::IndexError(err.to_string())
    }
}

impl From<std::io::Error> for SearchError {
    fn from(err: std::io::Error) -> Self {
        SearchError::Io(err)
    }
}

impl From<tantivy::query::QueryParserError> for SearchError {
    fn from(err: tantivy::query::QueryParserError) -> Self {
        SearchError::QueryError(err.to_string())
    }
}

impl From<tantivy::directory::error::OpenDirectoryError> for SearchError {
    fn from(err: tantivy::directory::error::OpenDirectoryError) -> Self {
        SearchError::Io(std::io::Error::other(err.to_string()))
    }
}

impl std::fmt::Display for SearchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SearchError::IndexError(msg) => write!(f, "Index error: {}", msg),
            SearchError::Io(err) => write!(f, "IO error: {}", err),
            SearchError::QueryError(msg) => write!(f, "Query error: {}", msg),
            SearchError::LockError(msg) => write!(f, "Lock error: {}", msg),
        }
    }
}

impl std::error::Error for SearchError {}

/// Chat message for indexing (from KV store)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessageKv {
    pub id: String,
    pub session_id: String,
    pub tenant_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: i64,
}

/// Document chunk for indexing (from KV store)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunkKv {
    pub chunk_id: String,
    pub document_id: String,
    pub tenant_id: String,
    pub chunk_index: i32,
    pub page_number: Option<i32>,
    pub text: String,
    pub chunk_hash: String,
}

/// Full-text search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: String,
    pub score: f32,
    pub snippet: Option<String>,
    pub document_type: String, // "chat_message" or "document_chunk"
}

/// Vector similarity search result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VectorSearchResult {
    pub chunk_id: String,
    pub document_id: String,
    pub similarity: f32,
    pub text_preview: String,
    pub page_number: Option<i32>,
}

/// Hybrid search result combining FTS and vector search
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HybridSearchResult {
    pub id: String,
    pub document_id: Option<String>,
    pub score: f32,
    pub fts_score: f32,
    pub vector_score: f32,
    pub snippet: Option<String>,
    pub document_type: String,
    pub page_number: Option<i32>,
}

/// Unified search service using Tantivy
///
/// This service provides full-text search and vector similarity search
/// with tenant isolation and deterministic ordering.
pub struct SearchService {
    index: Index,
    writer: Arc<Mutex<IndexWriter>>,

    // Field handles for efficient access
    id_field: Field,
    tenant_id_field: Field,
    document_type_field: Field,
    content_field: Field,
    document_id_field: Field,
    session_id_field: Field,
    role_field: Field,
    timestamp_field: Field,
    chunk_index_field: Field,
    page_number_field: Field,
    chunk_hash_field: Field,
    vector_field: Field,
}

impl SearchService {
    /// Open an existing index or create a new one at the specified path
    pub fn open(path: &Path) -> Result<Self, SearchError> {
        std::fs::create_dir_all(path)?;

        let schema = Self::build_schema();
        let index = Index::open_or_create(
            tantivy::directory::MmapDirectory::open(path)?,
            schema.clone(),
        )?;

        let writer = index.writer(50_000_000)?; // 50MB buffer

        let id_field = schema.get_field("id").unwrap();
        let tenant_id_field = schema.get_field("tenant_id").unwrap();
        let document_type_field = schema.get_field("document_type").unwrap();
        let content_field = schema.get_field("content").unwrap();
        let document_id_field = schema.get_field("document_id").unwrap();
        let session_id_field = schema.get_field("session_id").unwrap();
        let role_field = schema.get_field("role").unwrap();
        let timestamp_field = schema.get_field("timestamp").unwrap();
        let chunk_index_field = schema.get_field("chunk_index").unwrap();
        let page_number_field = schema.get_field("page_number").unwrap();
        let chunk_hash_field = schema.get_field("chunk_hash").unwrap();
        let vector_field = schema.get_field("vector").unwrap();

        info!(path = ?path, "Opened Tantivy search index");

        Ok(Self {
            index,
            writer: Arc::new(Mutex::new(writer)),
            id_field,
            tenant_id_field,
            document_type_field,
            content_field,
            document_id_field,
            session_id_field,
            role_field,
            timestamp_field,
            chunk_index_field,
            page_number_field,
            chunk_hash_field,
            vector_field,
        })
    }

    /// Create an in-memory index for testing
    pub fn open_in_memory() -> Result<Self, SearchError> {
        let schema = Self::build_schema();
        let index = Index::create_in_ram(schema.clone());

        let writer = index.writer(50_000_000)?; // 50MB buffer

        let id_field = schema.get_field("id").unwrap();
        let tenant_id_field = schema.get_field("tenant_id").unwrap();
        let document_type_field = schema.get_field("document_type").unwrap();
        let content_field = schema.get_field("content").unwrap();
        let document_id_field = schema.get_field("document_id").unwrap();
        let session_id_field = schema.get_field("session_id").unwrap();
        let role_field = schema.get_field("role").unwrap();
        let timestamp_field = schema.get_field("timestamp").unwrap();
        let chunk_index_field = schema.get_field("chunk_index").unwrap();
        let page_number_field = schema.get_field("page_number").unwrap();
        let chunk_hash_field = schema.get_field("chunk_hash").unwrap();
        let vector_field = schema.get_field("vector").unwrap();

        info!("Created in-memory Tantivy search index");

        Ok(Self {
            index,
            writer: Arc::new(Mutex::new(writer)),
            id_field,
            tenant_id_field,
            document_type_field,
            content_field,
            document_id_field,
            session_id_field,
            role_field,
            timestamp_field,
            chunk_index_field,
            page_number_field,
            chunk_hash_field,
            vector_field,
        })
    }

    /// Build the Tantivy schema for unified search
    fn build_schema() -> Schema {
        let mut schema_builder = Schema::builder();

        // Primary key
        schema_builder.add_text_field("id", STRING | STORED);

        // Tenant isolation
        schema_builder.add_text_field("tenant_id", STRING | STORED);

        // Document type: "chat_message" or "document_chunk"
        schema_builder.add_text_field("document_type", STRING | STORED);

        // Full-text searchable content
        schema_builder.add_text_field("content", TEXT | STORED);

        // Chat message fields
        schema_builder.add_text_field("session_id", STRING | STORED);
        schema_builder.add_text_field("role", STRING | STORED);
        schema_builder.add_i64_field("timestamp", INDEXED | STORED);

        // Document chunk fields
        schema_builder.add_text_field("document_id", STRING | STORED);
        schema_builder.add_i64_field("chunk_index", INDEXED | STORED);
        schema_builder.add_i64_field("page_number", INDEXED | STORED);
        schema_builder.add_text_field("chunk_hash", STRING | STORED);

        // Vector field for semantic search (stored as JSON string)
        let text_options = TextOptions::default()
            .set_indexing_options(
                TextFieldIndexing::default()
                    .set_tokenizer("raw")
                    .set_index_option(tantivy::schema::IndexRecordOption::Basic),
            )
            .set_stored();
        schema_builder.add_text_field("vector", text_options);

        schema_builder.build()
    }

    /// Index a chat message for full-text search
    pub async fn index_chat_message(&self, message: &ChatMessageKv) -> Result<(), SearchError> {
        let mut doc = TantivyDocument::default();
        doc.add_text(self.id_field, &message.id);
        doc.add_text(self.tenant_id_field, &message.tenant_id);
        doc.add_text(self.document_type_field, "chat_message");
        doc.add_text(self.content_field, &message.content);
        doc.add_text(self.session_id_field, &message.session_id);
        doc.add_text(self.role_field, &message.role);
        doc.add_i64(self.timestamp_field, message.timestamp);

        // Add empty values for document chunk fields
        doc.add_text(self.document_id_field, "");
        doc.add_i64(self.chunk_index_field, 0);
        doc.add_i64(self.page_number_field, 0);
        doc.add_text(self.chunk_hash_field, "");
        doc.add_text(self.vector_field, "");

        let mut writer = self
            .writer
            .lock()
            .map_err(|e| SearchError::LockError(format!("Failed to lock writer: {}", e)))?;

        writer.add_document(doc)?;
        writer.commit()?;

        debug!(message_id = %message.id, "Indexed chat message");
        Ok(())
    }

    /// Index a document chunk with embedding for hybrid search
    pub async fn index_document_chunk(
        &self,
        chunk: &DocumentChunkKv,
        embedding: &[f32],
    ) -> Result<(), SearchError> {
        let mut doc = TantivyDocument::default();
        doc.add_text(self.id_field, &chunk.chunk_id);
        doc.add_text(self.tenant_id_field, &chunk.tenant_id);
        doc.add_text(self.document_type_field, "document_chunk");
        doc.add_text(self.content_field, &chunk.text);
        doc.add_text(self.document_id_field, &chunk.document_id);
        doc.add_i64(self.chunk_index_field, chunk.chunk_index as i64);
        doc.add_i64(
            self.page_number_field,
            chunk.page_number.unwrap_or(0) as i64,
        );
        doc.add_text(self.chunk_hash_field, &chunk.chunk_hash);

        // Serialize vector as JSON string
        let vector_json = serde_json::to_string(embedding)
            .map_err(|e| SearchError::IndexError(format!("Failed to serialize vector: {}", e)))?;
        doc.add_text(self.vector_field, &vector_json);

        // Add empty values for chat message fields
        doc.add_text(self.session_id_field, "");
        doc.add_text(self.role_field, "");
        doc.add_i64(self.timestamp_field, 0);

        let mut writer = self
            .writer
            .lock()
            .map_err(|e| SearchError::LockError(format!("Failed to lock writer: {}", e)))?;

        writer.add_document(doc)?;
        writer.commit()?;

        debug!(chunk_id = %chunk.chunk_id, "Indexed document chunk");
        Ok(())
    }

    /// Full-text search across all indexed content
    pub async fn search_text(
        &self,
        tenant_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<SearchResult>, SearchError> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| SearchError::IndexError(e.to_string()))?;

        let searcher = reader.searcher();

        // Build query with tenant isolation
        let query_parser =
            tantivy::query::QueryParser::for_index(&self.index, vec![self.content_field]);

        let text_query = query_parser.parse_query(query)?;

        // Add tenant filter
        let tenant_query = tantivy::query::TermQuery::new(
            Term::from_field_text(self.tenant_id_field, tenant_id),
            tantivy::schema::IndexRecordOption::Basic,
        );

        let combined_query = tantivy::query::BooleanQuery::new(vec![
            (
                tantivy::query::Occur::Must,
                Box::new(text_query) as Box<dyn tantivy::query::Query>,
            ),
            (
                tantivy::query::Occur::Must,
                Box::new(tenant_query) as Box<dyn tantivy::query::Query>,
            ),
        ]);

        let top_docs = searcher.search(
            &combined_query,
            &tantivy::collector::TopDocs::with_limit(limit),
        )?;

        let mut results = Vec::new();
        for (score, doc_address) in top_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;

            let id = retrieved_doc
                .get_first(self.id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let content = retrieved_doc
                .get_first(self.content_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let document_type = retrieved_doc
                .get_first(self.document_type_field)
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();

            // Create snippet (first 200 chars)
            let snippet = if content.len() > 200 {
                Some(format!("{}...", &content[..200]))
            } else {
                Some(content)
            };

            results.push(SearchResult {
                id,
                score,
                snippet,
                document_type,
            });
        }

        debug!(
            tenant_id,
            query,
            results_count = results.len(),
            "Full-text search completed"
        );
        Ok(results)
    }

    /// Vector similarity search for semantic retrieval
    ///
    /// Note: This is a simplified implementation. For production use with large datasets,
    /// consider integrating with a dedicated vector database like Qdrant or using
    /// Tantivy's experimental vector search features.
    pub async fn search_similar(
        &self,
        tenant_id: &str,
        embedding: &[f32],
        top_k: usize,
    ) -> Result<Vec<VectorSearchResult>, SearchError> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| SearchError::IndexError(e.to_string()))?;

        let searcher = reader.searcher();

        // Filter by tenant and document_chunk type
        let tenant_query = tantivy::query::TermQuery::new(
            Term::from_field_text(self.tenant_id_field, tenant_id),
            tantivy::schema::IndexRecordOption::Basic,
        );

        let type_query = tantivy::query::TermQuery::new(
            Term::from_field_text(self.document_type_field, "document_chunk"),
            tantivy::schema::IndexRecordOption::Basic,
        );

        let filter_query = tantivy::query::BooleanQuery::new(vec![
            (
                tantivy::query::Occur::Must,
                Box::new(tenant_query) as Box<dyn tantivy::query::Query>,
            ),
            (
                tantivy::query::Occur::Must,
                Box::new(type_query) as Box<dyn tantivy::query::Query>,
            ),
        ]);

        // Get all matching documents (we'll compute similarity manually)
        let all_docs = searcher.search(
            &filter_query,
            &tantivy::collector::TopDocs::with_limit(10000), // Adjust based on dataset size
        )?;

        let mut scored_results: Vec<(f32, VectorSearchResult)> = Vec::new();

        for (_score, doc_address) in all_docs {
            let retrieved_doc: TantivyDocument = searcher.doc(doc_address)?;

            // Parse stored vector
            let vector_str = retrieved_doc
                .get_first(self.vector_field)
                .and_then(|v| v.as_str())
                .unwrap_or("[]");

            if vector_str.is_empty() || vector_str == "[]" {
                continue;
            }

            let stored_vector: Vec<f32> = serde_json::from_str(vector_str)
                .map_err(|e| SearchError::IndexError(format!("Failed to parse vector: {}", e)))?;

            // Compute cosine similarity
            let similarity = Self::cosine_similarity(embedding, &stored_vector);

            let chunk_id = retrieved_doc
                .get_first(self.id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let document_id = retrieved_doc
                .get_first(self.document_id_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let text_preview = retrieved_doc
                .get_first(self.content_field)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let page_number = retrieved_doc
                .get_first(self.page_number_field)
                .and_then(|v| v.as_i64())
                .filter(|&n| n > 0)
                .map(|n| n as i32);

            scored_results.push((
                similarity,
                VectorSearchResult {
                    chunk_id,
                    document_id,
                    similarity,
                    text_preview: if text_preview.len() > 200 {
                        format!("{}...", &text_preview[..200])
                    } else {
                        text_preview
                    },
                    page_number,
                },
            ));
        }

        // Sort by similarity (descending) with deterministic tie-breaking
        scored_results.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.chunk_id.cmp(&b.1.chunk_id))
        });

        let results: Vec<VectorSearchResult> = scored_results
            .into_iter()
            .take(top_k)
            .map(|(_, result)| result)
            .collect();

        debug!(
            tenant_id,
            results_count = results.len(),
            "Vector similarity search completed"
        );
        Ok(results)
    }

    /// Hybrid search combining FTS and vector similarity with re-ranking
    pub async fn hybrid_search(
        &self,
        tenant_id: &str,
        query: &str,
        query_embedding: &[f32],
        limit: usize,
    ) -> Result<Vec<HybridSearchResult>, SearchError> {
        // Run both searches
        let fts_results = self.search_text(tenant_id, query, limit * 2).await?;
        let vector_results = self
            .search_similar(tenant_id, query_embedding, limit * 2)
            .await?;

        // Combine results with normalized scores
        let mut combined_scores: std::collections::HashMap<
            String,
            (f32, f32, SearchResult, Option<VectorSearchResult>),
        > = std::collections::HashMap::new();

        // Normalize FTS scores
        let max_fts_score = fts_results.iter().map(|r| r.score).fold(0.0f32, f32::max);
        for result in fts_results {
            let normalized_score = if max_fts_score > 0.0 {
                result.score / max_fts_score
            } else {
                0.0
            };
            combined_scores.insert(result.id.clone(), (normalized_score, 0.0, result, None));
        }

        // Normalize vector scores (similarities are already in [0,1])
        for result in vector_results {
            combined_scores
                .entry(result.chunk_id.clone())
                .and_modify(|(_fts, vec, _, vec_result)| {
                    *vec = result.similarity;
                    *vec_result = Some(result.clone());
                })
                .or_insert_with(|| {
                    // Create a dummy SearchResult for vector-only matches
                    let search_result = SearchResult {
                        id: result.chunk_id.clone(),
                        score: 0.0,
                        snippet: Some(result.text_preview.clone()),
                        document_type: "document_chunk".to_string(),
                    };
                    (0.0, result.similarity, search_result, Some(result))
                });
        }

        // Re-rank with weighted combination (60% FTS, 40% vector)
        let fts_weight = 0.6;
        let vector_weight = 0.4;

        let mut hybrid_results: Vec<HybridSearchResult> = combined_scores
            .into_iter()
            .map(
                |(id, (fts_score, vector_score, search_result, vector_result))| {
                    let combined_score = (fts_score * fts_weight) + (vector_score * vector_weight);

                    HybridSearchResult {
                        id: id.clone(),
                        document_id: vector_result.as_ref().map(|v| v.document_id.clone()),
                        score: combined_score,
                        fts_score,
                        vector_score,
                        snippet: search_result.snippet,
                        document_type: search_result.document_type,
                        page_number: vector_result.and_then(|v| v.page_number),
                    }
                },
            )
            .collect();

        // Sort by combined score (descending) with deterministic tie-breaking
        hybrid_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.id.cmp(&b.id))
        });

        hybrid_results.truncate(limit);

        debug!(
            tenant_id,
            query,
            results_count = hybrid_results.len(),
            "Hybrid search completed"
        );
        Ok(hybrid_results)
    }

    /// Rebuild index from KV store backend
    ///
    /// This is a placeholder for integration with the KV backend.
    /// The actual implementation would depend on the KV backend trait.
    pub async fn rebuild_from_kv(&self, _backend: &dyn std::any::Any) -> Result<(), SearchError> {
        warn!("rebuild_from_kv not yet implemented - requires KV backend integration");
        Ok(())
    }

    /// Compute cosine similarity between two vectors
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        if a.len() != b.len() {
            return 0.0;
        }

        let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let magnitude_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let magnitude_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if magnitude_a == 0.0 || magnitude_b == 0.0 {
            return 0.0;
        }

        dot_product / (magnitude_a * magnitude_b)
    }

    /// Delete a document from the index
    pub async fn delete_document(&self, id: &str) -> Result<(), SearchError> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|e| SearchError::LockError(format!("Failed to lock writer: {}", e)))?;

        writer.delete_term(Term::from_field_text(self.id_field, id));
        writer.commit()?;

        debug!(id, "Deleted document from index");
        Ok(())
    }

    /// Optimize the index (merge segments)
    /// Note: Full segment merging requires recreating the index writer.
    /// This is a basic optimization that commits pending changes.
    pub async fn optimize(&self) -> Result<(), SearchError> {
        let mut writer = self
            .writer
            .lock()
            .map_err(|e| SearchError::LockError(format!("Failed to lock writer: {}", e)))?;

        // Commit any pending changes
        writer.commit()?;

        info!("Optimized search index (committed pending changes)");
        Ok(())
    }

    /// Get index statistics
    pub fn stats(&self) -> Result<IndexStats, SearchError> {
        let reader = self
            .index
            .reader_builder()
            .reload_policy(ReloadPolicy::OnCommitWithDelay)
            .try_into()
            .map_err(|e: tantivy::TantivyError| SearchError::IndexError(e.to_string()))?;

        let searcher = reader.searcher();
        let num_docs = searcher.num_docs() as usize;
        let num_segments = searcher.segment_readers().len();

        Ok(IndexStats {
            num_documents: num_docs,
            num_segments,
        })
    }
}

/// Index statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexStats {
    pub num_documents: usize,
    pub num_segments: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_chat_message_indexing() {
        let service = SearchService::open_in_memory().unwrap();

        let message = ChatMessageKv {
            id: "msg_001".to_string(),
            session_id: "session_001".to_string(),
            tenant_id: "tenant_001".to_string(),
            role: "user".to_string(),
            content: "How do I configure the router?".to_string(),
            timestamp: 1234567890,
        };

        service.index_chat_message(&message).await.unwrap();

        let results = service
            .search_text("tenant_001", "router", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "msg_001");
    }

    #[tokio::test]
    async fn test_document_chunk_indexing() {
        let service = SearchService::open_in_memory().unwrap();

        let chunk = DocumentChunkKv {
            chunk_id: "chunk_001".to_string(),
            document_id: "doc_001".to_string(),
            tenant_id: "tenant_001".to_string(),
            chunk_index: 0,
            page_number: Some(1),
            text: "AdapterOS provides K-sparse routing for efficient adapter selection."
                .to_string(),
            chunk_hash: "abc123".to_string(),
        };

        let embedding = vec![0.1; 384]; // Dummy 384-dim embedding

        service
            .index_document_chunk(&chunk, &embedding)
            .await
            .unwrap();

        let results = service
            .search_text("tenant_001", "routing", 10)
            .await
            .unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, "chunk_001");
    }

    #[tokio::test]
    async fn test_vector_similarity() {
        let service = SearchService::open_in_memory().unwrap();

        let embedding1 = vec![1.0, 0.0, 0.0, 0.0];
        let embedding2 = vec![0.9, 0.1, 0.0, 0.0];
        let embedding3 = vec![0.0, 0.0, 1.0, 0.0];

        let chunk1 = DocumentChunkKv {
            chunk_id: "chunk_001".to_string(),
            document_id: "doc_001".to_string(),
            tenant_id: "tenant_001".to_string(),
            chunk_index: 0,
            page_number: Some(1),
            text: "First chunk".to_string(),
            chunk_hash: "hash1".to_string(),
        };

        let chunk2 = DocumentChunkKv {
            chunk_id: "chunk_002".to_string(),
            document_id: "doc_001".to_string(),
            tenant_id: "tenant_001".to_string(),
            chunk_index: 1,
            page_number: Some(1),
            text: "Second chunk".to_string(),
            chunk_hash: "hash2".to_string(),
        };

        service
            .index_document_chunk(&chunk1, &embedding2)
            .await
            .unwrap();
        service
            .index_document_chunk(&chunk2, &embedding3)
            .await
            .unwrap();

        let results = service
            .search_similar("tenant_001", &embedding1, 2)
            .await
            .unwrap();
        assert_eq!(results.len(), 2);
        // chunk_001 should be more similar to embedding1
        assert_eq!(results[0].chunk_id, "chunk_001");
    }

    #[test]
    fn test_cosine_similarity() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let c = vec![0.0, 1.0, 0.0];

        assert!((SearchService::cosine_similarity(&a, &b) - 1.0).abs() < 0.001);
        assert!((SearchService::cosine_similarity(&a, &c)).abs() < 0.001);
    }
}
