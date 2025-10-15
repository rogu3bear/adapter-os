# Adapter Command Patch Plan

## Overview

This document outlines a comprehensive patch plan for the `adapter.rs` command implementation in `adapteros-cli`, following AdapterOS best practices and codebase standards.

## Current State Analysis

### Issues Identified

1. **Hardcoded socket path** - Uses `./var/run/aos/default/worker.sock` instead of configurable path
2. **Mock data fallback** - Inconsistent mock data presentation when worker unavailable
3. **Error handling** - Uses `anyhow::Result` instead of `adapteros_core::Result`
4. **Missing telemetry** - No CLI telemetry integration
5. **Inconsistent output** - Mixed use of `println!` instead of `OutputWriter`
6. **Missing validation** - No adapter ID validation or sanitization
7. **No JSON support** - Missing JSON output format support
8. **Missing error codes** - No structured error codes for adapter operations

## Patch Plan

### Phase 1: Core Infrastructure Improvements

#### 1.1 Socket Path Configuration
**File**: `crates/adapteros-cli/src/commands/adapter.rs`
**Lines**: 181, 290, 349, 374, 399, 424

**Issue**: Hardcoded socket path violates configuration precedence rules
**Reference**: [CLAUDE.md - Configuration Precedence](CLAUDE.md)

**Patch**:
```rust
// Replace hardcoded paths with configurable approach
let socket_path = get_worker_socket_path(tenant_id)?;
```

**Implementation**:
- Add `get_worker_socket_path()` function following config precedence
- Support `--tenant` parameter for multi-tenant operations
- Add socket path validation and error handling

#### 1.2 Error Handling Standardization
**File**: `crates/adapteros-cli/src/commands/adapter.rs`
**Lines**: 3, 45, 80, 115, 162

**Issue**: Uses `anyhow::Result` instead of `adapteros_core::Result`
**Reference**: [crates/adapteros-cli/src/main.rs L1016-L1068](crates/adapteros-cli/src/main.rs)

**Patch**:
```rust
// Replace anyhow::Result with adapteros_core::Result
use adapteros_core::Result;
```

**Implementation**:
- Update all function signatures to use `adapteros_core::Result`
- Add proper error context using `adapteros_core::AosError`
- Implement structured error codes for adapter operations

#### 1.3 Output Writer Integration
**File**: `crates/adapteros-cli/src/commands/adapter.rs`
**Lines**: 178, 286, 347, 372, 397, 422

**Issue**: Direct `println!` usage instead of `OutputWriter`
**Reference**: [crates/adapteros-cli/src/output.rs L59-L136](crates/adapteros-cli/src/output.rs)

**Patch**:
```rust
// Add OutputWriter parameter to all command functions
pub async fn handle_adapter_command(cmd: AdapterCommand, output: &OutputWriter) -> Result<()> {
    // Use output.success(), output.error(), etc.
}
```

**Implementation**:
- Add `OutputWriter` parameter to all adapter command functions
- Replace `println!` with appropriate `OutputWriter` methods
- Support JSON output format
- Add quiet mode support

### Phase 2: Command Structure Improvements

#### 2.1 Command Enum Enhancement
**File**: `crates/adapteros-cli/src/commands/adapter.rs`
**Lines**: 143-157

**Issue**: Missing command metadata and validation
**Reference**: [crates/adapteros-cli/src/main.rs L51-L1013](crates/adapteros-cli/src/main.rs)

**Patch**:
```rust
#[derive(Debug, Subcommand)]
pub enum AdapterCommand {
    /// List all adapters with their states
    #[command(after_help = "Examples:\n  aosctl adapter list\n  aosctl adapter list --json")]
    List {
        /// Output format
        #[arg(long)]
        json: bool,
        
        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,
    },
    
    /// Show detailed metrics for an adapter
    #[command(after_help = "Examples:\n  aosctl adapter profile adapter-1\n  aosctl adapter profile adapter-1 --json")]
    Profile { 
        /// Adapter ID
        adapter_id: String,
        
        /// Output format
        #[arg(long)]
        json: bool,
        
        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,
    },
    // ... other commands
}
```

**Implementation**:
- Add command help text with examples
- Add `--json` flag support
- Add `--tenant` parameter support
- Add adapter ID validation

#### 2.2 Input Validation
**File**: `crates/adapteros-cli/src/commands/adapter.rs`
**Lines**: 162-173

