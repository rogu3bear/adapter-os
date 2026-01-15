# adapterOS Metal Kernel Backend

High-performance, native Rust inference engine for production deployments on Apple Silicon.

## Features

*   **Native Metal Performance:** Direct integration with Metal Performance Shaders (MPS) and custom kernels for maximum throughput.
*   **Deterministic Execution:** Bit-exact reproducibility guaranteed via HKDF seeding and strict floating-point handling (no fast-math).
*   **Dynamic Model Support:** Runtime configuration for arbitrary transformer architectures (e.g., Qwen2.5-7B, Llama-3-8B) without recompilation.
*   **Fused Kernels:**
    *   **Fused MLP:** SwiGLU activation + LoRA injection + Bias in a single kernel pass.
    *   **Fused QKV:** Grouped Query Attention (GQA) + RoPE + LoRA injection.
    *   **Flash Attention:** Memory-efficient attention computation.
*   **K-Sparse Routing:** Efficiently handles Mixtures-of-Experts (MoE) with thousands of adapters using a `RouterRing` buffer.
*   **Zero-Copy I/O:** Shared memory buffers between CPU and GPU for low-latency token streaming.

## Architecture

This crate implements the "Kernel" layer of adapterOS. It provides:
1.  **Memory Management:** `GpuMemoryPool` for efficient buffer reuse and `VramTracker` for strict accounting.
2.  **Execution Engine:** `MetalKernels` struct that manages the command queue and kernel pipelines.
3.  **Safety:** GPU buffer fingerprinting to prevent memory corruption during adapter hot-swaps.

## Usage

```rust
use adapteros_lora_kernel_mtl::{MetalKernels, GqaConfig};

// 1. Initialize
let mut kernels = MetalKernels::new()?;

// 2. Configure (Dynamic Dimensions)
let config = GqaConfig::try_from_params(
    28,             // num_attention_heads
    4,              // num_key_value_heads
    3584,           // hidden_size
    1_000_000.0     // rope_theta
)?;
kernels.set_gqa_config(config);

// 3. Load Model Weights (SafeTensors)
kernels.load(&model_bytes)?;

// 4. Load Adapter (Hot-Swap)
kernels.load_adapter(adapter_id, &adapter_bytes)?;

// 5. Run Inference Step
kernels.run_step(&router_ring, &mut io_buffers)?;
```

## Determinism

This crate enforces determinism by:
*   Verifying the `metallib` hash against a signed manifest at runtime.
*   Disabling `-ffast-math` in kernel compilation.
*   Using deterministic reductions in kernels.

## Development

Run the standalone harness to verify kernel stability:
```bash
cargo run -p adapteros-lora-kernel-mtl --bin test_metal_engine
```
