use adapteros_core::{AosError, Result};
/// Adapter Record Refactoring for Schema Drift Prevention
///
/// This module organizes the 36+ fields of the Adapter record into logical
/// sub-structures, implementing:
/// 1. Field grouping by concern (core identity, lifecycle, semantic naming, etc.)
/// 2. Schema versioning and migration helpers
/// 3. Type-safe builders with validation
/// 4. Backward compatibility with existing data
///
/// Design Pattern: Struct composition with builder patterns for ease of use.
use serde::{Deserialize, Serialize};

// ============================================================================
// Sub-Structures: Logical grouping by concern
// ============================================================================

/// Core adapter identification and hash information
///
/// Immutable once created. These fields identify the adapter uniquely
/// and represent the adapter's content fingerprint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterIdentity {
    /// Primary database ID (UUIDv7)
    pub id: String,
    /// External adapter ID for lookups
    pub adapter_id: String,
    /// Human-readable name
    pub name: String,
    /// BLAKE3 content hash
    pub hash_b3: String,
}

impl AdapterIdentity {
    /// Create a new adapter identity
    pub fn new(id: String, adapter_id: String, name: String, hash_b3: String) -> Self {
        Self {
            id,
            adapter_id,
            name,
            hash_b3,
        }
    }

    /// Validate identity fields
    pub fn validate(&self) -> Result<()> {
        if self.id.is_empty() {
            return Err(AosError::Validation("id cannot be empty".into()));
        }
        if self.adapter_id.is_empty() {
            return Err(AosError::Validation("adapter_id cannot be empty".into()));
        }
        if self.name.is_empty() {
            return Err(AosError::Validation("name cannot be empty".into()));
        }
        if self.hash_b3.is_empty() {
            return Err(AosError::Validation("hash_b3 cannot be empty".into()));
        }
        Ok(())
    }
}

/// Semantic naming taxonomy (from migration 0061)
///
/// Supports the {tenant}/{domain}/{purpose}/{revision} naming convention
/// for better organization and discoverability.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SemanticNaming {
    /// Full semantic adapter name: {tenant_namespace}/{domain}/{purpose}/{revision}
    pub adapter_name: Option<String>,
    /// Tenant namespace (e.g., "acme-corp")
    pub tenant_namespace: Option<String>,
    /// Domain (e.g., "engineering", "sales")
    pub domain: Option<String>,
    /// Purpose (e.g., "code-review", "documentation")
    pub purpose: Option<String>,
    /// Revision (e.g., "r001", "r042")
    pub revision: Option<String>,
}

impl SemanticNaming {
    /// Validate semantic naming constraints
    ///
    /// If any field is provided, all fields must be provided.
    /// Revision must follow rNNN format.
    pub fn validate(&self) -> Result<()> {
        let has_any = self.adapter_name.is_some()
            || self.tenant_namespace.is_some()
            || self.domain.is_some()
            || self.purpose.is_some()
            || self.revision.is_some();

        if !has_any {
            return Ok(());
        }

        let has_all = self.adapter_name.is_some()
            && self.tenant_namespace.is_some()
            && self.domain.is_some()
            && self.purpose.is_some()
            && self.revision.is_some();

        if !has_all {
            return Err(AosError::Validation(
                "Semantic naming: all fields must be provided together".into(),
            ));
        }

        // Validate revision format (rNNN)
        if let Some(rev) = &self.revision {
            if !rev.starts_with('r') || rev.len() < 2 {
                return Err(AosError::Validation(
                    "Revision must follow rNNN format (e.g., r001, r042)".into(),
                ));
            }
            if let Some(num_str) = rev.strip_prefix('r') {
                if num_str.parse::<i32>().is_err() {
                    return Err(AosError::Validation(
                        "Revision number must be numeric".into(),
                    ));
                }
            }
        }

        Ok(())
    }
}

/// LoRA model configuration and training parameters
///
/// Defines the rank, alpha, and quantization characteristics of the adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoRAConfig {
    /// LoRA rank (determines size/capacity)
    pub rank: i32,
    /// LoRA alpha parameter (usually rank * 2)
    pub alpha: f64,
    /// Target modules for LoRA injection (JSON array)
    pub targets_json: String,
}

