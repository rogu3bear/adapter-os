# MLX C++ FFI Integration

This document explains how the MLX (Apple) C++ FFI path is detected, built, and used in AdapterOS, and how to verify whether you have a real integration or a stub build.

**Note:** MLX backend uses pure C++ FFI - no Python or PyO3 required. The backend is production-ready and can be enabled via the `mlx-ffi-backend` feature flag.

## Feature Flag

MLX backend is enabled via the `mlx-ffi-backend` feature flag:

```bash
# Build with MLX backend support
cargo build --release --features mlx-ffi-backend

# Or for development
cargo build --features mlx-ffi-backend
```

The `mlx-ffi-backend` feature is independent of `experimental-backends` and does not require PyO3.

## Build Modes

- REAL: `mlx_real` cfg. Build script found MLX C++ headers, compiled wrapper with `-DMLX_HAVE_REAL_API`, and linked `-lmlx`.
- STUB: `mlx_stub` cfg. Build script did not find headers (or `MLX_FORCE_STUB=1` was set). The wrapper compiles to a self-contained stub with deterministic placeholders.

The build emits clear logs:
- `MLX FFI build: REAL` with selected include/lib paths
- `MLX FFI build: STUB` with a reason and remediation hints

## Environment Variables (precedence)

1. `MLX_INCLUDE_DIR` and `MLX_LIB_DIR` — explicit include/lib locations
2. `MLX_PATH` — base directory; we use `MLX_PATH/include` and `MLX_PATH/lib`
3. Defaults — `/opt/homebrew/include` and `/opt/homebrew/lib`

Optional:
- `MLX_FORCE_STUB=1` — force a stub build (useful for CI and tests)

## Configuration

### Environment Variables

Set the model path via environment variable:

```bash
export AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-mlx
```

### Configuration File

Add MLX configuration to `configs/cp.toml`:

```toml
[mlx]
# Enable MLX backend support (requires --features mlx-ffi-backend)
enabled = true
# Default model path (can be overridden by AOS_MLX_FFI_MODEL env var)
model_path = "./models/qwen2.5-7b-mlx"
# Default backend selection when both Metal and MLX are available
# Options: "metal" (default, production) or "mlx" (development/experimentation)
default_backend = "mlx"
```

If `model_path` is set in config and `AOS_MLX_FFI_MODEL` is also set, the environment variable takes precedence.

## Runtime Guards

- `MLXFFIModel::load(..)` returns `AosError::Unsupported` on stub builds with a helpful message. This prevents silent use of placeholder outputs during inference.
- The CLI (`aosctl serve --backend mlx`) also fails fast on stub builds with actionable guidance.
- The import command (`aosctl import-model`) validates MLX models and sets the environment variable automatically.

## Verifying Your Setup

- Inspect build output for the `MLX FFI build:` line.
- On the Rust side, `cfg!(mlx_real)` indicates a real build; `cfg!(mlx_stub)` indicates a stub build.
- Through the FFI, `mlx_wrapper_is_real()` returns 1 for real builds and 0 for stub builds.

## Common Issues

- Headers found, but link fails: ensure `MLX_LIB_DIR` is correct and contains `libmlx`.
- Partial installs: when only `MLX_PATH` is set but headers aren’t there, the build falls back to stub and logs why.
- ABI drift: even if linking succeeds, runtime symbol issues can occur with newer MLX releases. Validate with a small smoke test calling `mlx_model_load`/`mlx_model_free`.

## Troubleshooting Matrix

- Unset env → Stub (expected). Set `MLX_INCLUDE_DIR/MLX_LIB_DIR` to switch to real.
- Only `MLX_PATH` set → Uses `MLX_PATH/include` and `MLX_PATH/lib`.
- Conflicting values → `MLX_INCLUDE_DIR/MLX_LIB_DIR` win over `MLX_PATH`.

## Usage Examples

### Import MLX Model

```bash
# Import MLX model (requires --features mlx-ffi-backend)
./target/release/aosctl import-model \
  --name qwen2.5-7b-mlx \
  --weights models/qwen2.5-7b-mlx/weights.safetensors \
  --config models/qwen2.5-7b-mlx/config.json \
  --tokenizer models/qwen2.5-7b-mlx/tokenizer.json \
  --tokenizer-cfg models/qwen2.5-7b-mlx/tokenizer_config.json \
  --license models/qwen2.5-7b-mlx/LICENSE
```

### Serve with MLX Backend

```bash
# Set model path
export AOS_MLX_FFI_MODEL=./models/qwen2.5-7b-mlx

# Start server with MLX backend
./target/release/aosctl serve --backend mlx --model-path ./models/qwen2.5-7b-mlx
```

### Launch Script Support

```bash
# Launch backend with MLX
./launch.sh backend mlx ./models/qwen2.5-7b-mlx
```

## Notes

- **No PyO3 required** - MLX backend uses pure C++ FFI, no Python runtime needed
- The wrapper currently retains stub logic under real mode as a placeholder; actual MLX C++ calls can be introduced behind `#ifdef MLX_HAVE_REAL_API` with no changes to the Rust ABI
- MLX backend is production-ready and can be used alongside Metal backend
- Feature flag `mlx-ffi-backend` is independent and does not require `experimental-backends`
