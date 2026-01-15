# MLX Backend Determinism

**adapterOS MLX backend provides bit-exact deterministic inference when properly configured.**

---

## Determinism Status

The MLX backend achieves **bit-exact determinism** under these conditions:

- ✅ HKDF-seeded RNG active
- ✅ Real MLX backend (not stub/fallback mode)
- ✅ No stub fallback active
- ✅ Deterministic executor integration

When conditions are met, MLX reports `DeterminismLevel::BitExact` and `deterministic: true`.

---

## Core Components

### Deterministic Executor

Tasks execute serially via `DeterministicExecutor`:

```rust
// Tasks spawn deterministically with global sequence numbering
pub fn spawn_deterministic<F>(
    &self,
    description: String,
    future: F
) -> Result<TaskId>
where
    F: Future<Output = ()> + Send + 'static,
{
    // Generate deterministic task ID using global sequence
    let seq = GLOBAL_TASK_SEQUENCE.fetch_add(1, Ordering::SeqCst);
    let task_id = TaskId::from_seed_and_seq(&self.config.global_seed, seq);

    // Queue task for serial execution
    self.task_queue.lock().push_back(task);
}
```

**Serial Execution:**
```rust
while let Some(mut task) = {
    let mut queue = self.task_queue.lock();
    queue.pop_front()  // Process one task at a time
} {
    // Poll task to completion
    match task.future.as_mut().unwrap().as_mut().poll(&mut context) {
        Poll::Ready(()) => {
            // Task completed deterministically
        }
        Poll::Pending => {
            // Continue polling
        }
    }
}
```

### HKDF Seed Derivation

RNG seeded via HKDF-SHA256 from manifest hash:

```rust
// Global seed from model manifest
let manifest_hash = manifest.compute_hash()?;
let global_seed = derive_seed(&manifest_hash, "executor");

// MLX backend seed derivation
let model_hash = B3Hash::hash(model_path.as_bytes());
let mlx_seed = derive_seed(&model_hash, "mlx-backend:{model_path_hash}");

// Per-step seed for token generation
let step_seed = derive_seed(&base_seed, &format!("mlx-step:{}", step));

// Set in MLX RNG
mlx_set_seed_from_bytes(&mlx_seed)?;
```

### GPU Synchronization

Operations synchronized via `mlx_synchronize()`:

```rust
// C++ implementation
extern "C" void mlx_synchronize(void) {
    mx::synchronize();  // Wait for all GPU operations to complete
}

// Rust integration
pub fn synchronize(&self) -> Result<()> {
    unsafe {
        super::mlx_synchronize();
    }
    Ok(())
}
```

**Synchronization Points:**
- Model loading completion
- Weight updates
- Inference operations
- Memory transfers

### IEEE 754 Compliance

Floating-point operations follow IEEE 754 standards:

```rust
// No -ffast-math flags allowed
// Kahan summation for numerical stability
let sum = kahan_sum(&values);  // FP64 accumulator

// Q15 quantization with proper rounding
let gate_q15 = (gate_f32 * 32767.0).round() as i16;
let gate_f32 = gate_q15 as f32 / 32767.0;
```

---

## Attestation Logic

### Conditions for Bit-Exact Determinism

```rust
fn attest_determinism(&self) -> Result<DeterminismReport> {
    // Check seeding status
    let seeded = self.rng_seeded.load(Ordering::Relaxed);

    // Check stub fallback state
    let is_stub_active = {
        let health = self.health_status.read();
        health.stub_fallback_active
    };

    // Build report
    let report = DeterminismReport {
        backend_type: BackendType::MLX,
        determinism_level: if seeded && !is_stub_active && IS_REAL_MLX {
            DeterminismLevel::BitExact
        } else {
            DeterminismLevel::None
        },
        deterministic: seeded && !is_stub_active && IS_REAL_MLX,
        rng_seed_method: RngSeedingMethod::HkdfSeeded,
        floating_point_mode: FloatingPointMode::Deterministic,
        runtime_version: Some("mlx-cpp-ffi".to_string()),
        ..
    };

    Ok(report)
}
```

### Configuration Flags

```rust
const IS_REAL_MLX: bool = cfg!(feature = "mlx") && !cfg!(mlx_stub);
```

- `feature = "mlx"`: Real MLX backend enabled
- `!cfg!(mlx_stub)`: Not using stub/fallback implementation

---

## Streaming Integration

### Deterministic Token Generation

