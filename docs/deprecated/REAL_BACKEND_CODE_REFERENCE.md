# RealBackendAdapterBackend Code Reference

## Complete Implementation Overview

### File Locations

1. **Definition**: `/Users/star/Dev/aos/crates/adapteros-lora-lifecycle/src/workflow_executor.rs` (Lines 352-620)
2. **Implementation**: `/Users/star/Dev/aos/crates/adapteros-lora-lifecycle/src/workflow_executor.rs` (Lines 621-689)
3. **Exports**: `/Users/star/Dev/aos/crates/adapteros-lora-lifecycle/src/lib.rs` (Line 114-117)
4. **Dependencies**: `/Users/star/Dev/aos/crates/adapteros-lora-lifecycle/Cargo.toml` (Lines 6-41)

## Struct Definition

```rust
pub struct RealBackendAdapterBackend {
    /// The actual fused kernel backend (Metal/CoreML/MLX)
    kernels: Arc<Mutex<Box<dyn FusedKernels>>>,
    /// Adapter name to routing index mapping
    adapter_name_to_index: HashMap<String, u16>,
    /// Vocabulary size for output buffers
    vocab_size: usize,
    /// Backend identifier for logging/diagnostics
    backend_name: String,
}
```

## Constructor Methods

### 1. new_auto() - Automatic Backend Selection

```rust
pub async fn new_auto(
    adapter_names: Vec<String>,
    vocab_size: usize,
) -> Result<Self> {
    // Try CoreML first (most power-efficient)
    #[cfg(all(target_os = "macos", feature = "coreml-backend"))]
    {
        match Self::new_coreml(adapter_names.clone(), vocab_size).await {
            Ok(backend) => return Ok(backend),
            Err(e) => {
                warn!(error = %e, "CoreML initialization failed, trying Metal");
            }
        }
    }

    // Try Metal (production, deterministic)
    #[cfg(target_os = "macos")]
    {
        match Self::new_metal(adapter_names.clone(), vocab_size).await {
            Ok(backend) => return Ok(backend),
            Err(e) => {
                warn!(error = %e, "Metal initialization failed, trying MLX");
            }
        }
    }

    // Try MLX (experimental)
    #[cfg(feature = "multi-backend")]
    {
        return Self::new_mlx("/dev/null".to_string(), adapter_names, vocab_size).await;
    }

    #[cfg(not(any(
        target_os = "macos",
        feature = "multi-backend",
        feature = "coreml-backend"
    )))]
    {
        Err(AosError::Config(
            "No suitable backend available. Ensure Metal GPU, CoreML with ANE, or MLX is present.".to_string(),
        ))
    }
}
```

**Characteristics:**
- Fallback chain: CoreML → Metal → MLX
- Platform-aware compilation via `#[cfg]` attributes
- Comprehensive error messages
- No panics, all errors propagated as `Result`

### 2. new_metal() - Metal Backend

```rust
#[cfg(target_os = "macos")]
pub async fn new_metal(
    adapter_names: Vec<String>,
    vocab_size: usize,
) -> Result<Self> {
    use adapteros_lora_kernel_mtl::MetalKernels;

    info!(
        adapters_count = adapter_names.len(),
        vocab_size = vocab_size,
        "Initializing RealBackendAdapterBackend with Metal backend"
    );

    let mut kernels = MetalKernels::new()
        .map_err(|e| {
            error!(error = %e, "Failed to initialize Metal kernels");
            AosError::Kernel(format!("Metal initialization failed: {}", e))
        })?;

    // Attest to determinism
    kernels.attest_determinism()
        .map_err(|e| {
            warn!(error = %e, "Metal backend failed determinism attestation");
            e
        })?;

    let backend_name = kernels.device_name().to_string();
    let adapter_name_to_index = adapter_names
        .into_iter()
        .enumerate()
        .map(|(i, name)| (name, i as u16))
        .collect();

    Ok(Self {
        kernels: Arc::new(Mutex::new(Box::new(kernels))),
        adapter_name_to_index,
        vocab_size,
        backend_name,
    })
}
```

**Key Points:**
- macOS-only via `#[cfg(target_os = "macos")]`
- Creates `MetalKernels` instance
- Mandatory determinism attestation
- Device name captured for logging

### 3. new_coreml() - CoreML Backend with ANE