impl LoRAConfig {
    /// Create a new LoRA configuration
    pub fn new(rank: i32, alpha: f64, targets_json: String) -> Self {
        Self {
            rank,
            alpha,
            targets_json,
        }
    }

    /// Validate LoRA configuration
    pub fn validate(&self) -> Result<()> {
        if self.rank < 1 {
            return Err(AosError::Validation("rank must be at least 1".into()));
        }
        if self.alpha < 0.0 {
            return Err(AosError::Validation("alpha must be non-negative".into()));
        }
        if self.targets_json.trim().is_empty() {
            return Err(AosError::Validation("targets_json cannot be empty".into()));
        }
        // Try to parse as JSON
        if serde_json::from_str::<Vec<String>>(&self.targets_json).is_err() {
            return Err(AosError::Validation(
                "targets_json must be a valid JSON array".into(),
            ));
        }
        Ok(())
    }
}

/// Adapter lifecycle and state management
///
/// Tracks the runtime state, memory usage, and activation history of an adapter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LifecycleState {
    /// Current state (e.g., "unloaded", "cold", "warm", "hot", "resident")
    pub current_state: String,
    /// Load state for runtime tracking
    pub load_state: String,
    /// Lifecycle state (draft/active/deprecated/retired)
    pub lifecycle_state: String,
    /// Memory usage in bytes
    pub memory_bytes: i64,
    /// Total activation count
    pub activation_count: i64,
    /// Last time this adapter was activated
    pub last_activated: Option<String>,
    /// Last time this adapter was loaded
    pub last_loaded_at: Option<String>,
    /// Is adapter pinned (soft-lock to prevent eviction)
    pub pinned: i32,
}

impl LifecycleState {
    /// Create a default lifecycle state (unloaded)
    pub fn default_unloaded() -> Self {
        Self {
            current_state: "unloaded".to_string(),
            load_state: "cold".to_string(),
            lifecycle_state: "active".to_string(),
            memory_bytes: 0,
            activation_count: 0,
            last_activated: None,
            last_loaded_at: None,
            pinned: 0,
        }
    }

    /// Validate lifecycle state
    pub fn validate(&self) -> Result<()> {
        let valid_states = ["unloaded", "cold", "warm", "hot", "resident"];
        if !valid_states.contains(&self.current_state.as_str()) {
            return Err(AosError::Validation(format!(
                "Invalid current_state: {}",
                self.current_state
            )));
        }

        let valid_load_states = ["cold", "warm", "hot"];
        if !valid_load_states.contains(&self.load_state.as_str()) {
            return Err(AosError::Validation(format!(
                "Invalid load_state: {}",
                self.load_state
            )));
        }

        let valid_lifecycle = ["draft", "active", "deprecated", "retired"];
        if !valid_lifecycle.contains(&self.lifecycle_state.as_str()) {
            return Err(AosError::Validation(format!(
                "Invalid lifecycle_state: {}",
                self.lifecycle_state
            )));
        }

        if self.memory_bytes < 0 {
            return Err(AosError::Validation(
                "memory_bytes cannot be negative".into(),
            ));
        }

        if self.activation_count < 0 {
            return Err(AosError::Validation(
                "activation_count cannot be negative".into(),
            ));
        }

        Ok(())
    }
}

/// Deployment configuration and tiering
///
/// Determines how the adapter is cached and prioritized in memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TierConfig {
    /// Tier: "persistent", "warm", or "ephemeral"
    pub tier: String,
    /// Category (e.g., "code", "classification")
    pub category: String,
    /// Scope (e.g., "global", "tenant-specific")
    pub scope: String,
    /// Active flag (soft delete indicator)
    pub active: i32,
}

impl TierConfig {
    /// Create a new tier configuration
    pub fn new(tier: String, category: String, scope: String) -> Self {
        Self {
            tier,
            category,
            scope,
            active: 1,
        }
    }

