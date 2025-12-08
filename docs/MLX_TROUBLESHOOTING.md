# MLX Troubleshooting

## Build/Link Errors
- **`mlx/` headers not found**: set `MLX_INCLUDE_DIR` (e.g., `/opt/homebrew/include`) and rerun `make verify-mlx-env`.
- **`libmlx.dylib` missing**: set `MLX_LIB_DIR` (e.g., `/opt/homebrew/lib`) or reinstall via Homebrew.
- **Swift/Metal toolchain missing**: install Xcode CLT (`xcode-select --install`).
- **Wrong feature set**: ensure `--features multi-backend,mlx` (not stub `mlx-backend` only).

## Runtime Issues
- **Backend reports stub mode**: build with `mlx`, clear `target/` if necessary, and rerun `make build-mlx`.
- **Circuit breaker trips**: raise `circuit_breaker_timeout_secs` gradually; check GPU/ANE telemetry.
- **OOM / memory pressure**: lower `max_memory_mb` or use quantized model; ensure swap not used heavily.
- **Latency regression**: compare against Metal baseline; enable HKDF-seeded determinism in configs.
- **Metal device not found** (`NSRangeException` crash): see [MLX_METAL_DEVICE_ACCESS.md](./MLX_METAL_DEVICE_ACCESS.md) for environmental fixes.
- **Model path validation fails**: confirm `AOS_MODEL_PATH` points to an MLX directory containing `config.json`.
- **Determinism reported as false**: ensure the worker passes the manifest hash (`create_backend_with_model_and_hash`) and build with `--features "mlx-backend,mlx"`; stub builds always report non-deterministic.

## Metal Device Access Issues

**Symptom:** Tests crash with `NSRangeException` or `MTLCreateSystemDefaultDevice()` returns nil

**Diagnosis:**
```bash
# Quick test
swift -e 'import Metal; print(MTLCreateSystemDefaultDevice() != nil ? "✅ Metal OK" : "❌ No Metal")'

# Full verification
./scripts/verify_metal_access.sh
```

**Common Causes:**
1. Running in SSH session (Metal requires GUI session)
2. Running in tmux/screen (may not inherit entitlements)
3. Running in restricted IDE terminal (check permissions)
4. Running in Docker/VM (no Metal passthrough)

**Fix:** Run tests from native Terminal.app or properly-configured IDE terminal

**Complete guide:** [MLX_METAL_DEVICE_ACCESS.md](./MLX_METAL_DEVICE_ACCESS.md)

## Validation Commands
- Backend list: `curl -s /v1/backends | jq`
- Capabilities: `curl -s /v1/backends/capabilities | jq`
- MLX status: `curl -s /v1/backends/mlx/status | jq`
- Determinism tests: `cargo test -p adapteros-lora-mlx-ffi determinism_tests --features mlx`
- Benchmarks: `make bench-mlx`

## Testing Modes
- Stub CI/default: `cargo test -p adapteros-lora-mlx-ffi` (no MLX runtime needed; real e2e suites are gated).
- Real MLX: `cargo test -p adapteros-lora-mlx-ffi --features "mlx-backend,mlx" -- --include-ignored` (runs e2e/integration; requires MLX install + fixtures).
- Focused e2e: `cargo test -p adapteros-lora-mlx-ffi --features "mlx-backend,mlx" e2e_workflow_tests`

## Log Signals
- Look for `MLX FFI build: REAL` in build output.
- Backend health warnings surfaced via `/v1/backends/{name}/status` (`warnings`/`errors` arrays).
- Determinism violations should surface as `AosError::DeterminismViolation`.

## Rollback
- Set `backend = "metal"` or `"coreml"` in config and reload service.
- Keep MLX assets on disk for fast re-enable once issues are resolved.

MLNavigator Inc Monday Dec 8, 2025.
