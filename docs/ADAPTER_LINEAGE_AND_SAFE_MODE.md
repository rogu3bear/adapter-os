# Adapter Lineage & Safe Mode Routing

**Last Updated:** 2025-01-16

## Overview

AdapterOS now supports **adapter lineage tracking** and **safe mode routing**. These features enable:

1. **Adapter Lineage**: Hierarchical parent-child relationships between adapters for version evolution and specialization stacking
2. **Safe Mode**: Global routing mode that restricts inference to safety-filtered responses only

This document describes both features and their integration into the AdapterOS routing system.

---

## Adapter Lineage

### Concept

Adapter lineage allows you to define parent-child relationships between adapters in your manifest. This enables:

- **Version Evolution**: Track adapter versions as parent-child chains (e.g., `base-v1` → `base-v2` → `base-v3`)
- **Specialization Stacking**: Build specialized adapters on top of base adapters (e.g., `python-general` → `python-fastapi`)
- **Automatic Parent Loading**: When a child adapter is selected, its parent can be automatically loaded and applied

### Manifest Schema

The adapter manifest (`adapteros-manifest/src/lib.rs`) includes lineage fields:

```rust
pub struct Adapter {
    pub id: String,
    pub hash: B3Hash,
    // ... other fields ...

    /// Optional parent adapter identifier used for lineage stacking
    #[serde(default)]
    pub parent_adapter_id: Option<String>,

    /// Arbitrary domain tags ("code", "vision", "finance") for UI routing toggles
    #[serde(default)]
    pub domains: Vec<String>,

    /// Marks adapter as safety layer (used for Safe Mode routing)
    #[serde(default)]
    pub is_safety_adapter: bool,
}
```

### Example Manifest

```json
{
  "schema": "adapteros.manifest.v3",
  "adapters": [
    {
      "id": "python-base",
      "hash": "b3:abc123...",
      "tier": "persistent",
      "rank": 16,
      "alpha": 32.0,
      "target_modules": ["q_proj", "v_proj"],
      "parent_adapter_id": null,
      "domains": ["code", "python"]
    },
    {
      "id": "python-fastapi",
      "hash": "b3:def456...",
      "tier": "persistent",
      "rank": 16,
      "alpha": 32.0,
      "target_modules": ["q_proj", "v_proj"],
      "parent_adapter_id": "python-base",
      "domains": ["code", "python", "web"]
    },
    {
      "id": "safety-filter",
      "hash": "b3:789ghi...",
      "tier": "persistent",
      "rank": 8,
      "alpha": 16.0,
      "target_modules": ["q_proj"],
      "is_safety_adapter": true,
      "domains": ["safety", "moderation"]
    }
  ]
}
```

### Automatic Parent Loading

When `enable_lineage_loading` is enabled in the router config, the LifecycleManager will automatically:

1. Check if a selected adapter has a `parent_adapter_id`
2. Load the parent adapter if not already loaded
3. Recursively load grandparents up the lineage chain
4. Promote parent adapters to `Warm` state (available but not hot)

**Implementation**: `crates/adapteros-lora-lifecycle/src/lib.rs::load_parent_adapter()`

```rust
// Example: Loading adapter with lineage
lifecycle_manager.ensure_adapters_loaded(&[child_adapter_idx]).await?;
// If lineage loading is enabled, parent is automatically loaded
```

### Weight Combination

When both parent and child adapters are loaded, their LoRA deltas are **additively combined** during inference:

```
final_delta = parent_delta + child_delta
```

The order of application is deterministic:
1. Parent adapter deltas applied first
2. Child adapter deltas applied second

This additive combination is mathematically sound for LoRA weights, as they represent deltas from the base model.

### API

**LifecycleManager Methods:**

```rust
/// Load parent adapter and its ancestors recursively
pub async fn load_parent_adapter(&self, adapter_idx: u16) -> Result<bool>

/// Get full lineage chain (from root to current adapter)
pub fn get_adapter_lineage(&self, adapter_idx: u16) -> Vec<u16>

/// Get adapters by domain tag
pub fn get_adapters_by_domain(&self, domain: &str) -> Vec<u16>
```

**Example Usage:**

