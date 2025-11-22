# Local Build Guide for AdapterOS

Complete reference for building AdapterOS locally on development hardware. This guide covers environment setup, feature flags, build commands, and troubleshooting.

**Last Updated**: 2025-01-18
**Target**: Local development on macOS with Apple Silicon
**Audience**: Contributors, developers, refactor teams

---

## Table of Contents

1. [Prerequisites](#prerequisites)
2. [Environment Setup](#environment-setup)
3. [Toolchain Configuration](#toolchain-configuration)
4. [Build Commands](#build-commands)
5. [Feature Flags](#feature-flags)
6. [Testing](#testing)
7. [Troubleshooting](#troubleshooting)
8. [Development Workflow](#development-workflow)

---

## Prerequisites

### Required Hardware

**Primary Platform: macOS 13.0+ with Apple Silicon (M1/M2/M3/M4)**
- **Metal Backend**: Production-ready GPU acceleration via Metal shaders
- **Unified Memory Architecture (UMA)**: Required for optimal performance
- **Minimum RAM**: 16GB recommended (32GB+ for large models)
- **Storage**: 10GB+ free space for builds and artifacts

**Linux/CI**: CPU-only builds supported with `--no-default-features` flag (Metal disabled)

### Required Software

**Rust Toolchain**:
```bash
# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Verify version (1.75+ required)
rustc --version
```

**Additional Tools**:
- **SQLite**: Bundled with macOS, no installation needed
- **Git**: For version control
- **pnpm** (optional, for UI development): `npm install -g pnpm`

---

## Environment Setup

### Required Environment Variables

Create a `.env` file in the project root or export these variables:

```bash
# Database Configuration (for sqlx compile-time verification)
export DATABASE_URL="sqlite://var/aos-cp.sqlite3"
# NOTE: May not be strictly required if sqlx offline mode is enabled
# Check cargo output for "SQLX validation disabled" message
```

### Optional Environment Variables

```bash
# Manifest path for deterministic seeding
# If not set, manifest must be provided via CLI flag
export AOS_MANIFEST_PATH="path/to/manifest.json"

# Logging level
export RUST_LOG="info,adapteros=debug"

# Build optimization
export CARGO_BUILD_JOBS=8  # Adjust based on CPU cores
```

### Environment Validation

Verify your environment is correctly configured:

```bash
# Check Rust toolchain
rustup show

# Expected output:
# active toolchain: nightly-aarch64-apple-darwin
# components: rustfmt, clippy

# Check environment variables
env | grep -E "(DATABASE_URL|AOS_MANIFEST_PATH|RUST_LOG)"

# Verify SQLite
sqlite3 --version
```

### Directory Structure Setup

Create required runtime directories:

```bash
# Create directories (run from project root)
mkdir -p var/          # Database and runtime state
mkdir -p adapters/     # Adapter storage
mkdir -p models/       # Model weights
mkdir -p artifacts/    # Build artifacts
mkdir -p plan/         # Plan storage
```

---

## Toolchain Configuration

AdapterOS uses a pinned Rust toolchain defined in `rust-toolchain.toml`:

```toml
[toolchain]
channel = "nightly"
components = ["rustfmt", "clippy"]
targets = ["aarch64-apple-darwin"]
```

**No manual configuration needed** - `rustup` automatically installs the correct toolchain when you run `cargo` commands.

### Verify Toolchain

```bash
# Show active toolchain
rustup show

# Install missing components (if needed)
rustup component add rustfmt clippy
```

---

## Build Commands

### Canonical Build (macOS with Metal)

**Standard development build**:
```bash
# Full workspace build with Metal backend
cargo build --release --locked --offline
```

**Explanation**:
- `--release`: Optimized build (slower compile, faster runtime)
- `--locked`: Use exact dependency versions from Cargo.lock
- `--offline`: Use cached dependencies (no network access)

**Quick validation** (faster, no optimization):
```bash
# Check compilation without building binaries
cargo check --workspace
```

### Linux/CI Build (No Metal)

**CPU-only build** (for Linux or CI environments):
```bash
# Build without Metal dependencies
cargo build --release --no-default-features
```

**Explanation**:
- `--no-default-features`: Disables `metal-backend` feature flag
- Suitable for non-macOS platforms
- Uses CPU-only backends (mock kernels for testing)

### Selective Builds

**Build specific crates**:
```bash
# Build worker crate only
cargo build --release -p adapteros-lora-worker

# Build router crate
cargo build --release -p adapteros-lora-router

# Build server (excludes currently broken crates)
cargo build --release -p adapteros-server
```

**Build with specific features**:
```bash
# Full feature set (telemetry + metrics + replay)
cargo build --release --features full

# Metal backend only
cargo build --release --features metal-backend

# Experimental MLX backend (currently disabled due to PyO3 linker issues)
cargo build --release --features multi-backend
```

### Makefile Shortcuts

Use the provided Makefile for common tasks:

```bash
# Full build (calls cargo with recommended flags)
make build

# Check workspace (fast validation)
make check

# Clean build artifacts
make clean

# Format and lint
make fmt
make clippy

# Build Metal shaders
make metal

# All checks (format + clippy + tests)
make test
```

---

## Feature Flags

AdapterOS uses feature flags to control compilation and enable/disable platform-specific functionality.

### Feature Flag Reference

| Feature Flag | Purpose | Platform | Status |
|--------------|---------|----------|--------|
| `deterministic-only` | **Default**: Core deterministic execution (no backends) | All | ✅ Stable |
| `metal-backend` | Metal GPU acceleration | macOS only | ✅ Stable |
| `mlx-backend` | MLX Python backend via PyO3 | macOS | ⚠️ Experimental (disabled) |
| `telemetry` | Structured event logging | All | ✅ Stable |
| `metrics` | Performance metrics collection | All | ✅ Stable |
| `replay` | Deterministic replay for debugging | All | ✅ Stable |
| `full` | `telemetry` + `metrics` + `replay` | All | ✅ Stable |
| `no-metal` | Explicitly disable Metal (same as `--no-default-features`) | Linux/CI | ✅ Stable |
| `multi-backend` | Alias for `mlx-backend` | macOS | ⚠️ Disabled |

### Feature Flag Usage Examples

**Default build** (deterministic-only, no backends):
```bash
cargo build --release
```

**Metal-enabled build** (macOS, GPU acceleration):
```bash
cargo build --release --features metal-backend
```

**Full observability** (telemetry + metrics + replay):
```bash
cargo build --release --features full
```

**Combined features**:
```bash
cargo build --release --features "metal-backend,telemetry,metrics"
```

**Check all feature combinations** (automated testing):
```bash
cargo xtask check-all
```

### Feature Flag Implementation

**Location**: `/Cargo.toml:77-96`

```toml
[features]
default = ["deterministic-only"]
deterministic-only = []
telemetry = []
metrics = []
replay = []
metal-backend = []
mlx-backend = ["dep:pyo3"]
full = ["telemetry", "metrics", "replay"]
no-metal = []
multi-backend = ["mlx-backend"]
```

---

## Testing

### Run All Tests

**Standard test suite** (excludes disabled crates):
```bash
# Run all workspace tests
cargo test --workspace --exclude adapteros-lora-mlx-ffi

# With output (see print statements)
cargo test --workspace --exclude adapteros-lora-mlx-ffi -- --nocapture

# Single-threaded (for debugging race conditions)
cargo test --workspace --exclude adapteros-lora-mlx-ffi -- --test-threads=1
```

### Run Specific Tests

**Test a single crate**:
```bash
cargo test -p adapteros-lora-router
cargo test -p adapteros-lora-worker
cargo test -p adapteros-deterministic-exec
```

**Test a specific function**:
```bash
# Run test by name
cargo test test_k_sparse_routing

# With output
cargo test test_adapter_loading -- --nocapture
```

**Integration tests**:
```bash
# Run integration test suite
cargo test --test integration_tests

# Run specific integration test file
cargo test --test adapter_hotswap
cargo test --test concurrency
```

### Test Categories

| Test Category | Location | Command | Purpose |
|---------------|----------|---------|---------|
| **Unit Tests** | `crates/*/src/**/*.rs` | `cargo test -p <crate>` | Crate-specific logic |
| **Integration Tests** | `tests/*.rs` | `cargo test --test <name>` | Cross-crate workflows |
| **Hot-Swap Tests** | `tests/adapter_hotswap.rs` | `cargo test --test adapter_hotswap` | Adapter swap logic |
| **Concurrency Tests** | `tests/concurrency.rs` | `cargo test --test concurrency` | Race condition validation |
| **Schema Tests** | `crates/adapteros-db/tests/` | `cargo test -p adapteros-db schema_consistency_tests` | Database schema validation |

### Feature-Gated Tests

Some tests require specific feature flags:

```bash
# Tests requiring Metal backend
cargo test --features metal-backend test_metal_kernels

# Tests requiring extended test suite
cargo test --features extended-tests
```

### Test Troubleshooting

**SQLite errors during tests**:
```bash
# Set DATABASE_URL for tests
export DATABASE_URL="sqlite::memory:"  # In-memory database
cargo test
```

**Flaky concurrency tests**:
```bash
# Run single-threaded
cargo test -- --test-threads=1
```

**Timeout issues**:
```bash
# Increase test timeout (for slow CI environments)
cargo test -- --test-threads=1 --nocapture
```

---

## Troubleshooting

### Common Build Issues

#### 1. "No such file or directory: DATABASE_URL"

**Cause**: `DATABASE_URL` environment variable not set (required for `sqlx` compile-time verification).

**Fix**:
```bash
export DATABASE_URL="sqlite://var/aos-cp.sqlite3"
cargo build --release
```

**Permanent fix**: Add to `.env` file or shell profile.

---

#### 2. "Cannot find -lMetal" (Linux builds)

**Cause**: Trying to build with Metal backend on Linux.

**Fix**:
```bash
# Use no-default-features to disable Metal
cargo build --release --no-default-features
```

---

#### 3. "Compilation failed: adapteros-server-api (62 errors)"

**Cause**: `adapteros-server-api` crate currently has compilation errors (known issue in alpha v0.01-1).

**Status**: Disabled in workspace, not blocking local builds.

**Workaround**: Build other crates individually:
```bash
cargo build --release -p adapteros-lora-worker
cargo build --release -p adapteros-lora-router
```

**Tracking**: See CLAUDE.md Known Build Issues section.

---

#### 4. "PyO3 linker errors" (MLX backend)

**Cause**: `mlx-backend` feature has PyO3 linker issues.

**Status**: Experimental, disabled by default.

**Fix**: Use Metal backend instead:
```bash
cargo build --release --features metal-backend
```

---

#### 5. "Merge conflicts in tests/adapter_hotswap.rs"

**Cause**: Unresolved merge conflict markers (`<<<<<<<` in code).

**Fix**: Resolve conflicts manually:
```bash
# Open file and search for conflict markers
code tests/adapter_hotswap.rs  # or your editor

# Look for:
# <<<<<<< HEAD
# =======
# >>>>>>> branch-name

# Resolve and remove markers, then:
cargo test --test adapter_hotswap
```

---

#### 6. "RUSTC_WRAPPER not found: sccache"

**Cause**: `RUSTC_WRAPPER` environment variable points to missing `sccache` binary.

**Fix**:
```bash
# Remove wrapper
unset RUSTC_WRAPPER

# Or install sccache
cargo install sccache
```

---

#### 7. "Cannot acquire lock on Cargo.toml"

**Cause**: Another `cargo` process is running.

**Fix**:
```bash
# Wait for other process to finish, or:
killall cargo
cargo build --release
```

---

### Performance Issues

#### Slow compilation times

**Enable parallel compilation**:
```bash
export CARGO_BUILD_JOBS=8  # Adjust based on CPU cores
cargo build --release
```

**Use incremental compilation** (dev builds):
```bash
# Enable in .cargo/config.toml
[build]
incremental = true
```

**Use faster linker** (macOS):
```bash
# Install zld (faster linker)
brew install michaeleisel/zld/zld

# Configure in .cargo/config.toml
[target.aarch64-apple-darwin]
rustflags = ["-C", "link-arg=-fuse-ld=zld"]
```

---

#### Out of memory during build

**Reduce parallel jobs**:
```bash
export CARGO_BUILD_JOBS=2
cargo build --release
```

**Build incrementally**:
```bash
# Build crates in dependency order
cargo build --release -p adapteros-core
cargo build --release -p adapteros-lora-kernel-api
cargo build --release -p adapteros-lora-router
cargo build --release -p adapteros-lora-worker
```

---

### Test Failures

#### "Database locked" errors

**Cause**: Parallel tests accessing same SQLite database.

**Fix**:
```bash
# Run tests sequentially
cargo test -- --test-threads=1

# Or use in-memory database
export DATABASE_URL="sqlite::memory:"
cargo test
```

---

#### Loom concurrency test failures

**Cause**: Concurrency model exceeded exploration budget.

**Fix**: Already configured in code with preemption bounds. If failures persist:
```bash
# Run specific test with increased iterations
LOOM_MAX_PREEMPTIONS=3 cargo test test_hotswap_loom
```

---

## Development Workflow

### Recommended Development Loop

```bash
# 1. Make code changes
vim crates/adapteros-lora-worker/src/lib.rs

# 2. Quick validation
make check  # Fast, no binary generation

# 3. Format and lint
make fmt
make clippy

# 4. Run affected tests
cargo test -p adapteros-lora-worker

# 5. Full workspace test (before commit)
cargo test --workspace --exclude adapteros-lora-mlx-ffi

# 6. Build release binary
cargo build --release
```

### Pre-Commit Checklist

```bash
# ✅ Format code
cargo fmt --all

# ✅ Lint (zero warnings)
cargo clippy --workspace -- -D warnings

# ✅ Tests pass
cargo test --workspace --exclude adapteros-lora-mlx-ffi

# ✅ Check for duplicated code (CRITICAL)
make dup

# ✅ Build succeeds
cargo build --release

# ✅ Schema migrations signed (if changed)
./scripts/sign_migrations.sh
```

---

## Verification Status

⚠️ **This build guide has not been tested on a fresh environment.**

| Claim Type | Status | Notes |
|------------|--------|-------|
| Build commands | ⏳ Untested | Commands inferred from Makefile and workspace config |
| Environment variables | 💭 Inferred | DATABASE_URL usage based on sqlx; may work without it |
| Troubleshooting solutions | 💭 Documented | Common issues from codebase analysis, not live testing |
| Platform compatibility | ✅ Verified | Checked against rust-toolchain.toml and Cargo.toml |

**Help improve this guide**: Test on your system and report issues or corrections.

---

## Quick Reference Card

**One-liner build commands** (copy-paste ready):

```bash
# macOS development build
cargo build --release --locked --offline

# Linux/CI build
cargo build --release --no-default-features

# Fast validation
cargo check --workspace

# All tests
cargo test --workspace --exclude adapteros-lora-mlx-ffi

# Full observability
cargo build --release --features full

# Clean rebuild
cargo clean && cargo build --release
```

---

## Next Steps

After successful local build:

1. **Run the system**: See [QUICKSTART.md](QUICKSTART.md) for deployment
2. **Explore architecture**: See [architecture.md](ARCHITECTURE.md) for system design
3. **Feature flags**: See [FEATURE_FLAGS.md](FEATURE_FLAGS.md) for detailed flag reference (if exists)
4. **Hot-swap details**: See [HOT_SWAP.md](HOT_SWAP.md) for adapter management (if exists)
5. **Contributing**: See [../CONTRIBUTING.md](../CONTRIBUTING.md) for PR guidelines

---

## Getting Help

- **Build issues**: Check [Troubleshooting](#troubleshooting) section above
- **Architecture questions**: See [docs/README.md](README.md) for documentation index
- **Test failures**: See `tests/README.md` for test documentation (if exists)
- **Feature requests**: See [../CONTRIBUTING.md](../CONTRIBUTING.md)

---

**Maintained by**: James KC Auchterlonie
**Copyright**: © 2025 JKCA / James KC Auchterlonie. All rights reserved.
