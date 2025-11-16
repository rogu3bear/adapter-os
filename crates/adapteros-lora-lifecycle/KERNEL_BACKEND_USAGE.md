# Kernel Backend Usage Guide

This document explains how to use the `KernelAdapterBackend` for real kernel-based workflow execution with Metal/MLX kernels.

## Architecture Overview

The workflow execution system supports two backends:

1. **MockAdapterBackend** - For testing and coordination (no actual LoRA transformations)
2. **KernelAdapterBackend** - For production execution with real Metal/MLX kernels

## Using KernelAdapterBackend

### Prerequisites

- Access to initialized `FusedKernels` (Metal or MLX)
- Adapter name to index mapping
- Vocabulary size (e.g., 152064 for Qwen2.5)

### Example: Standalone Usage

```rust
use adapteros_lora_kernel_mtl::MetalKernels;
use adapteros_lora_lifecycle::{
    KernelAdapterBackend, WorkflowExecutor, WorkflowType, WorkflowContext
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize kernels
    let kernels = MetalKernels::new(/* config */)?;
    let kernels_arc = Arc::new(Mutex::new(kernels));

    // 2. Define adapter names (must match kernel loading order)
    let adapter_names = vec![
        "code_review".to_string(),
        "documentation".to_string(),
        "bug_detection".to_string(),
    ];

    // 3. Create kernel backend
    let backend = Arc::new(KernelAdapterBackend::new(
        kernels_arc.clone(),
        adapter_names.clone(),
        152064  // Qwen2.5 vocab size
    ));

    // 4. Create workflow executor
    let executor = WorkflowExecutor::new(
        WorkflowType::UpstreamDownstream,
        adapter_names,
        backend
    );

    // 5. Create execution context
    let context = WorkflowContext {
        input_tokens: vec![100, 200, 300],
        model_state: HashMap::new(),
        metadata: HashMap::new(),
    };

    // 6. Execute workflow
    let result = executor.execute(context).await?;

    println!("Executed {} adapters in {} phases",
        result.stats.adapters_executed,
        result.stats.phases.len()
    );

    Ok(())
}
```

## Worker Integration

The `Worker` struct uses `Arc<Mutex<K>>` for kernels, enabling shared access with workflows. The `Worker::execute_workflow()` method uses real `KernelAdapterBackend` for production execution.

### Implementation (Option A - COMPLETED)

Worker has been refactored to use Arc<Mutex<K>>:

```rust
pub struct Worker<K: FusedKernels> {
    /// Kernels wrapped in Arc<Mutex<>> for shared access with workflows
    kernels: Arc<tokio::sync::Mutex<K>>,
    // ... other fields
}

impl<K: FusedKernels> Worker<K> {
    pub async fn execute_workflow(
        &self,
        workflow_type: WorkflowType,
        adapter_ids: Vec<String>,
        context: WorkflowContext,
    ) -> Result<WorkflowResult>
    where
        K: Send + Sync,  // Required for thread-safe sharing
    {
        use adapteros_lora_lifecycle::{KernelAdapterBackend, WorkflowExecutor};

        // Create kernel backend with shared access
        let adapter_names: Vec<String> = self
            .manifest
            .adapters
            .iter()
            .map(|a| a.id.clone())
            .collect();

        let backend = Arc::new(KernelAdapterBackend::new(
            self.kernels.clone(),  // ✅ Now works!
            adapter_names,
            152064  // Qwen2.5 vocab size
        ));

        let executor = WorkflowExecutor::new(workflow_type, adapter_ids, backend);
        executor.execute(context).await
    }
}
```

**Implementation Changes**:
All kernel usage in Worker locks the mutex:
```rust
// Kernel execution in Worker::infer_internal():
let kernel_start = Instant::now();
{
    let mut kernels = self.kernels.lock().await;
    kernels.run_step(&router_ring, &mut io_buffers)?;
}
let kernel_duration = kernel_start.elapsed();
```

**Status**: ✅ **COMPLETED** - Worker now uses Arc<Mutex<K>> and execute_workflow() uses real KernelAdapterBackend

### Alternative: Separate Workflow Instances (Option B)

If needed, you can create workflow executors outside Worker with separate kernel instances:

```rust
// In application code:
let worker = Worker::new(/* ... */)?;
let workflow_kernels = MetalKernels::new(/* same config */)?;

let backend = Arc::new(KernelAdapterBackend::new(
    Arc::new(Mutex::new(workflow_kernels)),
    adapter_names,
    152064
));

let executor = WorkflowExecutor::new(WorkflowType::Parallel, adapter_ids, backend);
let result = executor.execute(context).await?;
```

**Pros**: Independent kernel instances for isolation
**Cons**: Duplicate kernel instances, higher memory usage

### Testing: Use Mock Backend (Option C)

For testing workflow logic without real kernel execution:

```rust
let backend = Arc::new(MockAdapterBackend);
let executor = WorkflowExecutor::new(workflow_type, adapter_ids, backend);
```

**Use when**:
- Unit testing workflow logic
- Testing routing/selection only (no inference)
- Prototyping workflow strategies

## Workflow Types

### Sequential
Adapters execute one after another, output feeds into next:
```rust
WorkflowType::Sequential
// upstream_1 → upstream_2 → downstream_1
```

### Parallel
All adapters execute simultaneously, results merged:
```rust
WorkflowType::Parallel
// upstream_1 ─┐
// upstream_2 ─┼→ merged output
// upstream_3 ─┘
```

### UpstreamDownstream
Two-phase execution with data flow:
```rust
WorkflowType::UpstreamDownstream
// Phase 1: upstream_1 + upstream_2 → intermediate output
// Phase 2: downstream_1 + downstream_2 → final output
```

## Performance Considerations

1. **Kernel Locking**: KernelAdapterBackend locks kernels during execution. Consider:
   - Sequential workflows minimize lock contention
   - Parallel workflows may block on kernel access

2. **Memory**: Each KernelAdapterBackend holds Arc<Mutex<K>>, allowing shared access without duplication

3. **Execution Model**: Current implementation runs single inference steps. For autoregressive generation, integrate with Worker's generation loop.

## Testing

Use MockAdapterBackend for unit tests:

```rust
#[tokio::test]
async fn test_upstream_downstream_workflow() {
    let backend = Arc::new(MockAdapterBackend);
    let executor = WorkflowExecutor::new(
        WorkflowType::UpstreamDownstream,
        vec!["adapter_1".to_string(), "adapter_2".to_string()],
        backend
    );

    let result = executor.execute(context).await.unwrap();
    assert_eq!(result.stats.phases.len(), 2);
}
```

## Future Enhancements

- [x] **Refactor Worker to use Arc<Mutex<K>> for kernels** - ✅ COMPLETED
- [ ] Add autoregressive generation support to workflows
- [ ] Implement kernel pooling for parallel workflows
- [ ] Add workflow-level caching and optimization
- [ ] Support dynamic adapter selection during workflow execution
- [ ] Add batch workflow execution for multiple requests
- [ ] Implement workflow result streaming