    /// Validate tier configuration
    pub fn validate(&self) -> Result<()> {
        let valid_tiers = ["persistent", "warm", "ephemeral"];
        if !valid_tiers.contains(&self.tier.as_str()) {
            return Err(AosError::Validation(format!(
                "Invalid tier: {} (must be persistent, warm, or ephemeral)",
                self.tier
            )));
        }

        if self.category.is_empty() {
            return Err(AosError::Validation("category cannot be empty".into()));
        }

        if self.scope.is_empty() {
            return Err(AosError::Validation("scope cannot be empty".into()));
        }

        if ![0, 1].contains(&self.active) {
            return Err(AosError::Validation("active must be 0 or 1".into()));
        }

        Ok(())
    }
}

/// Code intelligence and framework metadata
///
/// Tracks the adapter's origin, framework compatibility, and code context.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CodeIntelligence {
    /// Framework type (e.g., "transformers", "pytorch")
    pub framework: Option<String>,
    /// Framework version
    pub framework_version: Option<String>,
    /// Source repository ID
    pub repo_id: Option<String>,
    /// Source commit SHA
    pub commit_sha: Option<String>,
    /// Supported languages (JSON array)
    pub languages_json: Option<String>,
    /// Intent or purpose description
    pub intent: Option<String>,
}

impl CodeIntelligence {
    /// Validate code intelligence fields
    pub fn validate(&self) -> Result<()> {
        if let Some(langs) = &self.languages_json {
            if serde_json::from_str::<Vec<String>>(langs).is_err() {
                return Err(AosError::Validation(
                    "languages_json must be a valid JSON array".into(),
                ));
            }
        }
        Ok(())
    }
}

/// Adapter fork tracking and lineage
///
/// Supports adapter creation through forking with parent relationships.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ForkMetadata {
    /// Parent adapter ID (if this is a fork)
    pub parent_id: Option<String>,
    /// Fork type: "parameter", "data", or "architecture"
    pub fork_type: Option<String>,
    /// Reason for fork
    pub fork_reason: Option<String>,
}

impl ForkMetadata {
    /// Validate fork metadata
    pub fn validate(&self) -> Result<()> {
        if let Some(fork_type) = &self.fork_type {
            let valid_types = ["parameter", "data", "architecture"];
            if !valid_types.contains(&fork_type.as_str()) {
                return Err(AosError::Validation(format!(
                    "Invalid fork_type: {} (must be parameter, data, or architecture)",
                    fork_type
                )));
            }
        }

        // If fork_type is set, parent_id should be set
        if self.fork_type.is_some() && self.parent_id.is_none() {
            return Err(AosError::Validation(
                "parent_id is required when fork_type is set".into(),
            ));
        }

        Ok(())
    }
}

/// Access control and multi-tenancy
///
/// Manages adapter visibility and access restrictions.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccessControl {
    /// Tenant ID (isolation boundary)
    pub tenant_id: String,
    /// Access control list (JSON)
    pub acl_json: Option<String>,
}

impl AccessControl {
    /// Create a new access control entry
    pub fn new(tenant_id: String) -> Self {
        Self {
            tenant_id,
            acl_json: None,
        }
    }

    /// Validate access control configuration
    pub fn validate(&self) -> Result<()> {
        if self.tenant_id.is_empty() {
            return Err(AosError::Validation("tenant_id cannot be empty".into()));
        }

        if let Some(acl) = &self.acl_json {
            if serde_json::from_str::<serde_json::Value>(acl).is_err() {
                return Err(AosError::Validation("acl_json must be valid JSON".into()));
            }
        }

        Ok(())
    }
}

/// File and artifact management
///
/// Supports the .aos archive format and artifact tracking.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ArtifactInfo {
    /// Path to .aos file (if applicable)
    pub aos_file_path: Option<String>,
    /// BLAKE3 hash of .aos file
    pub aos_file_hash: Option<String>,
}

