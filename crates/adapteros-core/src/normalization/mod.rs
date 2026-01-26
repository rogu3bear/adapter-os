//! Shared normalization utilities for adapter-os.
//!
//! This crate provides canonical implementations for normalizing repository identifiers,
//! slugs, and extracting metadata. All normalization functions are deterministic and
//! follow consistent conventions across the codebase.

pub mod metadata;
pub mod repo_id;
pub mod repo_slug;

// Re-export commonly used functions at crate root
pub use metadata::{
    extract_repo_identifier_from_metadata, sanitize_optional, sanitize_repo_identifier,
    sanitize_repo_slug,
};
pub use repo_id::{normalize_path_segments, normalize_repo_id};
pub use repo_slug::normalize_repo_slug;
