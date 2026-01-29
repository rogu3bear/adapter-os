# AdapterOS Policy System

## Overview

The policy system in `crates/adapteros-policy/` enforces determinism, security, and safety guarantees across AdapterOS. It consists of **29 canonical policy packs** with multi-stage enforcement hooks.

## Key Architecture Components

### 1. Policy Registry (`registry.rs`)

The `PolicyId` enum defines all 29 policy packs:

```rust
pub enum PolicyId {
    Egress = 1,      // Network egress control
    Determinism = 2, // Reproducible outputs
    Router = 3,      // Adapter selection
    Evidence = 4,    // Citation requirements
    Refusal = 5,     // Safety abstention
    Numeric = 6,     // Precision validation
    Rag = 7,         // Retrieval policies
    Isolation = 8,   // Tenant isolation
    Telemetry = 9,   // Logging policies
    Retention = 10,  // Data lifecycle
    Performance = 11,
    Memory = 12,
    Artifacts = 13,
    Secrets = 14,
    BuildRelease = 15,
    Compliance = 16,
    Incident = 17,
    Output = 18,
    Adapters = 19,
    DeterministicIo = 20,
    Drift = 21,
    Mplora = 22,
    Naming = 23,
    DependencySecurity = 24,
    CircuitBreaker = 25,
    Capability = 26,
    Language = 27,
    QueryIntent = 28,
    LiveData = 29,
}
```

### 2. Core Policy Trait (`registry.rs`)

```rust
pub trait Policy {
    fn id(&self) -> PolicyId;
    fn name(&self) -> &'static str;
    fn severity(&self) -> Severity;  // Critical, High, Medium, Low
    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit>;
}
```

### 3. Core Policies (Always Enabled)

```rust
// hooks.rs - These 4 policies are always active
pub const CORE_POLICIES: [PolicyPackId; 4] = [
    PolicyPackId::Egress,      // Zero data exfiltration
    PolicyPackId::Determinism, // Identical inputs = identical outputs
    PolicyPackId::Isolation,   // Tenant boundaries
    PolicyPackId::Evidence,    // Answers cite sources or abstain
];
```

## Policy Hook System (`hooks.rs`)

Three enforcement points in the request lifecycle:

| Hook | When | Purpose |
|------|------|---------|
| `OnRequestBeforeRouting` | Before adapter selection | Tenant isolation, rate limiting |
| `OnBeforeInference` | After routing, before inference | Resource checks, determinism setup |
| `OnAfterInference` | After inference | Output validation, evidence requirements |

### Hook Context

```rust
pub struct HookContext {
    pub tenant_id: String,
    pub user_id: Option<String>,
    pub request_id: String,
    pub hook: PolicyHook,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub metadata: HashMap<String, serde_json::Value>,
}
```

### Policy Decision Types

```rust
pub enum Decision {
    Allow,
    Deny,
    Modify { modifications: HashMap<String, serde_json::Value> },
}
```

## Quarantine System (`quarantine.rs`)

When policy violations occur, the system enters quarantine mode:

```rust
pub enum QuarantineOperation {
    Inference,        // DENIED in quarantine
    AdapterLoad,      // DENIED
    AdapterSwap,      // DENIED
    MemoryOperation,  // DENIED
    Training,         // DENIED
    PolicyUpdate,     // DENIED
    Audit,            // ALLOWED (read-only)
    Status,           // ALLOWED
    Metrics,          // ALLOWED
}
```

### QuarantineManager API

```rust
let mut manager = QuarantineManager::new();
manager.set_quarantined(true, "Policy hash mismatch".to_string());
manager.check_operation(QuarantineOperation::Inference)?; // Returns Err if quarantined
manager.release_quarantine();
```

## Policy Hash Watcher (`hash_watcher.rs`)

Detects runtime policy mutations via BLAKE3 hashing:

```rust
let watcher = PolicyHashWatcher::new(db, telemetry, cpid);

// Register baseline hash
watcher.register_baseline("determinism", &hash, signer_pubkey).await?;

// Validate at runtime
let result = watcher.validate_policy_pack("determinism", &current_hash).await?;
if !result.valid {
    watcher.trigger_quarantine("Policy hash mismatch").await?;
}
```

## Key Policy Implementations

### Determinism Policy (`packs/determinism.rs`)

