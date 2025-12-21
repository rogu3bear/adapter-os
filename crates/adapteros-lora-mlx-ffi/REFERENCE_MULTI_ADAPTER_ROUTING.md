# K-Sparse Multi-Adapter Routing for MLX Backend

**Status:** Implemented
**Date:** 2025-11-19
**Author:** AdapterOS team

## Overview

This document describes the K-sparse multi-adapter routing implementation for the MLX FFI backend. The implementation enables simultaneous application of multiple LoRA adapters (up to K=8) with Q15 quantized routing gates.

## Architecture

### Key Components

1. **C++ FFI Interface** (`wrapper.h`)
   - Declaration of `mlx_multi_lora_forward()` function
   - C-compatible interface for Rust FFI bindings

2. **Real MLX Implementation** (`mlx_cpp_wrapper_real.cpp`)
   - Efficient batched operations using MLX arrays
   - Q15 gate dequantization
   - Weighted combination of adapter outputs

3. **Stub Implementation** (`mlx_cpp_wrapper.cpp`)
   - Testing and development fallback
   - Simplified logic for non-MLX environments

## Function Signature

```c
mlx_array_t* mlx_multi_lora_forward(
    mlx_array_t* input,           // Input tensor [batch, seq_len, hidden_dim]
    mlx_array_t** lora_a_list,    // Array of LoRA A matrices [hidden_dim, rank]
    mlx_array_t** lora_b_list,    // Array of LoRA B matrices [rank, hidden_dim]
    int num_adapters,             // Number of adapters (K, max 8)
    const uint16_t* gates_q15,    // Q15 quantized gates [0-32767]
    float alpha,                  // LoRA scaling factor
    float rank                    // LoRA rank dimension
);
```

## Algorithm

### Step 1: Q15 Gate Dequantization

Q15 format uses 15 bits for fractional precision with symmetric range [-1.0, 1.0]:

```
gate_f32 = gate_u16 / 32767.0
```

**Example:**
- `gate_u16 = 32767` → `gate_f32 = 1.0` (full activation)
- `gate_u16 = 16384` → `gate_f32 = 0.5` (half activation)
- `gate_u16 = 0` → `gate_f32 = 0.0` (no activation)

### Step 2: LoRA Forward Pass

For each adapter `i` with gate weight `g_i`:

```
intermediate_i = input @ A_i              // [batch, seq_len, rank]
lora_output_i = intermediate_i @ B_i      // [batch, seq_len, hidden_dim]
scaled_i = lora_output_i * (g_i * alpha / rank)
```

### Step 3: Weighted Combination

Accumulate all adapter contributions:

```
result = input + Σ(scaled_i)
```

Full formula:
```
output = input + Σ(i=1 to K) [ g_i * (alpha/rank) * (input @ A_i @ B_i) ]
```

## MLX Implementation Details

### Efficient Batched Operations

The MLX implementation uses:
- `mx::matmul()` for matrix multiplication
- `mx::multiply()` for element-wise scaling
- `mx::add()` for accumulation
- `mx::eval()` to force immediate evaluation (MLX uses lazy evaluation)

### Memory Management

- Each adapter forward pass creates temporary intermediate arrays
- MLX's unified memory management handles allocation
- `mx::eval()` ensures results are computed before returning

### Performance Characteristics

- **Batch Processing:** All adapters processed sequentially in a single pass
- **Skip Logic:** Zero-weight adapters (gate < 1e-6) are skipped for efficiency
- **Memory Overhead:** O(K * rank * hidden_dim) for intermediate results

## Usage Example (Rust FFI)

```rust
use adapteros_lora_mlx_ffi::*;

// Setup: Load base model and adapters
let model = MLXFFIModel::load("/path/to/model")?;
let adapters = vec![
    load_lora_adapter("adapter1")?,
    load_lora_adapter("adapter2")?,
    load_lora_adapter("adapter3")?,
];

// Router selects top-K adapters with Q15 gates
let router_result = router.select_adapters(&prompt, 3)?;
let gates_q15: Vec<u16> = router_result.gates; // [32767, 24576, 16384]

// Prepare LoRA matrices
let mut lora_a_ptrs: Vec<*mut mlx_array_t> = Vec::new();
let mut lora_b_ptrs: Vec<*mut mlx_array_t> = Vec::new();

for adapter in &adapters {
    let (a, b) = adapter.get_lora_matrices("q_proj")?;
    lora_a_ptrs.push(a.as_ffi_ptr());
    lora_b_ptrs.push(b.as_ffi_ptr());
}

// Run multi-adapter forward pass
let input_array = create_mlx_array_from_tokens(&input_tokens)?;
let output = unsafe {
    mlx_multi_lora_forward(
        input_array,
        lora_a_ptrs.as_ptr(),
        lora_b_ptrs.as_ptr(),
        3,                  // num_adapters
        gates_q15.as_ptr(), // Q15 gates
        16.0,               // alpha
        8.0                 // rank
    )
};

// Process output
let logits = extract_logits_from_mlx_array(output)?;
```

