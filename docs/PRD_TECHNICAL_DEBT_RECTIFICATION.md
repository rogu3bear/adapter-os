# PRD: Technical Debt Rectification

**Document ID:** PRD-TECH-DEBT-001
**Version:** 1.0
**Date:** 2025-11-21
**Author:** James KC Auchterlonie
**Status:** Draft

---

## Executive Summary

This PRD documents 42 remaining TODO/FIXME items identified during comprehensive codebase rectification. These items represent technical debt that should be addressed to achieve production readiness. Items are prioritized by security impact, user-facing functionality, and maintenance burden.

### Summary Statistics

| Priority | Count | Impact |
|----------|-------|--------|
| **P0 (Critical)** | 4 | Security, type safety, API contracts |
| **P1 (High)** | 7 | Core functionality, performance |
| **P2 (Medium)** | 19 | Schema gaps, integrations |
| **P3 (Low)** | 12 | Cleanup, documentation |
| **Total** | 42 | |

---

## P0: Critical Priority

### 1. Error Code Mapping System

**Crate:** `adapteros-cli`
**File:** `src/error_codes.rs:522`
**Issue:** `#[allow(dead_code)]` - Error mapping not implemented

**Current State:**
```rust
#[allow(dead_code)] // TODO: Implement error mapping in future iteration
```

**Requirements:**
- Define `ErrorCode` enum with numeric codes for all `AosError` variants
- Implement `From<AosError>` for `ErrorCode`
- Add exit code mapping for CLI
- Document error codes in user-facing documentation

**Acceptance Criteria:**
- [ ] All CLI commands return appropriate exit codes
- [ ] Error codes documented in `docs/ERROR_CODES.md`
- [ ] `#[allow(dead_code)]` removed

**Effort:** 2-3 days

---

### 2. TelemetryEvent Consolidation

**Crate:** `adapteros-types`
**File:** `src/telemetry/mod.rs:8-9`
**Issue:** TelemetryEvent duplicated across 4 locations

**Current State:**
```rust
// TODO: Consolidate TelemetryEvent from 4 locations
// TODO: Add telemetry bundle types
```

**Requirements:**
- Create canonical `TelemetryEvent` in `adapteros-types`
- Update imports in: `adapteros-telemetry`, `adapteros-server-api`, `adapteros-orchestrator`, `adapteros-lora-worker`
- Add `TelemetryBundle` type with proper schema

**Acceptance Criteria:**
- [ ] Single source of truth for TelemetryEvent
- [ ] All crates import from `adapteros-types`
- [ ] No duplicate struct definitions

**Effort:** 1-2 days

---

### 3. API Type Serialization

**Crate:** `adapteros-api`
**File:** `src/types.rs:3`
**Issue:** Serialization disabled

**Current State:**
```rust
// use serde::{Deserialize, Serialize}; // TODO: Implement serialization in future iteration
```

**Requirements:**
- Enable serde derives on all API types
- Add `#[serde(rename_all = "camelCase")]` where appropriate
- Implement custom serializers for complex types (B3Hash, SystemTime)

**Acceptance Criteria:**
- [ ] All API types implement Serialize/Deserialize
- [ ] JSON round-trip tests pass
- [ ] OpenAPI schema generation works

**Effort:** 1 day

---

### 4. Device Fingerprinting

**Crate:** `adapteros-verify`
**File:** `src/keys.rs:9`
**Issue:** Hardware binding not implemented

**Current State:**
```rust
#[allow(dead_code)] // TODO: Implement device fingerprinting in future iteration
```

**Requirements:**
- Implement hardware fingerprinting (CPU ID, GPU UUID, MAC address)
- Create deterministic device ID from hardware attributes
- Support adapter binding to specific hardware

**Acceptance Criteria:**
- [ ] Device fingerprint reproducible across restarts
- [ ] Adapters can be bound to specific devices
- [ ] Fingerprint validation in adapter loading

**Effort:** 3-4 days

---

## P1: High Priority

### 5. Patch Generator AST Implementation

**Crate:** `adapteros-lora-worker`
**File:** `src/patch_generator.rs:450, 466, 469`
**Issue:** Generates placeholder TODO comments instead of actual code

**Current State:**
- Searches for TODO lines in diffs
- No actual code transformation

**Requirements:**
- Integrate with tree-sitter for AST parsing
- Implement syntax-aware transformations
- Support Rust, Python, TypeScript

**Acceptance Criteria:**
- [ ] Patches contain valid code, not TODOs
- [ ] Syntax validation before patch application
- [ ] Support for at least 3 languages

**Effort:** 5-7 days

---

### 6. MLX Embedding GPU Acceleration

**Crate:** `adapteros-lora-mlx-ffi`
**File:** `src/embedding.rs:426`
**Issue:** CPU-only embeddings (10-100x slower)