impl ArtifactInfo {
    /// Validate artifact information
    pub fn validate(&self) -> Result<()> {
        // If path is set, hash should be set
        if self.aos_file_path.is_some() && self.aos_file_hash.is_none() {
            return Err(AosError::Validation(
                "aos_file_hash is required when aos_file_path is set".into(),
            ));
        }
        Ok(())
    }
}

/// Schema versioning and metadata
///
/// Tracks schema compatibility and versioning for future migrations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaMetadata {
    /// Schema version (e.g., "1.0.0")
    pub version: String,
    /// Record creation timestamp (ISO 8601)
    pub created_at: String,
    /// Last update timestamp (ISO 8601)
    pub updated_at: String,
}

impl SchemaMetadata {
    /// Create new schema metadata with current timestamps
    pub fn new(version: String, created_at: String, updated_at: String) -> Self {
        Self {
            version,
            created_at,
            updated_at,
        }
    }

    /// Create with default version "1.0.0"
    pub fn default_v1(created_at: String, updated_at: String) -> Self {
        Self {
            version: "1.0.0".to_string(),
            created_at,
            updated_at,
        }
    }
}

// ============================================================================
// Comprehensive Adapter Record (composition of sub-structures)
// ============================================================================

/// Complete adapter record with schema versioning
///
/// Composed of specialized sub-structures to prevent schema drift and
/// facilitate future schema migrations.
///
/// # Design Principles:
/// 1. **Immutability**: Identity fields cannot change after creation
/// 2. **Composition**: Related concerns grouped in dedicated structs
/// 3. **Validation**: Each sub-structure validates its own fields
/// 4. **Backward Compatibility**: Can be serialized to/from flat row structure
/// 5. **Schema Version Tracking**: Supports future migration strategies
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterRecordV1 {
    // Core identity (immutable)
    pub identity: AdapterIdentity,

    // Access and tenancy
    pub access: AccessControl,

    // LoRA configuration
    pub lora: LoRAConfig,

    // Deployment and tiering
    pub tier_config: TierConfig,

    // Runtime lifecycle and state
    pub lifecycle: LifecycleState,

    // Code intelligence and framework info
    pub code_info: CodeIntelligence,

    // Semantic naming taxonomy
    pub semantic_naming: SemanticNaming,

    // Fork and lineage tracking
    pub fork_metadata: ForkMetadata,

    // File artifacts (.aos support)
    pub artifacts: ArtifactInfo,

    // Schema versioning and timestamps
    pub schema: SchemaMetadata,

    // Expiration/TTL support
    pub expires_at: Option<String>,
}

impl AdapterRecordV1 {
    /// Validate the entire adapter record
    ///
    /// Recursively validates all sub-structures and inter-field constraints.
    pub fn validate(&self) -> Result<()> {
        self.identity.validate()?;
        self.access.validate()?;
        self.lora.validate()?;
        self.tier_config.validate()?;
        self.lifecycle.validate()?;
        self.code_info.validate()?;
        self.semantic_naming.validate()?;
        self.fork_metadata.validate()?;
        self.artifacts.validate()?;

        Ok(())
    }

    /// Get schema version
    pub fn schema_version(&self) -> &str {
        &self.schema.version
    }
}

// ============================================================================
// Builder Pattern for Type-Safe Construction
// ============================================================================

/// Builder for constructing AdapterRecordV1 with validation
///
/// Ensures all required fields are provided and validated before construction.
#[derive(Debug, Default)]
pub struct AdapterRecordBuilder {
    identity: Option<AdapterIdentity>,
    access: Option<AccessControl>,
    lora: Option<LoRAConfig>,
    tier_config: Option<TierConfig>,
    lifecycle: Option<LifecycleState>,
    code_info: Option<CodeIntelligence>,
    semantic_naming: Option<SemanticNaming>,
    fork_metadata: Option<ForkMetadata>,
    artifacts: Option<ArtifactInfo>,
    schema: Option<SchemaMetadata>,
    expires_at: Option<String>,
}

