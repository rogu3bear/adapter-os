# AdapterOS Agent Guide

This document provides guidance for AI agents working on AdapterOS, including current state, capabilities, and operating procedures.

## Project Overview

AdapterOS is a directory attention layer for offline specialized AI - a Rust-based ML inference runtime optimized for Apple Silicon. It functions as a semantic attention mechanism that focuses AI capabilities on specific directories and codebases through a five-tier adapter hierarchy.

**Key Characteristics:**
- Directory-aware AI that understands any codebase structure
- High-security inference with zero network egress during serving
- Deterministic execution with reproducible outputs
- Multi-tenant isolation with per-tenant process boundaries
- Evidence-grounded responses with RAG integration
- 20 policy packs enforcing compliance, security, and quality
- K-sparse LoRA routing with Metal-optimized kernels

## Current State

### What's Working
- **Metal Backend**: Primary production backend with precompiled kernels
- **Domain Adapters**: Vision and telemetry adapters implemented
- **Policy Engine**: 20 policy packs enforced
- **Telemetry System**: Canonical JSON event logging with Merkle trees
- **Database Layer**: SQLite with migrations
- **CLI Tool**: `aosctl` with comprehensive commands
- **Server API**: REST API with authentication
- **Deterministic Execution**: HKDF-seeded RNG, precompiled kernels

### What Needs Implementation
- **MLX Backend**: Currently disabled due to PyO3 linker issues
- **CoreML Backend**: Marked as "not yet implemented"
- **Verification Framework**: TODO stubs need actual implementations
- **Testing Framework**: TODO stubs need actual implementations
- **CLI Output Writer**: Missing `table()` method and formatting helpers
- **MLX FFI**: Placeholder implementations need real functionality
- **Domain Adapter API**: Placeholder execution logic needs implementation
- **Noise Tracker**: Placeholder Metal buffer reading needs implementation

## Agent Capabilities

### What Agents Can Do
1. **Implement Missing Functionality**: Replace TODO stubs and placeholder implementations
2. **Create Focused PRs**: Keep changes under 500 lines per PR
3. **Maintain Code Quality**: Follow existing patterns and error handling
4. **Add Tests**: Comprehensive test coverage for new functionality
5. **Integrate with Existing Systems**: Use existing telemetry, error handling, and patterns

### What Agents Should NOT Do
1. **Duplicate Existing Functionality**: Check for existing implementations first
2. **Create Large PRs**: Keep changes focused and manageable
3. **Break Existing Patterns**: Follow established code style and architecture
4. **Ignore Policy Requirements**: Ensure compliance with 20 policy packs
5. **Modify Core Architecture**: Work within existing framework

## Operating Procedures

### Before Starting Work
1. **Search for Existing Implementations**: Use `codebase_search` and `grep` to check for duplicates
2. **Understand Current State**: Read existing code to understand patterns
3. **Check Policy Requirements**: Ensure compliance with relevant policy packs
4. **Plan Integration Points**: Identify how changes will integrate with existing systems

### During Implementation
1. **Follow Existing Patterns**: Use established error handling, logging, and structure
2. **Maintain Determinism**: Ensure reproducible outputs where applicable
3. **Add Comprehensive Tests**: Test new functionality thoroughly
4. **Use Existing Types**: Leverage `adapteros_core::Result`, `B3Hash`, etc.
5. **Integrate with Telemetry**: Add appropriate logging and metrics

### After Implementation
1. **Verify Changes**: Re-read files to confirm changes were applied
2. **Check Compilation**: Ensure code compiles without errors
3. **Run Tests**: Verify all tests pass
4. **Check for Duplicates**: Ensure no duplicate functionality was created
5. **Update Documentation**: Add or update relevant documentation

## Key Integration Points

### Error Handling
- Use `adapteros_core::Result` for all functions
- Use `AosError` variants for specific error types
- Follow existing error propagation patterns

### Logging and Telemetry
- Use `tracing` for logging (not `println!`)
- Integrate with `TelemetryWriter` for structured events
- Follow canonical JSON format for telemetry

### Policy Compliance
- Ensure deterministic execution where required
- Follow egress rules (no network during serving)
- Maintain isolation requirements
- Use proper signing and SBOM for artifacts

### Backend Integration
- Implement `FusedKernels` trait for new backends
- Use `BackendChoice` enum for backend selection
- Follow existing backend patterns
- Ensure determinism attestation

## File Locations

### Core Crates
- `crates/adapteros-core/`: Core types and error handling
- `crates/adapteros-lora-worker/`: Main worker implementation
- `crates/adapteros-lora-kernel-mtl/`: Metal kernel implementation
- `crates/adapteros-lora-kernel-api/`: Kernel API traits
- `crates/adapteros-policy/`: Policy engine implementation
- `crates/adapteros-telemetry/`: Telemetry system
- `crates/adapteros-db/`: Database layer

### API and CLI
- `crates/adapteros-server-api/`: REST API handlers
- `crates/adapteros-cli/`: Command-line tool
- `crates/adapteros-client/`: Client library

### Domain Adapters
- `crates/adapteros-domain/`: Domain adapter implementations
- `crates/adapteros-lora-worker/src/vision_adapter.rs`: Vision adapter
- `crates/adapteros-lora-worker/src/telemetry_adapter.rs`: Telemetry adapter

### Verification and Testing
- `crates/adapteros-verification/`: Verification framework
- `crates/adapteros-testing/`: Testing framework

## Success Criteria

### For Each Implementation
- [ ] Code compiles without errors
- [ ] All tests pass
- [ ] No duplicate functionality
- [ ] Follows existing patterns
- [ ] Integrates with existing systems
- [ ] Maintains policy compliance
- [ ] Adds appropriate telemetry
- [ ] Includes comprehensive tests

### For PRs
- [ ] Focused on single responsibility
- [ ] Clear integration points
- [ ] No breaking changes
- [ ] Maintains backward compatibility
- [ ] Follows existing code style
- [ ] Includes appropriate documentation

## Common Patterns

### Error Handling
```rust
use adapteros_core::{AosError, Result};

pub fn example_function() -> Result<()> {
    // Implementation
    Ok(())
}
```

### Logging
```rust
use tracing::{info, warn, error, debug};

info!("Operation completed successfully");
warn!("Non-critical issue occurred");
error!("Critical error: {}", error_message);
debug!("Debug information: {:?}", data);
```

### Telemetry
```rust
use adapteros_telemetry::TelemetryWriter;

let telemetry = TelemetryWriter::global();
telemetry.log("operation.completed", serde_json::json!({
    "operation": "example",
    "timestamp": chrono::Utc::now(),
}))?;
```

### Backend Implementation
```rust
use adapteros_lora_kernel_api::FusedKernels;

pub struct ExampleBackend {
    // Implementation
}

impl FusedKernels for ExampleBackend {
    // Implement required methods
}
```

## Resources

### Documentation
- `CLAUDE.md`: Project overview and architecture
- `docs/`: Comprehensive documentation
- `README.md`: Quick start guide

### Configuration
- `configs/cp.toml`: Server configuration
- `Cargo.toml`: Workspace dependencies
- `rust-toolchain.toml`: Rust toolchain version

### Tests
- `tests/`: Integration tests
- `golden_runs/`: Determinism tests
- `examples/`: Usage examples

This guide should help agents understand the project, current state, and how to operate effectively within the AdapterOS codebase.