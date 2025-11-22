# AdapterOS Performance Benchmarks

Comprehensive performance benchmarks for Metal kernels and cryptographic operations (BLAKE3/HKDF) in AdapterOS.

## Overview

This guide covers two main benchmark suites:
1. **Metal Kernel Benchmarks** - GPU kernel performance analysis
2. **BLAKE3/HKDF Benchmarks** - Cryptographic operation performance

## Metal Kernel Benchmarks

Location: `crates/adapteros-lora-kernel-mtl/benches/kernel_benchmarks.rs`

### Benchmarks Included

#### 1. Matrix Multiplication (`matrix_mult`)
Tests GPU compute kernel throughput across different hidden dimensions:
- Small hidden dimensions (768x768) - mobile models
- Medium dimensions (2048x2048) - typical transformer models
- Large dimensions (4096x4096) - Qwen2.5-7B scale
- Extra-large dimensions (6144x6144) - larger models

**What it measures:** GPU compute throughput (GB/s) and kernel efficiency

**Expected baseline:** 200-500 GB/s on modern Apple Silicon (M1/M2/M3)

#### 2. MLP Kernel Scaling (`mlp_scaling`)
Benchmarks the Fused MLP kernel with varying batch sizes:
- Batch sizes: 1, 4, 8, 16, 32, 64, 128
- Fixed hidden size: 4096
- Tests GPU occupancy and memory access patterns

**What it measures:** Efficiency of the SwiGLU activation (gate ⊙ up) @ down pattern

**Expected trend:** Throughput increases with batch size until GPU saturation (~32-64 tokens)

#### 3. QKV Kernel (`qkv_kernel`)
Tests Grouped Query Attention (GQA) implementation:
- Different attention head configurations:
  - GQA 4:1 (32 heads, 4 KV heads)
  - GQA 2:1 (32 heads, 8 KV heads)
  - Full MHA (32 heads, 32 KV heads)
- Varying sequence lengths (512, 2048)

**What it measures:**
- Q, K, V projection overhead
- GQA grouping efficiency
- RoPE (Rotary Position Embedding) cost
- KV cache impact on performance

**Expected:** GQA should be 1.5-2x faster than full MHA due to reduced KV operations

#### 4. Flash Attention (`flash_attention`)
Measures attention computation efficiency:
- Sequence lengths: 128, 512, 1024, 2048, 4096
- Tests the quadratic scaling behavior of attention

**What it measures:**
- Block-wise attention efficiency
- Memory I/O overhead
- Attention scaling (typically O(n²))

**Expected:** Linear throughput with block size, quadratic growth with sequence length

#### 5. Memory Bandwidth (`memory_bandwidth`)
Tests GPU memory transfer performance:
- Buffer sizes: 1MB, 10MB, 100MB, 512MB
- Measures sustained bandwidth and latency

**What it measures:**
- Memory-bound operation efficiency
- Round-trip CPU ↔ GPU transfer cost
- Cache behavior under load

**Expected baseline:** 200-300 GB/s on Apple Silicon (UMA architecture)

#### 6. Kernel Launch Overhead (`launch_overhead`)
Isolates the fixed cost of launching kernels:
- Minimal work configurations to measure just the launch overhead
- Batch sizes and hidden dimensions vary

**What it measures:**
- Command buffer creation overhead
- Pipeline state setup cost
- GPU synchronization overhead

**Expected overhead:** 50-200 microseconds per kernel launch

#### 7. LoRA Fusion (`lora_fusion`)
Tests K-sparse adapter fusion overhead:
- K values: 1, 2, 4, 8 (number of active adapters)
- Rank=16, Hidden=4096

**What it measures:**
- Per-adapter gate scaling cost
- Multi-adapter scaling efficiency
- Overhead of selective adapter application

**Expected:** Near-linear scaling with K (negligible overhead)

#### 8. Full Inference Pipeline (`full_pipeline`)
End-to-end performance: embedding → attention → MLP → logits
- Single token inference (4 adapters)
- Small batch (4 tokens)
- Medium batch (16 tokens)
- Large batch (32 tokens, 8 adapters)

**What it measures:**
- Real-world inference performance
- Combined overhead of all components
- Actual throughput in tokens/second