impl AdapterRecordBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self::default()
    }

    /// Set adapter identity (required)
    pub fn identity(mut self, identity: AdapterIdentity) -> Self {
        self.identity = Some(identity);
        self
    }

    /// Set access control (required)
    pub fn access(mut self, access: AccessControl) -> Self {
        self.access = Some(access);
        self
    }

    /// Set LoRA configuration (required)
    pub fn lora(mut self, lora: LoRAConfig) -> Self {
        self.lora = Some(lora);
        self
    }

    /// Set tier configuration (required)
    pub fn tier_config(mut self, tier_config: TierConfig) -> Self {
        self.tier_config = Some(tier_config);
        self
    }

    /// Set lifecycle state (optional, defaults to unloaded)
    pub fn lifecycle(mut self, lifecycle: LifecycleState) -> Self {
        self.lifecycle = Some(lifecycle);
        self
    }

    /// Set code intelligence (optional)
    pub fn code_info(mut self, code_info: CodeIntelligence) -> Self {
        self.code_info = Some(code_info);
        self
    }

    /// Set semantic naming (optional)
    pub fn semantic_naming(mut self, semantic_naming: SemanticNaming) -> Self {
        self.semantic_naming = Some(semantic_naming);
        self
    }

    /// Set fork metadata (optional)
    pub fn fork_metadata(mut self, fork_metadata: ForkMetadata) -> Self {
        self.fork_metadata = Some(fork_metadata);
        self
    }

    /// Set artifact information (optional)
    pub fn artifacts(mut self, artifacts: ArtifactInfo) -> Self {
        self.artifacts = Some(artifacts);
        self
    }

    /// Set schema metadata (optional, defaults to v1.0.0)
    pub fn schema(mut self, schema: SchemaMetadata) -> Self {
        self.schema = Some(schema);
        self
    }

    /// Set expiration timestamp (optional, TTL support)
    pub fn expires_at(mut self, expires_at: Option<String>) -> Self {
        self.expires_at = expires_at;
        self
    }

    /// Build the complete adapter record
    ///
    /// Returns error if required fields are missing.
    pub fn build(self) -> Result<AdapterRecordV1> {
        let identity = self
            .identity
            .ok_or_else(|| AosError::Validation("identity is required".into()))?;
        let access = self
            .access
            .ok_or_else(|| AosError::Validation("access is required".into()))?;
        let lora = self
            .lora
            .ok_or_else(|| AosError::Validation("lora is required".into()))?;
        let tier_config = self
            .tier_config
            .ok_or_else(|| AosError::Validation("tier_config is required".into()))?;

        let lifecycle = self
            .lifecycle
            .unwrap_or_else(LifecycleState::default_unloaded);
        let code_info = self.code_info.unwrap_or_default();
        let semantic_naming = self.semantic_naming.unwrap_or_default();
        let fork_metadata = self.fork_metadata.unwrap_or_default();
        let artifacts = self.artifacts.unwrap_or_default();

        // Get current timestamp if not provided
        let schema = self.schema.unwrap_or_else(|| {
            // This should be replaced with proper time handling in production
            // For now, use placeholder timestamps - caller should provide actual values
            let placeholder = "2025-11-21T00:00:00Z".to_string();
            SchemaMetadata::default_v1(placeholder.clone(), placeholder)
        });

        let record = AdapterRecordV1 {
            identity,
            access,
            lora,
            tier_config,
            lifecycle,
            code_info,
            semantic_naming,
            fork_metadata,
            artifacts,
            schema,
            expires_at: self.expires_at,
        };

        record.validate()?;
        Ok(record)
    }
}

// ============================================================================
// Migration Helpers (backward compatibility with flat schema)
// ============================================================================

/// Trait for converting between flat and structured representations
///
/// Enables backward compatibility with existing database rows.
pub trait SchemaCompatible: Sized {
    /// Convert from flat row representation to structured record
    fn from_flat(flat: &FlatAdapterRow) -> Result<Self>;

    /// Convert from structured record to flat row representation
    fn to_flat(&self) -> Result<FlatAdapterRow>;
}

