//! Document registry for anchoring training data to source documents

use super::types::{
    AnchoredChunk, ChangeType, DocumentChunkInfo, RegisteredDocument, SourceChangeEvent,
    SourceDocument,
};
use adapteros_core::{AosError, B3Hash, Result};
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Registry for managing source documents and their relationships to training data
///
/// The DocumentRegistry provides:
/// - Document registration with content hashing
/// - Chunk tracking with position information
/// - Change detection for document updates
/// - Reverse lookups from adapters to source documents
pub struct DocumentRegistry {
    /// In-memory cache of registered documents (by ID)
    documents: Arc<RwLock<HashMap<String, SourceDocument>>>,
    /// Index from content hash to document ID
    hash_index: Arc<RwLock<HashMap<String, String>>>,
    /// Chunks indexed by document ID
    chunks: Arc<RwLock<HashMap<String, Vec<DocumentChunkInfo>>>>,
    /// Document ID to adapter IDs mapping
    document_adapters: Arc<RwLock<HashMap<String, Vec<String>>>>,
}

impl DocumentRegistry {
    /// Create a new document registry
    pub fn new() -> Self {
        Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
            hash_index: Arc::new(RwLock::new(HashMap::new())),
            chunks: Arc::new(RwLock::new(HashMap::new())),
            document_adapters: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a document from its content
    ///
    /// Computes the BLAKE3 hash and stores the document metadata.
    /// Returns the registered document with its hash.
    pub async fn register_document(
        &self,
        path: &str,
        content: &[u8],
        tenant_id: Option<&str>,
    ) -> Result<SourceDocument> {
        // Compute content hash
        let content_hash = B3Hash::hash(content).to_hex();
        let size_bytes = content.len() as u64;

        // Generate document ID from hash prefix + timestamp
        let doc_id = format!(
            "doc-{}-{}",
            &content_hash[..8],
            Utc::now().timestamp_millis()
        );

        // Check if already registered by hash
        {
            let hash_idx = self.hash_index.read().await;
            if let Some(existing_id) = hash_idx.get(&content_hash) {
                let docs = self.documents.read().await;
                if let Some(existing) = docs.get(existing_id) {
                    debug!(
                        path = path,
                        hash = &content_hash[..16],
                        "Document already registered"
                    );
                    return Ok(existing.clone());
                }
            }
        }

        // Create new document
        let mut doc = SourceDocument::new(&doc_id, path, &content_hash, size_bytes);

        // Detect MIME type from path
        if path.ends_with(".md") || path.ends_with(".markdown") {
            doc = doc.with_mime_type("text/markdown");
        } else if path.ends_with(".pdf") {
            doc = doc.with_mime_type("application/pdf");
        } else if path.ends_with(".txt") {
            doc = doc.with_mime_type("text/plain");
        } else if path.ends_with(".rs") {
            doc = doc.with_mime_type("text/x-rust");
        } else if path.ends_with(".py") {
            doc = doc.with_mime_type("text/x-python");
        }

        if let Some(tid) = tenant_id {
            doc = doc.with_tenant(tid);
        }

        // Store document
        {
            let mut docs = self.documents.write().await;
            let mut hash_idx = self.hash_index.write().await;
            docs.insert(doc_id.clone(), doc.clone());
            hash_idx.insert(content_hash.clone(), doc_id.clone());
        }

        info!(
            doc_id = &doc_id,
            path = path,
            hash = &content_hash[..16],
            size_bytes = size_bytes,
            "Document registered"
        );

        Ok(doc)
    }

    /// Get a document by its ID
    pub async fn get_document(&self, doc_id: &str) -> Option<SourceDocument> {
        let docs = self.documents.read().await;
        docs.get(doc_id).cloned()
    }

    /// Get a document by its content hash
    pub async fn get_by_hash(&self, content_hash: &str) -> Option<SourceDocument> {
        let hash_idx = self.hash_index.read().await;
        if let Some(doc_id) = hash_idx.get(content_hash) {
            let docs = self.documents.read().await;
            return docs.get(doc_id).cloned();
        }
        None
    }

    /// Register chunks for a document
    pub async fn register_chunks(
        &self,
        doc_id: &str,
        chunk_infos: Vec<DocumentChunkInfo>,
    ) -> Result<()> {
        // Verify document exists
        {
            let docs = self.documents.read().await;
            if !docs.contains_key(doc_id) {
                return Err(AosError::not_found(format!(
                    "Document not found: {}",
                    doc_id
                )));
            }
        }

        let chunk_count = chunk_infos.len();
        {
            let mut chunks = self.chunks.write().await;
            chunks.insert(doc_id.to_string(), chunk_infos);
        }

        debug!(
            doc_id = doc_id,
            chunk_count = chunk_count,
            "Chunks registered"
        );

        Ok(())
    }

    /// Get chunks for a document
    pub async fn get_chunks(&self, doc_id: &str) -> Vec<DocumentChunkInfo> {
        let chunks = self.chunks.read().await;
        chunks.get(doc_id).cloned().unwrap_or_default()
    }

    /// Create anchored chunks from document content
    ///
    /// This is a convenience method that:
    /// 1. Registers the document
    /// 2. Chunks the content
    /// 3. Registers the chunks
    /// 4. Returns anchored chunks ready for synthesis
    pub async fn anchor_document(
        &self,
        path: &str,
        content: &str,
        chunk_size: usize,
        overlap: usize,
        tenant_id: Option<&str>,
    ) -> Result<Vec<AnchoredChunk>> {
        // Register the document
        let doc = self
            .register_document(path, content.as_bytes(), tenant_id)
            .await?;

        // Simple line-based chunking with overlap
        let lines: Vec<&str> = content.lines().collect();
        let mut chunks = Vec::new();
        let mut chunk_infos = Vec::new();

        let lines_per_chunk = chunk_size.max(1);
        let overlap_lines = overlap.min(lines_per_chunk - 1);

        let mut line_idx = 0;
        let mut chunk_idx = 0;
        let mut char_offset = 0;

        while line_idx < lines.len() {
            let chunk_start_line = line_idx;
            let chunk_end_line = (line_idx + lines_per_chunk).min(lines.len());

            // Build chunk text
            let chunk_lines: Vec<&str> = lines[chunk_start_line..chunk_end_line].to_vec();
            let chunk_text = chunk_lines.join("\n");

            // Calculate character offsets
            let char_start = char_offset;
            let char_end = char_start + chunk_text.len();

            // Hash the chunk
            let chunk_hash = B3Hash::hash(chunk_text.as_bytes()).to_hex();

            // Create chunk info
            let info = DocumentChunkInfo::new(
                chunk_idx,
                &chunk_hash,
                (chunk_start_line + 1) as u32, // 1-indexed
                chunk_end_line as u32,
                char_start,
                char_end,
            );

            chunk_infos.push(info.clone());

            // Create anchored chunk
            chunks.push(AnchoredChunk::new(
                chunk_text.clone(),
                &doc.id,
                &doc.content_hash_b3,
                info,
            ));

            if chunk_end_line >= lines.len() {
                break;
            }

            // Move to next chunk with overlap
            line_idx = chunk_end_line.saturating_sub(overlap_lines);
            if line_idx <= chunk_start_line {
                line_idx = chunk_end_line; // Avoid infinite loop
            }
            char_offset = char_end + 1; // +1 for newline
            chunk_idx += 1;
        }

        // Register the chunks
        self.register_chunks(&doc.id, chunk_infos).await?;

        info!(
            doc_id = &doc.id,
            path = path,
            chunk_count = chunks.len(),
            "Document anchored"
        );

        Ok(chunks)
    }

    /// Link an adapter to a document
    pub async fn link_adapter(&self, doc_id: &str, adapter_id: &str) -> Result<()> {
        let mut links = self.document_adapters.write().await;
        links
            .entry(doc_id.to_string())
            .or_default()
            .push(adapter_id.to_string());

        debug!(
            doc_id = doc_id,
            adapter_id = adapter_id,
            "Adapter linked to document"
        );

        Ok(())
    }

    /// Get adapters trained on a document
    pub async fn get_adapters_for_document(&self, doc_id: &str) -> Vec<String> {
        let links = self.document_adapters.read().await;
        links.get(doc_id).cloned().unwrap_or_default()
    }

    /// Check if a document has changed
    ///
    /// Compares the stored hash with the current content hash.
    /// Returns a SourceChangeEvent if the document has changed.
    pub async fn detect_change(
        &self,
        doc_id: &str,
        current_content: &[u8],
    ) -> Result<Option<SourceChangeEvent>> {
        let doc = self
            .get_document(doc_id)
            .await
            .ok_or_else(|| AosError::not_found(format!("Document not found: {}", doc_id)))?;

        let current_hash = B3Hash::hash(current_content).to_hex();

        if current_hash == doc.content_hash_b3 {
            return Ok(None); // No change
        }

        // Document has changed
        let size_delta = current_content.len() as i64 - doc.size_bytes as i64;
        let affected_adapters = self.get_adapters_for_document(doc_id).await;

        let event = SourceChangeEvent::new(
            &doc.id,
            &doc.path,
            &doc.content_hash_b3,
            &current_hash,
            ChangeType::Modified,
        )
        .with_size_delta(size_delta)
        .with_affected_adapters(affected_adapters);

        warn!(
            doc_id = doc_id,
            old_hash = &doc.content_hash_b3[..16],
            new_hash = &current_hash[..16],
            size_delta = size_delta,
            "Document change detected"
        );

        Ok(Some(event))
    }

    /// Get registration info for a document including counts
    pub async fn get_registration_info(&self, doc_id: &str) -> Option<RegisteredDocument> {
        let doc = self.get_document(doc_id).await?;

        let chunks = self.chunks.read().await;
        let chunk_count = chunks.get(doc_id).map(|c| c.len()).unwrap_or(0);

        let adapters = self.document_adapters.read().await;
        let adapter_count = adapters.get(doc_id).map(|a| a.len()).unwrap_or(0);

        // Estimate example count (typically 3-5 examples per chunk)
        let example_count = chunk_count * 4;

        Some(RegisteredDocument {
            document: doc,
            chunk_count,
            example_count,
            adapter_count,
            is_current: true, // Would need actual verification
            current_hash: None,
        })
    }

    /// List all registered documents
    pub async fn list_documents(&self) -> Vec<SourceDocument> {
        let docs = self.documents.read().await;
        docs.values().cloned().collect()
    }

    /// Count registered documents
    pub async fn document_count(&self) -> usize {
        let docs = self.documents.read().await;
        docs.len()
    }
}

impl Default for DocumentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_register_document() {
        let registry = DocumentRegistry::new();
        let content = b"# Test Document\n\nThis is test content.";

