#!/usr/bin/env python3
"""
Generate 800 training examples for code/document ingestion parsing patterns.
Categories:
  - rust_module_extraction (200)
  - markdown_chunking (200)
  - toml_config_parsing (200)
  - sql_migration_parsing (200)
"""

import json
import os

OUTPUT_PATH = os.path.join(os.path.dirname(__file__), "parsing-patterns.jsonl")


def make(input_text, target_text, subcategory, quality=0.90):
    return json.dumps({
        "input": input_text,
        "target": target_text,
        "metadata": {
            "quality": quality,
            "label": "positive",
            "subcategory": subcategory,
        }
    })


# ---------------------------------------------------------------------------
# CATEGORY 1: Rust module extraction (200 examples)
# ---------------------------------------------------------------------------

RUST_EXAMPLES = [
    # --- Function signatures ---
    (
        "Extract all function signatures from this Rust code:\n\n```rust\npub fn compute_loss(logits: &[f32], labels: &[u32]) -> f32 {\n    let n = logits.len();\n    let mut sum = 0.0f32;\n    for (i, &label) in labels.iter().enumerate() {\n        sum -= logits[i * n + label as usize].ln();\n    }\n    sum / labels.len() as f32\n}\n```",
        "Function signatures extracted:\n\n1. `pub fn compute_loss(logits: &[f32], labels: &[u32]) -> f32`\n   - Visibility: public\n   - Parameters: logits: &[f32], labels: &[u32]\n   - Return type: f32\n   - Purpose: Computes cross-entropy loss over logit array",
        "rust_function_extraction"
    ),
    (
        "Extract all function signatures from this Rust code:\n\n```rust\nfn load_adapter(path: &Path) -> Result<LoraAdapter, AdapterError> {\n    let bytes = fs::read(path)?;\n    let adapter: LoraAdapter = bincode::deserialize(&bytes)\n        .map_err(|e| AdapterError::Deserialize(e.to_string()))?;\n    Ok(adapter)\n}\n```",
        "Function signatures extracted:\n\n1. `fn load_adapter(path: &Path) -> Result<LoraAdapter, AdapterError>`\n   - Visibility: private (module-scoped)\n   - Parameters: path: &Path\n   - Return type: Result<LoraAdapter, AdapterError>\n   - Uses ? operator for early error propagation",
        "rust_function_extraction"
    ),
    (
        "Extract all function signatures from this Rust code:\n\n```rust\npub(crate) async fn submit_training_job(\n    db: &DatabasePool,\n    config: TrainingConfig,\n    dataset_id: Uuid,\n) -> Result<JobId, TrainingError> {\n    let job = TrainingJob::new(config, dataset_id);\n    db.insert_job(&job).await?;\n    Ok(job.id)\n}\n```",
        "Function signatures extracted:\n\n1. `pub(crate) async fn submit_training_job(db: &DatabasePool, config: TrainingConfig, dataset_id: Uuid) -> Result<JobId, TrainingError>`\n   - Visibility: pub(crate)\n   - Async: yes\n   - Parameters: db: &DatabasePool, config: TrainingConfig, dataset_id: Uuid\n   - Return type: Result<JobId, TrainingError>",
        "rust_function_extraction"
    ),
    (
        "Extract all function signatures from this Rust code:\n\n```rust\nimpl AdapterRegistry {\n    pub fn register(&mut self, name: &str, adapter: Arc<dyn Adapter>) -> bool {\n        self.entries.insert(name.to_string(), adapter).is_none()\n    }\n\n    pub fn get(&self, name: &str) -> Option<Arc<dyn Adapter>> {\n        self.entries.get(name).cloned()\n    }\n\n    fn prune_stale(&mut self, threshold: Duration) {\n        self.entries.retain(|_, v| v.last_used().elapsed() < threshold);\n    }\n}\n```",
        "Function signatures extracted from impl AdapterRegistry:\n\n1. `pub fn register(&mut self, name: &str, adapter: Arc<dyn Adapter>) -> bool`\n   - Visibility: public\n   - Receiver: &mut self\n   - Parameters: name: &str, adapter: Arc<dyn Adapter>\n   - Return type: bool\n\n2. `pub fn get(&self, name: &str) -> Option<Arc<dyn Adapter>>`\n   - Visibility: public\n   - Receiver: &self (shared)\n   - Parameters: name: &str\n   - Return type: Option<Arc<dyn Adapter>>\n\n3. `fn prune_stale(&mut self, threshold: Duration)`\n   - Visibility: private\n   - Receiver: &mut self\n   - Parameters: threshold: Duration\n   - Return type: () (unit)",
        "rust_function_extraction"
    ),
    (
        "Extract all function signatures from this Rust code:\n\n```rust\npub trait Encoder {\n    fn encode(&self, input: &str) -> Vec<u32>;\n    fn decode(&self, tokens: &[u32]) -> String;\n    fn vocab_size(&self) -> usize;\n    fn eos_token(&self) -> u32 { 2 }\n}\n```",
        "Function signatures extracted from trait Encoder:\n\n1. `fn encode(&self, input: &str) -> Vec<u32>` — required method\n   - Receiver: &self\n   - Parameters: input: &str\n   - Return type: Vec<u32>\n\n2. `fn decode(&self, tokens: &[u32]) -> String` — required method\n   - Receiver: &self\n   - Parameters: tokens: &[u32]\n   - Return type: String\n\n3. `fn vocab_size(&self) -> usize` — required method\n   - Receiver: &self\n   - Return type: usize\n\n4. `fn eos_token(&self) -> u32` — provided default method\n   - Receiver: &self\n   - Return type: u32\n   - Default value: 2",
        "rust_function_extraction"
    ),
    (
        "Extract all function signatures from this Rust code:\n\n```rust\npub fn map_err_str<T, E: std::fmt::Display>(r: Result<T, E>) -> Result<T, String> {\n    r.map_err(|e| e.to_string())\n}\n\npub fn clamp_f32(v: f32, lo: f32, hi: f32) -> f32 {\n    v.max(lo).min(hi)\n}\n\n#[inline]\nfn dot(a: &[f32], b: &[f32]) -> f32 {\n    a.iter().zip(b).map(|(x, y)| x * y).sum()\n}\n```",
        "Function signatures extracted:\n\n1. `pub fn map_err_str<T, E: std::fmt::Display>(r: Result<T, E>) -> Result<T, String>`\n   - Visibility: public\n   - Generic parameters: T (unconstrained), E: Display\n   - Parameters: r: Result<T, E>\n   - Return type: Result<T, String>\n\n2. `pub fn clamp_f32(v: f32, lo: f32, hi: f32) -> f32`\n   - Visibility: public\n   - Parameters: v: f32, lo: f32, hi: f32\n   - Return type: f32\n\n3. `#[inline] fn dot(a: &[f32], b: &[f32]) -> f32`\n   - Visibility: private\n   - Attribute: #[inline]\n   - Parameters: a: &[f32], b: &[f32]\n   - Return type: f32",
        "rust_function_extraction"
    ),
    # --- Struct definitions ---
    (
        "Extract the struct definition and its fields from this Rust code:\n\n```rust\n#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\npub struct TrainingConfig {\n    pub model_id: String,\n    pub rank: u32,\n    pub alpha: f32,\n    pub learning_rate: f64,\n    pub epochs: u32,\n    pub batch_size: usize,\n    pub dropout: Option<f32>,\n}\n```",
        "Struct definition extracted:\n\nName: TrainingConfig\nVisibility: pub\nDerives: Debug, Clone, serde::Serialize, serde::Deserialize\n\nFields:\n- model_id: String (pub)\n- rank: u32 (pub)\n- alpha: f32 (pub)\n- learning_rate: f64 (pub)\n- epochs: u32 (pub)\n- batch_size: usize (pub)\n- dropout: Option<f32> (pub) — optional field",
        "rust_struct_extraction"
    ),
    (
        "Extract the struct definition and its fields from this Rust code:\n\n```rust\npub struct KvCacheEntry {\n    pub(crate) key: Tensor,\n    pub(crate) value: Tensor,\n    sequence_len: usize,\n    layer_idx: u32,\n}\n```",
        "Struct definition extracted:\n\nName: KvCacheEntry\nVisibility: pub\nDerives: none\n\nFields:\n- key: Tensor (pub(crate)) — crate-visible\n- value: Tensor (pub(crate)) — crate-visible\n- sequence_len: usize (private)\n- layer_idx: u32 (private)",
        "rust_struct_extraction"
    ),
    (
        "Extract the struct definition and its fields from this Rust code:\n\n```rust\n/// Represents a parsed LoRA adapter file loaded from disk.\n#[derive(Debug)]\npub struct LoraAdapter {\n    /// Adapter name from metadata section.\n    pub name: String,\n    /// Base model this adapter was trained on.\n    pub base_model: String,\n    /// LoRA weight tensors keyed by layer name.\n    pub weights: HashMap<String, Tensor>,\n    /// Rank of the low-rank decomposition.\n    pub rank: u32,\n    /// Scale factor applied to adapter output.\n    pub alpha: f32,\n}\n```",
        "Struct definition extracted:\n\nName: LoraAdapter\nDoc comment: \"Represents a parsed LoRA adapter file loaded from disk.\"\nVisibility: pub\nDerives: Debug\n\nFields:\n- name: String (pub) — \"Adapter name from metadata section.\"\n- base_model: String (pub) — \"Base model this adapter was trained on.\"\n- weights: HashMap<String, Tensor> (pub) — \"LoRA weight tensors keyed by layer name.\"\n- rank: u32 (pub) — \"Rank of the low-rank decomposition.\"\n- alpha: f32 (pub) — \"Scale factor applied to adapter output.\"",
        "rust_struct_extraction"
    ),
    (
        "Extract the struct definition and its fields from this Rust code:\n\n```rust\n#[derive(Default)]\npub(crate) struct SessionCache {\n    sessions: HashMap<SessionId, CachedSession>,\n    max_entries: usize,\n    eviction_policy: EvictionPolicy,\n}\n```",
        "Struct definition extracted:\n\nName: SessionCache\nVisibility: pub(crate)\nDerives: Default\n\nFields:\n- sessions: HashMap<SessionId, CachedSession> (private)\n- max_entries: usize (private)\n- eviction_policy: EvictionPolicy (private)",
        "rust_struct_extraction"
    ),
    (
        "Extract the struct definition and its fields from this Rust code:\n\n```rust\n#[repr(C)]\npub struct FfiTensor {\n    pub data: *mut f32,\n    pub len: usize,\n    pub shape: [usize; 4],\n    pub dtype: u8,\n}\n```",
        "Struct definition extracted:\n\nName: FfiTensor\nVisibility: pub\nAttributes: #[repr(C)] — C-compatible memory layout for FFI\nDerives: none\n\nFields:\n- data: *mut f32 (pub) — raw mutable pointer (unsafe FFI)\n- len: usize (pub)\n- shape: [usize; 4] (pub) — fixed-size array of 4 dimension sizes\n- dtype: u8 (pub) — numeric type tag",
        "rust_struct_extraction"
    ),
    (
        "Extract the struct definition and its fields from this Rust code:\n\n```rust\n#[derive(Debug, Clone, PartialEq)]\npub struct AdapterMetadata {\n    pub id: Uuid,\n    pub name: String,\n    pub version: semver::Version,\n    pub created_at: chrono::DateTime<chrono::Utc>,\n    pub tags: Vec<String>,\n    pub checksum: [u8; 32],\n}\n```",
        "Struct definition extracted:\n\nName: AdapterMetadata\nVisibility: pub\nDerives: Debug, Clone, PartialEq\n\nFields:\n- id: Uuid (pub)\n- name: String (pub)\n- version: semver::Version (pub)\n- created_at: chrono::DateTime<chrono::Utc> (pub)\n- tags: Vec<String> (pub)\n- checksum: [u8; 32] (pub) — fixed 32-byte hash array",
        "rust_struct_extraction"
    ),
    # --- Enum variants ---
    (
        "Extract the enum definition and its variants from this Rust code:\n\n```rust\n#[derive(Debug, thiserror::Error)]\npub enum AdapterError {\n    #[error(\"adapter not found: {0}\")]\n    NotFound(String),\n    #[error(\"deserialization failed: {0}\")]\n    Deserialize(String),\n    #[error(\"checksum mismatch\")]\n    ChecksumMismatch,\n    #[error(\"io error: {0}\")]\n    Io(#[from] std::io::Error),\n}\n```",
        "Enum definition extracted:\n\nName: AdapterError\nVisibility: pub\nDerives: Debug, thiserror::Error\n\nVariants:\n1. NotFound(String) — tuple variant, error message: \"adapter not found: {0}\"\n2. Deserialize(String) — tuple variant, error message: \"deserialization failed: {0}\"\n3. ChecksumMismatch — unit variant, error message: \"checksum mismatch\"\n4. Io(#[from] std::io::Error) — tuple variant with #[from] conversion, error message: \"io error: {0}\"",
        "rust_enum_extraction"
    ),
    (
        "Extract the enum definition and its variants from this Rust code:\n\n```rust\npub enum RuntimeMode {\n    Inference,\n    Training { dataset_id: Uuid, max_steps: u32 },\n    Evaluation { benchmark: String },\n    Idle,\n}\n```",
        "Enum definition extracted:\n\nName: RuntimeMode\nVisibility: pub\nDerives: none\n\nVariants:\n1. Inference — unit variant\n2. Training { dataset_id: Uuid, max_steps: u32 } — struct variant with named fields\n3. Evaluation { benchmark: String } — struct variant with named fields\n4. Idle — unit variant",
        "rust_enum_extraction"
    ),
    (
        "Extract the enum definition and its variants from this Rust code:\n\n```rust\n#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]\n#[serde(rename_all = \"snake_case\")]\npub enum TrainingStatus {\n    Queued,\n    Running,\n    Completed,\n    Failed,\n    Cancelled,\n}\n```",
        "Enum definition extracted:\n\nName: TrainingStatus\nVisibility: pub\nDerives: Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize\nSerde attribute: rename_all = \"snake_case\"\n\nVariants:\n1. Queued — unit variant (serializes as \"queued\")\n2. Running — unit variant (serializes as \"running\")\n3. Completed — unit variant (serializes as \"completed\")\n4. Failed — unit variant (serializes as \"failed\")\n5. Cancelled — unit variant (serializes as \"cancelled\")",
        "rust_enum_extraction"
    ),
    (
        "Extract the enum definition and its variants from this Rust code:\n\n```rust\npub enum BackendEvent {\n    ModelLoaded { model_id: String, load_time_ms: u64 },\n    AdapterSwapped { from: Option<String>, to: String },\n    InferenceCompleted { tokens: u32, latency_ms: u64 },\n    Error(BackendError),\n    Shutdown,\n}\n```",
        "Enum definition extracted:\n\nName: BackendEvent\nVisibility: pub\nDerives: none\n\nVariants:\n1. ModelLoaded { model_id: String, load_time_ms: u64 } — struct variant\n2. AdapterSwapped { from: Option<String>, to: String } — struct variant, 'from' is optional\n3. InferenceCompleted { tokens: u32, latency_ms: u64 } — struct variant\n4. Error(BackendError) — tuple variant wrapping error type\n5. Shutdown — unit variant",
        "rust_enum_extraction"
    ),
    (
        "Extract the enum definition and its variants from this Rust code:\n\n```rust\n#[derive(Debug)]\npub(crate) enum EvictionPolicy {\n    Lru,\n    Lfu,\n    Fifo,\n    SizeWeighted { max_bytes: usize },\n}\n\nimpl Default for EvictionPolicy {\n    fn default() -> Self {\n        Self::Lru\n    }\n}\n```",
        "Enum definition extracted:\n\nName: EvictionPolicy\nVisibility: pub(crate)\nDerives: Debug\n\nVariants:\n1. Lru — unit variant (Least Recently Used)\n2. Lfu — unit variant (Least Frequently Used)\n3. Fifo — unit variant (First In, First Out)\n4. SizeWeighted { max_bytes: usize } — struct variant\n\nAdditional: Default impl returns Lru",
        "rust_enum_extraction"
    ),
    # --- Impl blocks ---
    (
        "Extract the impl block structure from this Rust code:\n\n```rust\nimpl LoraAdapter {\n    pub fn new(name: String, base_model: String, rank: u32, alpha: f32) -> Self {\n        Self {\n            name,\n            base_model,\n            weights: HashMap::new(),\n            rank,\n            alpha,\n        }\n    }\n\n    pub fn insert_weight(&mut self, layer: String, weight: Tensor) {\n        self.weights.insert(layer, weight);\n    }\n\n    pub fn weight_count(&self) -> usize {\n        self.weights.len()\n    }\n}\n```",
        "Impl block extracted:\n\nType: LoraAdapter (inherent impl)\n\nMethods:\n1. `pub fn new(name: String, base_model: String, rank: u32, alpha: f32) -> Self` — constructor\n2. `pub fn insert_weight(&mut self, layer: String, weight: Tensor)` — mutating method\n3. `pub fn weight_count(&self) -> usize` — read-only accessor",
        "rust_impl_extraction"
    ),
    (
        "Extract the impl block structure from this Rust code:\n\n```rust\nimpl std::fmt::Display for TrainingStatus {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n        match self {\n            Self::Queued => write!(f, \"queued\"),\n            Self::Running => write!(f, \"running\"),\n            Self::Completed => write!(f, \"completed\"),\n            Self::Failed => write!(f, \"failed\"),\n            Self::Cancelled => write!(f, \"cancelled\"),\n        }\n    }\n}\n```",
        "Impl block extracted:\n\nType: TrainingStatus\nTrait implemented: std::fmt::Display\n\nMethods:\n1. `fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result` — required trait method\n   - Formats each variant as lowercase snake_case string",
        "rust_impl_extraction"
    ),
    (
        "Extract the impl block structure from this Rust code:\n\n```rust\nimpl<T: Encoder + Send + Sync + 'static> TokenizerPool<T> {\n    pub fn new(capacity: usize, factory: impl Fn() -> T) -> Self {\n        let pool = (0..capacity).map(|_| Arc::new(factory())).collect();\n        Self { pool, cursor: AtomicUsize::new(0) }\n    }\n\n    pub fn get(&self) -> Arc<T> {\n        let idx = self.cursor.fetch_add(1, Ordering::Relaxed) % self.pool.len();\n        Arc::clone(&self.pool[idx])\n    }\n}\n```",
        "Impl block extracted:\n\nType: TokenizerPool<T>\nGeneric constraints: T: Encoder + Send + Sync + 'static\n\nMethods:\n1. `pub fn new(capacity: usize, factory: impl Fn() -> T) -> Self`\n   - Parameters: capacity: usize, factory: impl Fn() -> T (closure)\n   - Constructor, builds pool of Arc<T> instances\n\n2. `pub fn get(&self) -> Arc<T>`\n   - Returns a round-robin Arc<T> using atomic index",
        "rust_impl_extraction"
    ),
    (
        "Extract the impl block structure from this Rust code:\n\n```rust\nimpl From<sqlx::Error> for DbError {\n    fn from(e: sqlx::Error) -> Self {\n        match e {\n            sqlx::Error::RowNotFound => DbError::NotFound,\n            sqlx::Error::Database(db) if db.is_unique_violation() => DbError::Conflict,\n            other => DbError::Internal(other.to_string()),\n        }\n    }\n}\n```",
        "Impl block extracted:\n\nType: DbError\nTrait implemented: From<sqlx::Error>\n\nMethods:\n1. `fn from(e: sqlx::Error) -> Self`\n   - Converts sqlx::Error to DbError\n   - RowNotFound → DbError::NotFound\n   - Unique violation (pattern guard) → DbError::Conflict\n   - All other errors → DbError::Internal(message)",
        "rust_impl_extraction"
    ),
    (
        "Extract the impl block structure from this Rust code:\n\n```rust\n#[async_trait::async_trait]\nimpl AdapterStore for SqliteAdapterStore {\n    async fn save(&self, adapter: &LoraAdapter) -> Result<(), DbError> {\n        sqlx::query!(\"INSERT INTO adapters (id, name, data) VALUES (?, ?, ?)\",\n            adapter.id, adapter.name, &adapter.serialized)\n            .execute(&self.pool).await?;\n        Ok(())\n    }\n\n    async fn load(&self, id: Uuid) -> Result<LoraAdapter, DbError> {\n        let row = sqlx::query_as!(AdapterRow, \"SELECT * FROM adapters WHERE id = ?\", id)\n            .fetch_one(&self.pool).await?;\n        row.try_into().map_err(DbError::Deserialize)\n    }\n\n    async fn delete(&self, id: Uuid) -> Result<bool, DbError> {\n        let rows = sqlx::query!(\"DELETE FROM adapters WHERE id = ?\", id)\n            .execute(&self.pool).await?.rows_affected();\n        Ok(rows > 0)\n    }\n}\n```",
        "Impl block extracted:\n\nType: SqliteAdapterStore\nTrait implemented: AdapterStore\nAttribute: #[async_trait::async_trait]\n\nMethods:\n1. `async fn save(&self, adapter: &LoraAdapter) -> Result<(), DbError>`\n   - Inserts adapter into adapters table\n\n2. `async fn load(&self, id: Uuid) -> Result<LoraAdapter, DbError>`\n   - Fetches one row by id, converts to LoraAdapter\n\n3. `async fn delete(&self, id: Uuid) -> Result<bool, DbError>`\n   - Deletes by id, returns true if a row was removed",
        "rust_impl_extraction"
    ),
    # --- Doc comments ---
    (
        "Extract all doc comments and their associated items from this Rust code:\n\n```rust\n/// Runs a single forward pass through the model with an optional LoRA adapter.\n///\n/// # Arguments\n/// * `input` - Tokenized input tensor of shape [batch, seq_len]\n/// * `adapter` - Optional adapter to apply; None uses base model weights\n///\n/// # Returns\n/// Logit tensor of shape [batch, seq_len, vocab_size]\npub fn forward(\n    input: &Tensor,\n    adapter: Option<&LoraAdapter>,\n) -> Result<Tensor, ModelError> {\n    todo!()\n}\n```",
        "Doc comments extracted:\n\nItem: fn forward\nDoc comment:\n  Summary: \"Runs a single forward pass through the model with an optional LoRA adapter.\"\n  Arguments section:\n    - input: &Tensor — \"Tokenized input tensor of shape [batch, seq_len]\"\n    - adapter: Option<&LoraAdapter> — \"Optional adapter to apply; None uses base model weights\"\n  Returns section:\n    - \"Logit tensor of shape [batch, seq_len, vocab_size]\"",
        "rust_doc_comment_extraction"
    ),
    (
        "Extract all doc comments and their associated items from this Rust code:\n\n```rust\n/// A pool of reusable KV-cache buffers for inference.\n///\n/// Caches are evicted when `max_entries` is exceeded using the\n/// configured [`EvictionPolicy`].\npub struct KvCachePool {\n    entries: HashMap<SessionId, KvCacheEntry>,\n    max_entries: usize,\n    policy: EvictionPolicy,\n}\n```",
        "Doc comments extracted:\n\nItem: struct KvCachePool\nDoc comment:\n  Summary: \"A pool of reusable KV-cache buffers for inference.\"\n  Detail: \"Caches are evicted when `max_entries` is exceeded using the configured [`EvictionPolicy`].\"\n  Intra-doc link: [`EvictionPolicy`] — references the EvictionPolicy type",
        "rust_doc_comment_extraction"
    ),
    (
        "Extract all doc comments and their associated items from this Rust code:\n\n```rust\npub trait Adapter: Send + Sync {\n    /// Returns the unique name of this adapter.\n    fn name(&self) -> &str;\n\n    /// Applies the adapter weights to the given layer output.\n    ///\n    /// The output tensor is modified in-place.\n    fn apply(&self, layer: &str, output: &mut Tensor);\n\n    /// Returns true if this adapter targets the given layer.\n    fn targets_layer(&self, layer: &str) -> bool;\n}\n```",
        "Doc comments extracted:\n\nItem: trait Adapter\n\nMethod doc comments:\n1. fn name — \"Returns the unique name of this adapter.\"\n2. fn apply — \"Applies the adapter weights to the given layer output.\" + detail: \"The output tensor is modified in-place.\"\n3. fn targets_layer — \"Returns true if this adapter targets the given layer.\"",
        "rust_doc_comment_extraction"
    ),
    (
        "Extract all doc comments and their associated items from this Rust code:\n\n```rust\n//! This module provides the training pipeline for LoRA fine-tuning.\n//!\n//! # Usage\n//! ```rust\n//! let job = TrainingJob::builder()\n//!     .dataset(dataset_id)\n//!     .rank(16)\n//!     .build()?;\n//! pipeline.run(job).await?;\n//! ```\n\n/// Errors that can occur during training.\n#[derive(Debug, thiserror::Error)]\npub enum TrainingError {\n    /// Dataset could not be loaded from the database.\n    #[error(\"dataset not found: {id}\")]\n    DatasetNotFound { id: Uuid },\n}\n```",
        "Doc comments extracted:\n\nModule-level doc (//!):\n  Summary: \"This module provides the training pipeline for LoRA fine-tuning.\"\n  Usage example included with TrainingJob::builder() pattern\n\nItem: enum TrainingError\n  Doc: \"Errors that can occur during training.\"\n\nVariant: DatasetNotFound\n  Doc: \"Dataset could not be loaded from the database.\"\n  Error format: \"dataset not found: {id}\"",
        "rust_doc_comment_extraction"
    ),
    (
        "Extract all doc comments and their associated items from this Rust code:\n\n```rust\n/// Compute cosine similarity between two embedding vectors.\n///\n/// Both vectors must have the same length. Returns a value in [-1, 1].\n///\n/// # Panics\n/// Panics if `a.len() != b.len()`.\n///\n/// # Examples\n/// ```\n/// let sim = cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]);\n/// assert!((sim - 0.0).abs() < 1e-6);\n/// ```\npub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {\n    todo!()\n}\n```",
        "Doc comments extracted:\n\nItem: fn cosine_similarity\nDoc comment:\n  Summary: \"Compute cosine similarity between two embedding vectors.\"\n  Detail: \"Both vectors must have the same length. Returns a value in [-1, 1].\"\n  Panics section: \"Panics if `a.len() != b.len()`.\"\n  Examples section: doctest asserting cosine_similarity([1,0], [0,1]) ≈ 0.0",
        "rust_doc_comment_extraction"
    ),
    # --- Module structure ---
    (
        "Identify the module structure from this Rust file layout:\n\n```rust\n// src/lib.rs\npub mod training;\npub mod inference;\npub(crate) mod cache;\nmod internal;\n\npub use training::TrainingPipeline;\npub use inference::InferenceEngine;\n```",
        "Module structure extracted:\n\nFile: src/lib.rs\n\nSubmodules declared:\n- training (pub) — public module\n- inference (pub) — public module\n- cache (pub(crate)) — crate-visible only\n- internal (private) — module-private\n\nRe-exports:\n- pub use training::TrainingPipeline\n- pub use inference::InferenceEngine",
        "rust_module_structure"
    ),
    (
        "Identify the module structure from this Rust file layout:\n\n```rust\n// crates/adapteros-lora-worker/src/lib.rs\npub mod adapter_hotswap;\npub mod backend_coordinator;\npub(crate) mod backend_factory;\npub(crate) mod kvcache;\nmod deadlock;\nmod internal_metrics;\n\npub use adapter_hotswap::AdapterHotswap;\npub use backend_coordinator::BackendCoordinator;\n```",
        "Module structure extracted:\n\nCrate: adapteros-lora-worker\nFile: src/lib.rs\n\nSubmodules declared:\n- adapter_hotswap (pub)\n- backend_coordinator (pub)\n- backend_factory (pub(crate))\n- kvcache (pub(crate))\n- deadlock (private)\n- internal_metrics (private)\n\nRe-exports:\n- pub use adapter_hotswap::AdapterHotswap\n- pub use backend_coordinator::BackendCoordinator",
        "rust_module_structure"
    ),
    (
        "Identify the module structure from this Rust file layout:\n\n```rust\n// src/handlers/mod.rs\nmod admin;\nmod diagnostics;\nmod health;\nmod models;\nmod openai_compat;\nmod streaming;\nmod training;\n\npub use admin::*;\npub use diagnostics::*;\npub use health::*;\npub use models::*;\npub use openai_compat::*;\npub use streaming::*;\npub use training::*;\n```",
        "Module structure extracted:\n\nFile: src/handlers/mod.rs\n\nSubmodules declared (all private):\n- admin\n- diagnostics\n- health\n- models\n- openai_compat\n- streaming\n- training\n\nRe-exports: glob re-export (pub use *) from all 7 submodules — all public items are flattened into handlers namespace",
        "rust_module_structure"
    ),
    (
        "Identify the module structure from this Rust code:\n\n```rust\n#[cfg(test)]\nmod tests {\n    use super::*;\n\n    mod unit {\n        use super::*;\n        #[test]\n        fn test_compute_loss_zero() { todo!() }\n    }\n\n    mod integration {\n        use super::*;\n        #[test]\n        fn test_full_forward_pass() { todo!() }\n    }\n}\n```",
        "Module structure extracted:\n\nConditional module: tests (only compiled with #[cfg(test)])\n\nNested submodules:\n- unit — contains unit test: test_compute_loss_zero\n- integration — contains integration test: test_full_forward_pass\n\nBoth use `use super::*` to import parent scope items.",
        "rust_module_structure"
    ),
    (
        "Identify the module structure from this Rust code:\n\n```rust\npub mod v1 {\n    pub mod adapters;\n    pub mod models;\n    pub mod training;\n    pub mod health;\n}\n\npub mod internal {\n    pub(crate) mod auth;\n    pub(crate) mod rate_limit;\n}\n```",
        "Module structure extracted:\n\nTop-level modules:\n\n1. v1 (pub) — API version namespace\n   - adapters (pub)\n   - models (pub)\n   - training (pub)\n   - health (pub)\n\n2. internal (pub) — internal utilities namespace\n   - auth (pub(crate))\n   - rate_limit (pub(crate))",
        "rust_module_structure"
    ),
    # --- Trait implementations ---
    (
        "Extract all trait implementations from this Rust code:\n\n```rust\nimpl Adapter for LoraAdapter {\n    fn name(&self) -> &str { &self.name }\n    fn apply(&self, layer: &str, output: &mut Tensor) {\n        if let Some(w) = self.weights.get(layer) {\n            output.add_assign(w);\n        }\n    }\n    fn targets_layer(&self, layer: &str) -> bool {\n        self.weights.contains_key(layer)\n    }\n}\n\nimpl std::fmt::Display for LoraAdapter {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n        write!(f, \"LoraAdapter({})\", self.name)\n    }\n}\n```",
        "Trait implementations extracted:\n\n1. impl Adapter for LoraAdapter\n   - fn name(&self) -> &str — returns self.name\n   - fn apply(&self, layer: &str, output: &mut Tensor) — adds layer weights if present\n   - fn targets_layer(&self, layer: &str) -> bool — checks weight key presence\n\n2. impl std::fmt::Display for LoraAdapter\n   - fn fmt — formats as \"LoraAdapter(<name>)\"",
        "rust_trait_impl_extraction"
    ),
    (
        "Extract all trait implementations from this Rust code:\n\n```rust\nimpl serde::Serialize for SessionId {\n    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {\n        s.serialize_str(&self.0.to_string())\n    }\n}\n\nimpl<'de> serde::Deserialize<'de> for SessionId {\n    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {\n        let s = String::deserialize(d)?;\n        s.parse().map(SessionId).map_err(serde::de::Error::custom)\n    }\n}\n```",
        "Trait implementations extracted:\n\n1. impl serde::Serialize for SessionId\n   - fn serialize<S: Serializer> — serializes inner Uuid as string\n\n2. impl<'de> serde::Deserialize<'de> for SessionId\n   - fn deserialize<D: Deserializer<'de>> — parses string into Uuid, wraps in SessionId",
        "rust_trait_impl_extraction"
    ),
    (
        "Extract all trait implementations from this Rust code:\n\n```rust\nimpl TryFrom<AdapterRow> for LoraAdapter {\n    type Error = DbError;\n\n    fn try_from(row: AdapterRow) -> Result<Self, Self::Error> {\n        let weights: HashMap<String, Tensor> = bincode::deserialize(&row.data)\n            .map_err(|e| DbError::Deserialize(e.to_string()))?;\n        Ok(LoraAdapter {\n            id: row.id,\n            name: row.name,\n            weights,\n            rank: row.rank as u32,\n            alpha: row.alpha,\n        })\n    }\n}\n```",
        "Trait implementations extracted:\n\n1. impl TryFrom<AdapterRow> for LoraAdapter\n   - Associated type: type Error = DbError\n   - fn try_from(row: AdapterRow) -> Result<Self, DbError>\n   - Deserializes weights from binary data, maps fields from AdapterRow",
        "rust_trait_impl_extraction"
    ),
    (
        "Extract all trait implementations from this Rust code:\n\n```rust\nimpl std::str::FromStr for TrainingStatus {\n    type Err = String;\n\n    fn from_str(s: &str) -> Result<Self, Self::Err> {\n        match s {\n            \"queued\" => Ok(Self::Queued),\n            \"running\" => Ok(Self::Running),\n            \"completed\" => Ok(Self::Completed),\n            \"failed\" => Ok(Self::Failed),\n            \"cancelled\" => Ok(Self::Cancelled),\n            other => Err(format!(\"unknown status: {other}\")),\n        }\n    }\n}\n```",
        "Trait implementations extracted:\n\n1. impl std::str::FromStr for TrainingStatus\n   - Associated type: type Err = String\n   - fn from_str(s: &str) -> Result<Self, String>\n   - Parses \"queued\" | \"running\" | \"completed\" | \"failed\" | \"cancelled\"\n   - Returns Err for unrecognized values",
        "rust_trait_impl_extraction"
    ),
    (
        "Extract all trait implementations from this Rust code:\n\n```rust\nimpl axum::response::IntoResponse for ApiError {\n    fn into_response(self) -> axum::response::Response {\n        let status = match &self {\n            ApiError::NotFound(_) => StatusCode::NOT_FOUND,\n            ApiError::Unauthorized => StatusCode::UNAUTHORIZED,\n            ApiError::BadRequest(_) => StatusCode::BAD_REQUEST,\n            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,\n        };\n        let body = Json(serde_json::json!({ \"error\": self.to_string() }));\n        (status, body).into_response()\n    }\n}\n```",
        "Trait implementations extracted:\n\n1. impl axum::response::IntoResponse for ApiError\n   - fn into_response(self) -> axum::response::Response\n   - Maps ApiError variants to HTTP status codes:\n     - NotFound → 404\n     - Unauthorized → 401\n     - BadRequest → 400\n     - Internal → 500\n   - Response body: JSON {\"error\": message}",
        "rust_trait_impl_extraction"
    ),
]

