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
    "id, tenant_id, adapter_id, name, hash_b3, rank, alpha, lora_strength, tier, targets_json, acl_json, \
     languages_json, framework, category, scope, framework_id, framework_version, \
     repo_id, commit_sha, intent, current_state, pinned, memory_bytes, last_activated, \
     activation_count, expires_at, load_state, last_loaded_at, aos_file_path, aos_file_hash, \
     adapter_name, tenant_namespace, domain, purpose, revision, parent_id, fork_type, fork_reason, \
     version, lifecycle_state, \
     archived_at, archived_by, archive_reason, purged_at, \
     base_model_id, recommended_for_moe, manifest_schema_version, content_hash_b3, metadata_json, provenance_json, \
     repo_path, codebase_scope, dataset_version_id, registration_timestamp, manifest_hash, \
     adapter_type, base_adapter_id, stream_session_id, versioning_threshold, coreml_package_hash, \
     training_dataset_hash_b3, created_at, updated_at, active";

/// Training dataset table columns for SELECT queries
///
/// Used in get_training_dataset, list_training_datasets, and related operations.
pub const TRAINING_DATASET_COLUMNS: &str =
    "id, name, description, file_count, total_size_bytes, format, hash_b3, dataset_hash_b3, \
     storage_path, status, validation_status, validation_errors, metadata_json, \
     created_by, created_at, updated_at, dataset_type, purpose, \
     source_location, collection_method, ownership, tenant_id, workspace_id, \
     hash_needs_recompute, hash_algorithm_version, repo_slug, branch, commit_sha, \
     session_id, session_name, session_tags, \
     scope_repo_id, scope_repo, scope_scan_root, scope_remote_url, \
     scan_root_count, total_scan_root_files, total_scan_root_bytes, \
     scan_roots_content_hash, scan_roots_updated_at";

/// Dataset file table columns for SELECT queries
pub const DATASET_FILE_COLUMNS: &str =
    "id, dataset_id, file_name, file_path, size_bytes, hash_b3, mime_type, created_at";

/// Evidence entry table columns for SELECT queries
pub const EVIDENCE_ENTRY_COLUMNS: &str =
    "id, dataset_id, adapter_id, evidence_type, reference, description, \
     confidence, created_by, created_at, metadata_json";

/// Dataset-adapter link table columns for SELECT queries
pub const DATASET_ADAPTER_LINK_COLUMNS: &str = "id, dataset_id, adapter_id, link_type, created_at";

/// Dataset scan root table columns for SELECT queries
///
/// Stores scan root metadata for datasets created via code ingestion.
/// Each row represents a directory that was scanned to generate training data.
pub const DATASET_SCAN_ROOT_COLUMNS: &str = "id, dataset_id, dataset_version_id, session_id, \
     path, label, file_count, byte_count, content_hash_b3, scanned_at, ordinal, \
     repo_name, repo_slug, commit_sha, branch, remote_url, \
     hkdf_algorithm_version, parser_algorithm_version, path_normalization_version, codegraph_version, \
     tenant_id, created_at, created_by, metadata_json";

/// Codebase dataset row table columns for SELECT queries
///
/// Stores individual Q&A training examples extracted from code during scan-root runs.
/// Each row represents one prompt/response pair with full provenance tracking.
pub const CODEBASE_DATASET_ROW_COLUMNS: &str = "id, dataset_id, dataset_version_id, session_id, \
     prompt, response, weight, sample_role, \
     symbol_kind, language, file_path, start_line, end_line, qualified_name, \
     commit_sha, repo_name, repo_slug, repo_identifier, project_name, \
     has_docstring, content_hash_b3, metadata_json, tenant_id, created_at";

/// Training dataset row table columns for SELECT queries
///
/// Stores general-purpose prompt/response training pairs for datasets created from
/// uploads, synthetic generation, or other non-codebase sources.
/// Maps to CanonicalRow API format for compatibility with training workers.
pub const TRAINING_DATASET_ROW_COLUMNS: &str = "id, dataset_id, dataset_version_id, session_id, \
     prompt, response, weight, split, sample_role, content_hash_b3, \
     source_type, source_file, source_line, tenant_id, metadata_json, created_at, created_by";

/// Chat tag table columns for SELECT queries
pub const CHAT_TAG_COLUMNS: &str =
    "id, tenant_id, name, color, description, created_at, created_by";

/// Document collection table columns for SELECT queries
pub const COLLECTION_COLUMNS: &str =
    "id, tenant_id, name, description, created_at, updated_at, metadata_json";

/// Training job dataset link table columns for SELECT queries
///
/// Used for querying the many-to-many relationship between training jobs and datasets.
/// Evidence: migrations/0241_training_job_datasets.sql
pub const TRAINING_JOB_DATASET_LINK_COLUMNS: &str =
    "id, training_job_id, dataset_id, dataset_version_id, role, ordinal, weight, \
     hash_b3_at_link, tenant_id, created_at, created_by, metadata_json";

/// Adapter training lineage table columns for SELECT queries
///
/// Used for reverse lookups from dataset versions to trained adapters.
/// Enables "which adapters were trained on this dataset?" queries.
/// Evidence: migrations/0258_adapter_training_lineage.sql
pub const ADAPTER_TRAINING_LINEAGE_COLUMNS: &str =
    "id, adapter_id, dataset_id, dataset_version_id, training_job_id, \
     dataset_hash_b3_at_training, role, weight, ordinal, \
     tenant_id, created_at, created_by, metadata_json";

/// KV backend degradation event types
///
/// Used for logging and monitoring degradation events in the KV backend.
pub const DEGRADATION_EVENT_INIT_FAILED: &str = "kv_init_failed";
pub const DEGRADATION_EVENT_RUNTIME_FAILED: &str = "kv_runtime_failed";
pub const DEGRADATION_EVENT_RECOVERED: &str = "kv_recovered";
pub const DEGRADATION_EVENT_KV_UNSUPPORTED: &str = "kv_unsupported_mode";

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
        assert!(!DATASET_SCAN_ROOT_COLUMNS.is_empty());
        assert!(!CODEBASE_DATASET_ROW_COLUMNS.is_empty());
        assert!(!TRAINING_DATASET_ROW_COLUMNS.is_empty());
        assert!(!CHAT_TAG_COLUMNS.is_empty());
        assert!(!COLLECTION_COLUMNS.is_empty());
        assert!(!TRAINING_JOB_DATASET_LINK_COLUMNS.is_empty());
        assert!(!ADAPTER_TRAINING_LINEAGE_COLUMNS.is_empty());
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
        assert!(!DATASET_SCAN_ROOT_COLUMNS.trim().ends_with(','));
        assert!(!CODEBASE_DATASET_ROW_COLUMNS.trim().ends_with(','));
        assert!(!TRAINING_DATASET_ROW_COLUMNS.trim().ends_with(','));
        assert!(!TRAINING_JOB_DATASET_LINK_COLUMNS.trim().ends_with(','));
        assert!(!ADAPTER_TRAINING_LINEAGE_COLUMNS.trim().ends_with(','));
    }
}