#### 9. Adaptive Batching (`adaptive_batching`)
Tests performance across entire batch size spectrum (1-256):
- Identifies optimal GPU utilization point
- Helps determine batching strategy

**What it measures:**
- GPU occupancy vs batch size trade-off
- Sweet spot for throughput

### Running Metal Benchmarks

```bash
# Run all Metal kernel benchmarks
cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks

# Run specific benchmark group
cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks -- matrix_mult

# Run with verbose output
cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks -- --verbose

# Run with custom sample size (higher = more accurate but slower)
cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks -- --sample-size 100

# Save results to file for comparison
cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks -- --save-baseline main
cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks -- --baseline main
```

## BLAKE3/HKDF Benchmarks

Location: `crates/adapteros-core/benches/hash_benchmarks.rs`

### Benchmarks Included

#### 1. BLAKE3 Various File Sizes (`blake3_sizes`)
Tests hashing performance across expected file size range:
- 1KB - small config files
- 10KB - typical model adapters
- 100KB - smaller models
- 1MB - medium models
- 10MB - large adapters
- 100MB - large model weights

**What it measures:**
- Throughput at different scales (GB/s)
- Constant overhead cost
- Linearity of throughput

**Expected baseline:**
- Small files (< 1MB): 1-5 GB/s
- Large files (> 10MB): 20-40 GB/s
- Apple Silicon: 20-60 GB/s (single-threaded)

#### 2. BLAKE3 Hash Only (`blake3_hash_only`)
Isolates hashing cost from allocation overhead:
- Buffer sizes: 64B, 256B, 1KB, 4KB, 16KB, 64KB, 256KB, 1MB

**What it measures:**
- Pure hash computation performance
- Memory bandwidth utilization
- CPU cache efficiency

#### 3. HKDF Seed Derivation (`hkdf_derivation`)
Measures deterministic seed derivation performance:
- Basic derivation (single label)
- Typed derivation (with manifest context)
- Full entropy derivation (manifest + adapter_dir + worker_id + nonce)

**What it measures:**
- Per-derivation overhead
- Context incorporation cost
- HKDF-SHA256 efficiency

**Expected overhead:** 10-50 microseconds per derivation

#### 4. HKDF Batch Derivation (`hkdf_batch`)
Tests cost of deriving multiple seeds at once:
- Batch sizes: 1, 4, 8, 16, 32, 64

**What it measures:**
- Batching efficiency
- Amortized derivation cost

**Expected:** Near-constant per-seed cost even with batching

#### 5. Multi-Hash Operations (`blake3_multi`)
Tests hashing multiple byte slices without concatenation:
- Configurations: 2x512B, 4x256B, 8x128B, 16x64B, 32x32B

**What it measures:**
- Efficiency of hash_multi vs concatenate-then-hash
- Zero-copy operation benefit

**Expected:** 10-20% faster than concatenation approach

#### 6. BLAKE3 vs SHA256 (`blake3_vs_sha256`)
Direct performance comparison:
- File sizes: 1KB, 10KB, 100KB, 1MB

**What it measures:**
- BLAKE3 advantage over SHA256
- Parallelization benefit
- Modern instruction utilization

**Expected:** BLAKE3 is 2-4x faster on most platforms

#### 7. Zero-Copy Efficiency (`zero_copy`)
Measures benefit of avoiding data copies:
- Compares concat-then-hash vs hash_multi (zero-copy)
- 1MB total data, split 4 ways

**What it measures:**
- Memory allocation overhead
- Copy operation cost
- Zero-copy benefit percentage

**Expected:** 10-30% improvement with zero-copy pattern

#### 8. HKDF Entropy Isolation (`hkdf_isolation`)
Verifies entropy isolation is efficient:
- Manifest variation
- Worker ID variation
- Nonce variation

**What it measures:**
- Overhead of context incorporation
- Determinism property cost

#### 9. Hash File Operation (`hash_file`)
Tests reading and hashing from disk:
- File sizes: 1KB, 10KB, 100KB

**What it measures:**
- File I/O overhead
- Real-world adapter registration cost

**Expected:** I/O bound for files < 1MB

