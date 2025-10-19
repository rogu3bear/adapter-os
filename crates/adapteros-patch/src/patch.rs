//! Patch representation and operations
//!
//! Defines the core data structures for representing patches and their operations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use chrono::{DateTime, Utc};

/// Represents a complete patch with metadata and operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Patch {
    /// Unique patch identifier
    pub id: String,
    /// Patch metadata
    pub metadata: PatchMetadata,
    /// List of files to be modified
    pub files: Vec<PatchFile>,
    /// Cryptographic signature
    pub signature: Option<String>,
    /// Public key for verification
    pub public_key: Option<String>,
}

/// Metadata associated with a patch
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchMetadata {
    /// Human-readable description
    pub description: String,
    /// Version this patch targets
    pub target_version: String,
    /// Author information
    pub author: String,
    /// Creation timestamp
    pub created_at: DateTime<Utc>,
    /// Patch dependencies
    pub dependencies: Vec<String>,
    /// Policy compliance declarations
    pub policy_declarations: HashMap<String, bool>,
}

/// Individual file patch operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchFile {
    /// File path relative to project root
    pub path: String,
    /// Patch operations for this file
    pub operations: Vec<PatchOperation>,
}

/// Atomic patch operation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatchOperation {
    /// Replace text at specified location
    Replace {
        old_string: String,
        new_string: String,
    },
    /// Insert text at specified location
    Insert {
        position: usize,
        content: String,
    },
    /// Delete text at specified location
    Delete {
        start: usize,
        end: usize,
    },
    /// Move text from one location to another
    Move {
        from_start: usize,
        from_end: usize,
        to_position: usize,
    },
}
