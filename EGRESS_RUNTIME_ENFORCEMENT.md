# Runtime Egress Enforcement Implementation

## Overview

This document describes the implementation of runtime-aware egress enforcement in the AdapterOS policy system. The egress policy now supports different enforcement levels based on the runtime mode (dev, staging, prod), allowing violations to either block operations or issue warnings.

## Changes Made

### 1. Enhanced EgressConfig Structure

**File:** `crates/adapteros-policy/src/packs/egress.rs`

Added `enforcement_level` field to `EgressConfig`:

```rust
pub struct EgressConfig {
    // ... existing fields ...
    pub enforcement_level: EnforcementLevel,
}
```

### 2. New EnforcementLevel Enum

Three enforcement levels are now supported:

- **Warn**: Log violations only, don't block operations
- **Block**: Always block violations with errors
- **Auto**: Automatically determine based on runtime mode (default)

```rust
pub enum EnforcementLevel {
    Warn,
    Block,
    Auto,
}
```

### 3. RuntimeMode Enum

Defines three runtime modes with different security profiles:

- **Dev**: Development mode - relaxed enforcement (warns only)
- **Staging**: Staging mode - moderate enforcement (warns)
- **Prod**: Production mode - strict enforcement (blocks)

```rust
pub enum RuntimeMode {
    Dev,
    Staging,
    Prod,
}
```

### 4. Enhanced Policy Methods

New runtime-aware methods added to `EgressPolicy`:

- `should_block(runtime_mode)`: Determines if violations should block based on enforcement level and runtime mode
- `validate_no_network_sockets_with_mode(runtime_mode)`: Validates network sockets with runtime awareness
- `check_dns_policy_with_mode(domain, runtime_mode)`: Checks DNS policy with runtime awareness
- `check_network_egress(protocol, destination, runtime_mode)`: New method for checking network egress attempts

### 5. Updated Policy::enforce Implementation

The `enforce` method now:
- Extracts runtime mode from PolicyContext metadata
- Respects enforcement level configuration
- Returns warnings instead of violations when not blocking
- Provides detailed context in both violations and warnings

### 6. Enhanced EgressValidator

**File:** `crates/adapteros-policy/src/policy_packs.rs`

The `EgressValidator::validate` method now:
- Extracts runtime mode from request metadata
- Determines blocking behavior based on runtime mode
- Returns appropriate warnings or violations
- Includes runtime mode in violation/warning details

## Usage

### Setting Runtime Mode

Runtime mode can be set via PolicyContext metadata:

```rust
let mut metadata = HashMap::new();
metadata.insert("runtime_mode".to_string(), "prod".to_string());

// Use in policy enforcement
policy.enforce(&context);
```

### Setting Enforcement Level

```rust
let mut config = EgressConfig::default();
config.enforcement_level = EnforcementLevel::Block; // Always block
// or
config.enforcement_level = EnforcementLevel::Warn;  // Always warn
// or
config.enforcement_level = EnforcementLevel::Auto;  // Depends on runtime mode
```

### Examples

#### Development Mode (Auto Enforcement)
```rust
// In dev mode with Auto enforcement:
let policy = EgressPolicy::new(EgressConfig::default());
let result = policy.check_network_egress("tcp", "example.com:443", Some(RuntimeMode::Dev));
// Result: Ok(()) - warns but doesn't block
```

#### Production Mode (Auto Enforcement)
```rust
// In prod mode with Auto enforcement:
let policy = EgressPolicy::new(EgressConfig::default());
let result = policy.check_network_egress("tcp", "example.com:443", Some(RuntimeMode::Prod));
// Result: Err(AosError::PolicyViolation) - blocks the operation
```

#### Force Block Mode
```rust
// Always block regardless of runtime mode:
let mut config = EgressConfig::default();
config.enforcement_level = EnforcementLevel::Block;
let policy = EgressPolicy::new(config);

let result = policy.check_network_egress("tcp", "example.com:443", Some(RuntimeMode::Dev));
// Result: Err(AosError::PolicyViolation) - blocks even in dev mode
```

## Behavior Matrix

| Enforcement Level | Runtime Mode | Behavior |
|------------------|--------------|----------|
| Auto (default)   | Dev          | Warn     |
| Auto (default)   | Staging      | Warn     |
| Auto (default)   | Prod         | Block    |
| Warn             | Any          | Warn     |
| Block            | Any          | Block    |
| Auto             | None         | Warn     |

## Testing

Comprehensive tests have been added to verify the runtime enforcement behavior:

- `test_runtime_mode_enforcement`: Verifies RuntimeMode behavior
- `test_enforcement_level_warn`: Tests warn-only mode
- `test_enforcement_level_block`: Tests always-block mode
- `test_enforcement_level_auto`: Tests automatic mode selection
- `test_check_network_egress_with_runtime_mode`: Tests network egress checking
- `test_dns_policy_with_runtime_mode`: Tests DNS policy with runtime awareness

## Integration

### Server Integration

The server can resolve runtime mode and pass it to policy enforcement:

```rust
use adapteros_server_api::runtime_mode::{RuntimeMode, RuntimeModeResolver};

// Resolve runtime mode
let mode = RuntimeModeResolver::resolve(&config, &db).await?;

// Include in policy context metadata
let mut metadata = HashMap::new();
metadata.insert("runtime_mode".to_string(), mode.as_str().to_string());
```

### Policy Validation

When validating policies, include runtime mode in the request:

```rust
let request = PolicyRequest {
    request_id: "req_123".to_string(),
    request_type: RequestType::NetworkOperation,
    metadata: Some(serde_json::json!({
        "runtime_mode": "prod"
    })),
    // ... other fields ...
};

let result = egress_validator.validate(&request)?;
```

## Backward Compatibility

- Default enforcement level is `Auto`, which maintains existing behavior
- Existing code without runtime mode metadata defaults to `Warn` behavior
- All existing validation methods remain unchanged and available
- New `_with_mode` methods provide runtime-aware versions

## Security Considerations

1. **Production Safety**: In production mode (with Auto or Block enforcement), all egress violations are blocked by default
2. **Development Flexibility**: Development mode allows testing without hard blocks
3. **Explicit Control**: Block/Warn levels provide explicit control regardless of environment
4. **Audit Trail**: All violations are logged, whether blocked or warned
5. **Zero Trust**: UDS (Unix Domain Sockets) are always allowed, all other protocols require explicit permission

## Future Enhancements

Potential future improvements:
1. Per-protocol enforcement levels
2. Allowlist/denylist with runtime mode awareness
3. Integration with PF (Packet Filter) for kernel-level enforcement
4. Runtime mode change notifications
5. Policy pack-level enforcement configuration

## References

- `crates/adapteros-policy/src/packs/egress.rs`: Main egress policy implementation
- `crates/adapteros-policy/src/policy_packs.rs`: EgressValidator implementation
- `crates/adapteros-server-api/src/runtime_mode.rs`: Runtime mode resolution
- `CLAUDE.md`: AdapterOS developer guide and policy documentation