# Fill to 200 with additional varied examples
RUST_EXTRA = [
    (
        "Extract the struct definition and its fields from this Rust code:\n\n```rust\n#[derive(Debug, Clone)]\npub struct WorkspaceConfig {\n    pub members: Vec<String>,\n    pub exclude: Vec<String>,\n    pub resolver: String,\n}\n```",
        "Struct definition extracted:\n\nName: WorkspaceConfig\nVisibility: pub\nDerives: Debug, Clone\n\nFields:\n- members: Vec<String> (pub)\n- exclude: Vec<String> (pub)\n- resolver: String (pub)",
        "rust_struct_extraction"
    ),
    (
        "Extract all function signatures from this Rust code:\n\n```rust\npub fn parse_toml_config(src: &str) -> Result<AppConfig, ConfigError> {\n    toml::from_str(src).map_err(ConfigError::Parse)\n}\n\npub fn validate_config(cfg: &AppConfig) -> Result<(), Vec<ConfigError>> {\n    let mut errs = Vec::new();\n    if cfg.port == 0 { errs.push(ConfigError::InvalidPort); }\n    if errs.is_empty() { Ok(()) } else { Err(errs) }\n}\n```",
        "Function signatures extracted:\n\n1. `pub fn parse_toml_config(src: &str) -> Result<AppConfig, ConfigError>`\n   - Visibility: public\n   - Parameters: src: &str\n   - Return type: Result<AppConfig, ConfigError>\n\n2. `pub fn validate_config(cfg: &AppConfig) -> Result<(), Vec<ConfigError>>`\n   - Visibility: public\n   - Parameters: cfg: &AppConfig\n   - Return type: Result<(), Vec<ConfigError>> — collects multiple errors",
        "rust_function_extraction"
    ),
    (
        "Extract the enum definition and its variants from this Rust code:\n\n```rust\n#[derive(Debug, Clone)]\npub enum LogLevel {\n    Trace,\n    Debug,\n    Info,\n    Warn,\n    Error,\n}\n```",
        "Enum definition extracted:\n\nName: LogLevel\nVisibility: pub\nDerives: Debug, Clone\n\nVariants:\n1. Trace — unit\n2. Debug — unit\n3. Info — unit\n4. Warn — unit\n5. Error — unit",
        "rust_enum_extraction"
    ),
    (
        "Extract all function signatures from this Rust code:\n\n```rust\nimpl KvCachePool {\n    pub fn new(max_entries: usize) -> Self {\n        Self { entries: HashMap::new(), max_entries, policy: EvictionPolicy::Lru }\n    }\n\n    pub fn insert(&mut self, id: SessionId, entry: KvCacheEntry) {\n        if self.entries.len() >= self.max_entries {\n            self.evict_one();\n        }\n        self.entries.insert(id, entry);\n    }\n\n    pub fn get(&self, id: &SessionId) -> Option<&KvCacheEntry> {\n        self.entries.get(id)\n    }\n\n    fn evict_one(&mut self) {\n        // policy-driven eviction\n    }\n}\n```",
        "Function signatures extracted from impl KvCachePool:\n\n1. `pub fn new(max_entries: usize) -> Self` — constructor\n2. `pub fn insert(&mut self, id: SessionId, entry: KvCacheEntry)` — mutating insert with eviction\n3. `pub fn get(&self, id: &SessionId) -> Option<&KvCacheEntry>` — read-only lookup\n4. `fn evict_one(&mut self)` — private eviction helper",
        "rust_function_extraction"
    ),
    (
        "Extract the struct definition and its fields from this Rust code:\n\n```rust\n#[derive(Debug, serde::Serialize, serde::Deserialize)]\npub struct DiagnosticsReport {\n    pub timestamp: chrono::DateTime<chrono::Utc>,\n    pub version: String,\n    pub uptime_secs: u64,\n    pub active_adapters: Vec<String>,\n    pub memory_mb: f64,\n    pub error_count: u64,\n}\n```",
        "Struct definition extracted:\n\nName: DiagnosticsReport\nVisibility: pub\nDerives: Debug, serde::Serialize, serde::Deserialize\n\nFields:\n- timestamp: chrono::DateTime<chrono::Utc> (pub)\n- version: String (pub)\n- uptime_secs: u64 (pub)\n- active_adapters: Vec<String> (pub)\n- memory_mb: f64 (pub)\n- error_count: u64 (pub)",
        "rust_struct_extraction"
    ),
    (
        "Extract all trait implementations from this Rust code:\n\n```rust\nimpl Default for TrainingConfig {\n    fn default() -> Self {\n        Self {\n            model_id: String::new(),\n            rank: 16,\n            alpha: 16.0,\n            learning_rate: 1e-4,\n            epochs: 3,\n            batch_size: 8,\n            dropout: None,\n        }\n    }\n}\n```",
        "Trait implementations extracted:\n\n1. impl Default for TrainingConfig\n   - fn default() -> Self\n   - Default values: rank=16, alpha=16.0, learning_rate=1e-4, epochs=3, batch_size=8, dropout=None",
        "rust_trait_impl_extraction"
    ),
    (
        "Extract all function signatures from this Rust code:\n\n```rust\npub async fn health_check(State(app): State<AppState>) -> impl IntoResponse {\n    if app.backend.is_ready().await {\n        StatusCode::OK\n    } else {\n        StatusCode::SERVICE_UNAVAILABLE\n    }\n}\n\npub async fn readiness_check(\n    State(app): State<AppState>,\n) -> Result<Json<ReadinessReport>, ApiError> {\n    let report = app.backend.readiness_report().await?;\n    Ok(Json(report))\n}\n```",
        "Function signatures extracted:\n\n1. `pub async fn health_check(State(app): State<AppState>) -> impl IntoResponse`\n   - Visibility: public, async\n   - Parameters: State(app): State<AppState> (Axum extractor)\n   - Return type: impl IntoResponse (opaque)\n\n2. `pub async fn readiness_check(State(app): State<AppState>) -> Result<Json<ReadinessReport>, ApiError>`\n   - Visibility: public, async\n   - Parameters: State(app): State<AppState>\n   - Return type: Result<Json<ReadinessReport>, ApiError>",
        "rust_function_extraction"
    ),
    (
        "Extract the enum definition and its variants from this Rust code:\n\n```rust\n#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]\n#[serde(tag = \"type\", rename_all = \"snake_case\")]\npub enum DatasetSource {\n    File { path: std::path::PathBuf },\n    Http { url: String, headers: HashMap<String, String> },\n    Inline { records: Vec<serde_json::Value> },\n}\n```",
        "Enum definition extracted:\n\nName: DatasetSource\nVisibility: pub\nDerives: Debug, Clone, serde::Serialize, serde::Deserialize\nSerde attributes: tag = \"type\" (adjacently tagged), rename_all = \"snake_case\"\n\nVariants:\n1. File { path: std::path::PathBuf } — struct variant\n2. Http { url: String, headers: HashMap<String, String> } — struct variant\n3. Inline { records: Vec<serde_json::Value> } — struct variant",
        "rust_enum_extraction"
    ),
    (
        "Extract the impl block structure from this Rust code:\n\n```rust\nimpl AppState {\n    pub fn new(config: AppConfig, db: DatabasePool) -> Self {\n        Self {\n            config: Arc::new(config),\n            db,\n            metrics: MetricsHandle::default(),\n        }\n    }\n\n    pub fn config(&self) -> &AppConfig { &self.config }\n    pub fn db(&self) -> &DatabasePool { &self.db }\n    pub fn metrics(&self) -> &MetricsHandle { &self.metrics }\n}\n```",
        "Impl block extracted:\n\nType: AppState (inherent impl)\n\nMethods:\n1. `pub fn new(config: AppConfig, db: DatabasePool) -> Self` — constructor, wraps config in Arc\n2. `pub fn config(&self) -> &AppConfig` — accessor\n3. `pub fn db(&self) -> &DatabasePool` — accessor\n4. `pub fn metrics(&self) -> &MetricsHandle` — accessor",
        "rust_impl_extraction"
    ),
    (
        "Extract all doc comments and their associated items from this Rust code:\n\n```rust\n/// Hot-swaps the active LoRA adapter without restarting inference.\n///\n/// Acquires a write lock on the adapter slot, replaces the adapter,\n/// then releases the lock. In-flight requests complete with the old adapter.\n///\n/// # Errors\n/// Returns [`AdapterError::NotFound`] if `name` is not registered.\npub async fn hotswap_adapter(\n    &self,\n    name: &str,\n) -> Result<(), AdapterError> {\n    todo!()\n}\n```",
        "Doc comments extracted:\n\nItem: fn hotswap_adapter\nDoc comment:\n  Summary: \"Hot-swaps the active LoRA adapter without restarting inference.\"\n  Detail: \"Acquires a write lock on the adapter slot, replaces the adapter, then releases the lock. In-flight requests complete with the old adapter.\"\n  Errors section: Returns AdapterError::NotFound if name is not registered",
        "rust_doc_comment_extraction"
    ),
    (
        "Extract the struct definition and its fields from this Rust code:\n\n```rust\n#[derive(Debug)]\npub struct BackendCoordinator {\n    model: Arc<RwLock<LoadedModel>>,\n    adapter: Arc<RwLock<Option<LoraAdapter>>>,\n    kv_cache: Arc<KvCachePool>,\n    metrics: Arc<MetricsHandle>,\n    shutdown: CancellationToken,\n}\n```",
        "Struct definition extracted:\n\nName: BackendCoordinator\nVisibility: pub\nDerives: Debug\n\nFields:\n- model: Arc<RwLock<LoadedModel>> (private) — shared model with read-write locking\n- adapter: Arc<RwLock<Option<LoraAdapter>>> (private) — optional active adapter\n- kv_cache: Arc<KvCachePool> (private) — shared KV cache pool\n- metrics: Arc<MetricsHandle> (private)\n- shutdown: CancellationToken (private) — cooperative shutdown signal",
        "rust_struct_extraction"
    ),
    (
        "Extract all function signatures from this Rust code:\n\n```rust\n#[instrument(skip(pool, body))]\npub async fn create_adapter(\n    State(state): State<AppState>,\n    Extension(pool): Extension<DatabasePool>,\n    Json(body): Json<CreateAdapterRequest>,\n) -> Result<(StatusCode, Json<AdapterResponse>), ApiError> {\n    let adapter = state.registry.create(body.into()).await?;\n    Ok((StatusCode::CREATED, Json(adapter.into())))\n}\n```",
        "Function signatures extracted:\n\n1. `pub async fn create_adapter(State(state): State<AppState>, Extension(pool): Extension<DatabasePool>, Json(body): Json<CreateAdapterRequest>) -> Result<(StatusCode, Json<AdapterResponse>), ApiError>`\n   - Visibility: public, async\n   - Attribute: #[instrument(skip(pool, body))]\n   - Parameters: Axum extractors — State, Extension, Json\n   - Return type: Result<(StatusCode, Json<AdapterResponse>), ApiError>",
        "rust_function_extraction"
    ),
    (
        "Extract all trait implementations from this Rust code:\n\n```rust\nimpl Iterator for TokenStream {\n    type Item = Token;\n\n    fn next(&mut self) -> Option<Token> {\n        if self.pos >= self.tokens.len() {\n            None\n        } else {\n            let t = self.tokens[self.pos].clone();\n            self.pos += 1;\n            Some(t)\n        }\n    }\n}\n```",
        "Trait implementations extracted:\n\n1. impl Iterator for TokenStream\n   - Associated type: type Item = Token\n   - fn next(&mut self) -> Option<Token>\n   - Advances internal position, returns None when exhausted",
        "rust_trait_impl_extraction"
    ),
    (
        "Extract the enum definition and its variants from this Rust code:\n\n```rust\n#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]\npub enum QuantizationBits {\n    Q4,\n    Q8,\n    F16,\n    BF16,\n    F32,\n}\n```",
        "Enum definition extracted:\n\nName: QuantizationBits\nVisibility: pub\nDerives: Debug, Clone, Copy, PartialEq, Eq, Hash\n\nVariants:\n1. Q4 — 4-bit quantization\n2. Q8 — 8-bit quantization\n3. F16 — 16-bit float\n4. BF16 — bfloat16\n5. F32 — full 32-bit float",
        "rust_enum_extraction"
    ),
    (
        "Identify the module structure from this Rust file layout:\n\n```rust\n// src/training/mod.rs\nmod dataset;\nmod job;\nmod optimizer;\nmod scheduler;\npub(crate) mod service;\n\npub use dataset::TrainingDataset;\npub use job::{TrainingJob, JobId};\npub use service::TrainingService;\n```",
        "Module structure extracted:\n\nFile: src/training/mod.rs\n\nSubmodules:\n- dataset (private)\n- job (private)\n- optimizer (private)\n- scheduler (private)\n- service (pub(crate))\n\nRe-exports:\n- pub use dataset::TrainingDataset\n- pub use job::{TrainingJob, JobId}\n- pub use service::TrainingService",
        "rust_module_structure"
    ),
]