**Issue**: No input validation for adapter IDs
**Reference**: [crates/adapteros-cli/src/error_codes.rs L1-L86](crates/adapteros-cli/src/error_codes.rs)

**Patch**:
```rust
// Add validation function
fn validate_adapter_id(adapter_id: &str) -> Result<()> {
    if adapter_id.is_empty() {
        return Err(adapteros_core::AosError::InvalidInput("Adapter ID cannot be empty".to_string()));
    }
    
    if !adapter_id.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '_') {
        return Err(adapteros_core::AosError::InvalidInput(
            "Adapter ID must contain only alphanumeric characters, hyphens, and underscores".to_string()
        ));
    }
    
    Ok(())
}
```

**Implementation**:
- Add adapter ID format validation
- Add length limits (max 64 characters)
- Add sanitization for special characters
- Return structured error codes

### Phase 3: Telemetry Integration

#### 3.1 CLI Telemetry Integration
**File**: `crates/adapteros-cli/src/commands/adapter.rs`
**Lines**: 162-173

**Issue**: Missing CLI telemetry for adapter operations
**Reference**: [crates/adapteros-cli/src/main.rs L1030-L1067](crates/adapteros-cli/src/main.rs)

**Patch**:
```rust
// Add telemetry to command handler
pub async fn handle_adapter_command(cmd: AdapterCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_adapter_command_name(&cmd);
    let tenant_id = extract_tenant_from_adapter_command(&cmd);
    
    // Emit telemetry
    let _ = cli_telemetry::emit_cli_command(&command_name, tenant_id.as_deref(), true).await;
    
    match cmd {
        // ... command handling
    }
}
```

**Implementation**:
- Add telemetry emission for all adapter commands
- Track command success/failure rates
- Include tenant context in telemetry
- Add performance metrics

#### 3.2 Error Code Integration
**File**: `crates/adapteros-cli/src/commands/adapter.rs`
**Lines**: 45-70, 80-106, 115-141

**Issue**: No structured error codes for adapter operations
**Reference**: [crates/adapteros-cli/src/error_codes.rs L69-L86](crates/adapteros-cli/src/error_codes.rs)

**Patch**:
```rust
// Add error codes for adapter operations
error_code!(
    "E6001",
    "adapters",
    "Adapter Socket Connection Failed",
    "Cannot connect to worker socket for adapter operations",
    "Check if worker is running:\n  aosctl serve status\n\nStart worker if needed:\n  aosctl serve start"
);

error_code!(
    "E6002", 
    "adapters",
    "Adapter Not Found",
    "Specified adapter ID does not exist in the registry",
    "List available adapters:\n  aosctl adapter list\n\nRegister adapter if needed:\n  aosctl register-adapter <id> <hash>"
);
```

**Implementation**:
- Add E6xxx error codes for adapter operations
- Include helpful fix suggestions
- Map AosError variants to error codes
- Add error code documentation

### Phase 4: API Integration Improvements

#### 4.1 Client Integration Enhancement
**File**: `crates/adapteros-cli/src/commands/adapter.rs`
**Lines**: 45-70, 80-106, 115-141

**Issue**: Basic UDS client usage without proper error handling
**Reference**: [crates/adapteros-client/src/uds.rs L114-L173](crates/adapteros-client/src/uds.rs)

**Patch**:
```rust
// Enhanced client integration with proper error handling
async fn connect_and_fetch_adapter_states(
    socket_path: &std::path::Path,
    timeout: Duration,
) -> Result<Vec<AdapterState>> {
    use adapteros_client::UdsClient;
    
    let client = UdsClient::new(timeout);
    
    // Add retry logic for transient failures
    let mut retries = 3;
    while retries > 0 {
        match client.list_adapters(socket_path).await {
            Ok(json_response) => {
                let adapters: Vec<AdapterState> = serde_json::from_str(&json_response)
                    .map_err(|e| adapteros_core::AosError::SerializationError(format!("Failed to parse adapter response: {}", e)))?;
                
                return Ok(adapters);
            }
            Err(e) if retries > 1 => {
                retries -= 1;
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            Err(e) => {
                return Err(adapteros_core::AosError::NetworkError(format!("Failed to list adapters: {}", e)));
            }
        }
    }
    
    unreachable!()
}
```