        let doc = registry
            .register_document("test.md", content, Some("tenant-1"))
            .await
            .unwrap();

        assert!(doc.id.starts_with("doc-"));
        assert_eq!(doc.path, "test.md");
        assert_eq!(doc.mime_type, Some("text/markdown".to_string()));
        assert_eq!(doc.tenant_id, Some("tenant-1".to_string()));
        assert_eq!(doc.size_bytes, content.len() as u64);
    }

    #[tokio::test]
    async fn test_duplicate_registration() {
        let registry = DocumentRegistry::new();
        let content = b"Same content";

        let doc1 = registry
            .register_document("file1.txt", content, None)
            .await
            .unwrap();
        let doc2 = registry
            .register_document("file2.txt", content, None)
            .await
            .unwrap();

        // Same content should return same document
        assert_eq!(doc1.id, doc2.id);
        assert_eq!(doc1.content_hash_b3, doc2.content_hash_b3);
    }

    #[tokio::test]
    async fn test_get_by_hash() {
        let registry = DocumentRegistry::new();
        let content = b"Test content for hash lookup";

        let doc = registry
            .register_document("test.txt", content, None)
            .await
            .unwrap();

        let found = registry.get_by_hash(&doc.content_hash_b3).await;
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, doc.id);
    }

    #[tokio::test]
    async fn test_anchor_document() {
        let registry = DocumentRegistry::new();
        let content =
            "Line 1\nLine 2\nLine 3\nLine 4\nLine 5\nLine 6\nLine 7\nLine 8\nLine 9\nLine 10";

        let chunks = registry
            .anchor_document("test.txt", content, 5, 1, None)
            .await
            .unwrap();

        assert!(!chunks.is_empty());
        assert_eq!(chunks.len(), 3);

        // First chunk should start at line 1
        assert_eq!(chunks[0].info.line_start, 1);

        // Check provenance conversion
        let prov = chunks[0].to_provenance("test.txt");
        assert_eq!(prov.source_file, "test.txt");
        assert_eq!(prov.chunk_index, 0);
        assert!(!prov.source_hash_b3.is_empty());
    }

    #[tokio::test]
    async fn test_detect_change() {
        let registry = DocumentRegistry::new();
        let original = b"Original content";
        let modified = b"Modified content!";

        let doc = registry
            .register_document("test.txt", original, None)
            .await
            .unwrap();

        // No change with same content
        let no_change = registry.detect_change(&doc.id, original).await.unwrap();
        assert!(no_change.is_none());

        // Change detected with different content
        let change = registry.detect_change(&doc.id, modified).await.unwrap();
        assert!(change.is_some());

        let event = change.unwrap();
        assert_eq!(event.document_id, doc.id);
        assert_eq!(event.old_hash_b3, doc.content_hash_b3);
        assert_ne!(event.new_hash_b3, doc.content_hash_b3);
    }

    #[tokio::test]
    async fn test_adapter_linking() {
        let registry = DocumentRegistry::new();
        let content = b"Content";

        let doc = registry
            .register_document("test.txt", content, None)
            .await
            .unwrap();

        registry.link_adapter(&doc.id, "adapter-1").await.unwrap();
        registry.link_adapter(&doc.id, "adapter-2").await.unwrap();

        let adapters = registry.get_adapters_for_document(&doc.id).await;
        assert_eq!(adapters.len(), 2);
        assert!(adapters.contains(&"adapter-1".to_string()));
        assert!(adapters.contains(&"adapter-2".to_string()));
    }
}
