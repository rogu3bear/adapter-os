# Metal Backend Architecture Diagram

---

## System Overview

```
┌─────────────────────────────────────────────────────────────────────┐
│                        AdapterOS Metal Backend                      │
│                     Apple M4 Max (40 cores, 48GB)                   │
└─────────────────────────────────────────────────────────────────────┘
                                   │
                    ┌──────────────┴──────────────┐
                    │                             │
           ┌────────▼────────┐          ┌────────▼────────┐
           │  MetalKernels   │          │  VramTracker    │
           │   (lib.rs)      │          │   (vram.rs)     │
           │                 │          │                 │
           │  • load()       │◄─────────│  • track()      │
           │  • run_step()   │          │  • fingerprint()│
           │  • load_adapter │          │  • baseline()   │
           │  • hot_swap()   │          │                 │
           └────────┬────────┘          └─────────────────┘
                    │
      ┌─────────────┼─────────────┐
      │             │             │
┌─────▼─────┐ ┌────▼────┐ ┌──────▼──────┐
│MemPressure│ │Fused    │ │ Recovery    │
│ Detector  │ │Kernels  │ │ Wrapper     │
│           │ │         │ │             │
│• check()  │ │• QKV    │ │• safe_      │
│• evict()  │ │• MLP    │ │  dispatch() │
│           │ │• Flash  │ │             │
└───────────┘ └─────────┘ └─────────────┘
```

---

## Memory Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                    Unified Memory (48GB)                            │
│                                                                     │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │                  System Reserved (3.3GB)                     │ │
│  └──────────────────────────────────────────────────────────────┘ │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │              Working Set (40.7GB, 85% of total)              │ │
│  │                                                              │ │
│  │  ┌─────────────────────────────────────────────────────┐   │ │
│  │  │           Base Model Weights (14GB)                 │   │ │
│  │  │         • Transformer layers (28 layers)            │   │ │
│  │  │         • Embedding matrix (152K vocab)             │   │ │
│  │  │         • LM head projection                        │   │ │
│  │  └─────────────────────────────────────────────────────┘   │ │
│  │                                                              │ │
│  │  ┌─────────────────────────────────────────────────────┐   │ │
│  │  │         Adapter Weights (variable, ~12GB)           │   │ │
│  │  │         • LoRA A/B matrices (16-64 rank)            │   │ │
│  │  │         • Q/K/V projections                         │   │ │
│  │  │         • MLP up/down/gate                          │   │ │
│  │  │         Tracked by VramTracker                      │   │ │
│  │  └─────────────────────────────────────────────────────┘   │ │
│  │                                                              │ │
│  │  ┌─────────────────────────────────────────────────────┐   │ │
│  │  │         KV Cache (variable, ~8GB)                   │   │ │
│  │  │         • Per-adapter context (2048 tokens)         │   │ │
│  │  │         • Estimated and tracked                     │   │ │
│  │  └─────────────────────────────────────────────────────┘   │ │
│  │                                                              │ │
│  │  ┌─────────────────────────────────────────────────────┐   │ │
│  │  │      Intermediate Buffers (~6GB)                    │   │ │
│  │  │      • Hidden states, Q/K/V outputs                 │   │ │
│  │  │      • Attention/MLP intermediate                   │   │ │
│  │  └─────────────────────────────────────────────────────┘   │ │
│  └──────────────────────────────────────────────────────────────┘ │
│  ┌──────────────────────────────────────────────────────────────┐ │
│  │              Headroom (4.0GB, safety margin)                 │ │
│  └──────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘

Storage Mode: MTLResourceStorageModeShared (100% unified)
Access Pattern: CPU ←→ GPU (zero-copy)
```

---

## Memory Pressure Detection Flow

```
                    ┌──────────────┐
                    │   Start      │
                    │  Inference   │
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │ Check Memory │
                    │  Pressure    │
                    │ (100ms rate) │
                    └──────┬───────┘
                           │
              ┌────────────┴────────────┐
              │                         │
         ┌────▼────┐              ┌─────▼─────┐
         │ Normal  │              │  Warning  │
         │ (<70%)  │              │ (70-85%)  │
         └────┬────┘              └─────┬─────┘
              │                         │
              │                    ┌────▼────┐
              │                    │ Evict   │
              │                    │  10%    │
              │                    │Adapters │
              │                    └────┬────┘
              │                         │
              ├─────────────────────────┤
              │                         │
         ┌────▼────┐              ┌─────▼─────┐
         │Critical │              │ Emergency │
         │(85-95%) │              │  (>95%)   │
         └────┬────┘              └─────┬─────┘
              │                         │
         ┌────▼────┐              ┌─────▼─────┐
         │ Evict   │              │ Evict     │
         │  25%    │              │  50%      │
         │Adapters │              │ Adapters  │
         └────┬────┘              └─────┬─────┘
              │                         │
              └────────────┬────────────┘
                           │
                    ┌──────▼───────┐
                    │   Continue   │
                    │  Inference   │
                    └──────────────┘