#### 10. Hex Roundtrip (`hex_roundtrip`)
Tests serialization/deserialization:
- to_hex() encoding
- from_hex() decoding
- Full roundtrip

**What it measures:**
- String conversion overhead
- Database/API serialization cost

**Expected:** < 1 microsecond per operation

#### 11. Hash Serialization (`hash_serialization`)
Tests JSON serialization performance:
- serde_json serialization
- serde_json deserialization
- Full roundtrip

**What it measures:**
- JSON encoding overhead
- Database field conversion cost
- API response building time

### Running BLAKE3/HKDF Benchmarks

```bash
# Run all hash benchmarks
cargo bench -p adapteros-core --bench hash_benchmarks

# Run specific benchmark group
cargo bench -p adapteros-core --bench hash_benchmarks -- blake3_sizes

# Run with verbose output
cargo bench -p adapteros-core --bench hash_benchmarks -- --verbose

# Compare with baseline
cargo bench -p adapteros-core --bench hash_benchmarks -- --save-baseline main
cargo bench -p adapteros-core --bench hash_benchmarks -- --baseline main

# Run single benchmark
cargo bench -p adapteros-core --bench hash_benchmarks -- hkdf_derivation
```

## Analyzing Results

### Criterion Output

Criterion generates detailed output including:
- Mean execution time
- Standard deviation
- 95% confidence interval
- Throughput (elements/sec or bytes/sec)
- Comparison with baseline (if specified)

Example output:
```
blake3_sizes/file_size/1KB
                        time:   [1.2341 ms 1.2389 ms 1.2445 ms]
                        thrpt:  [806.39 MB/s 807.21 MB/s 809.04 MB/s]
```

### Baseline Comparison

Save and compare runs to track performance improvements:
```bash
# Save current performance as baseline
cargo bench -p adapteros-core --bench hash_benchmarks -- --save-baseline current_optimization

# Later, compare new results against baseline
cargo bench -p adapteros-core --bench hash_benchmarks -- --baseline current_optimization

# View baseline results
ls target/criterion/blake3_sizes/file_size/1KB/
```

### Performance Expectations

#### Metal Kernels
| Benchmark | Size | Expected | Notes |
|-----------|------|----------|-------|
| Matrix Mult | 4096x4096 | 100-300 GB/s | Compute-bound |
| MLP Scaling | Batch 32 | 250+ GB/s | Good occupancy |
| QKV Kernel | GQA 4:1 | 1-5 ms/token | Grouped attention |
| Flash Attention | Seq 1024 | 2-10 ms | Quadratic scaling |
| Memory BW | 100MB | 200-300 GB/s | Sustained |
| Launch Overhead | Minimal | 50-200 μs | Fixed cost |
| LoRA Fusion | K=4 | <100 μs | Negligible |
| Full Pipeline | Batch 16 | 10-50 ms | Real-world |

#### BLAKE3/HKDF
| Benchmark | Size | Expected | Notes |
|-----------|------|----------|-------|
| BLAKE3 | 1MB | 15-30 GB/s | Hash throughput |
| BLAKE3 | 100MB | 30-60 GB/s | Sustained (memory-bound) |
| HKDF Derivation | Single | 10-50 μs | Seed generation |
| HKDF Batch | 32 seeds | 300-1500 μs | Amortized |
| Zero-Copy | 1MB | 10-20% gain | Avoid copies |
| BLAKE3 vs SHA256 | 1MB | 2-4x faster | BLAKE3 advantage |
| Hash File | 100KB | 1-5 ms | I/O bound |

## Performance Bottleneck Analysis

### Identifying Bottlenecks

1. **Low Throughput (< 50% expected)**
   - Check GPU memory bandwidth saturation
   - Look for kernel launch overhead dominance
   - Verify no CPU throttling or thermal issues

2. **High Variance (> 10% std dev)**
   - System under load (background processes)
   - GPU frequency scaling enabled
   - Thermal throttling starting
   - Solution: Dedicate system, disable power management

3. **Scaling Non-Linearity**
   - Batch size tests show saturation point
   - Sequence length shows quadratic scaling degradation
   - May indicate register pressure or cache issues

### Common Issues

