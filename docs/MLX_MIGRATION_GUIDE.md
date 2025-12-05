# MLX Migration Guide (Stub → Real)

## Migration Goals
- Replace stub MLX backend with real C++/FFI backend.
- Keep CoreML/Metal as fallbacks with deterministic execution intact.

## Preconditions
- MLX installed (`brew install mlx` or manual install).
- Config updated to enable MLX: `backend = "mlx"` and `mlx.enabled = true`.
- Model downloaded (`./scripts/download_model.sh --format mlx --size 7b --quantized`).

## Migration Steps
1) **Enable real feature**  
   Build with `--features multi-backend,mlx` or run `make build-mlx`.
2) **Update config**  
   Use `configs/mlx-development.toml` (dev) or `configs/mlx-production.toml` (prod).
3) **Validate backend**  
   - List: `curl /v1/backends`  
   - Capabilities: `curl /v1/backends/capabilities`  
   - Status: `curl /v1/backends/mlx/status`
4) **Run tests**  
   `make test-mlx` (unit + integration) and determinism tests if applicable.
5) **Benchmark**  
   `make bench-mlx` and compare to Metal baseline (<20% delta target).
6) **Rollout**  
   Start server with `--backend mlx` (config-driven), monitor circuit breaker metrics.

## Rollback
- Switch config: `backend = "metal"` (or `coreml`) and reload service.
- Keep MLX model assets cached for quick re-enable.

## Success Signals
- Backend status shows `healthy`, `mode=real`, `deterministic=true`.
- Inference latency within target window; no circuit-breaker trips over 24h burn-in.

## Artifacts to Update
- Docs: `docs/MLX_INTEGRATION.md`, `docs/MLX_INSTALLATION_GUIDE.md`, `docs/MLX_TROUBLESHOOTING.md`
- CLI/UX: ensure backend selection guidance is visible (`aosctl backend status`).
