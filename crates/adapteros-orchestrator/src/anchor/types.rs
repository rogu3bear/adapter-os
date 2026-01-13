//! Types for document anchoring

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A source document registered in the system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDocument {
    /// Unique identifier for the document
    pub id: String,
    /// Original file path or URI
    pub path: String,
    /// BLAKE3 hash of the document content
    pub content_hash_b3: String,
    /// Size in bytes
    pub size_bytes: u64,
    /// MIME type if known
    pub mime_type: Option<String>,
    /// When the document was registered
    pub registered_at: DateTime<Utc>,
    /// When the document was last modified (if known)
    pub modified_at: Option<DateTime<Utc>>,
    /// Tenant that owns this document
    pub tenant_id: Option<String>,
    /// Additional metadata
    pub metadata: Option<serde_json::Value>,
}

impl SourceDocument {
    /// Create a new source document
    pub fn new(
        id: impl Into<String>,
        path: impl Into<String>,
        content_hash_b3: impl Into<String>,
        size_bytes: u64,
    ) -> Self {
        Self {
            id: id.into(),
            path: path.into(),
            content_hash_b3: content_hash_b3.into(),
            size_bytes,
            mime_type: None,
            registered_at: Utc::now(),
            modified_at: None,
            tenant_id: None,
            metadata: None,
        }
    }

    /// Builder: set MIME type
    pub fn with_mime_type(mut self, mime_type: impl Into<String>) -> Self {
        self.mime_type = Some(mime_type.into());
        self
    }

    /// Builder: set tenant
    pub fn with_tenant(mut self, tenant_id: impl Into<String>) -> Self {
        self.tenant_id = Some(tenant_id.into());
        self
    }

    /// Builder: set metadata
    pub fn with_metadata(mut self, metadata: serde_json::Value) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// A registered document with verification status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredDocument {
    /// The source document info
    pub document: SourceDocument,
    /// Number of chunks generated from this document
    pub chunk_count: usize,
    /// Number of training examples generated
    pub example_count: usize,
    /// Number of adapters trained on this document
    pub adapter_count: usize,
    /// Whether the document content still matches the registered hash
    pub is_current: bool,
    /// If not current, the new hash
    pub current_hash: Option<String>,
}

/// Information about a chunk within a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunkInfo {
    /// Index of this chunk in the document
    pub chunk_index: usize,
    /// BLAKE3 hash of the chunk content
    pub chunk_hash_b3: String,
    /// Starting line in the source document (1-indexed)
    pub line_start: u32,
    /// Ending line in the source document (1-indexed)
    pub line_end: u32,
    /// Character offset start
    pub char_start: usize,
    /// Character offset end
    pub char_end: usize,
    /// Token count (if tokenized)
    pub token_count: Option<usize>,
}

impl DocumentChunkInfo {
    /// Create chunk info
    pub fn new(
        chunk_index: usize,
        chunk_hash_b3: impl Into<String>,
        line_start: u32,
        line_end: u32,
        char_start: usize,
        char_end: usize,
    ) -> Self {
        Self {
            chunk_index,
            chunk_hash_b3: chunk_hash_b3.into(),
            line_start,
            line_end,
            char_start,
            char_end,
            token_count: None,
        }
    }

    /// Builder: set token count
    pub fn with_token_count(mut self, count: usize) -> Self {
        self.token_count = Some(count);
        self
    }
}

/// A chunk with its anchoring information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnchoredChunk {
    /// The chunk text
    pub text: String,
    /// Parent document ID
    pub document_id: String,
    /// Parent document hash
    pub document_hash_b3: String,
    /// Chunk positioning info
    pub info: DocumentChunkInfo,
}

impl AnchoredChunk {
    /// Create an anchored chunk
    pub fn new(
        text: impl Into<String>,
        document_id: impl Into<String>,
        document_hash_b3: impl Into<String>,
        info: DocumentChunkInfo,
    ) -> Self {
        Self {
            text: text.into(),
            document_id: document_id.into(),
            document_hash_b3: document_hash_b3.into(),
            info,
        }
    }

    /// Convert to ExampleProvenance for synthesis
    pub fn to_provenance(&self, source_file: &str) -> crate::synthesis::ExampleProvenance {
        crate::synthesis::ExampleProvenance::new(source_file, self.info.chunk_index)
            .with_source_hash(&self.document_hash_b3)
            .with_chunk_hash(&self.info.chunk_hash_b3)
            .with_lines(self.info.line_start, self.info.line_end)
            .with_char_range(self.info.char_start, self.info.char_end)
    }
}

/// Event indicating a source document has changed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceChangeEvent {
    /// Document that changed
    pub document_id: String,
    /// Original path
    pub path: String,
    /// Previous content hash
    pub old_hash_b3: String,
    /// New content hash
    pub new_hash_b3: String,
    /// When the change was detected
    pub detected_at: DateTime<Utc>,
    /// Size change in bytes (positive = grew, negative = shrunk)
    pub size_delta: i64,
    /// Affected adapters that were trained on this document
    pub affected_adapter_ids: Vec<String>,
}

impl SourceChangeEvent {
    /// Create a new change event
    pub fn new(
        document_id: impl Into<String>,
        path: impl Into<String>,
        old_hash: impl Into<String>,
        new_hash: impl Into<String>,
    ) -> Self {
        Self {
            document_id: document_id.into(),
            path: path.into(),
            old_hash_b3: old_hash.into(),
            new_hash_b3: new_hash.into(),
            detected_at: Utc::now(),
            size_delta: 0,
            affected_adapter_ids: Vec::new(),
        }
    }

    /// Builder: set size delta
    pub fn with_size_delta(mut self, delta: i64) -> Self {
        self.size_delta = delta;
        self
    }

    /// Builder: set affected adapters
    pub fn with_affected_adapters(mut self, adapter_ids: Vec<String>) -> Self {
        self.affected_adapter_ids = adapter_ids;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_document_builder() {
        let doc = SourceDocument::new("doc-1", "/path/to/doc.md", "abc123", 1024)
            .with_mime_type("text/markdown")
            .with_tenant("tenant-1");

        assert_eq!(doc.id, "doc-1");
        assert_eq!(doc.path, "/path/to/doc.md");
        assert_eq!(doc.content_hash_b3, "abc123");
        assert_eq!(doc.size_bytes, 1024);
        assert_eq!(doc.mime_type, Some("text/markdown".to_string()));
        assert_eq!(doc.tenant_id, Some("tenant-1".to_string()));
    }

    #[test]
    fn test_chunk_info() {
        let chunk = DocumentChunkInfo::new(0, "chunk_hash", 1, 50, 0, 2048).with_token_count(512);

        assert_eq!(chunk.chunk_index, 0);
        assert_eq!(chunk.line_start, 1);
        assert_eq!(chunk.line_end, 50);
        assert_eq!(chunk.token_count, Some(512));
    }

    #[test]
    fn test_anchored_chunk_to_provenance() {
        let info = DocumentChunkInfo::new(2, "chunk_hash", 100, 150, 5000, 7500);
        let chunk = AnchoredChunk::new("chunk text", "doc-1", "doc_hash", info);

        let prov = chunk.to_provenance("docs/api.md");

        assert_eq!(prov.source_file, "docs/api.md");
        assert_eq!(prov.chunk_index, 2);
        assert_eq!(prov.source_hash_b3, "doc_hash");
        assert_eq!(prov.chunk_hash_b3, "chunk_hash");
        assert_eq!(prov.line_start, Some(100));
        assert_eq!(prov.line_end, Some(150));
    }
}