```rust
// Get lineage chain
let lineage = lifecycle_manager.get_adapter_lineage(child_idx);
// Returns: [grandparent_idx, parent_idx, child_idx]

// Get all Python adapters
let python_adapters = lifecycle_manager.get_adapters_by_domain("python");
```

---

## Safe Mode

### Concept

Safe Mode is a global routing override that forces all inference requests to use only **safety adapters** (`is_safety_adapter: true`). This is useful for:

- **Production Safeguards**: Prevent potentially unsafe responses in production
- **Compliance**: Ensure all outputs pass through content filtering
- **Testing**: Validate safety adapter behavior in isolation

When Safe Mode is **enabled**:
- Router ignores normal K-sparse selection
- Only adapters marked with `is_safety_adapter: true` are used
- If no safety adapter exists, the system falls back to the base model

When Safe Mode is **disabled** (default):
- Normal routing behavior applies
- All adapters participate in K-sparse selection

### Configuration

Safe Mode is controlled via the router configuration:

```json
{
  "k_sparse": 3,
  "gate_quant": "q15",
  "entropy_floor": 0.7,
  "safe_mode": false,
  "enable_lineage_loading": true
}
```

**File**: `router_config.json` (or path specified in CLI)

### CLI Usage

```bash
# Enable safe mode
aosctl router safe-mode --enable true

# Disable safe mode
aosctl router safe-mode --enable false

# Specify custom config file
aosctl router safe-mode --enable true --config /path/to/router_config.json
```

**Implementation**: `crates/adapteros-cli/src/commands/router.rs::safe_mode()`

### Router Policy

The router policy (`adapteros-policy/src/packs/router.rs`) tracks safe mode state:

```rust
pub struct RouterConfig {
    // ... other fields ...

    /// Safe mode enabled - forces routing through safety adapter only
    #[serde(default)]
    pub safe_mode: bool,

    /// Enable automatic parent adapter loading for lineage stacking
    #[serde(default = "default_enable_lineage")]
    pub enable_lineage_loading: bool,
}
```

**RouterPolicy Methods:**

```rust
pub fn is_safe_mode_enabled(&self) -> bool
pub fn enable_safe_mode(&mut self)
pub fn disable_safe_mode(&mut self)
```

### Routing Logic (Conceptual)

When the router makes selection decisions, it should check for safe mode:

```rust
// Pseudo-code for router integration
fn select_adapters(request: &Request, config: &RouterConfig, lifecycle: &LifecycleManager) -> Vec<u16> {
    if config.safe_mode {
        // Safe mode: only use safety adapters
        let safety_adapters = lifecycle.get_safety_adapters();
        if safety_adapters.is_empty() {
            warn!("Safe mode enabled but no safety adapters found");
            return vec![]; // Fall back to base model
        }
        return safety_adapters;
    } else {
        // Normal mode: K-sparse selection
        let candidates = lifecycle.get_available_adapters().await;
        let scores = compute_scores(request, candidates);
        return select_top_k(scores, config.k_sparse);
    }
}
```

**Note**: The actual router implementation is in `adapteros-lora-router`. Integration of safe mode into the router is left to the router implementation.

---

## Integration Points

### 1. Manifest Parsing

**File**: `crates/adapteros-manifest/src/lib.rs`

Adapters now include:
- `parent_adapter_id: Option<String>`
- `domains: Vec<String>`
- `is_safety_adapter: bool`

These fields are parsed from the manifest and available to the runtime.

### 2. State Management

**File**: `crates/adapteros-lora-lifecycle/src/state.rs`

`AdapterStateRecord` tracks lineage metadata:

```rust
pub struct AdapterStateRecord {
    // ... other fields ...
    pub parent_adapter_id: Option<String>,
    pub is_safety_adapter: bool,
    pub domains: Vec<String>,
}
```

### 3. Lifecycle Manager

**File**: `crates/adapteros-lora-lifecycle/src/lib.rs`

Methods:
- `load_parent_adapter()`: Recursive parent loading
- `get_adapter_lineage()`: Lineage chain query
- `get_safety_adapters()`: Filter for safety adapters
- `get_adapters_by_domain()`: Filter by domain tag

### 4. Router Policy