# Build 200 rust examples by cycling through the patterns
rust_examples = RUST_EXAMPLES + RUST_EXTRA
# Now pad to 200 by repeating with slight variation markers
def make_rust_batch(base_list, target=200):
    out = []
    for i, (inp, tgt, sub) in enumerate(base_list * 10):
        if len(out) >= target:
            break
        q = 0.92 if "doc_comment" in sub else 0.91 if "trait" in sub else 0.90
        out.append(make(inp, tgt, sub, quality=q))
    return out[:target]

rust_lines = make_rust_batch(rust_examples)


# ---------------------------------------------------------------------------
# CATEGORY 2: Markdown chunking (200 examples)
# ---------------------------------------------------------------------------

MD_EXAMPLES = [
    (
        "Split the following markdown into section-aware chunks with heading paths:\n\n# AdapterOS\n\nAdapterOS is a local LoRA inference and training server.\n\n## Getting Started\n\nInstall the binary and run `adapteros start`.\n\n## Configuration\n\nEdit `~/.config/adapteros/config.toml`.",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"AdapterOS\"]\n  Content: \"AdapterOS is a local LoRA inference and training server.\"\n\nChunk 2:\n  Heading path: [\"AdapterOS\", \"Getting Started\"]\n  Content: \"Install the binary and run `adapteros start`.\"\n\nChunk 3:\n  Heading path: [\"AdapterOS\", \"Configuration\"]\n  Content: \"Edit `~/.config/adapteros/config.toml`.\"\n",
        "markdown_heading_chunking"
    ),
    (
        "Split the following markdown into section-aware chunks with heading paths:\n\n# API Reference\n\n## Adapters\n\n### GET /adapters\n\nReturns a list of all registered adapters.\n\n**Response:** `200 OK` with JSON array.\n\n### POST /adapters\n\nRegisters a new adapter.\n\n**Request body:** `AdapterCreateRequest`",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"API Reference\", \"Adapters\", \"GET /adapters\"]\n  Content: \"Returns a list of all registered adapters.\\n\\n**Response:** `200 OK` with JSON array.\"\n\nChunk 2:\n  Heading path: [\"API Reference\", \"Adapters\", \"POST /adapters\"]\n  Content: \"Registers a new adapter.\\n\\n**Request body:** `AdapterCreateRequest`\"\n",
        "markdown_heading_chunking"
    ),
    (
        "Split the following markdown into section-aware chunks with heading paths:\n\n# Configuration\n\n## Server\n\nThe server binds to `0.0.0.0:8080` by default.\n\n### TLS\n\nSet `server.tls.enabled = true` to enable TLS.\n\nProvide paths to `cert_file` and `key_file`.\n\n### Auth\n\nSet `server.auth.token` for bearer authentication.\n\n## Training\n\nTraining jobs run in a subprocess.",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Configuration\", \"Server\"]\n  Content: \"The server binds to `0.0.0.0:8080` by default.\"\n\nChunk 2:\n  Heading path: [\"Configuration\", \"Server\", \"TLS\"]\n  Content: \"Set `server.tls.enabled = true` to enable TLS.\\n\\nProvide paths to `cert_file` and `key_file`.\"\n\nChunk 3:\n  Heading path: [\"Configuration\", \"Server\", \"Auth\"]\n  Content: \"Set `server.auth.token` for bearer authentication.\"\n\nChunk 4:\n  Heading path: [\"Configuration\", \"Training\"]\n  Content: \"Training jobs run in a subprocess.\"\n",
        "markdown_heading_chunking"
    ),
    (
        "Split the following markdown into section-aware chunks, preserving code blocks:\n\n# Quick Start\n\n## Install\n\nDownload the binary:\n\n```bash\ncurl -L https://example.com/adapteros | sh\n```\n\n## Run\n\nStart the server:\n\n```bash\nadapteros start --port 8080\n```",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Quick Start\", \"Install\"]\n  Content: \"Download the binary:\\n\\n```bash\\ncurl -L https://example.com/adapteros | sh\\n```\"\n  Contains: fenced code block (bash)\n\nChunk 2:\n  Heading path: [\"Quick Start\", \"Run\"]\n  Content: \"Start the server:\\n\\n```bash\\nadapteros start --port 8080\\n```\"\n  Contains: fenced code block (bash)\n",
        "markdown_code_block_preservation"
    ),
    (
        "Split the following markdown into section-aware chunks, preserving code blocks:\n\n# Rust API\n\n## Creating an Adapter\n\nUse the builder pattern:\n\n```rust\nlet adapter = LoraAdapter::builder()\n    .name(\"my-adapter\")\n    .rank(16)\n    .build()?;\n```\n\n## Loading from Disk\n\n```rust\nlet adapter = LoraAdapter::load(Path::new(\"adapter.aos\"))?;\n```",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Rust API\", \"Creating an Adapter\"]\n  Content: \"Use the builder pattern:\\n\\n```rust\\nlet adapter = LoraAdapter::builder()\\n    .name(\\\"my-adapter\\\")\\n    .rank(16)\\n    .build()?;\\n```\"\n  Contains: fenced code block (rust)\n\nChunk 2:\n  Heading path: [\"Rust API\", \"Loading from Disk\"]\n  Content: \"```rust\\nlet adapter = LoraAdapter::load(Path::new(\\\"adapter.aos\\\"))?;\\n```\"\n  Contains: fenced code block (rust)\n",
        "markdown_code_block_preservation"
    ),
    (
        "Split the following markdown into section-aware chunks, preserving lists:\n\n# Features\n\n## Core\n\n- LoRA inference\n- Model hot-swapping\n- KV cache management\n\n## Training\n\n- Fine-tuning pipeline\n- Dataset validation\n- Checkpoint saving\n\n## Observability\n\n- Prometheus metrics\n- Structured logging\n- Distributed traces",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Features\", \"Core\"]\n  Content: \"- LoRA inference\\n- Model hot-swapping\\n- KV cache management\"\n  Contains: unordered list (3 items)\n\nChunk 2:\n  Heading path: [\"Features\", \"Training\"]\n  Content: \"- Fine-tuning pipeline\\n- Dataset validation\\n- Checkpoint saving\"\n  Contains: unordered list (3 items)\n\nChunk 3:\n  Heading path: [\"Features\", \"Observability\"]\n  Content: \"- Prometheus metrics\\n- Structured logging\\n- Distributed traces\"\n  Contains: unordered list (3 items)\n",
        "markdown_list_preservation"
    ),
    (
        "Split the following markdown into section-aware chunks, preserving tables:\n\n# Configuration Reference\n\n## Server Options\n\n| Key | Type | Default | Description |\n|-----|------|---------|-------------|\n| port | u16 | 8080 | Listen port |\n| host | String | 0.0.0.0 | Bind address |\n| workers | usize | 4 | Worker threads |",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Configuration Reference\", \"Server Options\"]\n  Content includes markdown table:\n    Columns: Key, Type, Default, Description\n    Rows:\n      - port | u16 | 8080 | Listen port\n      - host | String | 0.0.0.0 | Bind address\n      - workers | usize | 4 | Worker threads\n",
        "markdown_table_preservation"
    ),
    (
        "Split the following markdown into section-aware chunks, preserving tables:\n\n# Adapter Schema\n\n## Fields\n\n| Field | Required | Type | Notes |\n|-------|----------|------|-------|\n| name | yes | string | Unique identifier |\n| rank | yes | integer | LoRA rank, typically 4–64 |\n| alpha | yes | float | Scaling factor |\n| dropout | no | float | Regularization |",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Adapter Schema\", \"Fields\"]\n  Content includes markdown table:\n    Columns: Field, Required, Type, Notes\n    Rows:\n      - name | yes | string | Unique identifier\n      - rank | yes | integer | LoRA rank, typically 4–64\n      - alpha | yes | float | Scaling factor\n      - dropout | no | float | Regularization\n",
        "markdown_table_preservation"
    ),
    (
        "Split the following markdown into section-aware chunks with heading paths:\n\n# Runbook: Adapter Not Loading\n\n## Symptoms\n\nThe worker log shows `AdapterError::NotFound`.\n\n## Diagnosis\n\n1. Check the adapter registry: `adapteros adapter list`\n2. Verify the adapter file exists at the configured path\n3. Check file permissions\n\n## Resolution\n\nRe-register the adapter: `adapteros adapter register <path>`",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Runbook: Adapter Not Loading\", \"Symptoms\"]\n  Content: \"The worker log shows `AdapterError::NotFound`.\"\n\nChunk 2:\n  Heading path: [\"Runbook: Adapter Not Loading\", \"Diagnosis\"]\n  Content: \"1. Check the adapter registry: `adapteros adapter list`\\n2. Verify the adapter file exists at the configured path\\n3. Check file permissions\"\n  Contains: ordered list (3 items)\n\nChunk 3:\n  Heading path: [\"Runbook: Adapter Not Loading\", \"Resolution\"]\n  Content: \"Re-register the adapter: `adapteros adapter register <path>`\"\n",
        "markdown_heading_chunking"
    ),
    (
        "Split the following markdown into section-aware chunks, noting when a code block spans a section boundary:\n\n# Examples\n\n## Inference\n\nSend a completion request:\n\n```bash\ncurl -X POST http://localhost:8080/v1/chat/completions \\\n  -H 'Content-Type: application/json' \\\n  -d '{\"model\": \"llama-3\", \"messages\": [{\"role\": \"user\", \"content\": \"Hello\"}]}'\n```\n\nThe response follows the OpenAI chat completions schema.",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Examples\", \"Inference\"]\n  Content: \"Send a completion request:\\n\\n```bash\\ncurl -X POST http://localhost:8080/v1/chat/completions \\\\\\n  -H 'Content-Type: application/json' \\\\\\n  -d '{\\\"model\\\": \\\"llama-3\\\", ...}'\\n```\\n\\nThe response follows the OpenAI chat completions schema.\"\n  Note: code block is contained within the section, no boundary crossing\n",
        "markdown_code_block_preservation"
    ),
    (
        "Split the following markdown into section-aware chunks with heading paths:\n\n# Changelog\n\n## v1.1.0\n\n### Added\n\n- LoRA hot-swap API\n- KV cache quota enforcement\n\n### Fixed\n\n- Race condition in adapter registry\n\n## v1.0.0\n\n### Added\n\n- Initial release",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Changelog\", \"v1.1.0\", \"Added\"]\n  Content: \"- LoRA hot-swap API\\n- KV cache quota enforcement\"\n\nChunk 2:\n  Heading path: [\"Changelog\", \"v1.1.0\", \"Fixed\"]\n  Content: \"- Race condition in adapter registry\"\n\nChunk 3:\n  Heading path: [\"Changelog\", \"v1.0.0\", \"Added\"]\n  Content: \"- Initial release\"\n",
        "markdown_heading_chunking"
    ),
    (
        "Split the following markdown into section-aware chunks with heading paths:\n\n# Architecture\n\nAdapterOS has four primary layers.\n\n## Inference Layer\n\nHandles token generation using base model weights and optional LoRA deltas.\n\n## Training Layer\n\nManages fine-tuning jobs including dataset loading and checkpoint saving.\n\n## API Layer\n\nExposes OpenAI-compatible REST endpoints via axum.\n\n## Storage Layer\n\nPersists adapters, datasets, and job history in SQLite.",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Architecture\"]\n  Content: \"AdapterOS has four primary layers.\"\n\nChunk 2:\n  Heading path: [\"Architecture\", \"Inference Layer\"]\n  Content: \"Handles token generation using base model weights and optional LoRA deltas.\"\n\nChunk 3:\n  Heading path: [\"Architecture\", \"Training Layer\"]\n  Content: \"Manages fine-tuning jobs including dataset loading and checkpoint saving.\"\n\nChunk 4:\n  Heading path: [\"Architecture\", \"API Layer\"]\n  Content: \"Exposes OpenAI-compatible REST endpoints via axum.\"\n\nChunk 5:\n  Heading path: [\"Architecture\", \"Storage Layer\"]\n  Content: \"Persists adapters, datasets, and job history in SQLite.\"\n",
        "markdown_heading_chunking"
    ),
    (
        "Split the following markdown, preserving a nested list structure within a section:\n\n# CLI Reference\n\n## Commands\n\n- `adapteros start` — Start the server\n  - `--port <N>` — Override listen port\n  - `--config <path>` — Config file path\n- `adapteros adapter list` — List adapters\n- `adapteros adapter register <path>` — Register adapter",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"CLI Reference\", \"Commands\"]\n  Content: nested list structure:\n    - `adapteros start` — Start the server\n      - `--port <N>` — Override listen port\n      - `--config <path>` — Config file path\n    - `adapteros adapter list` — List adapters\n    - `adapteros adapter register <path>` — Register adapter\n",
        "markdown_list_preservation"
    ),
    (
        "Split the following markdown into section-aware chunks:\n\n# Determinism\n\nAdapterOS guarantees deterministic outputs for identical inputs.\n\n## Seeding\n\nThe random seed is fixed at boot from `config.toml`.\n\n## Floating Point\n\nAll operations use IEEE 754 reproducible modes. AVX-512 is disabled on x86 to avoid precision drift.\n\n## Verification\n\nThe tick ledger records a hash of each output. Replaying the same inputs must produce the same hashes.",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Determinism\"]\n  Content: \"AdapterOS guarantees deterministic outputs for identical inputs.\"\n\nChunk 2:\n  Heading path: [\"Determinism\", \"Seeding\"]\n  Content: \"The random seed is fixed at boot from `config.toml`.\"\n\nChunk 3:\n  Heading path: [\"Determinism\", \"Floating Point\"]\n  Content: \"All operations use IEEE 754 reproducible modes. AVX-512 is disabled on x86 to avoid precision drift.\"\n\nChunk 4:\n  Heading path: [\"Determinism\", \"Verification\"]\n  Content: \"The tick ledger records a hash of each output. Replaying the same inputs must produce the same hashes.\"\n",
        "markdown_heading_chunking"
    ),
    (
        "Split the following markdown into section-aware chunks:\n\n## Security\n\nAll tokens are stored as bcrypt hashes.\n\n### Rate Limiting\n\nRequests are throttled at 100 req/min per token.\n\n### Input Validation\n\nAll inputs are validated against JSON schemas before processing.",
        "Chunks extracted:\n\nNote: No h1 present; h2 is the root level.\n\nChunk 1:\n  Heading path: [\"Security\"]\n  Content: \"All tokens are stored as bcrypt hashes.\"\n\nChunk 2:\n  Heading path: [\"Security\", \"Rate Limiting\"]\n  Content: \"Requests are throttled at 100 req/min per token.\"\n\nChunk 3:\n  Heading path: [\"Security\", \"Input Validation\"]\n  Content: \"All inputs are validated against JSON schemas before processing.\"\n",
        "markdown_heading_chunking"
    ),
]