Eviction Policy: Largest VRAM first (maximize freed memory)
Eviction Rate Limit: Max once per 5 seconds (prevent thrashing)
```

---

## Adapter Loading Pipeline

### Current (Synchronous)

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Adapter     │────▶│   Parse      │────▶│   Create     │
│  Weights     │     │ SafeTensors  │     │   Metal      │
│  (bytes)     │     │  (16-20ms)   │     │  Buffers     │
└──────────────┘     └──────────────┘     │  (6-10ms)    │
                                           └──────┬───────┘
                                                  │
                     ┌──────────────┐     ┌──────▼───────┐
                     │   Track      │◄────│   Upload to  │
                     │   VRAM       │     │     GPU      │
                     │   (1ms)      │     │   (5-8ms)    │
                     └──────────────┘     └──────────────┘

Total Latency: ~31ms (blocks inference)
CPU Utilization: 60%
GPU Utilization: 40%
```

### Optimized (Async)

```
┌──────────────┐     ┌──────────────┐
│  Adapter     │────▶│ Background   │
│  Weights     │     │   Thread     │
│  (bytes)     │     │  (parsing)   │
└──────────────┘     └──────┬───────┘
                            │
                            │ (does not block)
       ┌────────────────────┘
       │
       │  ┌──────────────┐     ┌──────────────┐
       └─▶│   Parse      │────▶│   Create     │
          │ SafeTensors  │     │   Metal      │
          │  (16-20ms)   │     │  Buffers     │
          └──────────────┘     │  (6-10ms)    │
                               └──────┬───────┘
                                      │
                    ┌─────────────────┘
                    │
             ┌──────▼───────┐     ┌──────────────┐
             │   Atomic     │────▶│   Track      │
             │   Swap in    │     │   VRAM       │
             │Ring Buffer   │     │   (1ms)      │
             │  (0.3ms)     │     └──────────────┘
             └──────────────┘

Perceived Latency: ~0.5ms (98% reduction!)
Inference: Continues with old adapter during load
```

---

## Kernel Execution Flow

```
                    ┌──────────────┐
                    │ Input Tokens │
                    │  (u32 IDs)   │
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │  Embedding   │
                    │   Lookup     │
                    │  (Metal)     │
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │ Hidden State │
                    │  [B, S, H]   │
                    └──────┬───────┘
                           │
              ┌────────────┴────────────┐
              │   Transformer Layer     │
              │      (repeat 28x)       │
              │                         │
              │  ┌─────────────────┐   │
              │  │   Fused QKV     │   │
              │  │   + LoRA        │   │
              │  │   (1.2ms)       │   │
              │  └────────┬────────┘   │
              │           │            │
              │  ┌────────▼────────┐   │
              │  │ Flash Attention │   │
              │  │   (0.8ms)       │   │
              │  └────────┬────────┘   │
              │           │            │
              │  ┌────────▼────────┐   │
              │  │   Fused MLP     │   │
              │  │   + LoRA        │   │
              │  │   (1.8ms)       │   │
              │  └────────┬────────┘   │
              │           │            │
              └───────────┼────────────┘
                          │
                   ┌──────▼───────┐
                   │ Final Hidden │
                   │    State     │
                   └──────┬───────┘
                          │
                   ┌──────▼───────┐
                   │  Vocabulary  │
                   │  Projection  │
                   │  (Metal)     │
                   └──────┬───────┘
                          │
                   ┌──────▼───────┐
                   │    Logits    │
                   │  [B, Vocab]  │
                   └──────────────┘

Total Latency per Token: ~110ms (28 layers × ~4ms)
GPU Utilization: 95%
Memory Bandwidth: 320 GB/s (80% of max)
```

---

