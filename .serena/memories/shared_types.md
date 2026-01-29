# AdapterOS Shared Types Reference

## Overview

AdapterOS uses two primary type crates for sharing types across the codebase:

1. **`adapteros-types`** - Core domain types with minimal dependencies (serde, chrono, uuid)
2. **`adapteros-api-types`** - API request/response types that wrap and extend core types

## Crate Architecture

### adapteros-types (Core Domain Types)

**Location**: `crates/adapteros-types/`

**Design Principles**:
- No framework dependencies (only serde, chrono, uuid)
- snake_case serialization via `#[serde(rename_all = "snake_case")]`
- Explicit field naming for JSON serialization
- Versioned schemas (`TYPES_SCHEMA_VERSION: &str = "1.0"`)

**Features**:
- `server` (default): Enables sqlx, schemars
- `utoipa`: OpenAPI schema generation
- `wasm`: WASM-compatible build (excludes server deps)

**Module Structure**:
```
core/           - Domain-agnostic primitives (identity, temporal, pagination)
adapters/       - Adapter lifecycle and metadata types
training/       - Training job and configuration types
routing/        - Router decision and candidate types
telemetry/      - Telemetry event types
inference/      - Inference requests/responses and receipts
coreml/         - CoreML placement specification types
fusion/         - Fusion interval policy
nodes/          - Node and status types
tenants/        - Tenant and usage types
manifest/       - Manifest metadata
repository/     - Repository assurance tiers
api/            - Common API patterns
```

### adapteros-api-types (API Layer Types)

**Location**: `crates/adapteros-api-types/`

**Design Principles**:
- Wraps core types with API-specific additions
- Adds `schema_version` field to responses
- Server feature enables axum IntoResponse + utoipa schemas
- WASM feature for Leptos UI compatibility

**Features**:
- `server` (default): Enables axum, utoipa, adapteros-core deps
- `wasm`: WASM-compatible build for Leptos UI

**Module Structure** (40+ modules):
```
adapters.rs       - Adapter registration, response, health types
training.rs       - Training job requests/responses (largest file ~25k tokens)
inference.rs      - Inference request/response with citations
models.rs         - Model status and import types
tenants.rs        - Tenant management types
nodes.rs          - Node registration and status
workers.rs        - Worker management types
auth.rs           - Authentication types
policy.rs         - Policy types
routing.rs        - Routing types for API
... (35+ more modules)
```

## Key Domain Types

### Identity Primitives (`core/identity.rs`)

```rust
pub struct TenantId(pub String);
pub struct UserId(pub Uuid);
pub struct AdapterId(pub String);  // BLAKE3 hash
pub struct TrainingJobId(pub Uuid);
```

### Adapter Types

**AdapterInfo** - Basic adapter identification:
```rust
pub struct AdapterInfo {
    pub id: String,
    pub name: String,
    pub tier: String,           // "tier_0", "tier_1", "persistent", "ephemeral"
    pub rank: u32,              // LoRA rank
    pub activation_pct: f32,    // 0.0 to 1.0
    pub loaded: bool,
    pub hash_b3: Option<String>,
    pub languages: Vec<String>,
    pub reasoning_specialties: Vec<String>,
    // ... more fields
}
```

**AdapterMetadata** - Full adapter metadata:
```rust
pub struct AdapterMetadata {
    pub adapter_id: String,
    pub name: String,
    pub hash_b3: String,
    pub rank: i32,
    pub tier: i32,              // 0 = Metal, 1 = System RAM, 2 = Disk
    pub languages: Vec<String>,
    pub lora_tier: Option<LoraTier>,  // micro/standard/max
    pub lora_strength: Option<f32>,
    // ... many optional fields
}
```

**LifecycleState** - Adapter state machine:
```rust
pub enum LifecycleState {
    Registered, Loading, Active, Inactive, Unloading, Unloaded, Expired, Error
}
```

### Stack Types

```rust
pub struct StackRecord {
    pub id: String,
    pub tenant_id: String,
    pub name: String,
    pub adapter_ids_json: String,  // JSON-encoded list
    pub workflow_type: Option<String>,
    pub determinism_mode: Option<String>,
    pub routing_determinism_mode: Option<String>,
    // ...
}
```

### Training Types

**TrainingJobStatus**:
```rust
pub enum TrainingJobStatus {
    Pending, Running, Completed, Failed, Cancelled
}
```

**TrainingConfig** - LoRA training configuration:
```rust
pub struct TrainingConfig {
    pub rank: u32,                    // LoRA rank (4, 8, 16, 32)
    pub alpha: u32,                   // LoRA alpha (typically 2x rank)
    pub targets: Vec<String>,         // Target layers
    pub epochs: u32,
    pub learning_rate: f32,
    pub batch_size: u32,
    pub preferred_backend: Option<TrainingBackendKind>,
    pub coreml_placement: Option<CoreMLPlacementSpec>,
    // ... many more fields
}
```

**LoraTier** - Marketing/operational tier:
```rust
pub enum LoraTier { Micro, Standard, Max }
```

**TrainingBackendKind**:
```rust
pub enum TrainingBackendKind {
    Auto, CoreML, Mlx, Metal, Cpu
}
```

### Inference Types

**InferRequest** - Generic inference request:
```rust
pub struct InferRequest<Backend, Interval, StopPolicy> {
    pub prompt: String,
    pub model: Option<String>,
    pub stack_id: Option<String>,
    pub max_tokens: Option<usize>,
    pub temperature: Option<f32>,
    pub seed: Option<u64>,
    pub backend: Option<Backend>,
    pub stop_policy: Option<StopPolicy>,
    // ... many more fields
}
```

