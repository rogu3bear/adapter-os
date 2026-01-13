use serde::{Deserialize, Serialize};
use crate::adapteros_core::{Result, AosError, AdapterName};
use std::collections::HashMap;
use serde_json::Value;
use sqlx::FromRow;
use crate::adapters::aos_parser::{AosRegistrationMetadata};
use std::path::Path;

pub struct Adapter {
    // Core fields (from migration 0001)
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub tier: String, // TEXT enum: 'persistent', 'warm', 'ephemeral'
    pub hash_b3: String,
    pub rank: i32,
    pub alpha: f64,                 // LoRA alpha parameter (usually rank * 2)
    pub lora_strength: Option<f32>, // LoRA strength multiplier [0.0,1.0]
    pub targets_json: String,       // JSON array of target modules
    pub acl_json: Option<String>,   // Access control list
    pub adapter_id: Option<String>, // External adapter ID for lookups
    pub languages_json: Option<String>,
    pub framework: Option<String>,
    pub active: i32,

    // Code intelligence fields (from migration 0012)
    pub category: String,
    pub scope: String,
    pub framework_id: Option<String>,
    pub framework_version: Option<String>,
    pub repo_id: Option<String>,
    pub commit_sha: Option<String>,
    pub intent: Option<String>,

    // Lifecycle state management (from migration 0012)
    pub current_state: String,
    pub pinned: i32,
    pub memory_bytes: i64,
    pub last_activated: Option<String>,
    pub activation_count: i64,

    // Expiration (from migration 0044)
    pub expires_at: Option<String>,

    // Runtime load state (from migration 0031)
    pub load_state: String,
    pub last_loaded_at: Option<String>,

    // .aos file support (from migration 0045)
    pub aos_file_path: Option<String>,
    pub aos_file_hash: Option<String>,

    // Semantic naming (from migration 0061)
    pub adapter_name: Option<String>,
    pub tenant_namespace: Option<String>,
    pub domain: Option<String>,
    pub purpose: Option<String>,
    pub revision: Option<String>,
    pub parent_id: Option<String>,
    pub fork_type: Option<String>,
    pub fork_reason: Option<String>,

    // Metadata normalization (from migration 0068)
    pub version: String,         // Semantic version or monotonic
    pub lifecycle_state: String, // draft/training/ready/active/deprecated/retired/failed

    // Archive/GC fields (from migration 0138)
    pub archived_at: Option<String>,    // When adapter was archived
    pub archived_by: Option<String>,    // User/system that initiated archive
    pub archive_reason: Option<String>, // Reason for archival (e.g., "tenant_archived")
    pub purged_at: Option<String>,      // When .aos file was deleted by GC

    // Base model reference (from migration 0098)
    pub base_model_id: Option<String>,
    #[sqlx(default)]
    pub recommended_for_moe: Option<bool>,
    // Artifact hardening (from migration 0153)
    pub manifest_schema_version: Option<String>,
    pub content_hash_b3: Option<String>,
    #[sqlx(default)]
    pub metadata_json: Option<String>,
    pub provenance_json: Option<String>,

    // Scan root path (from migration 0243)
    #[sqlx(default)]
    pub repo_path: Option<String>,

    // Drift tracking fields (for CLI diagnostics)
    #[sqlx(default)]
    pub drift_tier: Option<String>,
    #[sqlx(default)]
    pub drift_metric: Option<f64>,
    #[sqlx(default)]
    pub drift_loss_metric: Option<f64>,
    #[sqlx(default)]
    pub drift_reference_backend: Option<String>,
    #[sqlx(default)]
    pub drift_baseline_backend: Option<String>,
    #[sqlx(default)]
    pub drift_test_backend: Option<String>,

    // Codebase adapter registration metadata (from migration 0231)
    /// Source repository/codebase reference for codebase adapters
    #[sqlx(default)]
    pub codebase_scope: Option<String>,
    /// Training dataset version ID for reproducibility
    #[sqlx(default)]
    pub dataset_version_id: Option<String>,
    /// ISO8601 timestamp when adapter was registered
    #[sqlx(default)]
    pub registration_timestamp: Option<String>,
    /// BLAKE3 hash of the adapter manifest for integrity verification
    #[sqlx(default)]
    pub manifest_hash: Option<String>,