```rust
impl StreamingGenerator {
    pub async fn generate<F>(
        &mut self,
        generate_token_fn: F,
        tx: mpsc::Sender<StreamEvent>
    ) -> Result<()>
    where
        F: Fn(usize, &B3Hash) -> Result<TokenGenerationOutput>,
    {
        // Spawn keep-alive task deterministically
        let _keep_alive = if self.config.keep_alive {
            Some(spawn_deterministic(
                format!("mlx-stream-keep-alive-{}", self.tokens_generated),
                async move { /* keep-alive logic */ }
            )?)
        } else {
            None
        };

        // Generation loop with deterministic seeds
        for step in 0..self.config.max_tokens {
            let step_seed = self.derive_step_seed(step);
            let gen_output = generate_token_fn(step, &step_seed)?;

            // Send token deterministically
            tx.send(StreamEvent::Token(gen_output)).await?;
        }

        Ok(())
    }

    fn derive_step_seed(&self, step: usize) -> B3Hash {
        let label = format!("mlx-stream-step:{}", step);
        B3Hash::from_bytes(derive_seed(&self.base_seed, &label))
    }
}
```

### Task Sequencing

Tasks execute in deterministic order:

```rust
// Task 1: Model loading
let load_task = spawn_deterministic("mlx-model-load", async {
    load_model(model_path).await?;
    mlx_synchronize();  // Ensure load completes
});

// Task 2: Inference (depends on Task 1 completion)
let inference_task = spawn_deterministic("mlx-inference", async {
    let result = run_inference(input).await?;
    mlx_synchronize();  // Ensure inference completes
    result
});
```

---

## Memory Management

### Deterministic Allocation

Memory operations include synchronization barriers:

```rust
impl MemoryManager {
    pub fn synchronize(&self) -> Result<()> {
        debug!("Synchronizing MLX GPU operations");
        unsafe {
            super::mlx_synchronize();
        }
        debug!("MLX GPU operations synchronized");
        Ok(())
    }

    pub fn allocate_deterministic(&self, size: usize) -> Result<MemoryHandle> {
        // Allocate memory
        let handle = self.allocate(size)?;

        // Synchronize to ensure deterministic state
        self.synchronize()?;

        Ok(handle)
    }
}
```

### Unified Memory Architecture

Apple Silicon unified memory ensures consistent access:

```rust
// Zero-copy CPU ↔ GPU data transfer
let gpu_buffer = metal_create_shared_buffer(context, size);
let cpu_ptr = metal_buffer_contents(gpu_buffer);  // Direct access

// Deterministic memory layout
mlx_set_memory_layout_policy(MLXMemoryLayout::RowMajorContiguous);
mlx_disable_memory_optimization();
```

---

## Verification

### Determinism Tests

```bash
# Run MLX determinism tests
cargo test -p adapteros-lora-mlx-ffi --test determinism_tests

# Test HKDF seed derivation
cargo test -p adapteros-core --test seed_derivation

# Verify IEEE 754 compliance
cargo test -p adapteros-lora-router --test q15_quantization
```

### Attestation Verification

```bash
# Check determinism status
curl http://localhost:8080/v1/backends/mlx/status

# Expected response when deterministic:
{
  "deterministic": true,
  "determinism_level": "BitExact",
  "rng_seed_method": "HkdfSeeded",
  "floating_point_mode": "Deterministic"
}
```

### Replay Testing

```bash
# Run determinism replay harness
cargo test --test determinism_replay_harness -- --test-threads=1

# Verify event log consistency
cargo test --test determinism_core_suite
```

---

## Performance Characteristics

### Determinism Overhead

- **Serial Execution:** ~70-80% throughput reduction (GPU utilization 20% vs 90%)
- **Synchronization Barriers:** Additional latency per operation
- **Memory Layout:** Fixed layouts prevent optimization
- **Thread Pinning:** Single-threaded execution on multi-core systems

### When Determinism Matters

```rust
// Production compliance requirements
if compliance_required {
    ensure_deterministic_config();
    // Accept performance trade-offs for auditability
}

// Development experimentation
if development_mode {
    use_fast_mode();  // Performance prioritized
}
```

---

## Configuration

### Enable Determinism

```bash
# Build with MLX determinism
cargo build --features mlx,deterministic-exec

# Configure deterministic executor
export AOS_DETERMINISTIC_EXECUTOR=true
export AOS_THREAD_PINNING=true
export AOS_MAX_TICKS_PER_TASK=10000

# Verify configuration
curl http://localhost:8080/v1/config/determinism
```

### Monitoring

```bash
# Check determinism status
curl http://localhost:8080/v1/backends/mlx/determinism

# Monitor task execution
curl http://localhost:8080/v1/executor/tasks

# Event log inspection
curl http://localhost:8080/v1/executor/events
```

---

## Related Documentation

- [DETERMINISM.md](DETERMINISM.md) — General determinism concepts
- [BACKEND_PARITY.md](BACKEND_PARITY.md) — Backend comparison
- [MLX_GUIDE.md](MLX_GUIDE.md) — MLX backend usage
- [CRYPTO_RECEIPTS.md](CRYPTO_RECEIPTS.md) — Receipt binding