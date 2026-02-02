# adapteros-lora-mlx-ffi

C FFI bridge to MLX C++ for inference and training.

## Build

Requires MLX headers. On macOS: `brew install ml-explore/mlx/mlx`

## Patterns

- **Inference**: `mlx_forward()` - stateless, takes weights + input
- **Training**: `mlx_value_and_grad()` - returns loss + gradients
- **Memory**: MLX uses unified memory; no explicit GPU transfers needed

## Stub vs Real

Two implementations exist: `mlx_wrapper_stub.cpp` (CI) and `mlx_wrapper.cpp` (real). Feature flag `mlx` controls which links.
