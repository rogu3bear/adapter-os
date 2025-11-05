# Policy Enforcement Architecture

## Overview

AdapterOS enforces 20 canonical policy packs across all system operations. Policy enforcement happens at multiple layers:

1. **Server Layer**: Pre-request validation before forwarding to workers
2. **Worker Layer**: Pre-inference and post-inference validation
3. **Pack-Based System**: Centralized validation through `PolicyPackManager`

## Architecture

### PolicyPackManager

The `PolicyPackManager` is the central coordinator for all policy packs. It:

- Maintains registry of all 20 policy pack validators
- Manages policy pack configurations (enabled/disabled, enforcement levels)
- Validates requests against all active policy packs
- Implements the `PolicyEnforcer` trait for unified enforcement interface

### PolicyEngine

The `PolicyEngine` wraps `PolicyPackManager` and integrates with manifest-based policies:

- Configures pack manager from manifest `Policies`
- Provides backward-compatible interface for legacy code
- Delegates enforcement to `PolicyPackManager`

### Enforcement Flow

```
Request → Server Policy Validation → Worker Pre-Inference Validation → 
Inference Execution → Worker Post-Inference Validation → Response
```

## Enforcement Levels

Each policy pack has an enforcement level that determines how violations are handled:

- **Info**: Violations are logged but never block operations
- **Warning**: Violations block only if severity is Error, Critical, or Blocker
- **Error**: Violations block if severity is Error, Critical, or Blocker
- **Critical**: All violations block operations

## Performance Optimizations

### Short-Circuiting

Validation stops early when a critical blocker violation is found that would block the request regardless of other packs. This reduces latency for clearly invalid requests.

### Validation Order

Policy packs are validated in registration order. Critical packs (Egress, Determinism) are validated early to catch blockers quickly.

## Integration Points

### Server Integration

**Location**: `crates/adapteros-server-api/src/handlers.rs`

The `/v1/infer` endpoint validates requests before forwarding:

```rust
let enforcement_result = state
    .policy_manager
    .enforce_policy(&policy_operation)
    .await?;

if !enforcement_result.allowed {
    return Err((StatusCode::FORBIDDEN, ...));
}
```

### Worker Integration

**Location**: `crates/adapteros-lora-worker/src/lib.rs`

Pre-inference validation in `infer_internal()`:
- Validates operation before starting inference
- Blocks requests with policy violations
- Logs all violations to telemetry

Post-inference validation:
- Validates outputs against output policy pack
- Checks for trace requirements, evidence citations
- Blocks outputs that violate policies

## Violation Handling

### Violation Structure

Each violation includes:
- `violation_id`: Unique identifier
- `policy_pack`: Which pack was violated
- `severity`: Info, Warning, Error, Critical, or Blocker
- `message`: Human-readable violation description
- `details`: Structured context data
- `remediation`: Actionable steps to resolve
- `timestamp`: When violation occurred

### Logging

All violations are logged to telemetry as security events:
- Policy pack name
- Violation ID
- Detailed message with remediation steps

### Error Responses

Blocked requests return structured error responses:
- HTTP 403 FORBIDDEN status
- Error code: `POLICY_VIOLATION`
- Details include all violations with remediation steps

## Configuration

### Manifest-Based Configuration

`PolicyEngine::new(policies)` automatically configures packs from manifest:

```rust
let engine = PolicyEngine::new(manifest.policies.clone());
// Pack manager is configured with evidence.require_open_book, etc.
```

### Pack-Level Configuration

Packs can be configured individually:

```rust
let mut config = PolicyPackConfig {
    id: PolicyPackId::Evidence,
    version: "1.0.0".to_string(),
    config: serde_json::json!({
        "require_open_book": true,
        "min_spans": 3,
    }),
    enabled: true,
    enforcement_level: EnforcementLevel::Error,
    last_updated: Utc::now(),
};

manager.update_pack_config(PolicyPackId::Evidence, config)?;
```

## Adding New Policy Packs

1. Add `PolicyPackId` variant
2. Implement `PolicyPackValidator` trait
3. Register in `PolicyPackManager::initialize_policy_packs()`
4. Add default configuration in `get_default_config()`
5. Update manifest mapping in `configure_from_manifest()` if needed

## Testing

### Unit Tests

Test individual policy pack validators in isolation.

### Integration Tests

Test full enforcement flow:
- `tests/policy_enforcement_integration.rs`: Basic enforcement tests
- `tests/policy_enforcement_comprehensive.rs`: Comprehensive tests including:
  - Enforcement level behavior
  - Short-circuiting optimization
  - Concurrent validation
  - Error message quality

## Troubleshooting

### Common Issues

**Policy violations blocking valid requests**:
- Check enforcement levels: may be set too strict
- Review violation details for false positives
- Verify pack configurations match expectations

**Missing violations**:
- Check if pack is enabled
- Verify enforcement level allows violations
- Review validator implementation

**Performance issues**:
- Ensure short-circuiting is working (check logs)
- Consider disabling non-critical packs
- Profile validation duration

## References

- `crates/adapteros-policy/src/policy_packs.rs`: Policy pack implementations
- `crates/adapteros-policy/src/unified_enforcement.rs`: Unified enforcement interface
- `crates/adapteros-policy/src/lib.rs`: PolicyEngine implementation
- `docs/POLICIES.md`: Policy pack definitions

MLNavigator Inc 2025-01-20.

