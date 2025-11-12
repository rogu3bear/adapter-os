# AdapterOS MVP Quick Start Guide

**Status:** v0.3-alpha - MVP Ready (with known limitations)

This guide helps you get started with AdapterOS MVP functionality.

## MVP Scope

The MVP includes:

1. **Core Inference** - Run inference with base model (Qwen 2.5 7B)
2. **LoRA Routing** - Load and route K-sparse LoRA adapters
3. **Server API** - REST API for inference requests (OpenAI-compatible)
4. **Adapter Registry** - Database-backed adapter management
5. **Basic Policy** - Core policy enforcement (determinism, egress)
6. **CLI** - Basic commands for model/adapter management

## Known Limitations

### Compilation Status
- ✅ Library crates compile successfully
- ⚠️ Server has compilation errors (async lifetime issues)
- ✅ CLI compiles successfully
- ✅ Integration tests enabled (some require server fixes)

### Runtime Status
- ✅ Core inference pipeline implemented
- ✅ Router implementation complete
- ✅ Database schema and migrations ready
- ⚠️ Server startup blocked by compilation errors
- ✅ Keychain crypto secure (no hardcoded keys)

### Testing Status
- ✅ Unit tests compile and run
- ✅ Integration tests enabled (basic tests available)
- ⚠️ Full E2E tests require server fixes
- ✅ Security audit passed (no hardcoded keys found)

## Quick Start

### Prerequisites

- macOS 13.0+ with Apple Silicon (M1/M2/M3/M4)
- Rust 1.75+
- SQLite database initialized

### Step 1: Build the Workspace

```bash
# Build all library crates (excludes server with known issues)
cargo build --workspace --exclude adapteros-server

# Or build specific components
cargo build --package adapteros-lora-worker
cargo build --package adapteros-cli
cargo build --package adapteros-lora-router
```

### Step 2: Initialize Database

```bash
# Initialize tenant
./target/release/aosctl init-tenant --id default --uid 1000 --gid 1000
```

### Step 3: Import Model

```bash
# Import Qwen 2.5 7B model
./target/release/aosctl import-model \
  --name qwen2.5-7b \
  --weights models/qwen2.5-7b-mlx/weights.safetensors \
  --config models/qwen2.5-7b-mlx/config.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
  --tokenizer-cfg models/qwen2.5-7b-mlx/tokenizer_config.json \
  --license models/qwen2.5-7b-mlx/LICENSE
```

### Step 4: Register Adapter

```bash
# Register a LoRA adapter
./target/release/aosctl register-adapter \
  --id my-lora \
  --hash <adapter-hash> \
  --tier 1 \
  --rank 16
```

### Step 5: Build Plan

```bash
# Build a plan for serving
./target/release/aosctl build-plan \
  --tenant-id default \
  --manifest configs/cp.toml
```

## Current Blockers

### Server Compilation Errors

The `adapteros-server` crate has async lifetime issues that prevent compilation:

```
error[E0373]: async block may outlive the current function, but it borrows `state_for_cleanup`
```

**Workaround:** Use CLI commands directly or wait for server fixes.

### Full E2E Testing

End-to-end tests require a running server. Until server compilation is fixed:

- Unit tests can be run: `cargo test --workspace`
- Integration tests are enabled but may fail without server
- Manual testing via CLI is recommended

## MVP Validation Checklist

- [x] Core inference pipeline compiles
- [x] Router implementation complete
- [x] Database migrations ready
- [x] CLI commands functional
- [x] Security audit passed
- [x] Integration tests enabled
- [ ] Server compiles and runs
- [ ] Full E2E test suite passes

## Next Steps

1. **Fix Server Compilation** - Resolve async lifetime issues in `adapteros-server`
2. **Run E2E Tests** - Execute full end-to-end test suite
3. **Performance Validation** - Benchmark inference pipeline
4. **Documentation** - Complete API reference

## Support

For issues or questions:
- GitHub: [@rogu3bear](https://github.com/rogu3bear)
- Email: vats-springs0m@icloud.com

---

**Last Updated:** 2025-01-15  
**MVP Status:** Core functionality ready, server fixes in progress