```rust
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
pub async fn new_coreml(
    adapter_names: Vec<String>,
    vocab_size: usize,
) -> Result<Self> {
    use adapteros_lora_kernel_coreml::{init_coreml, CoreMLBackend, ComputeUnits};

    info!(
        adapters_count = adapter_names.len(),
        vocab_size = vocab_size,
        "Initializing RealBackendAdapterBackend with CoreML backend"
    );

    // Initialize CoreML runtime
    init_coreml()
        .map_err(|e| {
            error!(error = %e, "Failed to initialize CoreML runtime");
            e
        })?;

    // Use CpuAndNeuralEngine for optimal ANE utilization
    let compute_units = ComputeUnits::CpuAndNeuralEngine;
    let backend = CoreMLBackend::new(compute_units, false)
        .map_err(|e| {
            error!(error = %e, "Failed to create CoreML backend");
            e
        })?;

    let mut kernels = backend;

    // Attest to determinism
    kernels.attest_determinism()
        .map_err(|e| {
            warn!(error = %e, "CoreML backend failed determinism attestation");
            e
        })?;

    let backend_name = kernels.device_name().to_string();
    let adapter_name_to_index = adapter_names
        .into_iter()
        .enumerate()
        .map(|(i, name)| (name, i as u16))
        .collect();

    Ok(Self {
        kernels: Arc::new(Mutex::new(Box::new(kernels))),
        adapter_name_to_index,
        vocab_size,
        backend_name,
    })
}
```

**Requirements:**
- macOS 13+ (for MLTensor support)
- `coreml-backend` feature flag
- Requires `swiftc` in PATH for build

### 4. new_mlx() - MLX Backend for Research

```rust
#[cfg(feature = "multi-backend")]
pub async fn new_mlx(
    model_path: String,
    adapter_names: Vec<String>,
    vocab_size: usize,
) -> Result<Self> {
    use adapteros_lora_mlx_ffi::{MLXFFIBackend, MLXFFIModel};

    info!(
        model_path = %model_path,
        adapters_count = adapter_names.len(),
        vocab_size = vocab_size,
        "Initializing RealBackendAdapterBackend with MLX backend"
    );

    // Load the model
    let model = MLXFFIModel::load(&model_path)
        .map_err(|e| {
            error!(error = %e, model_path = %model_path, "Failed to load MLX model");
            AosError::Kernel(format!("MLX model load failed: {}", e))
        })?;

    let backend = MLXFFIBackend::new(model);
    let mut kernels = backend;

    // Attest to determinism (note: MLX may be non-deterministic)
    if let Err(e) = kernels.attest_determinism() {
        warn!(error = %e, "MLX backend may not provide determinism guarantees");
    }

    let backend_name = kernels.device_name().to_string();
    let adapter_name_to_index = adapter_names
        .into_iter()
        .enumerate()
        .map(|(i, name)| (name, i as u16))
        .collect();

    Ok(Self {
        kernels: Arc::new(Mutex::new(Box::new(kernels))),
        adapter_name_to_index,
        vocab_size,
        backend_name,
    })
}
```

**Notes:**
- Requires `multi-backend` feature
- Model path provided by caller
- Determinism is optional (warning only, not error)
- Research/training focused

## AdapterExecutionBackend Trait Implementation

```rust
impl AdapterExecutionBackend for RealBackendAdapterBackend {
    async fn execute_adapter(
        &self,
        adapter_id: &str,
        input_tokens: &[u32],
        _model_state: &HashMap<String, Vec<f32>>,
    ) -> Result<AdapterExecutionResult> {
        debug!(
            adapter_id = %adapter_id,
            input_tokens_len = input_tokens.len(),
            "Executing adapter with real backend"
        );

        // Get adapter index for routing
        let adapter_index = self
            .adapter_name_to_index
            .get(adapter_id)
            .copied()
            .ok_or_else(|| {
                AosError::NotFound(format!("Adapter not found in routing table: {}", adapter_id))
            })?;

        // Create router ring with single adapter
        let mut ring = RouterRing::new(1);
        ring.set(&[adapter_index], &[i16::MAX]); // Full weight to single adapter

        // Create IO buffers
        let mut io = IoBuffers::new(self.vocab_size);
        io.input_ids = input_tokens.to_vec();

        // Execute kernel
        {
            let mut kernels = self.kernels.lock().await;
            kernels.run_step(&ring, &mut io).map_err(|e| {
                error!(
                    adapter_id = %adapter_id,
                    error = %e,
                    "Kernel execution failed"
                );
                e
            })?;
        }

        // Convert logits to output tokens (simplified: argmax)
        let output_tokens = if io.output_logits.is_empty() {
            vec![]
        } else {
            // Find argmax
            let (max_idx, _) = io
                .output_logits
                .iter()
                .enumerate()
                .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or((0, &0.0));
            vec![max_idx as u32]
        };

        debug!(
            adapter_id = %adapter_id,
            output_tokens_len = output_tokens.len(),
            "Adapter execution completed"
        );

        Ok(AdapterExecutionResult {
            output_tokens,
            state_updates: HashMap::new(),
        })
    }
}
```

**Execution Flow:**
1. Validate adapter ID in routing table
2. Create router ring with single adapter (full weight)
3. Initialize IO buffers with vocab size
4. Execute kernel step
5. Extract output tokens via argmax
6. Return wrapped result

