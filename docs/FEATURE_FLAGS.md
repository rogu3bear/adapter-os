# Feature Flags Reference

Complete reference for AdapterOS cargo feature flags, platform compatibility, and build configurations.

**Last Updated**: 2025-11-21
**Audience**: Developers, CI/CD engineers, platform engineers

---

## Table of Contents

1. [Overview](#overview)
2. [Feature Flag Catalog](#feature-flag-catalog)
3. [Platform Compatibility Matrix](#platform-compatibility-matrix)
4. [Common Combinations](#common-combinations)
5. [Testing Feature Flags](#testing-feature-flags)
6. [CI/CD Integration](#cicd-integration)
7. [Adding New Features](#adding-new-features)

---

## Overview

AdapterOS uses Cargo feature flags to control:

- **Platform-specific backends** (Metal, MLX)
- **Observability capabilities** (telemetry, metrics, replay)
- **Build targets** (Linux vs. macOS)
- **Experimental features** (MLX backend, new kernel implementations)

### Default Configuration

**Default build (macOS targets)**:
```bash
cargo build --release  # enables deterministic-only + coreml-backend
```

**Enabled by default**: `deterministic-only`, `coreml-backend` (macOS)
**Disabled by default**: `metal-backend`, `multi-backend` / `mlx-backend`, `mlx`, telemetry, metrics, replay

**Linux/CI note**: Use CPU-only by disabling defaults:
```bash
cargo build --release --no-default-features --features deterministic-only
```

### Design Philosophy

1. **Safe defaults**: Default build has no platform dependencies (CPU-only)
2. **Explicit opt-in**: Platform-specific features require explicit flags
3. **Feature orthogonality**: Most features are independent and can be combined
4. **Platform awareness**: Build errors if platform-specific features used on incompatible platforms

---

## Feature Flag Catalog

### Core Capability Flags

#### `deterministic-only` (Default)

**Purpose**: Core deterministic execution engine without platform-specific backends.

**Includes**:
- Deterministic task scheduler
- HKDF-based seeding hierarchy
- Merkle chain tick ledger
- Core data structures

**Excludes**:
- Metal GPU acceleration
- MLX Python backend
- Observability (telemetry/metrics)

**Platform**: All (Linux, macOS, CI)

**Usage**:
```bash
# Explicitly enable (same as default)
cargo build --release --features deterministic-only

# Or just use default
cargo build --release
```

**When to use**:
- CPU-only environments
- Testing deterministic behavior without GPU
- CI builds on Linux runners

---

#### `coreml-backend`

**Purpose**: Enable CoreML + Neural Engine (ANE) acceleration for inference.

**Includes**:
- CoreML model loading and inference
- Neural Engine (ANE) optimization
- MLTensor API support (macOS 15+)
- Guaranteed deterministic execution

**Platform**: **macOS 13.0+ with Apple Silicon**

**Requirements**:
- macOS 13.0+ (macOS 15+ for MLTensor optimizations)
- Apple Silicon (M1/M2/M3/M4)
- Xcode Command Line Tools

**Usage** (enabled by default on macOS targets):
```bash
cargo build --release --features coreml-backend   # explicit opt-in
```

**Crate integration**:
- Workspace feature propagates to `adapteros-lora-worker`
- Enables `adapteros-lora-kernel-coreml` as dependency
- Activates ANE detection and CoreML model loading

**Status**: ✅ **Production-ready** (Primary backend for macOS)

**See Also**: [CoreML Integration](COREML_INTEGRATION.md)

---

#### `metal-backend`

**Purpose**: Enable Apple Metal GPU acceleration for inference.

**Includes**:
- Metal shader kernels (`.metallib` precompiled blobs)
- UMA memory optimization
- Fused attention + LoRA kernels
- Deterministic Metal execution

**Platform**: **macOS 13.0+ with Apple Silicon only**

**Requirements**:
- macOS 13.0+
- Apple Silicon (M1/M2/M3/M4)
- Xcode Command Line Tools

**Usage**:
```bash
# Enable Metal backend
cargo build --release --features metal-backend
```

**Compilation behavior**:
- ✅ Succeeds on macOS with Apple Silicon
- ❌ Fails on Linux (linker error: cannot find -lMetal)
- ❌ Fails on Intel macOS (no Metal support)

**Status**: ✅ **Production-ready** (Fallback backend for non-ANE systems)

**See Also**: [Metal Kernels Documentation](metal/PHASE4-METAL-KERNELS.md)

---

#### `mlx-backend`

**Purpose**: Enable the MLX FFI crate. Builds a stub unless paired with `mlx`.

**Includes**:
- MLX C++ FFI wrapper (stub by default)
- Deterministic stubs for CI/CPU-only environments

**Platform**: macOS (compiles; real runtime requires Apple Silicon + MLX)

**Usage**:
```bash
# Stub (no MLX runtime required)
cargo build --release --features mlx-backend

# Real MLX (requires MLX C++ installed)
cargo build --release --features "mlx-backend,mlx"
```

**Status**: ⚠️ Stub by default; real only when `mlx` is also enabled.

**Recommendation**: Pair with `mlx` only on macOS when you need MLX; keep stub in CI.

---

#### `mlx`

**Purpose**: Enable real MLX library integration (vs stub implementation for testing).

**Includes**:
- Production MLX C++ FFI bindings
- GPU-accelerated tensor operations
- Full model inference capabilities
- Enterprise resilience features

**Platform**: macOS (requires MLX C++ library)

**Requirements**:
- macOS 13.0+
- MLX C++ library installed
- Xcode Command Line Tools

**Usage** (must be paired with `mlx-backend`/`multi-backend`):
```bash
# Enable real MLX (requires MLX C++ installed)
cargo build --release --features "mlx-backend,mlx"
```

**Status**: ⚠️ Real MLX only when combined with `mlx-backend`; otherwise ignored.

**Note**: Without this feature, `adapteros-lora-mlx-ffi` uses stub implementations
for testing purposes. With this feature enabled, full production MLX functionality is available. A deprecated alias `real-mlx` remains accepted for compatibility only.

**Testing modes**:
- Stub CI/default: `cargo test -p adapteros-lora-mlx-ffi` (no MLX libs required; e2e suites are gated).
- Real MLX: `cargo test -p adapteros-lora-mlx-ffi --features "mlx-backend,mlx" -- --include-ignored` (runs real MLX e2e/integration; ensure MLX libs and fixtures are present).

---

### Observability Flags

#### `telemetry`

**Purpose**: Enable structured event logging for observability.

**Includes**:
- Canonical JSON telemetry events
- BLAKE3 event hashing
- Bundle rotation and signing
- Event storage to SQLite

**Platform**: All

**Usage**:
```bash
cargo build --release --features telemetry
```

**Events logged** (examples):
- `barrier.generation_advanced`
- `adapter_promoted`, `adapter_demoted`
- `tick_ledger.consistent`, `tick_ledger.inconsistent`

**Event catalog**: See [CLAUDE.md Telemetry Event Catalog](../CLAUDE.md#telemetry-event-catalog)

**Query telemetry**:
```sql
SELECT * FROM telemetry_events
WHERE event_type = 'barrier.timeout'
  AND timestamp >= datetime('now', '-1 hour');
```

---

#### `metrics`

**Purpose**: Enable performance metrics collection.

**Includes**:
- Inference latency tracking
- Memory usage metrics
- Router decision metrics
- Adapter activation counters

**Platform**: All

**Usage**:
```bash
cargo build --release --features metrics
```

**Metrics exposed**:
- `inference_duration_ms`
- `tokens_per_second`
- `adapter_activation_count`
- `memory_usage_mb`

**Integration**: Metrics stored in SQLite, queryable via API.

---

#### `replay`

**Purpose**: Enable deterministic replay for debugging and verification.

**Includes**:
- Execution trace recording
- Replay from tick ledger
- Divergence detection
- Cross-host consistency checks

**Platform**: All

**Usage**:
```bash
cargo build --release --features replay
```

**Use cases**:
- Debugging non-deterministic behavior
- Cross-host execution verification
- Compliance audits

**Performance**: Adds <5% overhead due to trace recording.

---

### Combined Feature Sets

#### `full`

**Purpose**: Enable all observability features (telemetry + metrics + replay).

**Equivalent to**:
```bash
cargo build --release --features "telemetry,metrics,replay"
```

**Shorthand**:
```bash
cargo build --release --features full
```

**Platform**: All

**When to use**:
- Development environments
- Debugging production issues
- Full observability stack

**Performance impact**: ~5-10% overhead vs. default build.

---

#### `no-metal`

**Purpose**: Explicitly disable Metal backend (for Linux/CI).

**Equivalent to**:
```bash
cargo build --release --no-default-features
```

**Platform**: Linux, CI runners without macOS

**When to use**:
- Linux builds
- CI environments without macOS runners
- CPU-only testing

**Note**: Same effect as `--no-default-features` (disables all default features).

---

#### `multi-backend`

**Purpose**: Alias for `mlx-backend` (enables MLX FFI; stub unless `mlx` is also set).

**Equivalent to**:
```bash
cargo build --release --features mlx-backend          # stub
cargo build --release --features "multi-backend,mlx"  # real MLX
```

**Status**: ⚠️ Stub by default; add `mlx` for real MLX.

---

## Platform Compatibility Matrix

| Feature Flag | macOS (Apple Silicon) | macOS (Intel) | Linux | Windows | CI (Linux) |
|--------------|----------------------|---------------|-------|---------|------------|
| `deterministic-only` | ✅ | ✅ | ✅ | ⚠️ Untested | ✅ |
| `coreml-backend` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `metal-backend` | ✅ | ❌ | ❌ | ❌ | ❌ |
| `mlx-backend` | ⚠️ Stub only | ⚠️ Stub only | ❌ | ❌ | ❌ |
| `multi-backend` | ⚠️ Stub only | ⚠️ Stub only | ❌ | ❌ | ❌ |
| `telemetry` | ✅ | ✅ | ✅ | ⚠️ Untested | ✅ |
| `metrics` | ✅ | ✅ | ✅ | ⚠️ Untested | ✅ |
| `replay` | ✅ | ✅ | ✅ | ⚠️ Untested | ✅ |
| `full` | ✅ | ✅ | ✅ | ⚠️ Untested | ✅ |
| `no-metal` | ✅ | ✅ | ✅ | ⚠️ Untested | ✅ |

**Legend**:
- ✅ Supported and tested
- ⚠️ Experimental or untested
- ❌ Not supported (compilation fails)

**Notes**:
- Add `mlx` alongside `mlx-backend` / `multi-backend` for real MLX; otherwise stubs are used.
- On Linux/CI prefer `--no-default-features --features deterministic-only` to avoid CoreML build errors.

---

## Common Combinations

### Development (macOS)

**Full observability + Metal backend**:
```bash
cargo build --release --features "metal-backend,full"
```

**What you get**:
- Metal GPU acceleration
- Telemetry events
- Performance metrics
- Replay capability

**Use case**: Local development with full debugging capabilities.

---

### Production (macOS)

**Metal-only (minimal overhead)**:
```bash
cargo build --release --features metal-backend
```

**What you get**:
- Metal GPU acceleration
- No telemetry/metrics overhead
- Deterministic execution

**Use case**: Production deployment on macOS servers.

---

### CI/CD (Linux)

**CPU-only with full observability**:
```bash
cargo build --release --no-default-features --features full
```

**What you get**:
- CPU-only execution
- Telemetry + metrics + replay
- Cross-platform testing

**Use case**: GitHub Actions, GitLab CI on Linux runners.

---

### Testing (All Platforms)

**Default + metrics**:
```bash
cargo build --release --features metrics
```

**What you get**:
- Deterministic-only execution
- Performance metrics for benchmarking
- No platform dependencies

**Use case**: Unit tests, integration tests without GPU.

---

## Testing Feature Flags

### Automated Feature Matrix Testing

AdapterOS includes `xtask` to validate all feature combinations:

```bash
# Check all feature flag combinations
cargo xtask check-all
```

**What it does**:
1. Tests `default` (no features)
2. Tests `full` (all observability)
3. Tests `metal-backend` (macOS only)
4. Tests `no-metal` (Linux/CI)
5. Verifies platform compatibility

**Implementation**: `/xtask/src/check_all.rs:20-51`

---

### Manual Feature Testing

**Test specific feature**:
```bash
# Build with feature
cargo build --release --features telemetry

# Run tests with feature
cargo test --features telemetry

# Check compilation only
cargo check --features "metal-backend,metrics"
```

---

### Feature-Gated Code

**Example**: Enable code only when `metal-backend` is active:

```rust
#[cfg(feature = "metal-backend")]
use adapteros_lora_kernel_mtl::MetalKernels;

#[cfg(not(feature = "metal-backend"))]
use adapteros_lora_kernel_api::MockKernels as MetalKernels;
```

**Example**: Telemetry event logging:

```rust
#[cfg(feature = "telemetry")]
{
    use adapteros_telemetry::TelemetryEventBuilder;
    let event = TelemetryEventBuilder::new(
        EventType::Custom("adapter_loaded".into()),
        LogLevel::Info,
        format!("Adapter {} loaded", id),
    ).build();
    event.emit().await?;
}
```

---

## CI/CD Integration

### GitHub Actions Example

```yaml
name: CI

on: [push, pull_request]

jobs:
  # Linux build (no Metal)
  linux-build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build
        run: cargo build --release --no-default-features --features full

  # macOS build (with Metal)
  macos-build:
    runs-on: macos-13
    steps:
      - uses: actions/checkout@v4
      - name: Build
        run: cargo build --release --features "metal-backend,full"

  # Feature matrix test
  feature-matrix:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Test all feature combinations
        run: cargo xtask check-all
```

**See**: `.github/workflows/ci.yml:1-56` for actual CI configuration.

---

### Makefile Integration

```makefile
# Default build (macOS with Metal)
build:
    cargo build --release --features metal-backend

# Linux build
build-linux:
    cargo build --release --no-default-features

# Full observability build
build-full:
    cargo build --release --features full

# Test all feature combinations
check-features:
    cargo xtask check-all
```

---

## Adding New Features

### Step 1: Define Feature in Cargo.toml

```toml
[features]
# Add new feature flag
my-new-feature = ["dep:some-crate"]
```

### Step 2: Add Dependencies (if needed)

```toml
[dependencies]
some-crate = { version = "1.0", optional = true }
```

### Step 3: Gate Code with Feature

```rust
#[cfg(feature = "my-new-feature")]
pub fn new_functionality() {
    // Implementation
}

#[cfg(not(feature = "my-new-feature"))]
pub fn new_functionality() {
    unimplemented!("my-new-feature not enabled")
}
```

### Step 4: Update CI

Add feature to `xtask/src/check_all.rs`:

```rust
let features_to_test = vec![
    "default",
    "full",
    "metal-backend",
    "my-new-feature",  // Add here
];
```

### Step 5: Document in FEATURE_FLAGS.md

Add section above with:
- Purpose
- Platform compatibility
- Usage example
- Status (experimental/stable)

---

## Feature Flag Decision Tree

```
┌─────────────────────────────────────┐
│ What platform are you building for? │
└─────────────────────────────────────┘
         │
         ├─────────────┬─────────────┐
         ▼             ▼             ▼
    ┌─────────┐  ┌─────────┐  ┌─────────┐
    │  macOS  │  │  Linux  │  │   CI    │
    └─────────┘  └─────────┘  └─────────┘
         │             │             │
         ▼             ▼             ▼
    Do you need   No GPU       Need tests?
    GPU accel?    available         │
         │             │             ▼
    ┌────┴────┐        │        --no-default
    ▼         ▼        │        --features full
  Yes        No        │
    │         │        │
    ▼         ▼        ▼
 metal-   default   default
 backend
    │         │        │
    ▼         ▼        ▼
 Need obs? Need obs? Done
    │         │
  ┌─┴─┐     ┌─┴─┐
  ▼   ▼     ▼   ▼
 Yes  No   Yes  No
  │   │     │   │
  ▼   ▼     ▼   ▼
 ,full  Done ,full Done
```

---

## Quick Reference

**One-liner feature combinations**:

```bash
# macOS production (Metal only)
cargo build --release --features metal-backend

# macOS development (Metal + full observability)
cargo build --release --features "metal-backend,full"

# Linux/CI (no Metal, full observability)
cargo build --release --no-default-features --features full

# Testing (default + metrics)
cargo build --release --features metrics

# Check all combinations
cargo xtask check-all
```

---

## Verification Status

⚠️ **Feature flag combinations documented based on code analysis - not all tested**

| Feature Combination | Tested | Platform | Notes |
|---------------------|--------|----------|-------|
| `default` (no features) | ⏳ | All | Assumed to work |
| `coreml-backend` | ⏳ | macOS | Primary production backend |
| `metal-backend` | ⏳ | macOS | Fallback for non-ANE systems |
| `--no-default-features` | ⏳ | Linux | Not verified on Linux |
| `full` | ⏳ | All | Not verified |
| `coreml-backend,full` | ⏳ | macOS | Production + observability |
| `metal-backend,full` | ⏳ | macOS | Metal + observability |
| `multi-backend` | ⚠️ | macOS | Stub implementation only (alias for mlx-backend) |

**Legend**:
- ✅ Verified - Tested and works
- ⏳ Untested - Should work based on code analysis
- ⚠️ Incomplete - Exists but limited functionality
- ❌ Broken - Known not to work

**Help improve this guide**: Test feature combinations on your system and report results.

---

## Feature Flag Propagation

### Workspace vs. Crate Features

AdapterOS uses a two-level feature flag architecture:

1. **Workspace-level features** (root `Cargo.toml`):
   - Define cross-cutting features available to the entire workspace
   - Example: `coreml-backend`, `metal-backend`, `multi-backend`
   - These do NOT automatically propagate to crates

2. **Crate-level features** (per-crate `Cargo.toml`):
   - Each crate defines which workspace features it responds to
   - Use `dep:crate-name` syntax to enable optional dependencies
   - Example: `coreml-backend = ["dep:adapteros-lora-kernel-coreml"]`

### Feature Activation Chain

When you run `cargo build --features coreml-backend`:

```
Root Cargo.toml: coreml-backend = []
         │
         ▼
adapteros-lora-worker/Cargo.toml:
  coreml-backend = ["dep:adapteros-lora-kernel-coreml"]
         │
         ▼
adapteros-lora-kernel-coreml is compiled
         │
         ▼
#[cfg(feature = "coreml-backend")] code paths activated
```

### Important: Feature Names Must Match

For feature propagation to work, the feature name must be:
1. Defined in root `Cargo.toml` `[features]`
2. Defined in consuming crate's `Cargo.toml` `[features]`
3. The names must match exactly

**Example** (correct):
```toml
# Root Cargo.toml
[features]
coreml-backend = []

# adapteros-lora-worker/Cargo.toml
[features]
coreml-backend = ["dep:adapteros-lora-kernel-coreml"]
```

---

## Related Documentation

- [LOCAL_BUILD.md](LOCAL_BUILD.md) - Complete build guide
- [QUICKSTART.md](QUICKSTART.md) - Getting started
- [CLAUDE.md](../CLAUDE.md) - Developer guide (feature flag reference)
- [CI Configuration](.github/workflows/ci.yml) - GitHub Actions setup

---

## See Also

- [ADR_MULTI_BACKEND_STRATEGY.md](ADR_MULTI_BACKEND_STRATEGY.md) - Multi-backend architecture decision record
- [COREML_INTEGRATION.md](COREML_INTEGRATION.md) - CoreML backend with ANE acceleration
- [MLX_INTEGRATION.md](MLX_INTEGRATION.md) - MLX backend production deployment guide
- [ARCHITECTURE_PATTERNS.md](ARCHITECTURE_PATTERNS.md) - Detailed architectural patterns
- [crates/adapteros-lora-kernel-coreml/README.md](../crates/adapteros-lora-kernel-coreml/README.md) - CoreML crate documentation
- [crates/adapteros-lora-mlx-ffi/README.md](../crates/adapteros-lora-mlx-ffi/README.md) - MLX FFI crate documentation

---

**Maintained by**: James KC Auchterlonie
**Copyright**: © 2025 JKCA / James KC Auchterlonie. All rights reserved.
MLNavigator Inc 2025-12-08.