    // Codebase adapter type and stream binding (from migration 0261)
    /// Adapter classification: "standard" (portable), "codebase" (stream-scoped), "core" (baseline)
    #[sqlx(default)]
    pub adapter_type: Option<String>,
    /// Base adapter ID for codebase adapters (the core adapter they extend as delta)
    /// Distinct from parent_id which tracks version lineage
    #[sqlx(default)]
    pub base_adapter_id: Option<String>,
    /// Exclusive session binding for codebase adapters
    #[sqlx(default)]
    pub stream_session_id: Option<String>,
    /// Activation threshold for auto-versioning (default: 100)
    #[sqlx(default)]
    pub versioning_threshold: Option<i32>,
    /// BLAKE3 hash of fused CoreML package for deployment verification
    #[sqlx(default)]
    pub coreml_package_hash: Option<String>,
    /// BLAKE3 hash of training dataset content at training time.
    /// Used for receipt generation and lineage verification.
    /// (from migration 0282)
    #[sqlx(default)]
    pub training_dataset_hash_b3: Option<String>,

    pub created_at: String,
    pub updated_at: String,
}

pub struct AdapterActivation {
    pub id: String,
    pub adapter_id: String,
    pub request_id: Option<String>,
    pub gate_value: f64,
    pub selected: i32,
    pub created_at: String,
}

pub struct AdapterFileMetadata {
    /// Adapter ID (links to adapters table)
    pub adapter_id: String,
    /// Absolute path to the .aos file
    pub aos_file_path: String,
    /// BLAKE3 hash of the .aos file for integrity verification
    pub aos_file_hash: String,
    /// Path to extracted weights (if applicable)
    #[sqlx(default)]
    pub extracted_weights_path: Option<String>,
    /// Number of training examples used
    #[sqlx(default)]
    pub training_data_count: Option<i64>,
    /// Lineage version string for tracking
    #[sqlx(default)]
    pub lineage_version: Option<String>,
    /// Whether cryptographic signature is valid
    #[sqlx(default)]
    pub signature_valid: Option<bool>,
    /// File size in bytes
    #[sqlx(default)]
    pub file_size_bytes: Option<i64>,
    /// File modification timestamp (ISO 8601)
    #[sqlx(default)]
    pub file_modified_at: Option<String>,
    /// Number of segments in the .aos file
    #[sqlx(default)]
    pub segment_count: Option<i64>,
    /// Manifest schema version
    #[sqlx(default)]
    pub manifest_schema_version: Option<String>,
    /// Base model identifier
    #[sqlx(default)]
    pub base_model: Option<String>,
    /// Adapter category
    #[sqlx(default)]
    pub category: Option<String>,
    /// Adapter tier (ephemeral, warm, persistent)
    #[sqlx(default)]
    pub tier: Option<String>,
    /// Creation timestamp
    pub created_at: String,
    /// Last update timestamp
    pub updated_at: String,
}

pub struct StoreAdapterFileMetadataParams {
    /// Adapter ID (required)
    pub adapter_id: String,
    /// Absolute path to the .aos file (required)
    pub aos_file_path: String,
    /// BLAKE3 hash of the .aos file (required)
    pub aos_file_hash: String,
    /// Path to extracted weights
    pub extracted_weights_path: Option<String>,
    /// Number of training examples
    pub training_data_count: Option<i64>,
    /// Lineage version
    pub lineage_version: Option<String>,
    /// Signature validity
    pub signature_valid: Option<bool>,
    /// File size in bytes
    pub file_size_bytes: Option<i64>,
    /// File modification timestamp (ISO 8601)
    pub file_modified_at: Option<String>,
    /// Number of segments in the .aos file
    pub segment_count: Option<i64>,
    /// Manifest schema version
    pub manifest_schema_version: Option<String>,
    /// Base model identifier
    pub base_model: Option<String>,
    /// Adapter category
    pub category: Option<String>,
    /// Adapter tier
    pub tier: Option<String>,
}

impl StoreAdapterFileMetadataParams {
    /// Create new parameters with required fields
    pub fn new(
        adapter_id: impl Into<String>,
        aos_file_path: impl Into<String>,
        aos_file_hash: impl Into<String>,
    ) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            aos_file_path: aos_file_path.into(),
            aos_file_hash: aos_file_hash.into(),
            ..Default::default()
        }

pub struct AosMetadataUpdate {
    /// Adapter ID (required)
    pub adapter_id: String,
    /// Tenant ID (required for dual-write)
    pub tenant_id: String,
    /// Path to the .aos file
    pub aos_file_path: Option<String>,
    /// BLAKE3 hash of the .aos file
    pub aos_file_hash: Option<String>,
    /// Metadata from the .aos manifest (stored as metadata_json)
    pub manifest_metadata: Option<std::collections::HashMap<String, String>>,
    /// Base model identifier from manifest
    pub base_model_id: Option<String>,
    /// Manifest schema version
    pub manifest_schema_version: Option<String>,
    /// Content hash (BLAKE3 of manifest + weights)
    pub content_hash_b3: Option<String>,
    /// Full training provenance JSON
    pub provenance_json: Option<String>,
}

