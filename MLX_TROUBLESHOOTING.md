# MLX Troubleshooting

## Build/Link Errors
- **`mlx/` headers not found**: set `MLX_INCLUDE_DIR` (e.g., `/opt/homebrew/include`) and rerun `make verify-mlx-env`.
- **`libmlx.dylib` missing**: set `MLX_LIB_DIR` (e.g., `/opt/homebrew/lib`) or reinstall via Homebrew.
- **Swift/Metal toolchain missing**: install Xcode CLT (`xcode-select --install`).
- **Wrong feature set**: ensure `--features multi-backend,real-mlx` (not stub `mlx-backend` only).

## Runtime Issues
- **Backend reports stub mode**: build with `real-mlx`, clear `target/` if necessary, and rerun `make build-mlx`.
- **Circuit breaker trips**: raise `circuit_breaker_timeout_secs` gradually; check GPU/ANE telemetry.
- **OOM / memory pressure**: lower `max_memory_mb` or use quantized model; ensure swap not used heavily.
- **Latency regression**: compare against Metal baseline; enable HKDF-seeded determinism in configs.

## Validation Commands
- Backend list: `curl -s /v1/backends | jq`
- Capabilities: `curl -s /v1/backends/capabilities | jq`
- MLX status: `curl -s /v1/backends/mlx/status | jq`
- Determinism tests: `cargo test -p adapteros-lora-mlx-ffi determinism_tests --features real-mlx`
- Benchmarks: `make bench-mlx`

## Log Signals
- Look for `MLX FFI build: REAL` in build output.
- Backend health warnings surfaced via `/v1/backends/{name}/status` (`warnings`/`errors` arrays).
- Determinism violations should surface as `AosError::DeterminismViolation`.

## Rollback
- Set `backend = "metal"` or `"coreml"` in config and reload service.
- Keep MLX assets on disk for fast re-enable once issues are resolved.