## Integration with Router

### Router Decision Flow

1. **Feature Extraction:** Router analyzes prompt features
2. **Scoring:** Compute scores for all available adapters
3. **Top-K Selection:** Select K adapters with highest scores
4. **Gate Quantization:** Convert float gates to Q15 format
5. **FFI Call:** Invoke `mlx_multi_lora_forward()`

### Q15 Gate Quantization

```rust
fn quantize_gates_to_q15(gates: &[f32]) -> Vec<u16> {
    gates.iter().map(|&g| {
        let clamped = g.max(0.0).min(1.0);
        (clamped * 32767.0).round() as u16
    }).collect()
}
```

## Error Handling

The function returns `nullptr` on error. Check with `mlx_get_last_error()`:

```rust
let result = unsafe { mlx_multi_lora_forward(...) };
if result.is_null() {
    let error_msg = unsafe {
        std::ffi::CStr::from_ptr(mlx_get_last_error())
            .to_string_lossy()
            .to_string()
    };
    return Err(AosError::Mlx(error_msg));
}
```

### Common Errors

- **"Invalid parameters":** Null pointers or num_adapters <= 0
- **"Number of adapters exceeds maximum (K=8)":** Too many adapters
- **"Multi-adapter LoRA forward failed":** MLX exception during computation

## Performance Considerations

### K Value Selection

- **K=1:** Fastest, single-adapter inference
- **K=2-4:** Good balance of quality and speed
- **K=8:** Maximum diversity, 8x compute overhead

### Optimization Tips

1. **Gate Thresholding:** Skip adapters with gates < 0.01 (saves ~30% compute)
2. **Batch Size:** Larger batches amortize overhead better
3. **Rank Selection:** Lower rank (r=4-8) is faster than high rank (r=16-32)

## Testing

### Unit Tests

```bash
# Test stub implementation
cargo test --package adapteros-lora-mlx-ffi test_multi_lora_stub

# Test with real MLX (requires MLX installed)
cargo test --package adapteros-lora-mlx-ffi test_multi_lora_real --features mlx-real
```

### Validation

Verify correctness by comparing with reference CPU implementation:

```rust
#[test]
fn test_multi_lora_equivalence() {
    let input = create_test_input();
    let adapters = create_test_adapters(3);
    let gates_q15 = vec![32767, 24576, 16384];

    // MLX implementation
    let mlx_output = mlx_multi_lora_forward(...);

    // CPU reference
    let cpu_output = cpu_multi_lora_forward(...);

    // Allow small floating-point differences
    assert_tensors_close(&mlx_output, &cpu_output, 1e-5);
}
```

## Future Enhancements

1. **Adaptive K Selection:** Dynamically adjust K based on prompt complexity
2. **Gate Sparsity:** Exploit gate sparsity with conditional execution
3. **Fused Kernels:** Combine gate application with LoRA forward in single kernel
4. **Quantized Adapters:** Support INT8 LoRA weights for memory efficiency

## References

- [AdapterOS Router Documentation](../../adapteros-lora-router/README.md)
- [Q15 Fixed-Point Format](https://en.wikipedia.org/wiki/Q_(number_format))
- [LoRA Paper](https://arxiv.org/abs/2106.09685)
- [K-Sparse Routing](https://arxiv.org/abs/2101.03961)
- [MLX Documentation](https://ml-explore.github.io/mlx/)

## Citation

```bibtex
@software{adapteros_mlx_routing,
  title = {K-Sparse Multi-Adapter Routing for MLX Backend},
  author = {AdapterOS Team},
  year = {2025},
  url = {https://github.com/adapteros/adapteros}
}
```

---

**Implementation Status:** Complete
**Test Coverage:** Stub implementation validated, real MLX pending integration tests
**Performance:** Expected 2-8x overhead vs single adapter (linear in K)