**Implementation**:
- Add retry logic for transient failures
- Implement proper timeout handling
- Add connection pooling support
- Improve error messages with context

#### 4.2 Adapter State Enhancement
**File**: `crates/adapteros-cli/src/commands/adapter.rs`
**Lines**: 8-35

**Issue**: Basic adapter state structure missing important fields
**Reference**: [crates/adapteros-lora-worker/src/adapter_hotswap.rs L43-L51](crates/adapteros-lora-worker/src/adapter_hotswap.rs)

**Patch**:
```rust
/// Enhanced adapter state structure for UDS communication
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdapterState {
    pub id: String,
    pub hash: String, // B3Hash as string
    pub vram_mb: u64,
    pub active: bool,
    pub tier: String, // persistent, ephemeral, etc.
    pub rank: u32,
    pub activation_pct: f32,
    pub quality_delta: f32,
    pub last_activation: Option<u64>, // timestamp
    pub pinned: bool,
}

/// Enhanced adapter profile structure for UDS communication
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdapterProfile {
    pub state: String,
    pub activation_pct: f32,
    pub activations: u64,
    pub total_tokens: u64,
    pub avg_latency_us: f32,
    pub memory_kb: u64,
    pub quality_delta: f32,
    pub recent_activations: Vec<ActivationWindow>,
    pub performance_metrics: PerformanceMetrics,
    pub policy_compliance: PolicyCompliance,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PerformanceMetrics {
    pub p50_latency_us: f32,
    pub p95_latency_us: f32,
    pub p99_latency_us: f32,
    pub throughput_tokens_per_sec: f32,
    pub error_rate: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyCompliance {
    pub determinism_score: f32,
    pub evidence_coverage: f32,
    pub refusal_rate: f32,
    pub policy_violations: u64,
}
```

**Implementation**:
- Add missing fields from worker adapter state
- Include performance metrics
- Add policy compliance tracking
- Support tier and rank information

### Phase 5: Output Format Improvements

#### 5.1 JSON Output Support
**File**: `crates/adapteros-cli/src/commands/adapter.rs`
**Lines**: 176-282, 284-344

**Issue**: No JSON output format support
**Reference**: [crates/adapteros-cli/src/output.rs L59-L136](crates/adapteros-cli/src/output.rs)

**Patch**:
```rust
// Add JSON output support
async fn list_adapters(json: bool, tenant: Option<String>, output: &OutputWriter) -> Result<()> {
    let socket_path = get_worker_socket_path(tenant.as_deref())?;
    
    match connect_and_fetch_adapter_states(&socket_path, Duration::from_secs(5)).await {
        Ok(adapters) => {
            if json {
                output.result(&serde_json::to_string_pretty(&adapters)?);
            } else {
                // Table format
                let mut table = Table::new();
                // ... table setup
                output.result(&format!("{table}"));
            }
        }
        Err(e) => {
            if json {
                let error_response = serde_json::json!({
                    "error": format!("{}", e),
                    "adapters": []
                });
                output.result(&serde_json::to_string_pretty(&error_response)?);
            } else {
                output.error(&format!("Failed to connect to worker: {}", e));
            }
        }
    }
    
    Ok(())
}
```

**Implementation**:
- Add JSON output for all adapter commands
- Include error information in JSON responses
- Support pretty-printed JSON
- Add schema validation

#### 5.2 Table Format Enhancement
**File**: `crates/adapteros-cli/src/commands/adapter.rs`
**Lines**: 220-244

**Issue**: Basic table format missing important information
**Reference**: [crates/adapteros-cli/src/commands/adapter.rs L188-213](crates/adapteros-cli/src/commands/adapter.rs)

**Patch**:
```rust
// Enhanced table format with more information
let mut table = Table::new();
table
    .load_preset(UTF8_FULL)
    .apply_modifier(UTF8_ROUND_CORNERS)
    .set_header(vec![
        "ID",
        "Hash",
        "Tier",
        "Rank",
        "State",
        "Activation %",
        "Quality Δ",
        "Memory",
        "Pinned",
        "Last Active",
    ]);

for adapter in adapters {
    let state = if adapter.active { "active" } else { "staged" };
    let pinned = if adapter.pinned { "yes" } else { "no" };
    let last_active = adapter.last_activation
        .map(|ts| format!("{}s ago", (std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() - ts)))
        .unwrap_or_else(|| "never".to_string());
    
    table.add_row(vec![
        &adapter.id,
        &adapter.hash[..8], // Short hash
        &adapter.tier,
        &adapter.rank.to_string(),
        state,
        &format!("{:.1}%", adapter.activation_pct),
        &format!("{:.2}", adapter.quality_delta),
        &format!("{} MB", adapter.vram_mb),
        pinned,
        &last_active,
    ]);
}
```

