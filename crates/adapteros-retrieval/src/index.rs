//! Index backends for vector search

use serde::{Deserialize, Serialize};

/// Trait for index backends (flat, HNSW, etc.)
pub trait IndexBackend {
    // TODO: Define index backend interface
}

/// Metadata about an index
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndexMetadata {
    // TODO: Implement index metadata
}

/// Result from a search query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    // TODO: Implement search result
}
