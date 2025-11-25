# MLX Backend Integration Guide

Status: **Production-ready target** (real MLX), CoreML + Metal remain primary/secondary.

## Quick Start
- Install MLX: `brew install mlx` (or set `MLX_PATH`, `MLX_INCLUDE_DIR`, `MLX_LIB_DIR`).
- Build real backend: `make build-mlx` (runs env checks, uses features `multi-backend,real-mlx`).
- Run tests: `make test-mlx` (unit + integration).
- Benchmarks: `make bench-mlx`.
- Model: `./scripts/download_model.sh --format mlx --size 7b --quantized`; set `AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-mlx`.

## Required Environment
- macOS with Apple Silicon.
- Xcode CLT (for Swift/Metal toolchains).
- MLX C++ library headers and libs discoverable via:
  - `MLX_PATH` (prefix), `MLX_INCLUDE_DIR`, `MLX_LIB_DIR`.
  - Auto-detected by `scripts/build-mlx.sh` and `make verify-mlx-env`.

## Build Profiles
- `production-macos`: `coreml-backend + metal-backend + multi-backend + real-mlx`.
- `dev-macos-real`: Same as production profile, with development defaults.

## Commands
- `scripts/build-mlx.sh [--tests|--bench]` – auto-detect MLX, build, and optionally test/bench.
- `make build-mlx` – build only.
- `make test-mlx` – run MLX tests.
- `make bench-mlx` – run MLX benchmarks.

## Configuration
- Development: `configs/mlx-development.toml`
- Production: `configs/mlx-production.toml`
- Key knobs: `model_path`, `precision`, `max_memory_mb`, `circuit_breaker_timeout_secs`.

## API Support
- Inference: `backend` parameter (`auto|mlx|coreml|metal`) now in request schema.
- Endpoints: `/v1/backends`, `/v1/backends/capabilities`, `/v1/backends/{name}/status`.

## Success Criteria
- Real MLX builds (`real-mlx` flag) succeed.
- Tests/benchmarks pass within 20% of Metal baseline.
- Backend status endpoints report `healthy` with deterministic flag.
