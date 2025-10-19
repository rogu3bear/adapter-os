# MLX C++ FFI Integration

This document explains how the MLX (Apple) C++ FFI path is detected, built, and used in AdapterOS, and how to verify whether you have a real integration or a stub build.

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

## Runtime Guards

- `MLXFFIModel::load(..)` returns `AosError::Unsupported` on stub builds with a helpful message. This prevents silent use of placeholder outputs during inference.
- The CLI (`aosctl serve --backend mlx`) also fails fast on stub builds with actionable guidance.

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

## Notes

- No PyO3 is required for this integration.
- The wrapper currently retains stub logic under real mode as a placeholder; actual MLX C++ calls can be introduced behind `#ifdef MLX_HAVE_REAL_API` with no changes to the Rust ABI.
