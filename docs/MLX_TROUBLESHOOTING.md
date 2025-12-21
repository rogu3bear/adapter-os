# MLX Troubleshooting Guide

**Last Updated:** 2025-12-11
**Status:** Comprehensive troubleshooting reference

---

## Table of Contents

1. [Quick Diagnostics](#quick-diagnostics)
2. [Build & Link Errors](#build--link-errors)
3. [Runtime Issues](#runtime-issues)
4. [Metal Device Access Issues](#metal-device-access-issues)
5. [Migration Guide (Stub → Real)](#migration-guide-stub--real)
6. [Performance Issues](#performance-issues)
7. [Testing Modes](#testing-modes)
8. [Validation Commands](#validation-commands)

---

## Quick Diagnostics

### System Verification

```bash
# Check MLX installation
brew list mlx
brew info mlx

# Verify MLX library files
ls -la /opt/homebrew/lib/libmlx*
ls -la /opt/homebrew/include/mlx/

# Check environment variables
echo "MLX_INCLUDE_DIR=${MLX_INCLUDE_DIR}"
echo "MLX_LIB_DIR=${MLX_LIB_DIR}"
echo "MLX_PATH=${MLX_PATH}"
echo "AOS_MODEL_PATH=${AOS_MODEL_PATH}"

# Verify Metal support
system_profiler SPDisplaysDataType | grep -A 5 "Metal"
```

### Build Status Check

```bash
# Check build mode
cargo build -p adapteros-lora-mlx-ffi --features mlx 2>&1 | grep "MLX FFI build"

# Expected: "MLX FFI build: REAL"
# If you see stub, MLX headers weren't found or MLX_FORCE_STUB=1 is set
```

### Runtime Health Check

```bash
# Backend list
curl -s http://localhost:8080/v1/backends | jq

# MLX backend status
curl -s http://localhost:8080/v1/backends/mlx/status | jq

# Health check
curl -s http://localhost:8080/healthz/backend | jq
```

---

## Build & Link Errors

### Error: MLX Headers Not Found

**Symptom:**
```
error: could not find mlx/mlx.h
```

**Cause:** MLX include directory not in search path

**Solution:**
```bash
# Set MLX include directory
export MLX_INCLUDE_DIR=/opt/homebrew/include

# Verify headers exist
ls -la $MLX_INCLUDE_DIR/mlx/

# Rebuild
cargo clean
cargo build -p adapteros-lora-mlx-ffi --features mlx --release

# Or run environment verification
make verify-mlx-env
```

### Error: libmlx.dylib Missing

**Symptom:**
```
error: linking with 'cc' failed
ld: library not found for -lmlx
```

**Cause:** MLX library not in linker search path

**Solution:**
```bash
# Set MLX library directory
export MLX_LIB_DIR=/opt/homebrew/lib

# Verify library exists
ls -la $MLX_LIB_DIR/libmlx*

# Rebuild
cargo clean
cargo build -p adapteros-lora-mlx-ffi --features mlx --release

# Alternative: reinstall MLX
brew reinstall mlx
```

### Error: Swift/Metal Toolchain Missing

**Symptom:**
```
error: xcrun: error: unable to find utility "metal"
```

**Cause:** Xcode Command Line Tools not installed

**Solution:**
```bash
# Install Xcode Command Line Tools
xcode-select --install

# Verify installation
xcode-select -p
# Should output: /Library/Developer/CommandLineTools

# Rebuild
cargo build -p adapteros-lora-mlx-ffi --features mlx --release
```

### Error: Wrong Feature Set

**Symptom:** Build succeeds but stub mode is active

**Cause:** Built without `mlx` feature or with `MLX_FORCE_STUB=1`

**Solution:**
```bash
# Ensure correct features
cargo build -p adapteros-lora-mlx-ffi --features mlx --release

# Or workspace build
cargo build --release --features "multi-backend,mlx"

# Verify MLX_FORCE_STUB is NOT set
unset MLX_FORCE_STUB

# Clear target directory if necessary
cargo clean
```

### Error: ABI Mismatch

**Symptom:** Build succeeds but tests fail with segfaults or unexpected behavior

**Cause:** Mismatch between build-time and runtime MLX versions

**Solution:**
```bash
# Check MLX version
brew info mlx | grep -E "mlx:"

# Rebuild MLX from source if needed
brew uninstall mlx
brew install mlx --build-from-source

# Rebuild adapteros-lora-mlx-ffi
cargo clean
cargo build -p adapteros-lora-mlx-ffi --features mlx --release
```

---

## Runtime Issues

### Backend Reports Stub Mode

**Symptom:** Backend status shows `mode=stub` or warnings about stub implementation

**Diagnosis:**
```bash
# Check build output
cargo build -p adapteros-lora-mlx-ffi --features mlx 2>&1 | grep "MLX FFI build"

# Verify binary
nm target/release/libadapteros_lora_mlx_ffi.a | grep mlx_wrapper_is_real
```

**Solution:**
```bash
# Build with mlx feature
cargo build -p adapteros-lora-mlx-ffi --features mlx --release

# Clear target/ if necessary
cargo clean

# Rebuild with make
make build-mlx
```

### Model Path Validation Fails

**Symptom:**
```
Error: Model path validation failed
Please set AOS_MODEL_PATH to a directory containing config.json
```

**Cause:** Model path not set or points to invalid directory

**Solution:**
```bash
# Set model path
export AOS_MODEL_PATH=./models/qwen2.5-7b-mlx

# Verify directory structure
ls -la $AOS_MODEL_PATH/
# Should contain: config.json, model.safetensors, tokenizer.json

# If files are missing, re-download model
python -m mlx_lm.convert \
  --hf-path Qwen/Qwen2.5-7B-Instruct \
  --mlx-path ./models/qwen2.5-7b-mlx
```

### Circuit Breaker Trips

**Symptom:** Backend becomes unavailable after multiple failures

**Diagnosis:**
```bash
# Check backend status
curl http://localhost:8080/v1/backends/mlx/status | jq '.circuit_breaker'

# Check logs for failure patterns
journalctl -u aos-mlx.service --since "1h ago" | grep -i error
```

**Solution:**
```bash
# Increase circuit breaker timeout in config
# Edit configs/mlx.toml:
[mlx.resilience]
max_consecutive_failures = 5
circuit_breaker_timeout_secs = 600  # Increase from 300

# Restart service
sudo systemctl restart aos-mlx.service

# Check GPU/ANE telemetry for hardware issues
# Monitor memory pressure
curl http://localhost:8080/v1/metrics | grep mlx_backend_memory
```

### OOM / Memory Pressure

**Symptom:** Out-of-memory errors or system becomes unresponsive

**Diagnosis:**
```bash
# Monitor memory usage
watch -n 1 'ps aux | grep aosctl | grep -v grep'

# Check MLX memory stats
curl http://localhost:8080/v1/backends/mlx/memory | jq
```

**Solution:**
```bash
# Lower max_memory_mb in config
# Edit configs/mlx.toml:
[mlx]
max_memory_mb = 12000  # Reduce from 16000
gc_threshold_mb = 2000

# Use quantized model (if not already)
python -m mlx_lm.convert \
  --hf-path Qwen/Qwen2.5-7B-Instruct \
  --mlx-path ./models/qwen2.5-7b-mlx \
  --quantize

# Reduce batch size
[mlx.performance]
batch_size = 4  # Reduce from 8 or 16

# Ensure swap is not heavily used
vm_stat | grep "Pages active"
```

### Latency Regression

**Symptom:** Inference slower than expected

**Diagnosis:**
```bash
# Enable trace logging
export RUST_LOG=trace,adapteros_lora_mlx_ffi=trace

# Run inference and check logs
./target/release/aosctl infer \
  --model ./models/qwen2.5-7b-mlx \
  --prompt "test" \
  --max-tokens 10

# Compare against Metal baseline
make bench-mlx
make bench-metal
```

**Solution:**
```bash
# Enable HKDF-seeded determinism in configs
[mlx.determinism]
use_hkdf_seeding = true

# Enable KV cache
[mlx.performance]
enable_kv_cache = true
cache_warmup_tokens = 512

# Check if running on CPU instead of GPU
# Look for warning logs about GPU fallback
grep -i "cpu fallback" logs/aos-mlx.log
```

### Determinism Reported as False

**Symptom:** Attestation shows `deterministic: false`

**Expected Behavior:** This is **correct** for MLX backend

**Explanation:**
- MLX does NOT provide full execution-order determinism
- HKDF seeding ensures RNG operations (dropout, sampling) are deterministic
- GPU scheduling and floating-point rounding are non-deterministic
- This is documented behavior, not a bug

**Verification:**
```bash
# Ensure manifest hash is passed to backend
# Check worker initialization logs
grep "manifest hash" logs/aos-worker.log

# Build with mlx feature
cargo build -p adapteros-lora-mlx-ffi --features "mlx-backend,mlx"

# Stub builds always report non-deterministic
# Verify you have real build
cargo build -p adapteros-lora-mlx-ffi --features mlx 2>&1 | grep "MLX FFI build: REAL"
```

**When to Use MLX Despite Non-Determinism:**
- Production inference where small numerical variations are acceptable
- Training workloads (determinism less critical)
- Research/experimentation

**When to Use Different Backend:**
- Use **Metal** for guaranteed determinism in production
- Use **CoreML** for ANE acceleration with conditional determinism

---

## Metal Device Access Issues

### Symptom: NSRangeException or MTLCreateSystemDefaultDevice() Returns Nil

**Problem:** Tests crash with `NSRangeException` or Metal device not visible

**Root Cause:** Process cannot see Metal devices due to sandboxing/virtualization/entitlement restrictions

### Quick Test

```bash
# Test Metal device access
swift -e 'import Metal; print(MTLCreateSystemDefaultDevice() != nil ? "✅ Metal OK" : "❌ No Metal")'

# Expected: "✅ Metal OK"
# If you see "❌ No Metal", follow solutions below
```

### Verification Steps

#### 1. Verify Hardware Support

```bash
# Check Metal support (should show Apple M4 Max or similar)
system_profiler SPDisplaysDataType | grep -A 5 "Metal"
```

**Expected:** "Metal: Supported"

#### 2. Check Process Context

```bash
# Verify you're not in SSH session (Metal requires GUI session)
echo $SSH_CONNECTION
# Should be empty

# Verify Terminal.app or iTerm2 (not tmux/screen)
echo $TERM_PROGRAM
# Should be "Apple_Terminal" or "iTerm.app"

# Check if running in virtualized environment
sysctl -n machdep.cpu.brand_string
# Should show "Apple" processor, not emulated
```

### Common Causes & Fixes

#### Cause 1: SSH Session

**Problem:** Metal requires GUI session with WindowServer access

**Fix:** Run tests from local Terminal.app, not SSH

```bash
# ❌ Don't run from SSH
ssh user@localhost "cargo test"

# ✅ Run from local Terminal.app
cargo test -p adapteros-lora-mlx-ffi --features mlx --lib
```

#### Cause 2: tmux/screen Session

**Problem:** Terminal multiplexers may not inherit Metal entitlements

**Fix:** Exit tmux/screen and run from native terminal

```bash
# Exit tmux/screen
exit

# Run directly in Terminal.app
cargo test -p adapteros-lora-mlx-ffi --features mlx --lib
```

#### Cause 3: Virtualization (Docker/VM)

**Problem:** Containers and VMs don't have Metal device passthrough

**Fix:** Run on native macOS, not in container

```bash
# ❌ Don't run in Docker
docker run rust-test cargo test

# ✅ Run on host macOS
cargo test -p adapteros-lora-mlx-ffi --features mlx --lib
```

#### Cause 4: Code Signing / Entitlements

**Problem:** Rust test binary lacks Metal entitlements

**Fix:** Add entitlements to test binary

Create `metal-entitlements.plist`:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>com.apple.security.app-sandbox</key>
    <false/>
    <key>com.apple.security.device.metal</key>
    <true/>
</dict>
</plist>
```

Sign test binary:

```bash
# Build test binary
cargo test -p adapteros-lora-mlx-ffi --features mlx --lib --no-run

# Find test binary
TEST_BINARY=$(find target/debug/deps -name "adapteros_lora_mlx_ffi-*" -type f -perm +111 | head -1)

# Sign with entitlements
codesign --force --sign - --entitlements metal-entitlements.plist "$TEST_BINARY"

# Run signed binary
"$TEST_BINARY" --test-threads=1
```

#### Cause 5: Cursor/IDE Restrictions

**Problem:** IDE integrated terminals may have restricted permissions

**Fix:** Run from native Terminal.app, not IDE terminal

```bash
# Open native Terminal.app
open -a Terminal.app

# Navigate to project
cd /Users/mln-dev/Dev/adapter-os

# Run tests
cargo test -p adapteros-lora-mlx-ffi --features mlx --lib
```

### Recommended Solution

For immediate testing:

```bash
# 1. Open native Terminal.app (not Cursor, not SSH)
open -a Terminal.app

# 2. Navigate to project
cd /Users/mln-dev/Dev/adapter-os

# 3. Verify Metal access
swift -e 'import Metal; print(MTLCreateSystemDefaultDevice() != nil ? "✅ Metal OK" : "❌ No Metal")'

# 4. If Metal OK, run tests
cargo test -p adapteros-lora-mlx-ffi --features mlx --lib -- --nocapture

# 5. Run specific attention tests
cargo test -p adapteros-lora-mlx-ffi --features mlx attention::tests --nocapture
```

### Debugging Commands

```bash
# Check if running in restricted context
env | grep -E "(SSH|TERM|DISPLAY|XPC)"

# Check Metal framework loading
otool -L target/debug/deps/adapteros_lora_mlx_ffi-* | grep Metal

# Check code signature
codesign -d -vv target/debug/deps/adapteros_lora_mlx_ffi-*

# Trace Metal API calls (requires SIP disabled)
sudo dtruss -t open ./target/debug/deps/adapteros_lora_mlx_ffi-* 2>&1 | grep Metal
```

---

## Migration Guide (Stub → Real)

### Migration Goals

- Replace stub MLX backend with real C++/FFI backend
- Keep CoreML/Metal as fallbacks with deterministic execution intact

### Preconditions

- [ ] MLX installed (`brew install mlx` or manual install)
- [ ] Config updated to enable MLX: `backend = "mlx"` and `mlx.enabled = true`
- [ ] Model downloaded in MLX format

### Migration Steps

#### 1. Enable Real Feature

```bash
# Build with real MLX
make build-mlx

# Or manually
cargo build --features multi-backend,mlx --release

# Verify real build
cargo build -p adapteros-lora-mlx-ffi --features mlx 2>&1 | grep "MLX FFI build: REAL"
```

#### 2. Update Config

Use production config template:

```toml
# configs/mlx-production.toml
[mlx]
enabled = true
model_path = "/data/models/qwen2.5-7b-mlx"
default_backend = "mlx"

[mlx.resilience]
max_consecutive_failures = 5
circuit_breaker_timeout_secs = 300
enable_stub_fallback = false

[mlx.performance]
batch_size = 8
prefetch_adapters = true
enable_kv_cache = true

[mlx.determinism]
use_hkdf_seeding = true
```

#### 3. Validate Backend

```bash
# List backends
curl http://localhost:8080/v1/backends | jq

# Check capabilities
curl http://localhost:8080/v1/backends/capabilities | jq

# Check MLX status
curl http://localhost:8080/v1/backends/mlx/status | jq
```

#### 4. Run Tests

```bash
# Unit + integration tests
make test-mlx

# Or manually
cargo test -p adapteros-lora-mlx-ffi --features mlx

# Determinism tests
cargo test -p adapteros-lora-mlx-ffi --features mlx determinism_tests
```

#### 5. Benchmark

```bash
# Benchmark MLX
make bench-mlx

# Compare to Metal baseline
make bench-metal

# Target: <20% delta
```

#### 6. Rollout

```bash
# Start server with MLX backend
./target/release/aosctl serve \
  --backend mlx \
  --model-path /data/models/qwen2.5-7b-mlx

# Monitor circuit breaker metrics
watch -n 5 'curl -s http://localhost:8080/v1/metrics | grep mlx_backend_circuit_breaker'
```

### Rollback

If issues occur:

```bash
# Switch config
# Edit configs/mlx.toml:
[mlx]
default_backend = "metal"  # or "coreml"

# Reload service
sudo systemctl restart aos-mlx.service

# Keep MLX model assets cached for quick re-enable
```

### Success Signals

- Backend status shows `healthy`, `mode=real`, `deterministic=true` (for RNG operations)
- Inference latency within target window
- No circuit-breaker trips over 24h burn-in
- Memory usage stable

---

## Performance Issues

### Slow Forward Passes

**Diagnosis:**
```bash
# Profile inference
RUST_LOG=trace ./target/release/aosctl infer \
  --model ./models/qwen2.5-7b-mlx \
  --prompt "test" \
  --max-tokens 10

# Check GPU utilization
# Use Activity Monitor or:
ps aux | grep aosctl  # Check CPU %
```

**Solutions:**
```bash
# Enable KV cache
[mlx.performance]
enable_kv_cache = true
cache_warmup_tokens = 512

# Reduce batch size if memory-bound
batch_size = 4

# Check for CPU fallback warnings in logs
grep -i "cpu fallback" logs/aos-mlx.log
```

### High Memory Usage

**Diagnosis:**
```bash
# Monitor memory
watch -n 1 'ps aux | grep aosctl | grep -v grep | awk "{print \$6}"'

# Check MLX memory stats
curl http://localhost:8080/v1/backends/mlx/memory | jq
```

**Solutions:**
```bash
# Reduce batch size in config
[mlx.performance]
batch_size = 4

# Trigger GC more aggressively
[mlx]
gc_threshold_mb = 1000  # Lower threshold

# Use quantized model
python -m mlx_lm.convert \
  --hf-path Qwen/Qwen2.5-7B-Instruct \
  --mlx-path ./models/qwen2.5-7b-mlx \
  --quantize
```

### Adapter Loading Failures

**Diagnosis:**
```bash
# Verify adapter format
ls -la adapter.safetensors

# Check adapter tensor shapes
python -c "
from safetensors import safe_open
with safe_open('adapter.safetensors', framework='numpy') as f:
    for k in f.keys():
        print(f'{k}: {f.get_tensor(k).shape}')
"
```

**Solution:**
```bash
# Check adapter compatibility with model
# Model hidden_size must match adapter dimensions

# Verify adapter naming convention
# Expected: {module_name}.lora_A, {module_name}.lora_B
```

---

## Testing Modes

### Stub CI/Default Mode

**Purpose:** Tests compile/run without Metal (CI-safe)

```bash
# Stub tests (no Metal or MLX runtime needed)
cargo test -p adapteros-lora-mlx-ffi --lib

# Real e2e suites are gated in stub mode
```

### Real MLX Mode

**Purpose:** Full integration tests with real MLX runtime

```bash
# Real MLX tests (requires MLX install + fixtures)
cargo test -p adapteros-lora-mlx-ffi --features "mlx-backend,mlx" -- --include-ignored

# Run e2e/integration tests
cargo test -p adapteros-lora-mlx-ffi --features "mlx-backend,mlx" e2e_workflow_tests

# Run with output
cargo test -p adapteros-lora-mlx-ffi --features mlx -- --nocapture
```

### Focused Testing

```bash
# Specific test suite
cargo test -p adapteros-lora-mlx-ffi --features mlx determinism_tests

# Sequential execution (for debugging)
cargo test -p adapteros-lora-mlx-ffi --features mlx -- --test-threads=1

# With output
cargo test -p adapteros-lora-mlx-ffi --features mlx -- --nocapture
```

---

## Validation Commands

### Backend Status

```bash
# Backend list
curl -s http://localhost:8080/v1/backends | jq

# Capabilities
curl -s http://localhost:8080/v1/backends/capabilities | jq

# MLX specific status
curl -s http://localhost:8080/v1/backends/mlx/status | jq
```

### Health Checks

```bash
# Overall health
curl -s http://localhost:8080/healthz | jq

# Backend health
curl -s http://localhost:8080/healthz/backend | jq

# Memory status
curl -s http://localhost:8080/v1/backends/mlx/memory | jq
```

### Testing & Benchmarks

```bash
# Determinism tests
cargo test -p adapteros-lora-mlx-ffi determinism_tests --features mlx

# Full test suite
make test-mlx

# Benchmarks
make bench-mlx

# Compare backends
make bench-metal
make bench-mlx
```

### Log Signals

**Look for these in logs:**

```bash
# Real build confirmation
grep "MLX FFI build: REAL" build.log

# Backend health warnings
curl http://localhost:8080/v1/backends/mlx/status | jq '.warnings, .errors'

# Determinism violations (should not appear for MLX)
grep "DeterminismViolation" logs/aos-mlx.log
```

---

## Common Error Messages

### "Tokenizer not available"

**Cause:** tokenizer.json missing from model directory

**Solution:**
```bash
# Verify tokenizer exists
ls -la $AOS_MODEL_PATH/tokenizer.json

# Re-download model if missing
python -m mlx_lm.convert \
  --hf-path Qwen/Qwen2.5-7B-Instruct \
  --mlx-path ./models/qwen2.5-7b-mlx
```

### "Model loads but forward fails"

**Cause:** Model path wrong or corrupted files

**Solution:**
```bash
# Verify all files exist
ls -la $AOS_MODEL_PATH/
# Required: config.json, model.safetensors, tokenizer.json

# Check file integrity
md5sum $AOS_MODEL_PATH/*.safetensors

# Re-download if corrupted
```

### "Circuit breaker is open"

**Cause:** 3+ consecutive inference failures

**Solution:**
```bash
# Check health status
curl http://localhost:8080/v1/backends/mlx/status | jq '.health_status'

# Review logs for root cause
journalctl -u aos-mlx.service --since "1h ago" | grep -i error

# Manual recovery if needed (in code)
model.reset_circuit_breaker();
```

---

## Getting Help

### Pre-Support Checklist

Before requesting support:

1. [ ] Check documentation and existing issues
2. [ ] Enable debug logging: `RUST_LOG=debug`
3. [ ] Capture full error output and logs
4. [ ] Run diagnostics: `make verify-mlx-env`
5. [ ] Test with stub build to isolate MLX issues

### Debug Information to Collect

```bash
# System info
system_profiler SPHardwareDataType SPSoftwareDataType

# MLX installation
brew info mlx
ls -la /opt/homebrew/lib/libmlx*
ls -la /opt/homebrew/include/mlx/

# Build info
cargo build -p adapteros-lora-mlx-ffi --features mlx 2>&1 | tee build.log

# Runtime logs
RUST_LOG=debug ./target/release/aosctl serve --backend mlx 2>&1 | tee runtime.log

# Health status
curl http://localhost:8080/v1/backends/mlx/status | jq > backend-status.json
```

### Escalation Path

1. Check [MLX_GUIDE.md](./MLX_GUIDE.md) for usage patterns
2. Review [AGENTS.md](../AGENTS.md) for build commands
3. Search GitHub issues for similar problems
4. Submit new issue with collected debug information

---

## See Also

- [MLX_GUIDE.md](./MLX_GUIDE.md) - Main MLX documentation
- [ADR_MULTI_BACKEND_STRATEGY.md](./ADR_MULTI_BACKEND_STRATEGY.md) - Backend architecture
- [DETERMINISM.md](./DETERMINISM.md) - Determinism and replay details
- [AGENTS.md](../AGENTS.md) - Build & development commands

---

**Maintained by:** James KC Auchterlonie
**Last Updated:** 2025-12-11

MLNavigator Inc 2025