impl AosMetadataUpdate {
    /// Create a new AosMetadataUpdate with required fields
    pub fn new(adapter_id: impl Into<String>, tenant_id: impl Into<String>) -> Self {
        Self {
            adapter_id: adapter_id.into(),
            tenant_id: tenant_id.into(),
            ..Default::default()
        }

pub struct AdapterMetadataPatch {
    pub aos_file_path: Option<String>,
    pub aos_file_hash: Option<String>,
    pub base_model_id: Option<String>,
    pub manifest_schema_version: Option<String>,
    pub content_hash_b3: Option<String>,
    pub metadata_json: Option<String>,
    pub provenance_json: Option<String>,
    pub repo_path: Option<String>,
    pub codebase_scope: Option<String>,
    pub dataset_version_id: Option<String>,
    pub registration_timestamp: Option<String>,
    pub manifest_hash: Option<String>,
    // Codebase adapter type and stream binding (from migration 0261)
    pub adapter_type: Option<String>,
    pub base_adapter_id: Option<String>,
    pub stream_session_id: Option<String>,
    pub versioning_threshold: Option<i32>,
    pub coreml_package_hash: Option<String>,
}

impl AdapterMetadataPatch {
    fn from_params(params: &AdapterRegistrationParams) -> Self {
        fn sanitize(value: Option<&str>) -> Option<String> {
            value
                .map(str::trim)
                .filter(|v| !v.is_empty())
                .map(|v| v.to_string())
        }

pub struct AdapterAliasUpdate {
    pub adapter_name: Option<String>,
    pub tenant_namespace: Option<String>,
    pub domain: Option<String>,
    pub purpose: Option<String>,
    pub revision: Option<String>,
}

impl AdapterAliasUpdate {
    fn from_alias(alias: Option<&str>) -> Result<Self> {
        let trimmed = alias.map(str::trim).filter(|value| !value.is_empty());
        if let Some(alias) = trimmed {
            let parsed = AdapterName::parse(alias)?;
            return Ok(Self {
                adapter_name: Some(parsed.to_string()),
                tenant_namespace: Some(parsed.tenant().to_string()),
                domain: Some(parsed.domain().to_string()),
                purpose: Some(parsed.purpose().to_string()),
                revision: Some(parsed.revision().to_string()),
            }

pub struct AosMetadataValidation {
    /// Whether the validation passed
    pub is_valid: bool,
    /// List of validation errors (if any)
    pub errors: Vec<String>,
    /// List of validation warnings (non-fatal issues)
    pub warnings: Vec<String>,
}

impl AosMetadataValidation {
    /// Create a successful validation result
    pub fn valid() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }

struct AdapterSessionContext {
    session_id: String,
    session_name: Option<String>,
    session_tags: Option<Vec<String>>,
}

pub struct AliasUpdateGateConfig {
    /// Allow alias updates for Ready state when true.
    pub allow_ready: bool,
}

pub struct AtomicDualWriteConfig {
    /// Require KV writes to succeed; if true, failures surface as errors
    /// and registration attempts to rollback SQL inserts.
    pub require_kv_success: bool,
}

impl AtomicDualWriteConfig {
    /// Best-effort mode: KV failures are logged but do not fail the operation.
    pub fn best_effort() -> Self {
        Self::default()
    }

pub struct AdapterRegistrationParams {
    pub tenant_id: String,
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    pub tier: String, // 'persistent', 'warm', or 'ephemeral'
    pub alpha: f64,
    pub lora_strength: Option<f32>,
    pub targets_json: String,
    pub acl_json: Option<String>,
    pub languages_json: Option<String>,
    pub framework: Option<String>,
    pub category: String,
    pub scope: String,
    pub framework_id: Option<String>,
    pub framework_version: Option<String>,
    pub repo_id: Option<String>,
    pub commit_sha: Option<String>,
    pub intent: Option<String>,
    pub expires_at: Option<String>,
    // .aos file support (from migration 0045)
    pub aos_file_path: Option<String>,
    pub aos_file_hash: Option<String>,
    // Semantic naming taxonomy (from migration 0061)
    pub adapter_name: Option<String>,
    pub tenant_namespace: Option<String>,
    pub domain: Option<String>,
    pub purpose: Option<String>,
    pub revision: Option<String>,
    pub parent_id: Option<String>,
    pub fork_type: Option<String>,
    pub fork_reason: Option<String>,
    // Base model reference (from migration 0098)
    pub base_model_id: Option<String>,
    // MoE recommendation flag (0228)
    pub recommended_for_moe: Option<bool>,
    // Artifact hardening (from migration 0153)
    pub manifest_schema_version: Option<String>,
    /// Content hash (BLAKE3 of manifest + weights) - required for deduplication
    pub content_hash_b3: String,
    pub provenance_json: Option<String>,
    pub metadata_json: Option<String>,
    // Scan root path (from migration 0243)
    pub repo_path: Option<String>,
    // Codebase adapter registration metadata (from migration 0231)
    /// Source repository/codebase reference for codebase adapters
    pub codebase_scope: Option<String>,
    /// Training dataset version ID for reproducibility
    pub dataset_version_id: Option<String>,
    /// ISO8601 timestamp when adapter was registered
    pub registration_timestamp: Option<String>,
    /// BLAKE3 hash of the adapter manifest for integrity verification
    pub manifest_hash: Option<String>,
    // Codebase adapter type and stream binding (from migration 0261)
    /// Adapter classification: "standard" (portable), "codebase" (stream-scoped), "core" (baseline)
    pub adapter_type: Option<String>,
    /// Base adapter ID for codebase adapters (the core adapter they extend as delta)
    pub base_adapter_id: Option<String>,
    /// Exclusive session binding for codebase adapters
    pub stream_session_id: Option<String>,
    /// Activation threshold for auto-versioning (default: 100)
    pub versioning_threshold: Option<i32>,
    /// BLAKE3 hash of fused CoreML package for deployment verification
    pub coreml_package_hash: Option<String>,
    /// BLAKE3 hash of training dataset content at training time.
    /// Used for receipt generation and lineage verification.
    /// (from migration 0282)
    pub training_dataset_hash_b3: Option<String>,
}

pub struct AdapterRegistrationBuilder {
    tenant_id: Option<String>,
    adapter_id: Option<String>,
    name: Option<String>,
    hash_b3: Option<String>,
    rank: Option<i32>,
    tier: Option<String>, // 'persistent', 'warm', or 'ephemeral'
    alpha: Option<f64>,
    lora_strength: Option<f32>,
    targets_json: Option<String>,
    acl_json: Option<String>,
    languages_json: Option<String>,
    framework: Option<String>,
    category: Option<String>,
    scope: Option<String>,
    framework_id: Option<String>,
    framework_version: Option<String>,
    repo_id: Option<String>,
    commit_sha: Option<String>,
    intent: Option<String>,
    expires_at: Option<String>,
    aos_file_path: Option<String>,
    aos_file_hash: Option<String>,
    // Semantic naming taxonomy (from migration 0061)
    adapter_name: Option<String>,
    tenant_namespace: Option<String>,
    domain: Option<String>,
    purpose: Option<String>,
    revision: Option<String>,
    parent_id: Option<String>,
    fork_type: Option<String>,
    fork_reason: Option<String>,
    // Base model reference (from migration 0098)
    base_model_id: Option<String>,
    // MoE recommendation flag (0228)
    recommended_for_moe: Option<bool>,
    // Artifact hardening (from migration 0153)
    manifest_schema_version: Option<String>,
    content_hash_b3: Option<String>,
    provenance_json: Option<String>,
    metadata_json: Option<String>,
    // Scan root path (from migration 0243)
    repo_path: Option<String>,
    // Codebase adapter registration metadata (from migration 0231)
    codebase_scope: Option<String>,
    dataset_version_id: Option<String>,
    registration_timestamp: Option<String>,
    manifest_hash: Option<String>,
    // Codebase adapter type and stream binding (from migration 0261)
    adapter_type: Option<String>,
    base_adapter_id: Option<String>,
    stream_session_id: Option<String>,
    versioning_threshold: Option<i32>,
    coreml_package_hash: Option<String>,
    // Training dataset hash for lineage binding (from migration 0282)
    training_dataset_hash_b3: Option<String>,
}

impl AdapterRegistrationBuilder {
    /// Create a new adapter registration builder
    pub fn new() -> Self {
        Self::default()
    }

pub fn validate_aos_metadata(params: &StoreAdapterFileMetadataParams) -> AosMetadataValidation {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Validate required fields
    if params.adapter_id.is_empty() {
        errors.push("adapter_id is required".to_string());
    }

    if params.aos_file_path.is_empty() {
        errors.push("aos_file_path is required".to_string());
    } else {
        let path = Path::new(&params.aos_file_path);
        if !path.is_absolute() {
            errors.push(format!(
                "aos_file_path must be an absolute path: {}",
                params.aos_file_path
            ));
        }
        match path.extension().and_then(|e| e.to_str()) {
            Some("aos") => {}
            Some(ext) => {
                errors.push(format!(
                    "aos_file_path must have .aos extension, got .{}: {}",
                    ext, params.aos_file_path
                ));
            }
            None => {
                errors.push(format!(
                    "aos_file_path must have .aos extension: {}",
                    params.aos_file_path
                ));
            }
        }
    }

    if params.aos_file_hash.is_empty() {
        errors.push("aos_file_hash is required".to_string());
    } else {
        let hash = params
            .aos_file_hash
            .strip_prefix("b3:")
            .unwrap_or(&params.aos_file_hash);
        if hash.len() != 64 {
            errors.push(format!(
                "aos_file_hash must be 64 hex characters (BLAKE3), got {} characters",
                hash.len()
            ));
        } else if !hash.chars().all(|c| c.is_ascii_hexdigit()) {
            errors.push("aos_file_hash must contain only hexadecimal characters".to_string());
        }
    }

    if let Some(file_size) = params.file_size_bytes {
        if file_size < 0 {
            errors.push(format!("file_size_bytes cannot be negative: {}", file_size));
        } else if file_size == 0 {
            warnings.push("file_size_bytes is 0, which may indicate an empty file".to_string());
        }
    }

    if let Some(segment_count) = params.segment_count {
        if segment_count <= 0 {
            errors.push(format!(
                "segment_count must be positive, got: {}",
                segment_count
            ));
        }
    }

    if let Some(ref version) = params.manifest_schema_version {
        if !is_valid_semver(version) {
            errors.push(format!(
                "manifest_schema_version must be valid semver (e.g., '1.0.0'), got: {}",
                version
            ));
        }
    }

    if let Some(ref tier) = params.tier {
        if !["persistent", "warm", "ephemeral"].contains(&tier.as_str()) {
            errors.push(format!(
                "tier must be 'persistent', 'warm', or 'ephemeral', got: {}",
                tier
            ));
        }
    }

    if let Some(ref category) = params.category {
        let valid_categories = [
            "code",
            "documentation",
            "creative",
            "conversation",
            "analysis",
        ];
        if !valid_categories.contains(&category.as_str()) {
            warnings.push(format!(
                "category '{}' is non-standard (expected one of: {})",
                category,
                valid_categories.join(", ")
            ));
        }
    }

    if errors.is_empty() {
        let mut result = AosMetadataValidation::valid();
        result.warnings = warnings;
        result
    } else {
        let mut result = AosMetadataValidation::invalid(errors);
        result.warnings = warnings;
        result
    }
}

fn is_valid_semver(version: &str) -> bool {
    let parts: Vec<&str> = version.split('-').collect();
    let version_core = parts.first().unwrap_or(&"");
    let numbers: Vec<&str> = version_core.split('.').collect();
    if numbers.len() < 2 || numbers.len() > 3 {
        return false;
    }
    numbers.iter().all(|n| n.parse::<u32>().is_ok())
}

fn parse_session_tags_value(value: &Value) -> Option<Vec<String>> {
    let mut tags = match value {
        Value::String(raw) => raw
            .split(',')
            .map(|tag| tag.trim().to_string())
            .collect::<Vec<String>>(),
        Value::Array(values) => values
            .iter()
            .filter_map(value_to_trimmed_string)
            .collect::<Vec<String>>(),
        _ => return None,
    };
    normalize_session_tags(&mut tags);
    if tags.is_empty() {
        None
    } else {
        Some(tags)
    }
}

fn parse_session_context(metadata_json: Option<&str>) -> Option<AdapterSessionContext> {
    let metadata_json = metadata_json?;
    let value: Value = serde_json::from_str(metadata_json).ok()?;
    let obj = value.as_object()?;
    let session_id = obj.get("session_id").and_then(value_to_trimmed_string)?;
    let session_name = obj.get("session_name").and_then(value_to_trimmed_string);
    let session_tags = obj.get("session_tags").and_then(parse_session_tags_value);

    Some(AdapterSessionContext {
        session_id,
        session_name,
        session_tags,
    })
}

fn normalize_session_tags(tags: &mut Vec<String>) {
    tags.iter_mut().for_each(|tag| {
        *tag = tag.trim().to_string();
    });
    tags.retain(|tag| !tag.is_empty());
    tags.sort();
    tags.dedup();
}

fn value_to_trimmed_string(value: &Value) -> Option<String> {
    match value {
        Value::String(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        }
        _ => None,
    }
}

