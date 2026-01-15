//! Serialization utilities and versioned record types for schema evolution
//!
//! Provides versioned storage records with timestamps and hierarchical key building
//! for adapterOS entities (adapters, datasets, documents, collections, etc.)

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// Import the crate's error type
use crate::error::StorageError;

/// Version prefix for all stored records
pub const CURRENT_SCHEMA_VERSION: u8 = 1;

/// Wrapper for versioned storage with automatic schema evolution support
///
/// All records stored in adapterOS should use this wrapper to enable:
/// - Schema version tracking for migration
/// - Creation and update timestamps
/// - Backward compatibility with older data
///
/// # Example
///
/// ```rust
/// use adapteros_storage::types::VersionedRecord;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize)]
/// struct AdapterMetadata {
///     name: String,
///     tier: String,
/// }
///
/// let metadata = AdapterMetadata {
///     name: "code-review".to_string(),
///     tier: "warm".to_string(),
/// };
///
/// let record = VersionedRecord::new(metadata);
/// let bytes = record.serialize()?;
///
/// # Ok::<(), adapteros_storage::error::StorageError>(())
/// ```
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct VersionedRecord<T> {
    /// Schema version for this record
    pub version: u8,
    /// Unix timestamp when record was created
    pub created_at: i64,
    /// Unix timestamp when record was last updated
    pub updated_at: i64,
    /// The actual data payload
    pub data: T,
}

impl<T: Serialize> VersionedRecord<T> {
    /// Create a new versioned record with current timestamp
    ///
    /// Sets both created_at and updated_at to the current time.
    pub fn new(data: T) -> Self {
        let now = Utc::now().timestamp();
        Self {
            version: CURRENT_SCHEMA_VERSION,
            created_at: now,
            updated_at: now,
            data,
        }
    }

    /// Update the data payload and refresh the updated_at timestamp
    pub fn update(&mut self, data: T) {
        self.data = data;
        self.updated_at = Utc::now().timestamp();
    }

    /// Serialize the record to bytes using bincode
    ///
    /// # Errors
    ///
    /// Returns `StorageError::SerializationError` if bincode serialization fails
    pub fn serialize(&self) -> Result<Vec<u8>, StorageError> {
        bincode::serialize(self).map_err(|e| {
            StorageError::SerializationError(format!("Bincode serialization failed: {}", e))
        })
    }

    /// Get the creation timestamp as a DateTime
    pub fn created_at_utc(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.created_at, 0).unwrap_or(DateTime::UNIX_EPOCH)
    }

    /// Get the update timestamp as a DateTime
    pub fn updated_at_utc(&self) -> DateTime<Utc> {
        DateTime::from_timestamp(self.updated_at, 0).unwrap_or(DateTime::UNIX_EPOCH)
    }
}

impl<T: for<'de> Deserialize<'de>> VersionedRecord<T> {
    /// Deserialize a record from bytes using bincode
    ///
    /// # Errors
    ///
    /// Returns `StorageError::SerializationError` if bincode deserialization fails
    pub fn deserialize(bytes: &[u8]) -> Result<Self, StorageError> {
        bincode::deserialize(bytes).map_err(|e| {
            StorageError::SerializationError(format!("Bincode deserialization failed: {}", e))
        })
    }

    /// Deserialize and migrate record to current schema version
    ///
    /// # Errors
    ///
    /// Returns `StorageError::SerializationError` for deserialization failures or
    /// `StorageError::InvalidOperation` if the version is unsupported
    pub fn deserialize_and_migrate(bytes: &[u8]) -> Result<Self, StorageError> {
        let record = Self::deserialize(bytes)?;

        if record.version > CURRENT_SCHEMA_VERSION {
            return Err(StorageError::InvalidOperation(format!(
                "Record version {} is newer than current schema version {}",
                record.version, CURRENT_SCHEMA_VERSION
            )));
        }

        // Future: Add migration logic here when schema versions change
        // match record.version {
        //     1 => { /* already current */ }
        //     _ => { /* migrate older versions */ }
        // }

        Ok(record)
    }
}

