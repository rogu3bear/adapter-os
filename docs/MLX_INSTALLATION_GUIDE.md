# MLX Installation Guide (Apple Silicon)

## 1) Install MLX
- Homebrew (recommended): `brew install mlx`
- Verify artifacts:
  - Headers: `ls /opt/homebrew/include/mlx`
  - Library: `ls /opt/homebrew/lib/libmlx.dylib`

## 2) Export Paths
```bash
export MLX_PATH=/opt/homebrew
export MLX_INCLUDE_DIR=$MLX_PATH/include
export MLX_LIB_DIR=$MLX_PATH/lib
```
`make verify-mlx-env` validates the paths; `scripts/build-mlx.sh` auto-detects Homebrew if unset.

## 3) Build Real Backend
- Build: `make build-mlx` (uses features `multi-backend,real-mlx`).
- Scripted: `scripts/build-mlx.sh --tests` (build + tests).
- Expected log line: `MLX FFI build: REAL`.

## 4) Download Test Model
```bash
./scripts/download_model.sh --format mlx --size 7b --quantized
export AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-mlx
```

## 5) Validate
- Unit/Integration: `make test-mlx`
- Benchmarks: `make bench-mlx`
- Determinism: `cargo test -p adapteros-lora-mlx-ffi determinism_tests --features real-mlx`

## 6) Common Issues
- **Headers not found**: set `MLX_INCLUDE_DIR` explicitly; rerun `make verify-mlx-env`.
- **libmlx missing**: ensure `MLX_LIB_DIR` points to lib path; reinstall `brew reinstall mlx`.
- **Swift/Metal toolchain**: install Xcode Command Line Tools (`xcode-select --install`).
- **Memory pressure**: lower `max_memory_mb` in `configs/mlx-development.toml`.

## References
- Config templates: `configs/mlx-development.toml`, `configs/mlx-production.toml`
- Integration overview: `MLX_INTEGRATION.md`
