# adapteros-lora-lifecycle

Adapter lifecycle management for adapterOS: state transitions, memory pressure eviction, and K-sparse routing coordination.

## Purpose

This crate manages the **runtime lifecycle** of LoRA adapters:

1. **State transitions**: Cold -> Warm -> Hot -> Resident (and reverse)
2. **Memory pressure response**: Coordinated eviction when GPU memory is constrained
3. **K reduction**: Dynamic reduction of active adapter count under memory pressure
4. **Category-aware policies**: Different TTLs and eviction priorities per adapter category
5. **Model acquisition**: Download and verify models from model hub

## Adapter State Machine

```
Unloaded  <-->  Cold  <-->  Warm  <-->  Hot  <-->  Resident
    |            |           |          |            |
    v            v           v          v            v
Not loaded   On disk    In RAM    In VRAM     Pinned VRAM
             (mmap'd)   (cache)   (active)    (never evict)
```

### State Descriptions

| State | Location | Eviction Priority |
|-------|----------|-------------------|
| Unloaded | Not available | N/A |
| Cold | Disk (memory-mapped) | Highest |
| Warm | CPU RAM cache | Medium |
| Hot | GPU VRAM | Low |
| Resident | GPU VRAM (pinned) | Never evicted |

## Key Types

| Type | Purpose |
|------|---------|
| `LifecycleManager` | Central coordinator for adapter states |
| `AdapterStateRecord` | Per-adapter state and metadata |
| `AdapterState` | Enum: Unloaded, Cold, Warm, Hot, Resident |
| `LifecyclePolicy` | Promotion/demotion rules from manifest |
| `CategoryPolicyManager` | Category-specific TTL and eviction rules |
| `ActivationTracker` | Rolling window of adapter activations |
| `LifecycleKReductionCoordinator` | Memory pressure <-> K reduction bridge |

## K Reduction System

When memory pressure exceeds thresholds, the memory manager sends `KReductionRequest` events. The lifecycle manager:

1. Evaluates the request against current adapter states
2. Decides which adapters to evict (lowest activation, highest eviction priority)
3. Executes the eviction and updates K value for the router
4. Records the decision for audit trail

```rust
// Wire the K reduction channel from memory manager
lifecycle_manager.wire_k_reduction_channel(rx);

// Poll for requests (in background loop)
let processed = lifecycle_manager.poll_k_reduction_events().await?;
```

## Usage

```rust
use adapteros_lora_lifecycle::{LifecycleManager, AdapterHeatState};

// Create manager
let manager = LifecycleManager::new(
    adapter_names,
    adapter_hashes,
    &policies,
    adapters_path,
    telemetry,
    initial_k,
);

// Register a new adapter
let idx = manager.register_adapter(
    "my-adapter".into(),
    hash,
    Some("code".into()),
    true, // load immediately
)?;

// Promote an adapter
manager.promote_adapter(idx).await?;

// Handle memory pressure
manager.evict_adapters(pressure_level).await?;

// Get adapter handle for inference
let handle = manager.get_or_reload("my-adapter")?;
```

## Category Policies

Adapters can have category-specific behavior:

```rust
// Different categories have different policies
manager.category_policies.set_policy("code", CategoryPolicy {
    min_ttl: Duration::from_secs(300),
    max_ttl: Duration::from_secs(3600),
    eviction_priority: EvictionPriority::Medium,
});

manager.category_policies.set_policy("safety", CategoryPolicy {
    min_ttl: Duration::from_secs(0),
    max_ttl: Duration::from_secs(86400),
    eviction_priority: EvictionPriority::Low, // Hard to evict
});
```

## Telemetry Events

The manager emits telemetry for:
- `AdapterTransitionEvent`: State changes
- `AdapterActivationEvent`: Adapter usage
- `AdapterEvictionEvent`: Evictions with memory freed
- `GpuIntegrityVerificationEvent`: Buffer fingerprint checks
- `DownloadProgress`: Model acquisition progress

## Modules

- **`state`**: `AdapterState` enum and `AdapterStateRecord`
- **`loader`**: `AdapterLoader` for disk I/O with hash verification
- **`policy`**: `LifecyclePolicy` from manifest policies
- **`category_policies`**: Per-category TTL and eviction rules
- **`activation_tracker`**: Rolling activation window
- **`ttl_manager`**: TTL-based eviction scheduling
- **`k_reduction_coordinator`**: Memory-lifecycle coordination
- **`workflow_executor`**: Adapter execution backends

## Integration

- Receives memory pressure signals from `adapteros-memory`
- Loads adapters via `adapteros-aos` (memory-mapped files)
- Reports metrics to `adapteros-profiler`
- Persists state changes to `adapteros-db`