/// Key builder for hierarchical storage keys
///
/// Provides a fluent API for building consistent hierarchical keys used across
/// adapterOS for adapters, datasets, documents, collections, and other entities.
///
/// # Key Format
///
/// Keys follow the pattern: `entity_type[:tenant_id][:id][:sub_entity:sub_id]...`
///
/// # Examples
///
/// ```rust
/// use adapteros_storage::types::KeyBuilder;
///
/// // Simple entity key
/// let key = KeyBuilder::new("adapters")
///     .id("adapter-123")
///     .build();
/// assert_eq!(key, "adapters:adapter-123");
///
/// // Tenant-scoped entity
/// let key = KeyBuilder::new("datasets")
///     .tenant("default")
///     .id("dataset-456")
///     .build();
/// assert_eq!(key, "datasets:default:dataset-456");
///
/// // Nested entity
/// let key = KeyBuilder::new("collections")
///     .tenant("default")
///     .id("collection-1")
///     .sub("documents", "doc-789")
///     .build();
/// assert_eq!(key, "collections:default:collection-1:documents:doc-789");
/// ```
#[derive(Debug, Clone)]
pub struct KeyBuilder {
    parts: Vec<String>,
}

impl KeyBuilder {
    /// Create a new key builder with an entity type
    ///
    /// # Arguments
    ///
    /// * `entity_type` - The top-level entity type (e.g., "adapters", "datasets")
    pub fn new(entity_type: &str) -> Self {
        Self {
            parts: vec![entity_type.to_string()],
        }
    }

    /// Add a tenant ID to the key hierarchy
    ///
    /// Use this for tenant-isolated resources.
    pub fn tenant(mut self, tenant_id: &str) -> Self {
        self.parts.push(tenant_id.to_string());
        self
    }

    /// Add an entity ID to the key hierarchy
    ///
    /// This is typically the primary identifier for the entity.
    pub fn id(mut self, id: &str) -> Self {
        self.parts.push(id.to_string());
        self
    }

    /// Add a sub-entity to the key hierarchy
    ///
    /// Use this for nested resources (e.g., documents in a collection).
    ///
    /// # Arguments
    ///
    /// * `sub_entity` - The sub-entity type (e.g., "documents", "chunks")
    /// * `sub_id` - The sub-entity identifier
    pub fn sub(mut self, sub_entity: &str, sub_id: &str) -> Self {
        self.parts.push(sub_entity.to_string());
        self.parts.push(sub_id.to_string());
        self
    }

    /// Add an arbitrary segment to the key hierarchy
    ///
    /// Use this for custom key patterns not covered by other methods.
    pub fn segment(mut self, segment: &str) -> Self {
        self.parts.push(segment.to_string());
        self
    }

    /// Build the final key string
    ///
    /// Joins all parts with `:` separator.
    pub fn build(&self) -> String {
        self.parts.join(":")
    }

    /// Get the parts of the key for inspection
    pub fn parts(&self) -> &[String] {
        &self.parts
    }
}

/// Common key patterns for adapterOS entities
impl KeyBuilder {
    /// Create a key for an adapter
    ///
    /// Pattern: `adapters:tenant_id:adapter_id`
    pub fn adapter(tenant_id: &str, adapter_id: &str) -> Self {
        Self::new("adapters").tenant(tenant_id).id(adapter_id)
    }

    /// Create a key for an adapter stack
    ///
    /// Pattern: `stacks:tenant_id:stack_id`
    pub fn stack(tenant_id: &str, stack_id: &str) -> Self {
        Self::new("stacks").tenant(tenant_id).id(stack_id)
    }

    /// Create a key for a training dataset
    ///
    /// Pattern: `datasets:tenant_id:dataset_id`
    pub fn dataset(tenant_id: &str, dataset_id: &str) -> Self {
        Self::new("datasets").tenant(tenant_id).id(dataset_id)
    }

    /// Create a key for a training job
    ///
    /// Pattern: `training:tenant_id:job_id`
    pub fn training_job(tenant_id: &str, job_id: &str) -> Self {
        Self::new("training").tenant(tenant_id).id(job_id)
    }

    /// Create a key for a document
    ///
    /// Pattern: `documents:tenant_id:document_id`
    pub fn document(tenant_id: &str, document_id: &str) -> Self {
        Self::new("documents").tenant(tenant_id).id(document_id)
    }

    /// Create a key for a document collection
    ///
    /// Pattern: `collections:tenant_id:collection_id`
    pub fn collection(tenant_id: &str, collection_id: &str) -> Self {
        Self::new("collections").tenant(tenant_id).id(collection_id)
    }

    /// Create a key for a document chunk
    ///
    /// Pattern: `documents:tenant_id:document_id:chunks:chunk_id`
    pub fn document_chunk(tenant_id: &str, document_id: &str, chunk_id: &str) -> Self {
        Self::new("documents")
            .tenant(tenant_id)
            .id(document_id)
            .sub("chunks", chunk_id)
    }