#### Metal Kernels
- **GPU memory exhaustion**: Reduce batch size or sequence length
- **Launch overhead dominance**: Batch more work together
- **KV cache pressure**: Use GQA to reduce cache size
- **Register pressure**: Check kernel occupancy with `metal-trace`

#### BLAKE3/HKDF
- **I/O bound (hash_file)**: Expected for < 1MB files
- **High variance**: Disable frequency scaling: `pmset -g thermlog`
- **Serialization overhead**: Use binary format instead of JSON for frequent ops

## Optimization Strategies

### For Metal Kernels

1. **Batch Size Optimization**
   - Use results from `adaptive_batching` to find sweet spot
   - Typical sweet spot: 16-64 tokens

2. **Sequence Length Management**
   - Use KV cache with rotation (not full cache)
   - Implement sliding window attention for long sequences
   - Consider Flash Attention for long sequences

3. **LoRA Fusion**
   - K-sparse selection has negligible overhead
   - Safe to use 8+ adapters simultaneously

4. **Memory Efficiency**
   - Use the GPU memory pool (`gpu_memory_pool`)
   - Enable buffer reuse
   - Monitor memory pressure

### For BLAKE3/HKDF

1. **Large File Hashing**
   - Use parallel hashing for files > 10MB
   - Consider streaming for very large files (> 1GB)

2. **Seed Derivation**
   - Batch derivations when possible
   - Cache derived seeds if reused frequently

3. **Serialization**
   - Use hex encoding for external APIs
   - Keep internal operations in binary
   - Cache serialized forms when frequently accessed

4. **Zero-Copy Operations**
   - Use hash_multi instead of concatenating slices
   - Avoid unnecessary allocations in hot paths

## Continuous Performance Monitoring

### CI Integration

Add to your CI pipeline:
```bash
# Run benchmarks and compare against main branch baseline
cargo bench -p adapteros-lora-kernel-mtl --bench kernel_benchmarks -- --baseline main
cargo bench -p adapteros-core --bench hash_benchmarks -- --baseline main

# Fail CI if regression > 10%
# (Implementation depends on your CI system)
```

### Scheduled Benchmarking

Run benchmarks weekly/monthly on dedicated hardware:
1. Disable frequency scaling
2. Disable background apps
3. Run 3+ times to get variance
4. Compare against previous runs
5. Investigate > 5% regressions

## References

- Metal Performance Shaders: https://developer.apple.com/documentation/metalperformanceshaders
- BLAKE3 Algorithm: https://github.com/BLAKE3-team/BLAKE3
- HKDF Specification: https://tools.ietf.org/html/rfc5869
- Criterion.rs: https://github.com/bheisler/criterion.rs
- Flash Attention: https://arxiv.org/abs/2205.14135
- GQA: https://arxiv.org/abs/2305.13245
- LoRA: https://arxiv.org/abs/2106.09685

## Troubleshooting

### Benchmark Variability

High variance (> 10%) indicates system issues:

```bash
# Check thermal status
system_profiler SPPowerDataType | grep "Processor Cores"

# Disable CPU frequency scaling (requires sudo)
sudo pmset -a disablesleep 1
sudo pmset -a hibernatemode 0

# Check for background processes
top -l 1 | head -20
```

### GPU Not Scaling

Metal kernels may not utilize full GPU:
1. Check GPU index: `AOS_GPU_INDEX=0 cargo bench`
2. Verify with `metal-trace` or `Instruments.app`
3. Check KV cache size not exceeding VRAM

### Low HKDF Performance

Seed derivation should be < 50μs. If slower:
1. Check SHA256 performance (baseline)
2. Verify no hashing collisions (rare)
3. Profile with `cargo flamegraph`

## Conclusions

These benchmarks provide comprehensive performance analysis for:
- GPU compute efficiency
- Memory bandwidth utilization
- Cryptographic operations
- End-to-end inference latency

Regular benchmarking ensures AdapterOS maintains performance across:
- Different hardware (M1/M2/M3/M4)
- Model architectures (different vocab/hidden sizes)
- Adapter configurations (different K values)
- Workload patterns (variable batch sizes)

Use these benchmarks to:
- Verify performance improvements
- Detect regressions early
- Guide optimization efforts
- Make hardware procurement decisions
