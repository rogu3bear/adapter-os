//! Corpus and chunk types for document collections

use serde::{Deserialize, Serialize};

/// Type of chunk content
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChunkType {
    /// Text content
    Text,
    /// Code content
    Code,
    /// Structured data (tables, etc.)
    Structured,
}

/// A chunk of content from a document
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    // TODO: Implement chunk fields
}

/// A corpus of documents for retrieval
pub struct Corpus {
    // TODO: Implement corpus
}