**Current State:**
```rust
// Simple forward pass (CPU-based for now, MLX acceleration TODO)
```

**Requirements:**
- Migrate to MLX embedding kernel calls
- Use GPU for embedding computation
- Batch embedding support

**Acceptance Criteria:**
- [ ] Embeddings computed on GPU
- [ ] 10x+ performance improvement
- [ ] Benchmark comparison documented

**Effort:** 3-4 days

---

### 7. CLI Telemetry Command

**Crate:** `adapteros-cli`
**File:** `src/commands/mod.rs:53`
**Issue:** Telemetry show command not implemented

**Current State:**
```rust
// pub mod telemetry_show; // TODO: Implement telemetry_show command
```

**Requirements:**
- Implement `aosctl telemetry show` command
- Query and filter telemetry events
- Support JSON and table output formats

**Acceptance Criteria:**
- [ ] Filter by time range, event type, level
- [ ] Pagination support
- [ ] Output format selection

**Effort:** 2-3 days

---

### 8. Model Runtime MLX Loading

**Crate:** `adapteros-server-api`
**File:** `src/model_runtime.rs:625`
**Issue:** MLX loading placeholder

**Current State:**
```rust
// Replace with actual MLX loading when feature is available
```

**Requirements:**
- Implement MLX model loading
- Weight format conversion if needed
- Memory management for loaded models

**Acceptance Criteria:**
- [ ] MLX models loadable via API
- [ ] Memory usage tracked accurately
- [ ] Unload works correctly

**Effort:** 3-4 days

---

### 9. ToSchema for Telemetry Types

**Crate:** `adapteros-telemetry-types`
**File:** `src/bundle.rs:15`
**Issue:** ToSchema derive disabled for OpenAPI

**Current State:**
```rust
// ToSchema derive disabled due to B3Hash/SystemTime not implementing ToSchema
```

**Requirements:**
- Implement custom ToSchema for B3Hash
- Implement custom ToSchema for SystemTime
- Or use wrapper types with derives

**Acceptance Criteria:**
- [ ] Telemetry bundle schema in OpenAPI docs
- [ ] All fields documented
- [ ] Schema validates correctly

**Effort:** 1-2 days

---

### 10. Circuit Breaker API Migration

**Crate:** `adapteros-telemetry`
**File:** `src/events/mod.rs:3`
**Issue:** Event builder API mismatch

**Current State:**
```rust
// TODO: Update circuit_breaker and schema_validation to use new TelemetryEventBuilder API
```

**Requirements:**
- Update circuit breaker to new builder pattern
- Update schema validation similarly
- Ensure event validation works

**Acceptance Criteria:**
- [ ] All event creation uses TelemetryEventBuilder
- [ ] Validation passes for all event types
- [ ] No deprecated API usage

**Effort:** 1 day

---

### 11. Qwen INT4 MLX FFI

**Crate:** `adapteros-base-llm`
**File:** `src/qwen_int4_mlx.rs:333`
**Issue:** MLX FFI not ready for weight loading

**Current State:**
```rust
// Call mlx_model_load_from_weights when FFI is ready
```

**Requirements:**
- Implement `mlx_model_load_from_weights()` FFI function
- Handle weight buffer format
- Memory management for loaded weights

**Acceptance Criteria:**
- [ ] Qwen INT4 loads from weight buffer
- [ ] No redundant dequantization
- [ ] Memory usage optimized

**Effort:** 3-4 days

---

## P2: Medium Priority

### 12. Git Repository Schema - Last Scan Field

**Crate:** `adapteros-git`
**File:** `src/subsystem.rs:409`
**Issue:** Cannot track last successful scan

**Requirements:**
- Add migration: `ALTER TABLE git_repositories ADD COLUMN last_scan DATETIME`
- Update `GitRepository` struct
- Set on successful scan completion

**Effort:** 1 day

---

### 13. Branch State Persistence

**Crate:** `adapteros-git`
**File:** `src/branch_manager.rs:62`
**Issue:** Branch state ephemeral across restarts

**Requirements:**
- Implement `save_session()` and `load_session()` methods
- Persist to database or file
- Handle migration/versioning

**Effort:** 2 days

---

### 14. Metal Memory Pool Management

**Crate:** `adapteros-lora-kernel-mtl`
**File:** `src/metal3x.rs:239, 244, 365`
**Issue:** Metal kernel memory not pooled

**Requirements:**
- Implement ring buffer memory pool
- Reuse allocations across kernel launches
- Track pool statistics

**Effort:** 3-4 days

---

### 15. Kernel Counter Sampling

**Crate:** `adapteros-lora-kernel-prof`
**File:** `src/lib.rs:117`
**Issue:** Performance profiling synthetic

**Requirements:**
- Wire up to system perf counters
- Use PAPI or macOS PMC
- Real timing measurements