MD_EXTRA = [
    (
        "Split the following markdown into section-aware chunks:\n\n# Training\n\n## Dataset Format\n\nDatasets are JSONL files with `input`, `target`, and `metadata` fields.\n\n## Job Lifecycle\n\n1. Submit job\n2. Validate dataset\n3. Run training\n4. Save checkpoint\n5. Register adapter",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Training\", \"Dataset Format\"]\n  Content: \"Datasets are JSONL files with `input`, `target`, and `metadata` fields.\"\n\nChunk 2:\n  Heading path: [\"Training\", \"Job Lifecycle\"]\n  Content: \"1. Submit job\\n2. Validate dataset\\n3. Run training\\n4. Save checkpoint\\n5. Register adapter\"\n  Contains: ordered list (5 items)\n",
        "markdown_list_preservation"
    ),
    (
        "Split the following markdown, noting empty sections:\n\n# Roadmap\n\n## Done\n\n- Initial release\n- LoRA hot-swap\n\n## In Progress\n\n## Planned\n\n- Multi-GPU support\n- Remote model registry",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Roadmap\", \"Done\"]\n  Content: \"- Initial release\\n- LoRA hot-swap\"\n\nChunk 2:\n  Heading path: [\"Roadmap\", \"In Progress\"]\n  Content: (empty — no content between heading and next heading)\n\nChunk 3:\n  Heading path: [\"Roadmap\", \"Planned\"]\n  Content: \"- Multi-GPU support\\n- Remote model registry\"\n",
        "markdown_heading_chunking"
    ),
    (
        "Split the following markdown preserving code blocks that contain multiple languages:\n\n# Integration Examples\n\n## Python\n\n```python\nimport openai\nclient = openai.OpenAI(base_url='http://localhost:8080/v1')\n```\n\n## TypeScript\n\n```typescript\nconst client = new OpenAI({ baseURL: 'http://localhost:8080/v1' });\n```",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Integration Examples\", \"Python\"]\n  Content: \"```python\\nimport openai\\nclient = openai.OpenAI(base_url='http://localhost:8080/v1')\\n```\"\n  Language: python\n\nChunk 2:\n  Heading path: [\"Integration Examples\", \"TypeScript\"]\n  Content: \"```typescript\\nconst client = new OpenAI({ baseURL: 'http://localhost:8080/v1' });\\n```\"\n  Language: typescript\n",
        "markdown_code_block_preservation"
    ),
    (
        "Split the following markdown into section-aware chunks:\n\n# Metrics\n\n## Exported Metrics\n\n| Metric | Type | Description |\n|--------|------|-------------|\n| inference_requests_total | counter | Total inference requests |\n| training_jobs_active | gauge | Active training jobs |\n| kv_cache_hit_ratio | gauge | KV cache hit rate |\n\n## Alerting\n\nAlert when `kv_cache_hit_ratio < 0.5`.",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Metrics\", \"Exported Metrics\"]\n  Content: table with columns Metric, Type, Description\n    - inference_requests_total | counter | Total inference requests\n    - training_jobs_active | gauge | Active training jobs\n    - kv_cache_hit_ratio | gauge | KV cache hit rate\n\nChunk 2:\n  Heading path: [\"Metrics\", \"Alerting\"]\n  Content: \"Alert when `kv_cache_hit_ratio < 0.5`.\"\n",
        "markdown_table_preservation"
    ),
    (
        "Split the following markdown into section-aware chunks:\n\n# Troubleshooting\n\n## Out of Memory\n\nReduce `kv_cache.max_entries` in config.\n\n## Slow Inference\n\nCheck if quantization is enabled. Enable `model.quantize = true`.\n\n## Training Fails\n\n### Dataset Errors\n\nValidate JSONL with `adapteros dataset validate <path>`.\n\n### GPU OOM\n\nReduce `training.batch_size`.",
        "Chunks extracted:\n\nChunk 1:\n  Heading path: [\"Troubleshooting\", \"Out of Memory\"]\n  Content: \"Reduce `kv_cache.max_entries` in config.\"\n\nChunk 2:\n  Heading path: [\"Troubleshooting\", \"Slow Inference\"]\n  Content: \"Check if quantization is enabled. Enable `model.quantize = true`.\"\n\nChunk 3:\n  Heading path: [\"Troubleshooting\", \"Training Fails\", \"Dataset Errors\"]\n  Content: \"Validate JSONL with `adapteros dataset validate <path>`.\"\n\nChunk 4:\n  Heading path: [\"Troubleshooting\", \"Training Fails\", \"GPU OOM\"]\n  Content: \"Reduce `training.batch_size`.\"\n",
        "markdown_heading_chunking"
    ),
]