    /// Create a key for a chat session
    ///
    /// Pattern: `chat:tenant_id:session_id`
    pub fn chat_session(tenant_id: &str, session_id: &str) -> Self {
        Self::new("chat").tenant(tenant_id).id(session_id)
    }

    /// Create a key for a chat message
    ///
    /// Pattern: `chat:tenant_id:session_id:messages:message_id`
    pub fn chat_message(tenant_id: &str, session_id: &str, message_id: &str) -> Self {
        Self::new("chat")
            .tenant(tenant_id)
            .id(session_id)
            .sub("messages", message_id)
    }

    /// Create a key for inference evidence
    ///
    /// Pattern: `evidence:tenant_id:evidence_id`
    pub fn evidence(tenant_id: &str, evidence_id: &str) -> Self {
        Self::new("evidence").tenant(tenant_id).id(evidence_id)
    }

    /// Create a key for a policy pack
    ///
    /// Pattern: `policies:policy_id`
    pub fn policy(policy_id: &str) -> Self {
        Self::new("policies").id(policy_id)
    }

    /// Create a key for audit logs
    ///
    /// Pattern: `audit:tenant_id:log_id`
    pub fn audit_log(tenant_id: &str, log_id: &str) -> Self {
        Self::new("audit").tenant(tenant_id).id(log_id)
    }

    /// Create a key for telemetry bundles
    ///
    /// Pattern: `telemetry:tenant_id:bundle_id`
    pub fn telemetry_bundle(tenant_id: &str, bundle_id: &str) -> Self {
        Self::new("telemetry").tenant(tenant_id).id(bundle_id)
    }

    /// Create a key for telemetry events with deterministic ordering
    ///
    /// Pattern: `telemetry:tenant_id:events:seq`
    pub fn telemetry_event(tenant_id: &str, seq: &str) -> Self {
        Self::new("telemetry").tenant(tenant_id).sub("events", seq)
    }

    /// Create a key for replay metadata
    ///
    /// Pattern: `replay:tenant_id:metadata:inference_id`
    pub fn replay_metadata(tenant_id: &str, inference_id: &str) -> Self {
        Self::new("replay")
            .tenant(tenant_id)
            .sub("metadata", inference_id)
    }

    /// Create a key for replay executions
    ///
    /// Pattern: `replay:tenant_id:executions:execution_id`
    pub fn replay_execution(tenant_id: &str, execution_id: &str) -> Self {
        Self::new("replay")
            .tenant(tenant_id)
            .sub("executions", execution_id)
    }

    /// Create a key for replay sessions
    ///
    /// Pattern: `replay:tenant_id:sessions:session_id`
    pub fn replay_session(tenant_id: &str, session_id: &str) -> Self {
        Self::new("replay")
            .tenant(tenant_id)
            .sub("sessions", session_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Serialize, Deserialize, Debug, PartialEq)]
    struct TestData {
        name: String,
        value: i32,
    }

    #[test]
    fn test_versioned_record_new() {
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };
        let record = VersionedRecord::new(data);