/// Flat adapter row structure (mirrors database schema)
///
/// Used for serialization to/from database without intermediate conversions.
/// This structure maps 1:1 with database columns for zero-copy operations.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct FlatAdapterRow {
    // Core identity
    pub id: String,
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,

    // Access and tenancy
    pub tenant_id: String,
    pub acl_json: Option<String>,

    // LoRA configuration
    pub rank: i32,
    pub alpha: f64,
    pub targets_json: String,

    // Deployment and tiering
    pub tier: String,
    pub category: String,
    pub scope: String,
    pub active: i32,

    // Runtime lifecycle and state
    pub current_state: String,
    pub load_state: String,
    pub lifecycle_state: String,
    pub memory_bytes: i64,
    pub activation_count: i64,
    pub last_activated: Option<String>,
    pub last_loaded_at: Option<String>,
    pub pinned: i32,

    // Code intelligence
    pub framework: Option<String>,
    pub framework_version: Option<String>,
    pub repo_id: Option<String>,
    pub commit_sha: Option<String>,
    pub languages_json: Option<String>,
    pub intent: Option<String>,

    // Semantic naming
    pub adapter_name: Option<String>,
    pub tenant_namespace: Option<String>,
    pub domain: Option<String>,
    pub purpose: Option<String>,
    pub revision: Option<String>,

    // Fork metadata
    pub parent_id: Option<String>,
    pub fork_type: Option<String>,
    pub fork_reason: Option<String>,

    // File artifacts
    pub aos_file_path: Option<String>,
    pub aos_file_hash: Option<String>,

    // Schema metadata
    pub version: String,
    pub created_at: String,
    pub updated_at: String,

    // Expiration
    pub expires_at: Option<String>,
}

impl SchemaCompatible for AdapterRecordV1 {
    /// Convert from flat database row to structured record
    fn from_flat(flat: &FlatAdapterRow) -> Result<Self> {
        let identity = AdapterIdentity::new(
            flat.id.clone(),
            flat.adapter_id.clone(),
            flat.name.clone(),
            flat.hash_b3.clone(),
        );

        let access = AccessControl {
            tenant_id: flat.tenant_id.clone(),
            acl_json: flat.acl_json.clone(),
        };

        let lora = LoRAConfig::new(flat.rank, flat.alpha, flat.targets_json.clone());

        let tier_config = TierConfig {
            tier: flat.tier.clone(),
            category: flat.category.clone(),
            scope: flat.scope.clone(),
            active: flat.active,
        };

        let lifecycle = LifecycleState {
            current_state: flat.current_state.clone(),
            load_state: flat.load_state.clone(),
            lifecycle_state: flat.lifecycle_state.clone(),
            memory_bytes: flat.memory_bytes,
            activation_count: flat.activation_count,
            last_activated: flat.last_activated.clone(),
            last_loaded_at: flat.last_loaded_at.clone(),
            pinned: flat.pinned,
        };

        let code_info = CodeIntelligence {
            framework: flat.framework.clone(),
            framework_version: flat.framework_version.clone(),
            repo_id: flat.repo_id.clone(),
            commit_sha: flat.commit_sha.clone(),
            languages_json: flat.languages_json.clone(),
            intent: flat.intent.clone(),
        };

        let semantic_naming = SemanticNaming {
            adapter_name: flat.adapter_name.clone(),
            tenant_namespace: flat.tenant_namespace.clone(),
            domain: flat.domain.clone(),
            purpose: flat.purpose.clone(),
            revision: flat.revision.clone(),
        };

        let fork_metadata = ForkMetadata {
            parent_id: flat.parent_id.clone(),
            fork_type: flat.fork_type.clone(),
            fork_reason: flat.fork_reason.clone(),
        };

        let artifacts = ArtifactInfo {
            aos_file_path: flat.aos_file_path.clone(),
            aos_file_hash: flat.aos_file_hash.clone(),
        };

        let schema = SchemaMetadata::new(
            flat.version.clone(),
            flat.created_at.clone(),
            flat.updated_at.clone(),
        );

        let record = AdapterRecordV1 {
            identity,
            access,
            lora,
            tier_config,
            lifecycle,
            code_info,
            semantic_naming,
            fork_metadata,
            artifacts,
            schema,
            expires_at: flat.expires_at.clone(),
        };

        record.validate()?;
        Ok(record)
    }

