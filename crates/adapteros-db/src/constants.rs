//! Database query constants
//!
//! Centralized definitions of SELECT column lists to prevent duplication
//! and ensure consistency across the codebase.
//!
//! # Usage
//!
//! ```ignore
//! use crate::constants::TRAINING_DATASET_COLUMNS;
//!
//! let query = format!(
//!     "SELECT {} FROM training_datasets WHERE id = ?",
//!     TRAINING_DATASET_COLUMNS
//! );
//! ```

/// Adapter table columns for SELECT queries
///
/// Used across adapter listing, detail, and search operations.
pub const ADAPTER_COLUMNS: &str =
    "id, tenant_id, adapter_id, name, hash_b3, rank, alpha, tier, targets_json, acl_json, \
     languages_json, framework, category, scope, framework_id, framework_version, \
     repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated, \
     activation_count, expires_at, load_state, last_loaded_at, aos_file_path, aos_file_hash, \
     adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, \
     version, lifecycle_state, \
     archived_at, archived_by, archive_reason, purged_at, \
     base_model_id, manifest_schema_version, content_hash_b3, provenance_json, \
     created_at, updated_at, active";

/// Training dataset table columns for SELECT queries
///
/// Used in get_training_dataset, list_training_datasets, and related operations.
pub const TRAINING_DATASET_COLUMNS: &str =
    "id, name, description, file_count, total_size_bytes, format, hash_b3, \
     storage_path, validation_status, validation_errors, metadata_json, \
     created_by, created_at, updated_at, dataset_type, purpose, \
     source_location, collection_method, ownership, tenant_id";

/// Dataset file table columns for SELECT queries
pub const DATASET_FILE_COLUMNS: &str =
    "id, dataset_id, file_name, file_path, size_bytes, hash_b3, mime_type, created_at";

/// Evidence entry table columns for SELECT queries
pub const EVIDENCE_ENTRY_COLUMNS: &str =
    "id, dataset_id, adapter_id, evidence_type, reference, description, \
     confidence, created_by, created_at, metadata_json";

/// Dataset-adapter link table columns for SELECT queries
pub const DATASET_ADAPTER_LINK_COLUMNS: &str = "id, dataset_id, adapter_id, link_type, created_at";

/// Chat tag table columns for SELECT queries
pub const CHAT_TAG_COLUMNS: &str =
    "id, tenant_id, name, color, description, created_at, created_by";

/// Document collection table columns for SELECT queries
pub const COLLECTION_COLUMNS: &str =
    "id, tenant_id, name, description, created_at, updated_at, metadata_json";

/// KV backend degradation event types
///
/// Used for logging and monitoring degradation events in the KV backend.
pub const DEGRADATION_EVENT_INIT_FAILED: &str = "kv_init_failed";
pub const DEGRADATION_EVENT_RUNTIME_FAILED: &str = "kv_runtime_failed";
pub const DEGRADATION_EVENT_RECOVERED: &str = "kv_recovered";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants_not_empty() {
        assert!(!ADAPTER_COLUMNS.is_empty());
        assert!(!TRAINING_DATASET_COLUMNS.is_empty());
        assert!(!DATASET_FILE_COLUMNS.is_empty());
        assert!(!EVIDENCE_ENTRY_COLUMNS.is_empty());
        assert!(!DATASET_ADAPTER_LINK_COLUMNS.is_empty());
        assert!(!CHAT_TAG_COLUMNS.is_empty());
        assert!(!COLLECTION_COLUMNS.is_empty());
        assert!(!DEGRADATION_EVENT_INIT_FAILED.is_empty());
        assert!(!DEGRADATION_EVENT_RUNTIME_FAILED.is_empty());
        assert!(!DEGRADATION_EVENT_RECOVERED.is_empty());
    }

    #[test]
    fn test_constants_no_trailing_comma() {
        // Ensure constants don't end with comma which would cause SQL errors
        assert!(!ADAPTER_COLUMNS.trim().ends_with(','));
        assert!(!TRAINING_DATASET_COLUMNS.trim().ends_with(','));
        assert!(!DATASET_FILE_COLUMNS.trim().ends_with(','));
    }
}