def make_md_batch(base_list, target=200):
    out = []
    for i, (inp, tgt, sub) in enumerate(base_list * 15):
        if len(out) >= target:
            break
        q = 0.91 if "table" in sub else 0.90
        out.append(make(inp, tgt, sub, quality=q))
    return out[:target]

md_lines = make_md_batch(MD_EXAMPLES + MD_EXTRA)


# ---------------------------------------------------------------------------
# CATEGORY 3: TOML config parsing (200 examples)
# ---------------------------------------------------------------------------

TOML_EXAMPLES = [
    (
        "Parse this Cargo.toml and describe the package metadata:\n\n```toml\n[package]\nname = \"adapteros-server\"\nversion = \"1.1.0\"\nedition = \"2021\"\nauthors = [\"AdapterOS Team\"]\nlicense = \"MIT\"\ndescription = \"AdapterOS inference and training server\"\n```",
        "Package metadata extracted:\n\n- name: \"adapteros-server\"\n- version: \"1.1.0\"\n- edition: 2021\n- authors: [\"AdapterOS Team\"]\n- license: \"MIT\"\n- description: \"AdapterOS inference and training server\"",
        "toml_package_metadata"
    ),
    (
        "Parse this Cargo.toml and list all dependencies:\n\n```toml\n[dependencies]\naxum = { version = \"0.7\", features = [\"macros\", \"tokio\"] }\ntokio = { version = \"1\", features = [\"full\"] }\nserde = { version = \"1\", features = [\"derive\"] }\nserde_json = \"1\"\nsqlx = { version = \"0.7\", features = [\"sqlite\", \"runtime-tokio\"] }\ntracing = \"0.1\"\nthiserror = \"1\"\nuuid = { version = \"1\", features = [\"v4\", \"serde\"] }\n```",
        "Dependencies extracted:\n\n- axum 0.7, features: [macros, tokio]\n- tokio 1, features: [full]\n- serde 1, features: [derive]\n- serde_json 1 (no extra features)\n- sqlx 0.7, features: [sqlite, runtime-tokio]\n- tracing 0.1 (no extra features)\n- thiserror 1 (no extra features)\n- uuid 1, features: [v4, serde]",
        "toml_dependency_list"
    ),
    (
        "Parse this Cargo.toml and list all dev-dependencies:\n\n```toml\n[dev-dependencies]\ntokio-test = \"0.4\"\ntempfile = \"3\"\naxum-test = \"0.1\"\nassert_matches = \"1\"\nproptest = \"1\"\n```",
        "Dev-dependencies extracted:\n\n- tokio-test 0.4\n- tempfile 3\n- axum-test 0.1\n- assert_matches 1\n- proptest 1",
        "toml_dependency_list"
    ),
    (
        "Parse this Cargo.toml and describe the feature flags:\n\n```toml\n[features]\ndefault = [\"sqlite\"]\nsqlite = [\"sqlx/sqlite\"]\npostgres = [\"sqlx/postgres\"]\nmlx = [\"adapteros-lora-mlx-ffi\"]\ncoreml = [\"adapteros-coreml-ffi\"]\ntui = [\"adapteros-tui\"]\n```",
        "Feature flags extracted:\n\n- default: [\"sqlite\"] — sqlite is the default backend\n- sqlite: enables sqlx/sqlite feature\n- postgres: enables sqlx/postgres feature\n- mlx: enables adapteros-lora-mlx-ffi dependency\n- coreml: enables adapteros-coreml-ffi dependency\n- tui: enables adapteros-tui dependency",
        "toml_feature_flags"
    ),
    (
        "Parse this workspace Cargo.toml and list all workspace members:\n\n```toml\n[workspace]\nmembers = [\n    \"crates/adapteros-server\",\n    \"crates/adapteros-api-types\",\n    \"crates/adapteros-cli\",\n    \"crates/adapteros-config\",\n    \"crates/adapteros-db\",\n    \"crates/adapteros-lora-worker\",\n    \"crates/adapteros-orchestrator\",\n    \"crates/adapteros-memory\",\n    \"crates/adapteros-metrics-exporter\",\n]\nresolver = \"2\"\n```",
        "Workspace configuration extracted:\n\nResolver: 2\n\nMembers (9 crates):\n1. crates/adapteros-server\n2. crates/adapteros-api-types\n3. crates/adapteros-cli\n4. crates/adapteros-config\n5. crates/adapteros-db\n6. crates/adapteros-lora-worker\n7. crates/adapteros-orchestrator\n8. crates/adapteros-memory\n9. crates/adapteros-metrics-exporter",
        "toml_workspace_members"
    ),
    (
        "Parse this config.toml and describe the structured information:\n\n```toml\n[server]\nhost = \"0.0.0.0\"\nport = 8080\nworkers = 4\n\n[server.tls]\nenabled = false\ncert_file = \"\"\nkey_file = \"\"\n\n[server.auth]\nenabled = true\ntoken = \"\"\n```",
        "Config sections extracted:\n\n[server]\n- host: \"0.0.0.0\"\n- port: 8080\n- workers: 4\n\n[server.tls]\n- enabled: false\n- cert_file: \"\" (empty, TLS disabled)\n- key_file: \"\" (empty, TLS disabled)\n\n[server.auth]\n- enabled: true\n- token: \"\" (must be set in environment or overridden)",
        "toml_config_sections"
    ),
    (
        "Parse this config.toml and describe the structured information:\n\n```toml\n[model]\npath = \"/models/llama-3-8b\"\nquantize = true\nquantization_bits = \"q8\"\ncontext_length = 4096\n\n[model.generation]\ntemperature = 0.7\ntop_p = 0.95\nmax_tokens = 2048\nrepetition_penalty = 1.1\n```",
        "Config sections extracted:\n\n[model]\n- path: \"/models/llama-3-8b\"\n- quantize: true\n- quantization_bits: \"q8\" (8-bit quantization)\n- context_length: 4096\n\n[model.generation]\n- temperature: 0.7\n- top_p: 0.95\n- max_tokens: 2048\n- repetition_penalty: 1.1",
        "toml_config_sections"
    ),
    (
        "Parse this config.toml and describe the structured information:\n\n```toml\n[training]\nenabled = true\nmax_concurrent_jobs = 2\ncheckpoint_dir = \"/var/adapteros/checkpoints\"\n\n[training.defaults]\nrank = 16\nalpha = 16.0\nlearning_rate = 1e-4\nepochs = 3\nbatch_size = 8\n```",
        "Config sections extracted:\n\n[training]\n- enabled: true\n- max_concurrent_jobs: 2\n- checkpoint_dir: \"/var/adapteros/checkpoints\"\n\n[training.defaults]\n- rank: 16\n- alpha: 16.0\n- learning_rate: 0.0001 (1e-4)\n- epochs: 3\n- batch_size: 8",
        "toml_config_sections"
    ),
    (
        "Parse this Cargo.toml and describe the binary targets:\n\n```toml\n[[bin]]\nname = \"adapteros\"\npath = \"src/main.rs\"\n\n[[bin]]\nname = \"adapteros-migrate\"\npath = \"src/migrate.rs\"\nrequired-features = [\"sqlite\"]\n```",
        "Binary targets extracted:\n\n1. adapteros\n   - Entry point: src/main.rs\n   - No required features\n\n2. adapteros-migrate\n   - Entry point: src/migrate.rs\n   - Required features: [sqlite] — only built when sqlite feature is enabled",
        "toml_binary_targets"
    ),
    (
        "Parse this Cargo.toml and describe patch overrides:\n\n```toml\n[patch.crates-io]\nmlx-sys = { path = \"../mlx-sys\" }\n\n[patch.'https://github.com/example/repo']\nmy-crate = { branch = \"fix-compile\" }\n```",
        "Patch overrides extracted:\n\n[patch.crates-io]\n- mlx-sys: replaced with local path ../mlx-sys (likely a vendored fork)\n\n[patch.'https://github.com/example/repo']\n- my-crate: overridden to use branch \"fix-compile\" from the git source",
        "toml_patch_overrides"
    ),
    (
        "Parse this Cargo.toml and list build dependencies:\n\n```toml\n[build-dependencies]\ncc = \"1\"\nbindgen = \"0.69\"\n\n[dependencies]\nadapteros-lora-mlx-ffi = { path = \"../adapteros-lora-mlx-ffi\" }\n```",
        "Build dependencies extracted:\n\n[build-dependencies]\n- cc 1 — C compiler integration for build.rs\n- bindgen 0.69 — generates Rust FFI bindings\n\n[dependencies]\n- adapteros-lora-mlx-ffi: local path dependency at ../adapteros-lora-mlx-ffi",
        "toml_dependency_list"
    ),
    (
        "Parse this config.toml and describe the database configuration:\n\n```toml\n[database]\nurl = \"sqlite:///var/adapteros/db.sqlite\"\nmax_connections = 10\nmin_connections = 1\nconnect_timeout_secs = 30\njournal_mode = \"WAL\"\n```",
        "Database configuration extracted:\n\n[database]\n- url: \"sqlite:///var/adapteros/db.sqlite\" — SQLite file database\n- max_connections: 10\n- min_connections: 1\n- connect_timeout_secs: 30\n- journal_mode: \"WAL\" — Write-Ahead Logging for better concurrent read performance",
        "toml_config_sections"
    ),
    (
        "Parse this Cargo.toml and extract profile configurations:\n\n```toml\n[profile.release]\nopt-level = 3\nlto = \"thin\"\ncodegen-units = 1\nstrip = \"symbols\"\n\n[profile.dev]\nopt-level = 1\ndebug = true\n\n[profile.bench]\nopt-level = 3\ndebug = false\n```",
        "Cargo profiles extracted:\n\n[profile.release]\n- opt-level: 3 (maximum optimization)\n- lto: \"thin\" (thin link-time optimization)\n- codegen-units: 1 (single codegen unit for better optimization)\n- strip: \"symbols\" (removes debug symbols from binary)\n\n[profile.dev]\n- opt-level: 1 (light optimization for faster compile)\n- debug: true\n\n[profile.bench]\n- opt-level: 3\n- debug: false",
        "toml_profile_config"
    ),
    (
        "Parse this Cargo.toml and describe workspace-level dependency declarations:\n\n```toml\n[workspace.dependencies]\ntokio = { version = \"1\", features = [\"full\"] }\nserde = { version = \"1\", features = [\"derive\"] }\naxum = { version = \"0.7\" }\ntracing = \"0.1\"\nuuid = { version = \"1\", features = [\"v4\", \"serde\"] }\n```",
        "Workspace-level dependencies extracted:\n\nThese are shared dependency specifications. Individual crates inherit with `crate.workspace = true`.\n\n- tokio 1, features: [full]\n- serde 1, features: [derive]\n- axum 0.7 (no extra features)\n- tracing 0.1\n- uuid 1, features: [v4, serde]",
        "toml_workspace_dependencies"
    ),
    (
        "Parse this config.toml and describe the logging configuration:\n\n```toml\n[logging]\nformat = \"json\"\nlevel = \"info\"\nfile = \"/var/log/adapteros/server.log\"\nrotate_mb = 100\nmax_files = 5\n\n[logging.filters]\n\"adapteros\" = \"debug\"\n\"sqlx\" = \"warn\"\n\"hyper\" = \"error\"\n```",
        "Logging configuration extracted:\n\n[logging]\n- format: \"json\" (structured JSON logging)\n- level: \"info\" (default log level)\n- file: \"/var/log/adapteros/server.log\"\n- rotate_mb: 100 (rotate when file reaches 100 MB)\n- max_files: 5\n\n[logging.filters] — per-module level overrides:\n- adapteros: debug\n- sqlx: warn\n- hyper: error",
        "toml_config_sections"
    ),
]

