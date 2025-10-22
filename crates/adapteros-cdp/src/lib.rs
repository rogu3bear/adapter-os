//! Commit Delta Pack (CDP) core types and functionality
use adapteros_core::{AosError, B3Hash, Result};
use adapteros_git::{ChangedSymbol, DiffSummary};
use adapteros_lora_worker::{LinterResult, TestResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// (All structs and impls from mod.rs and metadata.rs will be pasted here)

/// Unique identifier for a Commit Delta Pack
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct CdpId(String);

/// Metadata for a Commit Delta Pack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpMetadata {
    // ... fields ...
}

/// Extract metadata from git repository
pub struct MetadataExtractor {
    // ... fields ...
}

/// A complete Commit Delta Pack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitDeltaPack {
    // ... fields ...
}