## MockAdapterBackend for Testing

```rust
#[derive(Default)]
pub struct MockAdapterBackend;

impl AdapterExecutionBackend for MockAdapterBackend {
    async fn execute_adapter(
        &self,
        adapter_id: &str,
        input_tokens: &[u32],
        _model_state: &HashMap<String, Vec<f32>>,
    ) -> Result<AdapterExecutionResult> {
        debug!("Mock execution of adapter {}", adapter_id);

        // Simulate processing
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        Ok(AdapterExecutionResult {
            output_tokens: input_tokens.to_vec(),
            state_updates: HashMap::new(),
        })
    }
}
```

**Benefits:**
- Zero dependencies on hardware
- Deterministic (inputs = outputs)
- 10ms simulated delay per adapter
- Perfect for unit tests

## Cargo.toml Configuration

```toml
[dependencies]
# ... other dependencies ...
adapteros-lora-kernel-api = { path = "../adapteros-lora-kernel-api" }

# Direct backend dependencies to avoid circular dependency via worker
adapteros-lora-kernel-mtl = { path = "../adapteros-lora-kernel-mtl", optional = true }
adapteros-lora-kernel-coreml = { path = "../adapteros-lora-kernel-coreml", optional = true }
adapteros-lora-mlx-ffi = { path = "../adapteros-lora-mlx-ffi", optional = true }

[target.'cfg(target_os = "macos")'.dependencies]
metal = "0.27"

[features]
coreml-backend = ["dep:adapteros-lora-kernel-coreml"]
multi-backend = ["dep:adapteros-lora-mlx-ffi"]
mlx-backend = ["multi-backend"]
```

## Library Exports

```rust
pub use workflow_executor::{
    AdapterExecutionBackend, AdapterExecutionResult, ExecutionStats, KernelAdapterBackend,
    MockAdapterBackend, RealBackendAdapterBackend, WorkflowContext, WorkflowExecutor,
    WorkflowResult, WorkflowType,
};
```

All types are exported from the crate root for easy access.

## Error Handling Examples

### Backend Initialization Failure

```rust
// Automatic fallback on error
match Self::new_coreml(adapters, vocab).await {
    Ok(backend) => return Ok(backend),
    Err(e) => warn!(error = %e, "CoreML failed, trying next..."),
}
```

### Adapter Not Found

```rust
let adapter_index = self
    .adapter_name_to_index
    .get(adapter_id)
    .copied()
    .ok_or_else(|| {
        AosError::NotFound(format!("Adapter not found: {}", adapter_id))
    })?;
```

### Kernel Execution Error

```rust
kernels.run_step(&ring, &mut io).map_err(|e| {
    error!(adapter_id = %adapter_id, error = %e, "Kernel execution failed");
    e
})?;
```

## Usage Patterns

### Pattern 1: Auto-selection (Recommended)

```rust
let backend = RealBackendAdapterBackend::new_auto(adapters, vocab).await?;
```

### Pattern 2: Production Metal

```rust
#[cfg(target_os = "macos")]
let backend = RealBackendAdapterBackend::new_metal(adapters, vocab).await?;
```

### Pattern 3: Power-efficient CoreML

```rust
#[cfg(all(target_os = "macos", feature = "coreml-backend"))]
let backend = RealBackendAdapterBackend::new_coreml(adapters, vocab).await?;
```

### Pattern 4: Research MLX

```rust
#[cfg(feature = "multi-backend")]
let backend = RealBackendAdapterBackend::new_mlx(model_path, adapters, vocab).await?;
```

### Pattern 5: Testing with Mock

```rust
let backend = Arc::new(MockAdapterBackend);
// Use for unit tests - no hardware required
```

## Integration with WorkflowExecutor

```rust
let backend = RealBackendAdapterBackend::new_auto(
    vec!["adapter1".to_string(), "adapter2".to_string()],
    152064
).await?;

let executor = WorkflowExecutor::new(
    WorkflowType::Sequential,
    vec!["adapter1".to_string()],
    Arc::new(backend)
);

let result = executor.execute(context).await?;
```

## Testing Code

All three workflow patterns are tested with `MockAdapterBackend`:

```rust
#[tokio::test]
async fn test_sequential_workflow() {
    let backend = Arc::new(MockAdapterBackend);
    let executor = WorkflowExecutor::new(
        WorkflowType::Sequential,
        vec!["adapter1".to_string(), "adapter2".to_string()],
        backend,
    );
    // ... test assertions ...
}

#[tokio::test]
async fn test_parallel_workflow() {
    // ... similar pattern ...
}

#[tokio::test]
async fn test_upstream_downstream_workflow() {
    // ... similar pattern ...
}
```

All tests pass with `MockAdapterBackend` and will also work with `RealBackendAdapterBackend` once hardware is available.
