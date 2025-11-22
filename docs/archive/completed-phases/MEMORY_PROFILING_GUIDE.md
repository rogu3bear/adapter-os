# Metal Kernel Memory Profiling Guide

**Purpose:** Identify and optimize memory access patterns in AdapterOS Metal kernels  
**Tool:** macOS Instruments - Metal System Trace  
**Target:** >100 GB/s memory bandwidth on M3 Max

## Prerequisites

- macOS 11+ (Big Sur or later)
- Apple Silicon Mac (M1 or newer)
- Xcode Command Line Tools installed
- Compiled Metal kernels (`aos_kernels.metallib`)
- Release build of mplora-server

## Setup

### 1. Build Release Binary
```bash
cd /Users/star/Dev/adapter-os
cargo build --release
```

### 2. Verify Metal Kernels
```bash
# Check if metallib is compiled
ls -lh metal/aos_kernels.metallib

# If not compiled, build it
cd metal
bash ci_build.sh
```

### 3. Install Xcode Command Line Tools
```bash
xcode-select --install
```

## Profiling Workflow

### Step 1: Record Metal System Trace

```bash
# Start profiling with Metal System Trace template
xcrun xctrace record \
  --template 'Metal System Trace' \
  --launch ./target/release/mplora-server \
  --output ~/Desktop/metal_trace_$(date +%Y%m%d_%H%M%S).trace \
  --time-limit 30s
```

**Alternative: Use Instruments GUI**
```bash
# Open Instruments
open -a Instruments

# Steps:
# 1. Select "Metal System Trace" template
# 2. Choose mplora-server as target
# 3. Click Record
# 4. Run inference workload
# 5. Stop after 30-60 seconds
```

### Step 2: Generate Workload

While profiling is running, generate inference load:

```bash
# In another terminal
cd /Users/star/Dev/adapter-os

# Run sample inference
./target/release/aos-cli inference \
  --prompt "Explain quantum computing in simple terms" \
  --max-tokens 100 \
  --model qwen2.5-7b
```

### Step 3: Analyze Trace

Open the trace file in Instruments:
```bash
open ~/Desktop/metal_trace_*.trace
```

**Key Metrics to Review:**

1. **Memory Bandwidth:**
   - Target: >100 GB/s sustained
   - Look for: Bandwidth utilization %
   - Red flags: <80% utilization with low GPU occupancy

2. **GPU Occupancy:**
   - Target: >80% during inference
   - Look for: Idle periods between kernel dispatches
   - Red flags: Frequent stalls or bubbles

3. **Buffer Allocation:**
   - Look for: Excessive allocations
   - Red flags: Allocations in hot path

4. **Kernel Duration:**
   - Compare against baselines in `metal/baselines/`
   - Red flags: Kernels taking >8ms (MLP) or >6ms (QKV)

## Analysis Checklist

### Memory Access Patterns

- [ ] **Sequential Access:** Are reads/writes sequential?
  - Tool: Look at "Memory Timeline" in Instruments
  - Good: Contiguous memory access
  - Bad: Random scattered reads/writes

- [ ] **Coalesced Access:** Are threads accessing adjacent memory?
  - Tool: Check "Memory Load Efficiency"
  - Good: >90% efficiency
  - Bad: <70% efficiency (indicates scattered access)

- [ ] **Bank Conflicts:** Are there SRAM bank conflicts?
  - Tool: "Threadgroup Memory" view
  - Good: Minimal conflicts
  - Bad: High contention on threadgroup memory

### Kernel Performance

- [ ] **Occupancy:** GPU utilization per kernel
  - Target: >80%
  - Check: "GPU Utilization" timeline
  
- [ ] **Stalls:** CPU-GPU synchronization stalls
  - Target: Minimal stalls between dispatches
  - Check: "Command Buffer" timeline

- [ ] **Throughput:** Memory read/write bytes per second
  - Target: >100 GB/s
  - Check: "Memory Bandwidth" graph

## Common Bottlenecks

### 1. Non-Coalesced Memory Access

**Symptom:** Low memory load efficiency (<70%)

**Example Issue:**
```metal
// BAD: Strided access
for (uint i = tid; i < size; i += stride) {
    output[i] = input[i * stride];  // Non-coalesced
}

// GOOD: Sequential access
for (uint i = tid; i < size; i += threadgroup_size) {
    output[i] = input[i];  // Coalesced
}
```