```rust
pub struct DeterminismConfig {
    pub require_metallib_embed: bool,
    pub require_kernel_hash_match: bool,
    pub rng: RngSeedingMethod,  // HkdfSeeded, FixedSeed, SystemEntropy
    pub epsilon_bounds: EpsilonBounds,
    pub enforcement_mode: EnforcementMode,  // Strict or BestEffort
    pub kernel_allow_list: Option<Vec<String>>,
    pub kernel_deny_list: Vec<String>,
    pub fallback_mappings: HashMap<String, String>,
}
```

Enforcement modes:
- **Strict**: Reject non-deterministic operations immediately
- **BestEffort**: Warn and substitute deterministic fallback if available

### Refusal Policy (`packs/refusal.rs`)

```rust
pub struct RefusalConfig {
    pub abstain_threshold: f32,       // Default: 0.40
    pub best_effort_threshold: f32,   // Default: 0.70
    pub safety_checks: SafetyChecks,
    pub redaction_rules: RedactionRules,
}

pub enum ResponseMode {
    Complete,                           // High confidence
    BestEffort { assumptions: Vec<String> },  // Moderate confidence
    Abstain,                            // Low confidence or safety violation
}
```

High-stakes domain thresholds:
- Medical: 0.85
- Legal: 0.80
- Financial: 0.80

### Isolation Policy (`packs/isolation.rs`)

```rust
pub struct IsolationConfig {
    pub process_model: ProcessModel,  // PerTenant, Shared, Hybrid
    pub forbid_shm: bool,             // Forbid shared memory
    pub keys: KeyConfig,              // SecureEnclave, TPM, Software
    pub filesystem: FilesystemIsolation,
    pub network: NetworkIsolation,
}
```

## Unified Enforcement (`unified_enforcement.rs`)

```rust
pub trait PolicyEnforcer {
    async fn validate_request(&self, request: &PolicyRequest) -> Result<PolicyValidationResult>;
    async fn is_operation_allowed(&self, operation: &Operation) -> Result<bool>;
    async fn enforce_policy(&self, operation: &Operation) -> Result<PolicyEnforcementResult>;
    async fn get_compliance_report(&self) -> Result<PolicyComplianceReport>;
}
```

## Policy Pack Manager (`policy_packs.rs`)

Central manager for all policy packs:

```rust
let manager = PolicyPackManager::new();
let validation = manager.validate_request(&request)?;
// validation.violations and validation.warnings
```

## Signed Policy Packs (`policy_pack.rs`)

Ed25519 signing for policy integrity:

```rust
let signed_pack = SignedPolicyPack::sign(
    "determinism",
    "1.0",
    policy_data,
    &signing_key
)?;

// Registry with trusted keys
let mut registry = PolicyPackRegistry::new();
registry.add_trusted_key(pubkey);
registry.register_pack(signed_pack)?;
```

## Creating a New Policy

1. Add variant to `PolicyId` enum in `registry.rs`
2. Create policy module in `packs/` with:
   - `XxxConfig` struct with defaults
   - `XxxPolicy` struct implementing `Policy` trait
3. Add to `packs/mod.rs` exports
4. Add factory method in `PolicyPackFactory`
5. Register in `PolicyPackManager`

## Actionable Patterns

### Checking Policy Before Operation

```rust
use adapteros_policy::{PolicyEngine, QuarantineManager, QuarantineOperation};

// Check quarantine first
manager.check_operation(QuarantineOperation::Inference)?;

// Then validate specific policies
engine.validate_input_content(&prompt)?;
engine.check_confidence(0.75)?;
engine.check_resource_limits(max_tokens)?;
```

### Creating Policy Decision Chain

```rust
let chain = engine.evaluate_inference_policies(request_id, metadata)?;
// chain.validation - PolicyValidationResult
// chain.decisions - Vec<PolicyDecisionRecord>
// chain.digest - B3Hash for audit trail
```

### High-Stakes Domain Detection

```rust
let refusal_policy = RefusalPolicy::new(RefusalConfig::default());
let domain = refusal_policy.detect_high_stakes_domain(content);
let threshold = refusal_policy.get_domain_threshold(&domain);
if confidence < threshold {
    return refusal_policy.generate_refusal_response(
        RefusalReason::HighStakesDomain, None, confidence, None
    );
}
```

## File Locations

- `lib.rs` - Main exports and PolicyEngine
- `registry.rs` - PolicyId enum and Policy trait
- `hooks.rs` - Hook system (CORE_POLICIES defined here)
- `quarantine.rs` - QuarantineManager
- `hash_watcher.rs` - PolicyHashWatcher
- `policy_pack.rs` - SignedPolicyPack
- `policy_packs.rs` - PolicyPackManager
- `unified_enforcement.rs` - UnifiedPolicyEnforcer
- `packs/` - Individual policy implementations