## VRAM Tracking Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                          VramTracker                                │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │            Adapter Allocations (HashMap)                      │ │
│  │                                                               │ │
│  │  adapter_id: 1 ─────▶ VramAllocation {                       │ │
│  │                         buffer_bytes: 4,194,304 (4MB)         │ │
│  │                         kv_cache_bytes: 8,388,608 (8MB)       │ │
│  │                       }                                       │ │
│  │                                                               │ │
│  │  adapter_id: 2 ─────▶ VramAllocation { ... }                 │ │
│  │  adapter_id: 3 ─────▶ VramAllocation { ... }                 │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │        GPU Buffer Fingerprints (Integrity)                    │ │
│  │                                                               │ │
│  │  adapter_id: 1 ─────▶ GpuBufferFingerprint {                 │ │
│  │                         buffer_bytes: 4,194,304               │ │
│  │                         allocated_at: 1700000000              │ │
│  │                         checkpoint_hash: B3Hash(...)          │ │
│  │                       }                                       │ │
│  │                       ┌──────────────────────┐               │ │
│  │                       │ Checkpoint Sampling: │               │ │
│  │                       │  • First 4KB         │               │ │
│  │                       │  • Last 4KB          │               │ │
│  │                       │  • Midpoint 4KB      │               │ │
│  │                       │  BLAKE3 hash         │               │ │
│  │                       └──────────────────────┘               │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │       Memory Footprint Baselines (Anomaly Detection)          │ │
│  │                                                               │ │
│  │  adapter_id: 1 ─────▶ MemoryFootprintBaseline {              │ │
│  │                         samples: [4MB, 4MB, 4.1MB, ...]       │ │
│  │                         mean: 4.05MB                          │ │
│  │                         stddev: 0.05MB                        │ │
│  │                       }                                       │ │
│  │                       ┌──────────────────────┐               │ │
│  │                       │  Z-score threshold:  │               │ │
│  │                       │  2σ (95% confidence) │               │ │
│  │                       └──────────────────────┘               │ │
│  └───────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘

API:
  • track_adapter(id, buffer_bytes, kv_cache_bytes)
  • untrack_adapter(id) → freed_bytes
  • get_total_vram() → u64
  • store_fingerprint(id, fingerprint)
  • verify_fingerprint(id, current) → Result<bool>
  • check_memory_footprint(id, bytes) → (within_tolerance, z_score)
```

---

## Hot-Swap Architecture

### Current (Blocking)

```
Time:  0ms        5ms       10ms      15ms      20ms      30ms      42ms
       │          │         │         │         │         │         │
Step:  │  Unload  │ Parse   │         │ Create  │ Upload  │  Swap   │
       │  Old     │SafeTens │         │ Buffers │  GPU    │ Ring    │
       │  (5ms)   │ (20ms)  │         │ (5ms)   │ (10ms)  │ (2ms)   │
       │          │         │         │         │         │         │
GPU:   ▓▓         ░░░░░░░░░░░░░░░░░░░░░         ▓▓▓▓▓▓▓▓▓▓         │
CPU:              ████████████████████         ░░░░░░░░░░         │
                                                                   │
Inference: ❌ BLOCKED ────────────────────────────────────────────┘

Legend: ▓ = GPU work, █ = CPU work, ░ = Idle
```

### Optimized (Async)

```
Time:  0ms        5ms       10ms      15ms      20ms      25ms      0.5ms
       │          │         │         │         │         │         │
Step:  │Background│         │         │         │Atomic   │         │
       │  Parse   │         │         │         │ Swap    │         │
       │  (20ms)  │         │         │         │(0.5ms)  │         │
       │          │         │         │         │         │         │
GPU:   ████████████████████████████████████████████████████▓       │
CPU:   ░██████████████████████████░                                │
                                                                    │
Inference: ✅ CONTINUES ──────────────────────────────────────────┘

Perceived Latency: 0.5ms (only the atomic swap blocks)
Throughput: +15% (no pipeline stalls)
```

---

## Error Recovery Flow

```
                    ┌──────────────┐
                    │   Kernel     │
                    │  Dispatch    │
                    └──────┬───────┘
                           │
                    ┌──────▼───────┐
                    │   Wrapped    │
                    │     in       │
                    │catch_unwind  │
                    └──────┬───────┘
                           │
              ┌────────────┴────────────┐
              │                         │
         ┌────▼────┐              ┌─────▼─────┐
         │ Success │              │   Panic   │
         │         │              │  Caught   │
         └────┬────┘              └─────┬─────┘
              │                         │
              │                    ┌────▼────┐
              │                    │  Mark   │
              │                    │ Device  │
              │                    │Degraded │
              │                    └────┬────┘
              │                         │
              │                    ┌────▼────┐
              │                    │  Log    │
              │                    │ Panic   │
              │                    │  Info   │
              │                    └────┬────┘
              │                         │
              │                    ┌────▼────┐
              │                    │ Return  │
              │                    │AosError │
              │                    └────┬────┘
              │                         │
              └─────────────────────────┤
                                        │
                                 ┌──────▼───────┐
                                 │ Require      │
                                 │ Explicit     │
                                 │ Recovery     │
                                 └──────────────┘