**Effort:** 3-4 days

---

### 16. Training Service Parameters

**Crate:** `adapteros-orchestrator`
**File:** `src/training.rs:159-160`
**Issue:** Missing db and storage_root parameters

**Requirements:**
- Pass `Arc<Db>` to TrainingService
- Pass `PathBuf` for storage_root
- Use in job execution

**Effort:** 1 day

---

### 17. Telemetry Signature Metadata

**Crate:** `adapteros-orchestrator`
**File:** `src/gates/telemetry.rs:72`
**Issue:** Signature metadata not validated

**Requirements:**
- Load signature metadata from database
- Verify signatures on telemetry events
- Handle invalid signatures

**Effort:** 1-2 days

---

### 18-31. Additional Medium Priority Items

| # | Crate | File | Issue | Effort |
|---|-------|------|-------|--------|
| 18 | adapteros-server-api | handlers/git_repository.rs:650 | Author extraction from commits | 1d |
| 19 | adapteros-server-api | handlers/models.rs:459 | Memory tracking (DONE) | - |
| 20 | adapteros-config | guards.rs:198 | Feature flags (DONE) | - |
| 21 | adapteros-cli | cli_telemetry.rs:51 | Event emission (DONE) | - |
| 22 | adapteros-server-api | routes.rs:75, 676 | Federation routes (DONE) | - |
| 23 | adapteros-lora-worker | lib.rs:439 | Lifecycle threading (DONE) | - |
| 24 | adapteros-lora-worker | signal.rs:400 | Multi-signal handler (DONE) | - |
| 25 | adapteros-server | domain_adapters.rs | Load/unload handlers (DONE) | - |

---

## P3: Low Priority

### 26-42. Cleanup and Documentation Items

| # | Crate | Issue | Effort |
|---|-------|-------|--------|
| 26 | adapteros-patch | PatchEngine placeholder (DONE) | - |
| 27 | adapteros-system-metrics | Metrics schema (DONE) | - |
| 28 | adapteros-server-api | Service auth (DONE) | - |
| 29 | adapteros-server-api | Promotion signing (DONE) | - |
| 30 | adapteros-node | setuid/setgid (DONE) | - |
| 31 | adapteros-server-api | Path validation (DONE) | - |
| 32-42 | Various | Documentation, tests, cleanup | 1-2d each |

---

## Implementation Phases

### Phase 1: Error Handling & Type System (Week 1-2)
- P0 items 1-4
- Foundation for subsequent phases
- Enables better error reporting and debugging

### Phase 2: Core Functionality (Week 3-4)
- P1 items 5-11
- Performance improvements
- Feature completeness

### Phase 3: Integration & Polish (Week 5-6)
- P2 items 12-31
- Schema completeness
- Integration testing

### Phase 4: Cleanup (Week 7)
- P3 items 32-42
- Remove all `#[allow(dead_code)]`
- Documentation updates

---

## Success Metrics

### Code Quality
- [ ] Zero `#[allow(dead_code)]` in production code
- [ ] Zero hardcoded placeholder values
- [ ] All TODOs resolved or converted to GitHub issues

### Test Coverage
- [ ] Unit tests for all new implementations
- [ ] Integration tests for cross-crate functionality
- [ ] Benchmark tests for performance-critical paths

### Documentation
- [ ] Error codes documented
- [ ] API types in OpenAPI spec
- [ ] Architecture diagrams updated

---

## Risks and Mitigations

| Risk | Impact | Mitigation |
|------|--------|------------|
| MLX FFI instability | High | Feature flag, fallback to CPU |
| Breaking API changes | Medium | Versioned endpoints, deprecation notices |
| Performance regression | Medium | Benchmark suite, CI performance gates |

---

## Dependencies

- `tree-sitter` for AST parsing (P1.5)
- `sysinfo` crate for device fingerprinting (P0.4)
- MLX framework updates for GPU embedding (P1.6)

---

## Appendix A: Crate Impact Analysis

| Crate | Items | Priority Mix |
|-------|-------|--------------|
| adapteros-cli | 3 | P0, P1, P2 |
| adapteros-server-api | 8 | P1, P2 (mostly DONE) |
| adapteros-lora-worker | 4 | P1, P2 (mostly DONE) |
| adapteros-git | 2 | P2 |
| adapteros-telemetry | 2 | P0, P1 |
| adapteros-types | 1 | P0 |
| Others | 22 | Mixed |

---

## Appendix B: Related Documents

- [CLAUDE.md](../CLAUDE.md) - Developer guide
- [docs/ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - Architecture reference
- [docs/DETERMINISTIC_EXECUTION.md](DETERMINISTIC_EXECUTION.md) - Determinism requirements

---

**Document History:**

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| 1.0 | 2025-11-21 | JKC Auchterlonie | Initial draft from rectification analysis |