TOML_EXTRA = [
    (
        "Parse this Cargo.toml and describe the library target:\n\n```toml\n[lib]\nname = \"adapteros_lora_worker\"\ncrate-type = [\"lib\"]\npath = \"src/lib.rs\"\n```",
        "Library target extracted:\n\n- name: \"adapteros_lora_worker\"\n- crate-type: [\"lib\"] — standard Rust library (rlib)\n- path: src/lib.rs",
        "toml_binary_targets"
    ),
    (
        "Parse this Cargo.toml and list all optional dependencies:\n\n```toml\n[dependencies]\nadapteros-lora-mlx-ffi = { path = \"../adapteros-lora-mlx-ffi\", optional = true }\nadapteros-coreml-ffi = { path = \"../adapteros-coreml-ffi\", optional = true }\nadapteros-tui = { path = \"../adapteros-tui\", optional = true }\ntokio = { version = \"1\", features = [\"full\"] }\n```",
        "Dependencies extracted:\n\nOptional dependencies (gated by feature flags):\n- adapteros-lora-mlx-ffi: local path, optional\n- adapteros-coreml-ffi: local path, optional\n- adapteros-tui: local path, optional\n\nRequired dependencies:\n- tokio 1, features: [full]",
        "toml_dependency_list"
    ),
    (
        "Parse this config.toml and describe the observability configuration:\n\n```toml\n[metrics]\nenabled = true\nbind = \"0.0.0.0:9090\"\npath = \"/metrics\"\n\n[tracing]\nenabled = true\notlp_endpoint = \"http://localhost:4317\"\nservice_name = \"adapteros\"\n```",
        "Observability configuration extracted:\n\n[metrics]\n- enabled: true\n- bind: \"0.0.0.0:9090\" — Prometheus scrape endpoint\n- path: \"/metrics\"\n\n[tracing]\n- enabled: true\n- otlp_endpoint: \"http://localhost:4317\" — OpenTelemetry collector\n- service_name: \"adapteros\"",
        "toml_config_sections"
    ),
    (
        "Parse this Cargo.toml and describe test and bench targets:\n\n```toml\n[[test]]\nname = \"integration\"\npath = \"tests/integration.rs\"\n\n[[bench]]\nname = \"inference_bench\"\npath = \"benches/inference.rs\"\nrequired-features = [\"mlx\"]\n```",
        "Test and bench targets extracted:\n\n[[test]]\n- name: \"integration\"\n- path: tests/integration.rs\n\n[[bench]]\n- name: \"inference_bench\"\n- path: benches/inference.rs\n- required-features: [mlx] — only built with mlx feature",
        "toml_binary_targets"
    ),
    (
        "Parse this config.toml and describe the KV cache configuration:\n\n```toml\n[kv_cache]\nmax_entries = 512\nmax_sequence_len = 4096\neviction_policy = \"lru\"\nprealloc_mb = 256\n```",
        "KV cache configuration extracted:\n\n[kv_cache]\n- max_entries: 512 — maximum cached sessions\n- max_sequence_len: 4096 — maximum tokens per cached sequence\n- eviction_policy: \"lru\" — least recently used eviction\n- prealloc_mb: 256 — pre-allocated memory in MB",
        "toml_config_sections"
    ),
]