        assert_eq!(record.version, CURRENT_SCHEMA_VERSION);
        assert!(record.created_at > 0);
        assert_eq!(record.created_at, record.updated_at);
        assert_eq!(record.data.name, "test");
        assert_eq!(record.data.value, 42);
    }

    #[test]
    fn test_versioned_record_update() {
        let data = TestData {
            name: "test".to_string(),
            value: 42,
        };
        let mut record = VersionedRecord::new(data);
        let original_created = record.created_at;

        // Delay to ensure timestamps differ (timestamps are in seconds)
        std::thread::sleep(std::time::Duration::from_secs(1));

        let new_data = TestData {
            name: "updated".to_string(),
            value: 100,
        };
        record.update(new_data);

        assert_eq!(record.created_at, original_created);
        assert!(record.updated_at > record.created_at);
        assert_eq!(record.data.name, "updated");
        assert_eq!(record.data.value, 100);
    }

    #[test]
    fn test_versioned_record_serialize_deserialize() {
        let data = TestData {
            name: "serialize-test".to_string(),
            value: 123,
        };
        let record = VersionedRecord::new(data);

        let bytes = record.serialize().expect("Serialization failed");
        let deserialized: VersionedRecord<TestData> =
            VersionedRecord::deserialize(&bytes).expect("Deserialization failed");

        assert_eq!(deserialized.version, record.version);
        assert_eq!(deserialized.created_at, record.created_at);
        assert_eq!(deserialized.updated_at, record.updated_at);
        assert_eq!(deserialized.data, record.data);
    }

    #[test]
    fn test_versioned_record_timestamps() {
        let data = TestData {
            name: "timestamp-test".to_string(),
            value: 456,
        };
        let record = VersionedRecord::new(data);

        let created = record.created_at_utc();
        let updated = record.updated_at_utc();

        assert!(created.timestamp() > 0);
        assert_eq!(created, updated);
    }

    #[test]
    fn test_key_builder_basic() {
        let key = KeyBuilder::new("adapters").id("adapter-123").build();
        assert_eq!(key, "adapters:adapter-123");
    }

    #[test]
    fn test_key_builder_with_tenant() {
        let key = KeyBuilder::new("datasets")
            .tenant("default")
            .id("dataset-456")
            .build();
        assert_eq!(key, "datasets:default:dataset-456");
    }

    #[test]
    fn test_key_builder_with_sub_entity() {
        let key = KeyBuilder::new("collections")
            .tenant("default")
            .id("collection-1")
            .sub("documents", "doc-789")
            .build();
        assert_eq!(key, "collections:default:collection-1:documents:doc-789");
    }

    #[test]
    fn test_key_builder_adapter_pattern() {
        let key = KeyBuilder::adapter("default", "code-review").build();
        assert_eq!(key, "adapters:default:code-review");
    }

    #[test]
    fn test_key_builder_stack_pattern() {
        let key = KeyBuilder::stack("default", "production").build();
        assert_eq!(key, "stacks:default:production");
    }

    #[test]
    fn test_key_builder_dataset_pattern() {
        let key = KeyBuilder::dataset("default", "training-001").build();
        assert_eq!(key, "datasets:default:training-001");
    }

    #[test]
    fn test_key_builder_training_job_pattern() {
        let key = KeyBuilder::training_job("default", "job-123").build();
        assert_eq!(key, "training:default:job-123");
    }

    #[test]
    fn test_key_builder_document_pattern() {
        let key = KeyBuilder::document("default", "manual.pdf").build();
        assert_eq!(key, "documents:default:manual.pdf");
    }

    #[test]
    fn test_key_builder_collection_pattern() {
        let key = KeyBuilder::collection("default", "docs").build();
        assert_eq!(key, "collections:default:docs");
    }

    #[test]
    fn test_key_builder_document_chunk_pattern() {
        let key = KeyBuilder::document_chunk("default", "manual.pdf", "chunk-0").build();
        assert_eq!(key, "documents:default:manual.pdf:chunks:chunk-0");
    }

    #[test]
    fn test_key_builder_chat_session_pattern() {
        let key = KeyBuilder::chat_session("default", "session-abc").build();
        assert_eq!(key, "chat:default:session-abc");
    }

    #[test]
    fn test_key_builder_chat_message_pattern() {
        let key = KeyBuilder::chat_message("default", "session-abc", "msg-1").build();
        assert_eq!(key, "chat:default:session-abc:messages:msg-1");
    }

    #[test]
    fn test_key_builder_evidence_pattern() {
        let key = KeyBuilder::evidence("default", "evidence-xyz").build();
        assert_eq!(key, "evidence:default:evidence-xyz");
    }

    #[test]
    fn test_key_builder_policy_pattern() {
        let key = KeyBuilder::policy("egress").build();
        assert_eq!(key, "policies:egress");
    }

    #[test]
    fn test_key_builder_audit_log_pattern() {
        let key = KeyBuilder::audit_log("default", "log-001").build();
        assert_eq!(key, "audit:default:log-001");
    }

    #[test]
    fn test_key_builder_telemetry_bundle_pattern() {
        let key = KeyBuilder::telemetry_bundle("default", "bundle-2024-01").build();
        assert_eq!(key, "telemetry:default:bundle-2024-01");
    }

    #[test]
    fn test_key_builder_segment() {
        let key = KeyBuilder::new("custom")
            .segment("layer1")
            .segment("layer2")
            .id("final-id")
            .build();
        assert_eq!(key, "custom:layer1:layer2:final-id");
    }

    #[test]
    fn test_key_builder_parts() {
        let builder = KeyBuilder::new("test").tenant("default").id("123");

        let parts = builder.parts();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], "test");
        assert_eq!(parts[1], "default");
        assert_eq!(parts[2], "123");
    }
}