**Fix:** Restructure loops for contiguous access

### 2. Excessive Global Memory Access

**Symptom:** High memory bandwidth usage, low occupancy

**Example Issue:**
```metal
// BAD: Reading from global memory repeatedly
for (uint i = 0; i < head_dim; i++) {
    float val = input[base_idx + i];  // Global read
    output[i] = val * scale;          // Every iteration
}

// GOOD: Use threadgroup memory for shared data
threadgroup float shared_input[128];
shared_input[tid] = input[base_idx + tid];
threadgroup_barrier(mem_flags::mem_threadgroup);
for (uint i = 0; i < head_dim; i++) {
    output[i] = shared_input[i] * scale;  // Local read
}
```

**Fix:** Use threadgroup memory for frequently accessed data

### 3. Small Threadgroup Sizes

**Symptom:** Low GPU occupancy, underutilized compute units

**Example Issue:**
```metal
// BAD: Small threadgroup
let threadgroup_size = MTLSize::new(8, 8, 1);  // 64 threads

// GOOD: Larger threadgroup
let threadgroup_size = MTLSize::new(16, 16, 1);  // 256 threads
// or
let threadgroup_size = MTLSize::new(32, 8, 1);   // 256 threads
```

**Fix:** Increase threadgroup size to 256-512 threads

### 4. Unaligned Buffer Access

**Symptom:** Poor memory performance despite sequential access

**Example Issue:**
```rust
// BAD: Misaligned buffer
let buffer = device.new_buffer_with_data(
    data.as_ptr() as *const _,
    data.len() as u64,
    MTLResourceOptions::StorageModeShared
);

// GOOD: Ensure 16-byte alignment
let aligned_data = align_to_16_bytes(&data);
let buffer = device.new_buffer_with_data(
    aligned_data.as_ptr() as *const _,
    aligned_data.len() as u64,
    MTLResourceOptions::StorageModeShared
);
```

**Fix:** Align all buffers to 16-byte boundaries

## Optimization Strategies

### Strategy 1: Memory Prefetching

Add prefetch hints for predictable access patterns:

```metal
// Prefetch next iteration's data
for (uint i = 0; i < num_iterations; i++) {
    // Prefetch i+1 data
    if (i + 1 < num_iterations) {
        __builtin_prefetch(&input[(i+1) * stride]);
    }
    
    // Process current data
    float val = input[i * stride];
    output[i] = process(val);
}
```

### Strategy 2: Tiled Computation

Break large computations into tiles that fit in threadgroup memory:

```metal
// Process head_dim in tiles of 64
const uint tile_size = 64;
for (uint tile = 0; tile < head_dim; tile += tile_size) {
    // Load tile into threadgroup memory
    threadgroup float tile_data[64];
    if (tid < tile_size) {
        tile_data[tid] = input[tile + tid];
    }
    threadgroup_barrier(mem_flags::mem_threadgroup);
    
    // Process tile
    for (uint i = 0; i < tile_size; i++) {
        accumulator += tile_data[i] * weights[i];
    }
}
```

### Strategy 3: Reduce Synchronization

Minimize threadgroup barriers:

```metal
// BAD: Barrier in every iteration
for (uint i = 0; i < N; i++) {
    shared_mem[tid] = compute(i);
    threadgroup_barrier(mem_flags::mem_threadgroup);  // Expensive!
    result += shared_mem[other_tid];
}

// GOOD: Single barrier after loading all data
for (uint i = 0; i < N; i++) {
    shared_mem[tid * N + i] = compute(i);
}
threadgroup_barrier(mem_flags::mem_threadgroup);  // Once
for (uint i = 0; i < N; i++) {
    result += shared_mem[other_tid * N + i];
}
```

## Automated Profiling Script

Create `scripts/profile_kernels.sh`:

```bash
#!/bin/bash
# Profile Metal kernels and extract key metrics

set -e

TRACE_DIR="$HOME/Desktop/metal_profiles"
mkdir -p "$TRACE_DIR"

TIMESTAMP=$(date +%Y%m%d_%H%M%S)
TRACE_FILE="$TRACE_DIR/metal_trace_$TIMESTAMP.trace"

echo "🔍 Starting Metal profiling..."
echo "Output: $TRACE_FILE"

# Record trace
xcrun xctrace record \
  --template 'Metal System Trace' \
  --launch ./target/release/mplora-server \
  --output "$TRACE_FILE" \
  --time-limit 30s &

PROFILE_PID=$!

# Wait for profiler to start
sleep 3

# Generate workload
echo "🚀 Running inference workload..."
./target/release/aos-cli inference \
  --prompt "Explain quantum computing" \
  --max-tokens 50 \
  --model qwen2.5-7b

# Wait for profiler to finish
wait $PROFILE_PID

echo "✅ Profiling complete: $TRACE_FILE"
echo ""
echo "Next steps:"
echo "1. Open trace: open '$TRACE_FILE'"
echo "2. Analyze memory bandwidth in 'Metal System Trace' view"
echo "3. Check GPU occupancy in 'GPU' timeline"
echo "4. Review kernel durations against baselines"
```

Make it executable:
```bash
chmod +x scripts/profile_kernels.sh
```

## Performance Targets

### Memory Bandwidth
| Metric | Target | M1 Max | M2 Max | M3 Max | M4 Max |
|--------|--------|---------|---------|---------|---------|
| Peak Bandwidth | 100 GB/s | 200 GB/s | 200 GB/s | 300 GB/s | 273 GB/s |
| Sustained (Target) | >100 GB/s | >150 GB/s | >150 GB/s | >200 GB/s | >180 GB/s |
| Utilization | >80% | >75% | >75% | >80% | >80% |

### Kernel Latency
| Kernel | Target p95 | Acceptable | Critical |
|--------|-----------|------------|----------|
| fused_mlp | ≤8ms | <10ms | >12ms |
| fused_qkv | ≤6ms | <8ms | >10ms |
| flash_attention | ≤4ms | <6ms | >8ms |
| apply_rope | ≤2ms | <3ms | >4ms |

### GPU Occupancy
- **Target:** >80% during active inference
- **Acceptable:** >70%
- **Critical:** <60% (indicates severe bottleneck)

## Reporting

### Create Performance Report

After profiling, document findings:

```markdown
# Memory Profiling Report - [Date]

## Hardware
- Device: [Apple M3 Max]
- Memory: [96GB]
- GPU Cores: [40]

## Metrics
- **Memory Bandwidth:** [185 GB/s] (Target: >100 GB/s) ✅
- **GPU Occupancy:** [82%] (Target: >80%) ✅
- **MLP Kernel:** [7.2ms] (Target: ≤8ms) ✅
- **QKV Kernel:** [5.8ms] (Target: ≤6ms) ✅
- **Flash Attention:** [3.1ms] (Target: ≤4ms) ✅

## Bottlenecks Identified
1. [Description of bottleneck]
   - Impact: [High/Medium/Low]
   - Location: [kernel name, line number]
   - Fix: [Proposed optimization]

## Optimizations Applied
1. [Optimization description]
   - Before: [X ms]
   - After: [Y ms]
   - Improvement: [Z%]

## Next Steps
- [ ] [Action item 1]
- [ ] [Action item 2]
```

## Troubleshooting

### Issue: "No Metal devices found"
**Solution:** Ensure running on Apple Silicon Mac with Metal support

### Issue: "Permission denied" for xctrace
**Solution:**
```bash
sudo xcode-select --switch /Applications/Xcode.app/Contents/Developer
```

### Issue: Trace file too large
**Solution:** Reduce profiling time:
```bash
xcrun xctrace record --time-limit 15s ...
```

### Issue: Can't see Metal counters
**Solution:** Metal counters require macOS 11+. Check system version:
```bash
sw_vers
```

## Additional Resources

- [Metal Best Practices Guide](https://developer.apple.com/documentation/metal/metal_sample_code_library/optimizing_performance_with_the_gpu_counters_instrument)
- [Metal Memory Management](https://developer.apple.com/documentation/metal/resource_objects/managing_resource_memory)
- [Instruments User Guide](https://help.apple.com/instruments/mac/)

---

**Last Updated:** October 8, 2025  
**Maintainer:** AdapterOS Team