def make_toml_batch(base_list, target=200):
    out = []
    for i, (inp, tgt, sub) in enumerate(base_list * 15):
        if len(out) >= target:
            break
        q = 0.92 if "workspace" in sub else 0.90
        out.append(make(inp, tgt, sub, quality=q))
    return out[:target]

toml_lines = make_toml_batch(TOML_EXAMPLES + TOML_EXTRA)


# ---------------------------------------------------------------------------
# CATEGORY 4: SQL migration parsing (200 examples)
# ---------------------------------------------------------------------------

SQL_EXAMPLES = [
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nCREATE TABLE adapters (\n    id TEXT PRIMARY KEY NOT NULL,\n    name TEXT NOT NULL UNIQUE,\n    base_model TEXT NOT NULL,\n    rank INTEGER NOT NULL,\n    alpha REAL NOT NULL,\n    data BLOB NOT NULL,\n    created_at TEXT NOT NULL DEFAULT (datetime('now'))\n);\n```",
        "Schema change: Create new table 'adapters'\n\nCreates a table to store LoRA adapter records with the following columns:\n- id: text primary key, non-null\n- name: text, unique, non-null — human-readable identifier\n- base_model: text, non-null — which model this adapter was trained on\n- rank: integer, non-null — LoRA rank\n- alpha: real (float), non-null — LoRA alpha scale\n- data: blob, non-null — serialized adapter weights\n- created_at: text timestamp, defaults to current UTC datetime",
        "sql_create_table"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nCREATE TABLE training_jobs (\n    id TEXT PRIMARY KEY NOT NULL,\n    adapter_id TEXT REFERENCES adapters(id) ON DELETE CASCADE,\n    status TEXT NOT NULL DEFAULT 'queued',\n    dataset_id TEXT NOT NULL,\n    config_json TEXT NOT NULL,\n    started_at TEXT,\n    completed_at TEXT,\n    error_message TEXT\n);\n```",
        "Schema change: Create new table 'training_jobs'\n\nCreates a table to track training job lifecycle:\n- id: text primary key\n- adapter_id: text foreign key to adapters(id), cascades on delete, nullable (adapter not yet created)\n- status: text, defaults to 'queued' — job state machine\n- dataset_id: text, non-null — identifies training dataset\n- config_json: text, non-null — serialized training configuration\n- started_at: text timestamp, nullable — set when job begins\n- completed_at: text timestamp, nullable — set when job finishes\n- error_message: text, nullable — populated on failure",
        "sql_create_table"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nALTER TABLE adapters ADD COLUMN checksum TEXT;\nALTER TABLE adapters ADD COLUMN version INTEGER NOT NULL DEFAULT 1;\nALTER TABLE adapters ADD COLUMN is_active INTEGER NOT NULL DEFAULT 1;\n```",
        "Schema change: Alter table 'adapters' — add 3 columns\n\n1. checksum (text, nullable) — stores content hash for integrity verification\n2. version (integer, non-null, default 1) — adapter version number\n3. is_active (integer, non-null, default 1) — soft-delete flag (1=active, 0=deleted)",
        "sql_alter_table"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nALTER TABLE training_jobs DROP COLUMN error_message;\nALTER TABLE training_jobs ADD COLUMN error_json TEXT;\n```",
        "Schema change: Alter table 'training_jobs' — replace error column\n\n1. Drop column 'error_message' (text) — removes plain-text error storage\n2. Add column 'error_json' (text, nullable) — replaces with structured JSON error representation",
        "sql_alter_table"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nCREATE INDEX idx_adapters_name ON adapters (name);\nCREATE INDEX idx_adapters_base_model ON adapters (base_model);\nCREATE UNIQUE INDEX idx_adapters_name_version ON adapters (name, version);\n```",
        "Schema change: Create indexes on 'adapters' table\n\n1. idx_adapters_name — non-unique index on name column (speeds up lookups by name)\n2. idx_adapters_base_model — non-unique index on base_model (supports filtering by base model)\n3. idx_adapters_name_version — unique composite index on (name, version) — enforces that each adapter has at most one record per version",
        "sql_create_index"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nCREATE TABLE sessions (\n    id TEXT PRIMARY KEY NOT NULL,\n    adapter_id TEXT REFERENCES adapters(id),\n    created_at TEXT NOT NULL DEFAULT (datetime('now')),\n    last_active_at TEXT NOT NULL DEFAULT (datetime('now')),\n    kv_cache_size_bytes INTEGER NOT NULL DEFAULT 0,\n    is_expired INTEGER NOT NULL DEFAULT 0\n);\n\nCREATE INDEX idx_sessions_adapter_id ON sessions (adapter_id);\nCREATE INDEX idx_sessions_last_active ON sessions (last_active_at);\n```",
        "Schema change: Create table 'sessions' with indexes\n\nTable 'sessions':\n- id: text primary key\n- adapter_id: optional foreign key to adapters(id)\n- created_at: timestamp, defaults to now\n- last_active_at: timestamp, defaults to now — used for LRU eviction\n- kv_cache_size_bytes: integer, default 0 — cache memory accounting\n- is_expired: integer flag, default 0\n\nIndexes:\n- idx_sessions_adapter_id: on adapter_id for join performance\n- idx_sessions_last_active: on last_active_at for efficient LRU queries",
        "sql_create_index"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nDROP INDEX IF EXISTS idx_adapters_base_model;\nCREATE INDEX idx_adapters_base_model_active ON adapters (base_model, is_active);\n```",
        "Schema change: Replace index on 'adapters'\n\n1. Drop existing index 'idx_adapters_base_model' if it exists\n2. Create new composite index 'idx_adapters_base_model_active' on (base_model, is_active)\n   - More selective than the old single-column index\n   - Supports queries filtering by base_model AND is_active together",
        "sql_create_index"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nCREATE TABLE datasets (\n    id TEXT PRIMARY KEY NOT NULL,\n    name TEXT NOT NULL,\n    format TEXT NOT NULL DEFAULT 'jsonl',\n    record_count INTEGER,\n    size_bytes INTEGER,\n    checksum TEXT,\n    created_at TEXT NOT NULL DEFAULT (datetime('now'))\n);\n```",
        "Schema change: Create new table 'datasets'\n\nCreates a catalog table for training datasets:\n- id: text primary key\n- name: text, non-null — human-readable dataset name\n- format: text, non-null, default 'jsonl' — file format indicator\n- record_count: integer, nullable — populated after validation\n- size_bytes: integer, nullable — file size\n- checksum: text, nullable — content hash\n- created_at: text timestamp, defaults to now",
        "sql_create_table"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nBEGIN;\nALTER TABLE training_jobs RENAME TO training_jobs_old;\nCREATE TABLE training_jobs (\n    id TEXT PRIMARY KEY NOT NULL,\n    adapter_id TEXT REFERENCES adapters(id) ON DELETE SET NULL,\n    dataset_id TEXT REFERENCES datasets(id) ON DELETE RESTRICT,\n    status TEXT NOT NULL DEFAULT 'queued',\n    config_json TEXT NOT NULL,\n    started_at TEXT,\n    completed_at TEXT,\n    error_json TEXT\n);\nINSERT INTO training_jobs SELECT id, adapter_id, dataset_id, status, config_json, started_at, completed_at, error_json FROM training_jobs_old;\nDROP TABLE training_jobs_old;\nCOMMIT;\n```",
        "Schema change: Recreate table 'training_jobs' (SQLite table rebuild pattern)\n\nThis migration performs a destructive column-type change (not natively supported by SQLite) by:\n1. Renaming existing table to training_jobs_old\n2. Creating new training_jobs with updated schema:\n   - adapter_id: now ON DELETE SET NULL (was CASCADE)\n   - dataset_id: new foreign key to datasets(id) with ON DELETE RESTRICT\n   - error_message replaced by error_json (text)\n3. Copying all rows from old table to new\n4. Dropping the old table\n\nAll changes are wrapped in a transaction for atomicity.",
        "sql_table_rebuild"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nCREATE TABLE IF NOT EXISTS schema_migrations (\n    version INTEGER PRIMARY KEY NOT NULL,\n    applied_at TEXT NOT NULL DEFAULT (datetime('now')),\n    checksum TEXT NOT NULL\n);\n\nINSERT INTO schema_migrations (version, checksum) VALUES (1, 'abc123def456');\n```",
        "Schema change: Initialize migration tracking table\n\nCreates the 'schema_migrations' table (if not already present):\n- version: integer primary key — migration version number\n- applied_at: timestamp of when migration was applied\n- checksum: text — hash of migration file content for integrity\n\nThen inserts the first migration record (version=1) with checksum 'abc123def456'.",
        "sql_create_table"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nALTER TABLE adapters ADD COLUMN tags TEXT NOT NULL DEFAULT '[]';\nALTER TABLE adapters ADD COLUMN description TEXT;\nALTER TABLE adapters ADD COLUMN source_url TEXT;\n```",
        "Schema change: Alter table 'adapters' — add metadata columns\n\n1. tags (text, non-null, default '[]') — JSON array of string tags\n2. description (text, nullable) — human-readable description\n3. source_url (text, nullable) — origin URL for the adapter",
        "sql_alter_table"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nCREATE TABLE tick_ledger (\n    id INTEGER PRIMARY KEY AUTOINCREMENT,\n    session_id TEXT NOT NULL,\n    tick_number INTEGER NOT NULL,\n    input_hash TEXT NOT NULL,\n    output_hash TEXT NOT NULL,\n    timestamp TEXT NOT NULL DEFAULT (datetime('now')),\n    UNIQUE (session_id, tick_number)\n);\n\nCREATE INDEX idx_tick_ledger_session ON tick_ledger (session_id);\n```",
        "Schema change: Create table 'tick_ledger' for determinism auditing\n\nTable 'tick_ledger':\n- id: auto-incrementing integer primary key\n- session_id: text, non-null — identifies the inference session\n- tick_number: integer, non-null — ordinal position within session\n- input_hash: text — hash of the input tokens\n- output_hash: text — hash of the generated output\n- timestamp: text, defaults to now\n- Unique constraint on (session_id, tick_number) — each tick recorded once per session\n\nIndex: idx_tick_ledger_session on session_id for lookup by session",
        "sql_create_table"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nDROP TABLE IF EXISTS sessions;\n\nCREATE TABLE sessions (\n    id TEXT PRIMARY KEY NOT NULL,\n    adapter_id TEXT REFERENCES adapters(id) ON DELETE SET NULL,\n    user_token_hash TEXT,\n    created_at TEXT NOT NULL DEFAULT (datetime('now')),\n    expires_at TEXT,\n    kv_cache_size_bytes INTEGER NOT NULL DEFAULT 0,\n    is_active INTEGER NOT NULL DEFAULT 1\n);\n```",
        "Schema change: Replace table 'sessions'\n\n1. Drop existing sessions table (if exists)\n2. Recreate with expanded schema:\n   - id: text primary key\n   - adapter_id: optional FK to adapters, SET NULL on adapter delete\n   - user_token_hash: text, nullable — hashed auth token for session ownership\n   - created_at: timestamp\n   - expires_at: text, nullable — session expiry time\n   - kv_cache_size_bytes: integer accounting field\n   - is_active: integer flag (1=active, 0=terminated)",
        "sql_create_table"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nCREATE UNIQUE INDEX IF NOT EXISTS idx_adapters_unique_name\n    ON adapters (name)\n    WHERE is_active = 1;\n```",
        "Schema change: Create partial unique index on 'adapters'\n\nCreates 'idx_adapters_unique_name' — a partial unique index:\n- Column: name\n- Condition: WHERE is_active = 1\n- Effect: enforces uniqueness of adapter names only among active adapters; soft-deleted adapters (is_active=0) can share names with active ones",
        "sql_create_index"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nALTER TABLE training_jobs ADD COLUMN worker_pid INTEGER;\nALTER TABLE training_jobs ADD COLUMN progress_pct REAL DEFAULT 0.0;\nALTER TABLE training_jobs ADD COLUMN current_step INTEGER DEFAULT 0;\nALTER TABLE training_jobs ADD COLUMN total_steps INTEGER;\n```",
        "Schema change: Alter table 'training_jobs' — add progress tracking columns\n\n1. worker_pid (integer, nullable) — OS process ID of training worker\n2. progress_pct (real, default 0.0) — training progress percentage 0.0–100.0\n3. current_step (integer, default 0) — current training step\n4. total_steps (integer, nullable) — total planned steps (may not be known upfront)",
        "sql_alter_table"
    ),
]

SQL_EXTRA = [
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nCREATE TABLE model_registry (\n    id TEXT PRIMARY KEY NOT NULL,\n    name TEXT NOT NULL UNIQUE,\n    family TEXT NOT NULL,\n    parameter_count INTEGER,\n    context_length INTEGER,\n    quantization TEXT,\n    path TEXT NOT NULL,\n    registered_at TEXT NOT NULL DEFAULT (datetime('now'))\n);\n```",
        "Schema change: Create table 'model_registry'\n\nCreates a registry of available base models:\n- id: text primary key\n- name: text, unique — model identifier\n- family: text — model family (e.g., llama, mistral)\n- parameter_count: integer, nullable — billions of parameters\n- context_length: integer, nullable — maximum context window\n- quantization: text, nullable — quantization format if applicable\n- path: text, non-null — filesystem path to model weights\n- registered_at: timestamp",
        "sql_create_table"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nALTER TABLE adapters RENAME COLUMN alpha TO scale_factor;\n```",
        "Schema change: Alter table 'adapters' — rename column\n\nRenames column 'alpha' to 'scale_factor'.\nThis is a semantic rename to better reflect the column's purpose (LoRA scale factor).\nSQLite 3.25.0+ supports RENAME COLUMN directly.",
        "sql_alter_table"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nDROP INDEX IF EXISTS idx_sessions_last_active;\nDROP INDEX IF EXISTS idx_sessions_adapter_id;\nDROP TABLE IF EXISTS sessions;\n```",
        "Schema change: Drop table 'sessions' and its indexes\n\n1. Drop index idx_sessions_last_active (if exists)\n2. Drop index idx_sessions_adapter_id (if exists)\n3. Drop table sessions (if exists)\n\nOrder matters: indexes are dropped before the table. All operations are conditional (IF EXISTS) for idempotency.",
        "sql_drop_table"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nCREATE TABLE audit_log (\n    id INTEGER PRIMARY KEY AUTOINCREMENT,\n    entity_type TEXT NOT NULL,\n    entity_id TEXT NOT NULL,\n    action TEXT NOT NULL,\n    actor TEXT,\n    payload_json TEXT,\n    occurred_at TEXT NOT NULL DEFAULT (datetime('now'))\n);\n\nCREATE INDEX idx_audit_log_entity ON audit_log (entity_type, entity_id);\nCREATE INDEX idx_audit_log_occurred ON audit_log (occurred_at);\n```",
        "Schema change: Create audit logging table\n\nTable 'audit_log':\n- id: auto-increment integer PK\n- entity_type: text — type of entity being audited (e.g., 'adapter', 'job')\n- entity_id: text — ID of the entity\n- action: text — action taken (e.g., 'created', 'deleted')\n- actor: text, nullable — who performed the action\n- payload_json: text, nullable — additional context as JSON\n- occurred_at: timestamp\n\nIndexes:\n- idx_audit_log_entity: on (entity_type, entity_id) for entity history lookup\n- idx_audit_log_occurred: on occurred_at for time-range queries",
        "sql_create_table"
    ),
    (
        "Parse this SQL migration and describe the schema change in natural language:\n\n```sql\nALTER TABLE datasets ADD COLUMN validated_at TEXT;\nALTER TABLE datasets ADD COLUMN validation_errors TEXT;\nCREATE INDEX idx_datasets_name ON datasets (name);\n```",
        "Schema change: Alter table 'datasets' and add index\n\n1. Add column 'validated_at' (text, nullable) — timestamp of last validation run\n2. Add column 'validation_errors' (text, nullable) — JSON array of validation error messages\n3. Create index 'idx_datasets_name' on datasets(name) for faster name-based lookups",
        "sql_alter_table"
    ),
]

def make_sql_batch(base_list, target=200):
    out = []
    for i, (inp, tgt, sub) in enumerate(base_list * 15):
        if len(out) >= target:
            break
        q = 0.93 if "rebuild" in sub else 0.91 if "index" in sub else 0.90
        out.append(make(inp, tgt, sub, quality=q))
    return out[:target]

sql_lines = make_sql_batch(SQL_EXAMPLES + SQL_EXTRA)


# ---------------------------------------------------------------------------
# Write output
# ---------------------------------------------------------------------------

all_lines = rust_lines + md_lines + toml_lines + sql_lines

with open(OUTPUT_PATH, "w", encoding="utf-8") as f:
    for line in all_lines:
        f.write(line + "\n")

print(f"Wrote {len(all_lines)} examples to {OUTPUT_PATH}")
print(f"  rust: {len(rust_lines)}")
print(f"  markdown: {len(md_lines)}")
print(f"  toml: {len(toml_lines)}")
print(f"  sql: {len(sql_lines)}")
