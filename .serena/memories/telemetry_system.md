# AdapterOS Telemetry System

## Overview

The telemetry system in `crates/adapteros-telemetry/` provides deterministic, auditable event logging with cryptographic integrity verification. It is designed for air-gapped deployments with zero network egress during serving.

**Key Files:**
- `/Users/star/Dev/adapter-os/crates/adapteros-telemetry/src/lib.rs` - Main entry point, TelemetryWriter
- `/Users/star/Dev/adapter-os/crates/adapteros-telemetry/src/merkle.rs` - Merkle tree for audit trails
- `/Users/star/Dev/adapter-os/crates/adapteros-telemetry/src/ring_buffer.rs` - Lock-free event buffer
- `/Users/star/Dev/adapter-os/crates/adapteros-telemetry/src/unified_events.rs` - Unified event schema
- `/Users/star/Dev/adapter-os/crates/adapteros-telemetry/src/bundle_store.rs` - Content-addressed storage
- `/Users/star/Dev/adapter-os/crates/adapteros-telemetry/src/sampling.rs` - Event sampling strategies

## Architecture

### 1. Event Logging Architecture

**TelemetryWriter** (`lib.rs`):
- Background thread-based writer with bounded channel (50,000 event capacity)
- Non-blocking send to avoid stalling inference operations
- Uses `crossbeam::channel` for backpressure management
- Events dropped under burst load are tracked via `DROPPED_EVENTS` atomic counter
- All bundles signed with persistent Ed25519 key (per Artifacts Ruleset #13)

**Event Flow:**
1. `log_event()` - Non-blocking try_send to channel
2. Background thread receives events via `run_writer()`
3. Events validated and serialized to NDJSON
4. Event hashes collected for Merkle tree
5. Bundle rotated at `max_events` or `max_bytes` threshold
6. Bundle finalized with Merkle root and Ed25519 signature

**Commands:**
- `TelemetryCommand::Event(Box<UnifiedTelemetryEvent>)` - Log event
- `TelemetryCommand::Flush(Sender<Result<()>>)` - Sync flush
- `TelemetryCommand::Shutdown(Sender<Result<()>>)` - Graceful shutdown

### 2. Merkle Tree for Audit Trails

**Location:** `/Users/star/Dev/adapter-os/crates/adapteros-telemetry/src/merkle.rs`

**Implementation:**
- Deterministic ordering via canonical JSON serialization (serde_jcs)
- BLAKE3 hashing for all operations
- Binary tree construction bottom-up
- Parent = BLAKE3(left || right)
- Odd leaf count: duplicate last leaf

**Functions:**
- `compute_merkle_root<T: Serialize>(events)` - Compute root hash
- `build_merkle_tree<T: Serialize>(events)` - Build full tree with nodes
- `generate_proof<T: Serialize>(events, index)` - Generate inclusion proof
- `verify_proof(leaf_hash, proof)` - Verify event inclusion

**MerkleProof Structure:**
```rust
pub struct MerkleProof {
    pub index: usize,           // Event index
    pub siblings: Vec<B3Hash>,  // Sibling hashes to root
    pub root: B3Hash,           // Root hash
}
```

### 3. Telemetry Buffers and Flushing

**TelemetryRingBuffer** (`ring_buffer.rs`):
- Lock-free circular buffer using atomic operations
- Configurable capacity (default 10,000 events)
- Automatic eviction of oldest events when full
- Drop warning threshold (default 10% of capacity)
- Async read operations with RwLock

**Methods:**
- `push(event)` - Add event (overwrites oldest if full)
- `read_all()` - Get all events in chronological order
- `read_recent(n)` - Get N most recent events
- `read_filtered(predicate)` - Filter events
- `stats()` - Get utilization metrics

**Flush Mechanism:**
- `TelemetryWriter::flush()` - Blocking flush
- `TelemetryWriter::flush_with_timeout(duration)` - Timeout-bounded flush
- Bundle rotation triggers automatic flush
- Shutdown triggers final flush

### 4. Event Types and Schemas

**Unified Event Schema** (`unified_events.rs`):
```rust
pub struct TelemetryEvent {
    pub id: String,                    // UUID v7
    pub timestamp: DateTime<Utc>,      // ISO 8601
    pub event_type: String,            // e.g., "system.start"
    pub level: LogLevel,               // Debug/Info/Warn/Error/Critical
    pub message: String,               // Human-readable
    pub component: Option<String>,     // Source component
    pub identity: IdentityEnvelope,    // Required tenant context
    pub user_id: Option<String>,
    pub metadata: Option<Value>,       // Arbitrary JSON
    pub trace_id: Option<String>,      // Distributed tracing
    pub span_id: Option<String>,
    pub hash: Option<String>,          // BLAKE3 integrity hash
    pub sampling_rate: Option<f32>,
}
```

**Event Categories** (`EventType` enum):
- System: `SystemStart`, `SystemStop`, `SystemError`, `SystemWarning`
- Adapter: `AdapterLoaded`, `AdapterUnloaded`, `AdapterEvicted`, `AdapterPinned`, `AdapterExpired`
- Inference: `InferenceStart`, `InferenceComplete`, `InferenceError`, `InferenceTimeout`
- Policy: `PolicyViolation`, `PolicyEnforcement`, `PolicyCheck`, `PolicyUpdate`
- Memory: `MemoryPressure`, `MemoryEviction`, `MemoryAllocation`
- Training: `TrainingStart`, `TrainingComplete`, `TrainingError`, `TrainingProgress`
- Router: `RouterDecision`, `RouterCalibration`, `RouterError`
- Security: `SecurityViolation`, `SecurityCheck`, `SecurityAlert`
- Custom: `Custom(String)` for arbitrary event types

**Specialized Event Types** (`events/telemetry_events.rs`):
- `InferenceEvent` - RNG tracking for deterministic replay
- `RouterDecisionEvent` - Per-token routing decisions with Q15 gates
- `AbstainEvent` - Policy abstention with reasons
- `AdapterEvictionEvent` - Memory pressure evictions
- `KReductionEvent` - K-sparse reduction lifecycle
- `PolicyHashValidationEvent` - Runtime policy hash validation
- `ResidencyProbeEvent` - Base model residency tracking
- `KvResidencyEvent` - KV cache state transitions
- `BackendSelectionEvent` - Backend fallback/downgrade tracking

### 5. Server Integration

**AppState Integration** (`crates/adapteros-server-api/src/state.rs`):
```rust
pub struct AppState {
    // Telemetry fields
    pub metrics_collector: Arc<MetricsCollector>,
    pub metrics_registry: Arc<MetricsRegistry>,
    pub telemetry_buffer: Arc<TelemetryBuffer>,
    pub trace_buffer: Arc<TraceBuffer>,
    pub telemetry_tx: TelemetrySender,
    pub telemetry_bundle_store: Arc<RwLock<BundleStore>>,
    pub diagnostics_service: Option<Arc<DiagnosticsService>>,
}
```

**Bundle Store** (`bundle_store.rs`):
- Content-addressed storage with BLAKE3 hashing
- Retention policy per CPID (default 12 bundles)
- Incident bundle protection (never evicted)
- Promotion bundle preservation for provenance
- Chain verification via `prev_bundle_hash`
- Garbage collection with policy enforcement

**Retention Policy:**
```rust
pub struct RetentionPolicy {
    pub keep_bundles_per_cpid: usize,  // Default: 12
    pub keep_incident_bundles: bool,    // Default: true
    pub keep_promotion_bundles: bool,   // Default: true
    pub evict_strategy: EvictionStrategy,
}
```

### 6. Sampling Strategies

**Location:** `/Users/star/Dev/adapter-os/crates/adapteros-telemetry/src/sampling.rs`

**Sampling Rules (Policy Pack #9):**
- Security events: 100% (MUST NOT be sampled)
- Policy violations: 100%
- Egress attempts: 100%
- System errors: 100%
- Performance metrics: 10% default
- Router decisions: 10%
- Inference events: 10%
- Debug events: 1%

**Strategies:**
- `SamplingStrategy::Always` - 100% sampling
- `SamplingStrategy::Never` - 0% sampling
- `SamplingStrategy::Fixed(rate)` - Fixed probability
- `SamplingStrategy::HeadSampling { count, window_secs }` - First N per window
- `SamplingStrategy::Adaptive { base_rate, min_rate, max_rate }` - Dynamic adjustment

**Deterministic RNG:**
- Uses BLAKE3-derived seed from process entropy
- StdRng for reproducible sampling decisions

### 7. Observability Helpers

**Location:** `/Users/star/Dev/adapter-os/crates/adapteros-telemetry/src/observability.rs`

**Canonical Event Builders:**
- `build_health_event()` - Worker health lifecycle
- `build_inference_metrics_event()` - Inference performance
- `build_routing_event()` - Router decisions with chain
- `build_auth_event()` - Authentication flows
- `build_model_load_failed_event()` - Model loading failures
- `build_adapter_load_failed_event()` - Adapter loading failures

### 8. Key Patterns

**Identity Envelope Required:**
All events require `IdentityEnvelope` with tenant_id, domain, purpose, version.

**BLAKE3 Everywhere:**
- Event hashes
- Bundle hashes
- Merkle tree nodes
- Key derivation

**Graceful Degradation:**
- Dropped events tracked, not fatal
- Disabled bundle store mode available
- Timeout-bounded operations

**Determinism Support:**
- Seed checksums in inference events
- RNG state snapshots for replay
- Canonical JSON serialization (JCS)