**Implementation**:
- Add more columns with useful information
- Include hash, tier, rank information
- Add last activation time
- Improve formatting and alignment

### Phase 6: Testing and Validation

#### 6.1 Unit Tests
**File**: `crates/adapteros-cli/src/commands/adapter.rs`
**Lines**: End of file

**Issue**: No unit tests for adapter commands
**Reference**: [tests/ directory](tests/)

**Patch**:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use adapteros_core::Result;
    
    #[test]
    fn test_validate_adapter_id() {
        assert!(validate_adapter_id("valid-adapter-1").is_ok());
        assert!(validate_adapter_id("adapter_2").is_ok());
        assert!(validate_adapter_id("").is_err());
        assert!(validate_adapter_id("invalid@adapter").is_err());
    }
    
    #[test]
    fn test_get_adapter_command_name() {
        assert_eq!(get_adapter_command_name(&AdapterCommand::List { json: false, tenant: None }), "adapter_list");
        assert_eq!(get_adapter_command_name(&AdapterCommand::Profile { adapter_id: "test".to_string(), json: false, tenant: None }), "adapter_profile");
    }
    
    #[tokio::test]
    async fn test_list_adapters_mock() {
        // Test mock data fallback
        let output = OutputWriter::new(OutputMode::Normal, false);
        let result = list_adapters(false, None, &output).await;
        assert!(result.is_ok());
    }
}
```

**Implementation**:
- Add unit tests for validation functions
- Add tests for command name extraction
- Add tests for mock data fallback
- Add integration tests for UDS communication

#### 6.2 Integration Tests
**File**: `tests/adapter_commands.rs`

**Issue**: No integration tests for adapter commands
**Reference**: [tests/ directory](tests/)

**Patch**:
```rust
// Integration tests for adapter commands
#[tokio::test]
async fn test_adapter_list_integration() {
    // Test with mock worker
    // Test error handling
    // Test JSON output
}

#[tokio::test]
async fn test_adapter_profile_integration() {
    // Test profile retrieval
    // Test error handling for non-existent adapter
    // Test JSON output
}
```

**Implementation**:
- Add integration tests for all adapter commands
- Test error scenarios
- Test JSON output format
- Test multi-tenant scenarios

## Implementation Order

1. **Phase 1**: Core infrastructure (socket path, error handling, output writer)
2. **Phase 2**: Command structure (enum enhancement, validation)
3. **Phase 3**: Telemetry integration (CLI telemetry, error codes)
4. **Phase 4**: API integration (client enhancement, state structures)
5. **Phase 5**: Output format (JSON support, table enhancement)
6. **Phase 6**: Testing and validation (unit tests, integration tests)

## Verification Checklist

- [ ] All functions use `adapteros_core::Result` instead of `anyhow::Result`
- [ ] All output uses `OutputWriter` instead of direct `println!`
- [ ] Socket paths are configurable and follow precedence rules
- [ ] Adapter IDs are validated and sanitized
- [ ] JSON output format is supported for all commands
- [ ] CLI telemetry is integrated for all operations
- [ ] Structured error codes are implemented
- [ ] Unit tests cover all validation functions
- [ ] Integration tests cover all command scenarios
- [ ] Documentation is updated with examples

## References

- [CLAUDE.md - Configuration Precedence](CLAUDE.md)
- [crates/adapteros-cli/src/main.rs L1016-L1068](crates/adapteros-cli/src/main.rs)
- [crates/adapteros-cli/src/output.rs L59-L136](crates/adapteros-cli/src/output.rs)
- [crates/adapteros-cli/src/error_codes.rs L1-L86](crates/adapteros-cli/src/error_codes.rs)
- [crates/adapteros-client/src/uds.rs L114-L173](crates/adapteros-client/src/uds.rs)
- [crates/adapteros-lora-worker/src/adapter_hotswap.rs L43-L51](crates/adapteros-lora-worker/src/adapter_hotswap.rs)
- [tests/ directory](tests/)