**RunReceipt** - Verifiable run receipt:
```rust
pub struct RunReceipt<Hash = String> {
    pub trace_id: String,
    pub run_head_hash: Hash,
    pub output_digest: Hash,
    pub receipt_digest: Hash,
    pub stop_reason_code: Option<StopReasonCode>,
    pub prefix_cached_token_count: u32,
    pub billed_input_tokens: u32,
    // ... KV cache, lineage fields
}
```

**StopReasonCode** - Exhaustive stop reasons:
```rust
pub enum StopReasonCode {
    Length, BudgetMax, CompletionConfident, RepetitionGuard,
    StopSequence, Cancelled, SystemError
}
```

### Routing Types

**RouterDecision** - Per-step routing decision:
```rust
pub struct RouterDecision {
    pub step: usize,
    pub candidate_adapters: Vec<RouterCandidate>,
    pub entropy: f64,
    pub tau: f64,
    pub stack_hash: Option<String>,
    pub backend_type: Option<String>,
    // ...
}
```

**RouterCandidate**:
```rust
pub struct RouterCandidate {
    pub adapter_idx: u16,
    pub raw_score: f32,
    pub gate_q15: i16,  // Q15 quantized gate value
}
```

### Tenant Types

```rust
pub struct Tenant {
    pub id: String,
    pub name: String,
    pub itar_flag: bool,
    pub default_stack_id: Option<String>,
    pub max_adapters: Option<i32>,
    pub max_kv_cache_bytes: Option<i64>,
    // ...
}

pub struct TenantUsage {
    pub tenant_id: String,
    pub active_adapters_count: i32,
    pub storage_used_gb: f64,
    pub gpu_usage_pct: f64,
    // ...
}
```

## Serialization Patterns

### snake_case Convention
All types use `#[serde(rename_all = "snake_case")]` for JSON field names.

### Optional Field Handling
```rust
#[serde(skip_serializing_if = "Option::is_none")]
pub field: Option<T>,

#[serde(default)]  // Use Default::default() if missing
pub field: T,

#[serde(default = "function_name")]  // Custom default
pub field: T,
```

### Schema Version Pattern
API responses include schema version:
```rust
#[serde(default = "schema_version")]
pub schema_version: String,
```

### Flattening for Composition
```rust
#[serde(flatten)]
pub tenant: Tenant,  // Embeds all Tenant fields
```

### Aliases for Backward Compatibility
```rust
#[serde(alias = "error")]
pub message: String,
```

## WASM Compatibility Patterns

### Feature-Gated Server Dependencies
```rust
#[cfg(feature = "server")]
use adapteros_core::B3Hash;

#[cfg(feature = "server")]
pub type InferRequest = RootInferRequest<BackendKind, FusionInterval, StopPolicySpec>;
```

### Conditional Derives
```rust
#[cfg_attr(feature = "server", derive(utoipa::ToSchema))]
#[cfg_attr(feature = "server", derive(schemars::JsonSchema))]
```

### Conditional Fields
```rust
#[cfg(feature = "server")]
#[serde(skip_serializing_if = "Option::is_none")]
pub run_receipt: Option<RunReceipt>,
```

### WASM UUID Support
```toml
[target.'cfg(target_arch = "wasm32")'.dependencies]
uuid = { version = "1.6", features = ["v4", "v7", "serde", "js"] }
```

## Type Validation Patterns

### Builder Pattern
```rust
impl AdapterInfo {
    pub fn new(id: impl Into<String>) -> Self { ... }
    pub fn with_name(mut self, name: impl Into<String>) -> Self { ... }
    pub fn with_tier(mut self, tier: impl Into<String>) -> Self { ... }
}
```

### Validation Methods
```rust
impl TrainingConfigRequest {
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.rank == 0 {
            errors.push("rank must be > 0".to_string());
        }
        // ...
    }
}
```

### State Machine Methods
```rust
impl LifecycleState {
    pub fn is_usable(&self) -> bool { matches!(self, Self::Active) }
    pub fn is_transitional(&self) -> bool { matches!(self, Self::Loading | Self::Unloading) }
}

impl TrainingJobStatus {
    pub fn is_terminal(&self) -> bool { ... }
    pub fn is_active(&self) -> bool { ... }
}
```

## Key Constants

- `TYPES_SCHEMA_VERSION: &str = "1.0"` - Types schema version
- `API_SCHEMA_VERSION: &str = "1.0.0"` - API schema version
- `TRAINING_DATA_CONTRACT_VERSION: &str` - Training data contract
- `STOP_Q15_DENOM: f32 = 32767.0` - Q15 quantization denominator

## Import Guidelines

1. **Specific imports** (recommended):
   ```rust
   use adapteros_types::adapters::AdapterInfo;
   use adapteros_types::core::Uuid;
   ```

2. **Aggregate imports** (convenience):
   ```rust
   use adapteros_types::AdapterInfo;  // Re-exported at root
   ```

3. **Module imports** (namespace clarity):
   ```rust
   use adapteros_types::{adapters, core, routing};
   ```

## Usage in Leptos UI

The UI uses `adapteros-api-types` with the `wasm` feature:
```toml
adapteros-api-types = { path = "../adapteros-api-types", features = ["wasm"] }
```

This ensures compile-time type consistency between server and WASM client while excluding server-only dependencies like axum and utoipa.