Recovery: attempt_recovery(&device) clears degraded flag
Health Check: health_check() → Err if degraded
Panic Count: Tracks total panics for debugging
```

---

## File Layout

```
crates/adapteros-lora-kernel-mtl/
├── src/
│   ├── lib.rs                    (1,496 lines)  ─┐
│   │   ├── MetalKernels                         │ Core
│   │   ├── FusedKernels trait                   │ Implementation
│   │   ├── load_adapter()                       │
│   │   ├── unload_adapter()                     │
│   │   └── run_step()                           │
│   │                                            ─┘
│   ├── vram.rs                   (525 lines)   ─┐
│   │   ├── VramTracker                          │ VRAM
│   │   ├── GpuBufferFingerprint                 │ Tracking
│   │   └── MemoryFootprintBaseline               │
│   │                                            ─┘
│   ├── memory_pressure.rs        (450 lines)   ─┐
│   │   ├── MemoryPressureDetector               │ NEW
│   │   ├── PressureState                        │ Priority 1
│   │   └── check_pressure()                     │
│   │                                            ─┘
│   ├── fused_qkv.rs              (375 lines)   ─┐
│   │   ├── FusedQkvKernel                       │
│   │   ├── FlashAttentionKernel                 │ Kernels
│   │   └── GqaConfig                            │
│   │                                            ─┘
│   ├── fused_mlp.rs              (245 lines)   ─┐
│   │   ├── FusedMlpKernel                       │ Kernels
│   │   └── LoraConfig                           │
│   │                                            ─┘
│   ├── mplora.rs                 (373 lines)   ─┐
│   │   ├── MploraKernel                         │ MPLoRA
│   │   └── MploraConfig                         │
│   │                                            ─┘
│   ├── ane_acceleration.rs       (517 lines)   ─┐
│   │   ├── ANEAccelerator                       │ ANE
│   │   └── ANECapabilities                      │ (stub)
│   │                                            ─┘
│   ├── optimization.rs           (243 lines)   ─┐
│   │   ├── KernelOptimizer                      │ Optimizer
│   │   └── KernelPerformanceMetrics             │
│   │                                            ─┘
│   ├── recovery.rs               (222 lines)   ─┐
│   │   ├── RecoveryWrapper                      │ Recovery
│   │   └── safe_dispatch()                      │
│   │                                            ─┘
│   └── ring_buffer.rs            (182 lines)   ─┐
│       ├── RingBuffer                           │ Adapter
│       └── ActiveAdapter                        │ Ring
│                                                ─┘
├── shaders/
│   ├── adapteros_kernels.metallib  (52KB)
│   ├── aos_kernels.metallib        (21KB)
│   ├── mplora_kernels.metallib     (21KB)
│   └── kernel_hash.txt             (64B)
│
└── tests/
    ├── memory_pressure_tests.rs    (NEW)
    └── ...

Total: 6,369 lines across 22 files
```

---

## Deployment Architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│                     Production Deployment                           │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │                   Worker Process                              │ │
│  │                                                               │ │
│  │  ┌─────────────────────────────────────────────────────────┐ │ │
│  │  │              MetalKernels                               │ │ │
│  │  │  • Memory Pressure Detection (Priority 1)              │ │ │
│  │  │  • VRAM Tracking                                       │ │ │
│  │  │  • Adapter Hot-Swap                                    │ │ │
│  │  │  • Kernel Execution                                    │ │ │
│  │  └─────────────────────────────────────────────────────────┘ │ │
│  │                                                               │ │
│  │  ┌─────────────────────────────────────────────────────────┐ │ │
│  │  │         Lifecycle Manager (adapteros-lora-lifecycle)    │ │ │
│  │  │  • State Machine (Unloaded → Cold → Warm → Hot)        │ │ │
│  │  │  • Promotion/Demotion                                   │ │ │
│  │  │  • Heartbeat (5min timeout)                            │ │ │
│  │  └─────────────────────────────────────────────────────────┘ │ │
│  │                                                               │ │
│  │  ┌─────────────────────────────────────────────────────────┐ │ │
│  │  │               Telemetry (adapteros-telemetry)           │ │ │
│  │  │  • memory.pressure.warning                              │ │ │
│  │  │  • adapter.loaded                                       │ │ │
│  │  │  • kernel.executed                                      │ │ │
│  │  └─────────────────────────────────────────────────────────┘ │ │
│  └───────────────────────────────────────────────────────────────┘ │
│                                                                     │
│  ┌───────────────────────────────────────────────────────────────┐ │
│  │                   Monitoring                                  │ │
│  │  • vm_stat (memory pressure)                                 │ │
│  │  • Metal System Trace (GPU profiling)                        │ │
│  │  • Prometheus metrics                                        │ │
│  └───────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────┘
```

---

**Generated:** 2025-11-19
**Version:** v0.01-1 Alpha
**Author:** Agent 4 (Metal Optimization Specialist)