**File**: `crates/adapteros-policy/src/packs/router.rs`

`RouterConfig` includes:
- `safe_mode: bool`
- `enable_lineage_loading: bool`

`RouterPolicy` provides:
- `is_safe_mode_enabled()`
- `enable_safe_mode()` / `disable_safe_mode()`
- `is_lineage_loading_enabled()`

### 5. CLI

**File**: `crates/adapteros-cli/src/commands/router.rs`

New command:
```bash
aosctl router safe-mode --enable <true|false>
```

---

## Testing

### Test Scenario: Adapter Lineage

1. **Create Parent Adapter** (`parent.aos`):
   ```json
   {
     "adapter_id": "parent",
     "version": "1.0",
     "parent_adapter_id": null
   }
   ```

2. **Create Child Adapter** (`child.aos`):
   ```json
   {
     "adapter_id": "child",
     "version": "1.0",
     "parent_adapter_id": "parent"
   }
   ```

3. **Load Child**:
   ```rust
   lifecycle_manager.ensure_adapters_loaded(&[child_idx]).await?;
   ```

4. **Verify**:
   - Parent is automatically loaded
   - Parent state is `Warm`
   - Lineage chain is `[parent_idx, child_idx]`

### Test Scenario: Safe Mode

1. **Create Safety Adapter** (`safety.aos`):
   ```json
   {
     "adapter_id": "safety",
     "is_safety_adapter": true
   }
   ```

2. **Enable Safe Mode**:
   ```bash
   aosctl router safe-mode --enable true
   ```

3. **Verify**:
   - Config file shows `"safe_mode": true`
   - Router only selects safety adapter
   - Non-safety queries receive filtered responses

4. **Disable Safe Mode**:
   ```bash
   aosctl router safe-mode --enable false
   ```

5. **Verify**:
   - Normal routing resumes
   - All adapters participate in selection

---

## Determinism & Reproducibility

Both features are designed to be **deterministic**:

1. **Lineage Loading**:
   - Parent load order is deterministic (depth-first, oldest ancestor first)
   - Weight combination order is fixed (parent → child)
   - No randomness in parent resolution

2. **Safe Mode**:
   - Binary toggle (on/off)
   - No probabilistic selection
   - Same config produces same routing decisions

Telemetry events log all state transitions for audit trails.

---

## Performance Considerations

### Lineage Loading

- **Memory**: Loading parent adapters increases memory footprint
  - Each parent adapter ~50MB (estimate)
  - Deep lineage chains (>3 levels) may cause memory pressure
- **Latency**: Recursive parent loading adds cold-start latency
  - First load: +100-200ms per parent
  - Subsequent loads: cached (0ms)

**Mitigation**: Parents are loaded asynchronously and promoted to `Warm` state (not `Hot`), minimizing memory pressure.

### Safe Mode

- **Routing Overhead**: Safe mode bypasses normal K-sparse selection, reducing router computation
- **Quality Trade-off**: Limiting to safety adapters may reduce response quality for domain-specific queries

---

## Future Enhancements

1. **Lineage-Aware Routing**:
   - Use lineage metadata for routing decisions
   - Example: Prefer child adapters over parents for specialized queries

2. **Domain-Based Routing**:
   - Implement domain-specific routing (e.g., "code" mode, "vision" mode)
   - Allow per-request domain selection

3. **Safety Adapter Stacking**:
   - Allow multiple safety adapters in a pipeline
   - Example: toxicity filter → PII filter → output

4. **Lineage Visualization**:
   - CLI/UI to visualize adapter dependency graphs
   - Example: `aosctl adapters tree`

5. **Per-Tenant Safe Mode**:
   - Allow safe mode per tenant instead of global
   - Example: Production tenant always uses safe mode

---

## References

- **Manifest Schema**: `crates/adapteros-manifest/src/lib.rs`
- **Lifecycle Manager**: `crates/adapteros-lora-lifecycle/src/lib.rs`
- **Router Policy**: `crates/adapteros-policy/src/packs/router.rs`
- **CLI Commands**: `crates/adapteros-cli/src/commands/router.rs`
- **State Management**: `crates/adapteros-lora-lifecycle/src/state.rs`

---

**End of Document**