    /// Convert from structured record to flat database row
    fn to_flat(&self) -> Result<FlatAdapterRow> {
        Ok(FlatAdapterRow {
            id: self.identity.id.clone(),
            adapter_id: self.identity.adapter_id.clone(),
            name: self.identity.name.clone(),
            hash_b3: self.identity.hash_b3.clone(),
            tenant_id: self.access.tenant_id.clone(),
            acl_json: self.access.acl_json.clone(),
            rank: self.lora.rank,
            alpha: self.lora.alpha,
            targets_json: self.lora.targets_json.clone(),
            tier: self.tier_config.tier.clone(),
            category: self.tier_config.category.clone(),
            scope: self.tier_config.scope.clone(),
            active: self.tier_config.active,
            current_state: self.lifecycle.current_state.clone(),
            load_state: self.lifecycle.load_state.clone(),
            lifecycle_state: self.lifecycle.lifecycle_state.clone(),
            memory_bytes: self.lifecycle.memory_bytes,
            activation_count: self.lifecycle.activation_count,
            last_activated: self.lifecycle.last_activated.clone(),
            last_loaded_at: self.lifecycle.last_loaded_at.clone(),
            pinned: self.lifecycle.pinned,
            framework: self.code_info.framework.clone(),
            framework_version: self.code_info.framework_version.clone(),
            repo_id: self.code_info.repo_id.clone(),
            commit_sha: self.code_info.commit_sha.clone(),
            languages_json: self.code_info.languages_json.clone(),
            intent: self.code_info.intent.clone(),
            adapter_name: self.semantic_naming.adapter_name.clone(),
            tenant_namespace: self.semantic_naming.tenant_namespace.clone(),
            domain: self.semantic_naming.domain.clone(),
            purpose: self.semantic_naming.purpose.clone(),
            revision: self.semantic_naming.revision.clone(),
            parent_id: self.fork_metadata.parent_id.clone(),
            fork_type: self.fork_metadata.fork_type.clone(),
            fork_reason: self.fork_metadata.fork_reason.clone(),
            aos_file_path: self.artifacts.aos_file_path.clone(),
            aos_file_hash: self.artifacts.aos_file_hash.clone(),
            version: self.schema.version.clone(),
            created_at: self.schema.created_at.clone(),
            updated_at: self.schema.updated_at.clone(),
            expires_at: self.expires_at.clone(),
        })
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_identity_validation() {
        let identity = AdapterIdentity::new(
            "id-123".to_string(),
            "adapter-1".to_string(),
            "My Adapter".to_string(),
            "b3:abc123".to_string(),
        );
        assert!(identity.validate().is_ok());

        let invalid = AdapterIdentity::new(
            "".to_string(),
            "adapter-1".to_string(),
            "My Adapter".to_string(),
            "b3:abc123".to_string(),
        );
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn test_semantic_naming_validation() {
        // Valid: all fields provided
        let valid = SemanticNaming {
            adapter_name: Some("tenant/domain/purpose/r001".to_string()),
            tenant_namespace: Some("tenant".to_string()),
            domain: Some("domain".to_string()),
            purpose: Some("purpose".to_string()),
            revision: Some("r001".to_string()),
        };
        assert!(valid.validate().is_ok());

        // Valid: all fields empty
        let empty = SemanticNaming::default();
        assert!(empty.validate().is_ok());

        // Invalid: partial fields
        let partial = SemanticNaming {
            adapter_name: Some("name".to_string()),
            tenant_namespace: Some("tenant".to_string()),
            ..Default::default()
        };
        assert!(partial.validate().is_err());

        // Invalid: bad revision format
        let bad_revision = SemanticNaming {
            adapter_name: Some("name".to_string()),
            tenant_namespace: Some("tenant".to_string()),
            domain: Some("domain".to_string()),
            purpose: Some("purpose".to_string()),
            revision: Some("invalid".to_string()),
        };
        assert!(bad_revision.validate().is_err());
    }

    #[test]
    fn test_lora_config_validation() {
        let config = LoRAConfig::new(16, 32.0, r#"["q_proj", "v_proj"]"#.to_string());
        assert!(config.validate().is_ok());

        let invalid_rank = LoRAConfig::new(0, 32.0, r#"["q_proj"]"#.to_string());
        assert!(invalid_rank.validate().is_err());

        let invalid_targets = LoRAConfig::new(16, 32.0, "invalid json".to_string());
        assert!(invalid_targets.validate().is_err());
    }

    #[test]
    fn test_tier_config_validation() {
        let config = TierConfig::new("warm".to_string(), "code".to_string(), "global".to_string());
        assert!(config.validate().is_ok());

        let invalid_tier = TierConfig::new(
            "invalid".to_string(),
            "code".to_string(),
            "global".to_string(),
        );
        assert!(invalid_tier.validate().is_err());
    }

    #[test]
    fn test_lifecycle_state_defaults() {
        let lifecycle = LifecycleState::default_unloaded();
        assert_eq!(lifecycle.current_state, "unloaded");
        assert_eq!(lifecycle.load_state, "cold");
        assert_eq!(lifecycle.lifecycle_state, "active");
        assert_eq!(lifecycle.memory_bytes, 0);
        assert!(lifecycle.validate().is_ok());
    }

    #[test]
    fn test_adapter_record_builder() {
        let record = AdapterRecordBuilder::new()
            .identity(AdapterIdentity::new(
                "id-1".to_string(),
                "adapter-1".to_string(),
                "Test".to_string(),
                "b3:hash".to_string(),
            ))
            .access(AccessControl::new("tenant-1".to_string()))
            .lora(LoRAConfig::new(16, 32.0, r#"["q_proj"]"#.to_string()))
            .tier_config(TierConfig::new(
                "warm".to_string(),
                "code".to_string(),
                "global".to_string(),
            ))
            .build();

        assert!(record.is_ok());
        let record = record.unwrap();
        assert_eq!(record.identity.adapter_id, "adapter-1");
        assert_eq!(record.access.tenant_id, "tenant-1");
    }

    #[test]
    fn test_flat_to_structured_conversion() {
        let flat = FlatAdapterRow {
            id: "id-1".to_string(),
            adapter_id: "adapter-1".to_string(),
            name: "Test".to_string(),
            hash_b3: "b3:hash".to_string(),
            tenant_id: "tenant-1".to_string(),
            acl_json: None,
            rank: 16,
            alpha: 32.0,
            targets_json: r#"["q_proj"]"#.to_string(),
            tier: "warm".to_string(),
            category: "code".to_string(),
            scope: "global".to_string(),
            active: 1,
            current_state: "unloaded".to_string(),
            load_state: "cold".to_string(),
            lifecycle_state: "active".to_string(),
            memory_bytes: 0,
            activation_count: 0,
            last_activated: None,
            last_loaded_at: None,
            pinned: 0,
            framework: None,
            framework_version: None,
            repo_id: None,
            commit_sha: None,
            languages_json: None,
            intent: None,
            adapter_name: None,
            tenant_namespace: None,
            domain: None,
            purpose: None,
            revision: None,
            parent_id: None,
            fork_type: None,
            fork_reason: None,
            aos_file_path: None,
            aos_file_hash: None,
            version: "1.0.0".to_string(),
            created_at: "2025-11-21T00:00:00Z".to_string(),
            updated_at: "2025-11-21T00:00:00Z".to_string(),
            expires_at: None,
        };

        let structured = AdapterRecordV1::from_flat(&flat);
        assert!(structured.is_ok());

        let structured = structured.unwrap();
        assert_eq!(structured.identity.adapter_id, "adapter-1");

        let flat_again = structured.to_flat();
        assert!(flat_again.is_ok());
    }
}
