# MLX vs CoreML Guide

## When to Choose CoreML
- Production default on Apple Silicon with ANE acceleration.
- Highest determinism guarantees; attestation available.
- Works without MLX install; minimal system dependencies.

## When to Choose MLX
- Training or research workloads that benefit from MLX primitives.
- Real-time routing experiments with Q-sparse gates and HKDF-seeded RNG.
- When GPU memory >16GB and ANE is saturated or unavailable.

## Performance Expectations
- Target: MLX within 20% of Metal baseline for inference; better on training-heavy paths.
- Use `make bench-mlx` and compare to CoreML/Metal runs; watch `latency_ms` and tokens/sec.

## Configuration
- CoreML: `backend = "coreml"` (default), works with ANE-first routing.
- MLX: `backend = "mlx"` with `mlx` build; configs in `configs/mlx-*.toml`.
- Metal fallback: `backend = "metal"` for non-ANE hardware.

## Operational Notes
- All backends obey deterministic execution; MLX requires HKDF seeding (already wired).
- Circuit breaker enabled for MLX via `circuit_breaker_timeout_secs`.
- Health endpoints: `/v1/backends` for availability, `/v1/backends/{name}/status` for details.

## Recommended Backends by Scenario
- Low-latency production API: **CoreML**
- Mixed inference/training lab: **MLX** (real) with CoreML as fallback
- Legacy GPUs / no ANE: **Metal**
