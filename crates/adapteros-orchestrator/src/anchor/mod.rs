//! Document Anchoring for the AARA Lifecycle
//!
//! The anchor module provides document registration and tracking to ensure
//! every training example can be traced back to its source document.
//!
//! # AARA Lifecycle - ANCHOR Phase
//!
//! The anchor phase grounds all training data in verifiable source documents:
//!
//! 1. **Register** - Documents are registered with content hashes
//! 2. **Chunk** - Documents are chunked with position tracking
//! 3. **Synthesize** - Training examples carry full provenance
//! 4. **Verify** - Source hashes enable change detection
//!
//! # Example
//!
//! ```ignore
//! use adapteros_orchestrator::anchor::{DocumentRegistry, RegisteredDocument};
//!
//! let registry = DocumentRegistry::new(db_pool);
//!
//! // Register a document
//! let doc = registry.register_document("api-docs.md", content).await?;
//!
//! // Get document by hash for verification
//! let found = registry.get_by_hash(&doc.content_hash_b3).await?;
//! ```

mod registry;
mod types;

pub use registry::DocumentRegistry;
pub use types::{
    AnchoredChunk, ChangeType, DocumentChunkInfo, RegisteredDocument, SourceChangeEvent,
    SourceDocument,
};
