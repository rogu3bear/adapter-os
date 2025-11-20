# InferencePipeline Quick Start Guide

## Basic Setup

### 1. Default Configuration (No Adapters)
```rust
use adapteros_lora_worker::inference_pipeline::{
    InferencePipeline,
    InferencePipelineConfig,
};

let config = InferencePipelineConfig::default();
let pipeline = InferencePipeline::new(
    tokenizer_path,
    router,
    kernels,
    policy,
    telemetry,
    config,
    circuit_breaker
).await?;
```

### 2. With Initial Adapters
```rust
let mut config = InferencePipelineConfig::default();
config.adapter_base_path = Some(PathBuf::from("./var/adapters"));
config.initial_adapter_ids = vec![
    "python-general".to_string(),
    "rust-general".to_string(),
    "typescript-general".to_string(),
];

// These will be loaded from:
// - ./var/adapters/python-general.aos
// - ./var/adapters/rust-general.aos
// - ./var/adapters/typescript-general.aos

let pipeline = InferencePipeline::new(
    tokenizer_path,
    router,
    kernels,
    policy,
    telemetry,
    config,
    circuit_breaker
).await?;
```

## Runtime Operations

### Load Adapter (Hot-Swap)
```rust
let adapter_idx = pipeline.load_adapter(
    "django-specific",
    Path::new("./var/adapters/django-specific.aos")
).await?;

println!("Loaded adapter at index: {}", adapter_idx);
```

### Unload Adapter (Hot-Swap)
```rust
pipeline.unload_adapter("django-specific").await?;
println!("Adapter unloaded successfully");
```

### Query Loaded Adapters
```rust
let indices = pipeline.get_loaded_adapter_indices().await;
println!("Currently loaded: {:?}", indices);
```

## Running Inference

### Single Inference
```rust
use adapteros_lora_worker::inference_pipeline::InferenceRequest;

let request = InferenceRequest {
    prompt: "Write a Python function to calculate fibonacci".to_string(),
    max_tokens: 256,
    cpid: "req-001".to_string(),
    require_evidence: false,
    stack_id: Some("stack-python".to_string()),
    stack_version: Some(1),
};

let response = pipeline.infer(request).await?;

println!("Generated: {}", response.text);
println!("Tokens: {}", response.token_count);
println!("Latency: {}ms", response.latency_ms);
```

### Batch Inference
```rust
let requests = vec![
    InferenceRequest {
        prompt: "Python code for sorting".to_string(),
        max_tokens: 128,
        cpid: "req-001".to_string(),
        require_evidence: false,
        stack_id: None,
        stack_version: None,
    },
    InferenceRequest {
        prompt: "Rust code for async I/O".to_string(),
        max_tokens: 128,
        cpid: "req-002".to_string(),
        require_evidence: false,
        stack_id: None,
        stack_version: None,
    },
];

let responses = pipeline.infer_batch(requests).await?;
for (i, response) in responses.iter().enumerate() {
    println!("Response {}: {} tokens", i, response.token_count);
}
```

## Error Handling

### Graceful Degradation
```rust
// Missing adapter during init - logged as warning, pipeline continues
let mut config = InferencePipelineConfig::default();
config.adapter_base_path = Some(PathBuf::from("./adapters"));
config.initial_adapter_ids = vec![
    "existing-adapter".to_string(),
    "missing-adapter".to_string(),  // This will warn but not fail
];

let pipeline = InferencePipeline::new(...).await?;
// Pipeline created successfully with only existing adapters
```

### Handling Load Failures
```rust
match pipeline.load_adapter("new-adapter", path).await {
    Ok(idx) => println!("Loaded at index {}", idx),
    Err(AosError::Io(e)) => eprintln!("File error: {}", e),
    Err(AosError::Worker(e)) => eprintln!("Backend error: {}", e),
    Err(e) => eprintln!("Unexpected error: {}", e),
}
```

### Handling Unload Failures
```rust
match pipeline.unload_adapter("adapter-id").await {
    Ok(_) => println!("Unloaded successfully"),
    Err(AosError::Worker(msg)) if msg.contains("not loaded") => {
        eprintln!("Adapter was already unloaded");
    }
    Err(e) => eprintln!("Unload failed: {}", e),
}
```

## RouterRing Behavior

### k=0 (No Adapters Selected)
When the router selects zero adapters (no good matches), the pipeline:
1. Detects empty RouterRing
2. Logs debug message: "Router selected k=0 adapters, using base model only"
3. Passes empty ring to kernel
4. Kernel uses base model without adapters

### k=1 (Single Adapter)
Single adapter selection works naturally:
```rust
// Router selects 1 adapter
let decision = router.route(&features, &priors);
// decision.indices = [3]
// decision.gates_q15 = [32767] (full weight)

// Pipeline converts to RouterRing
let ring = RouterRing::from(&decision);
// ring.k = 1
// ring.indices[0] = 3
// ring.gates_q15[0] = 32767

// Kernel receives and executes with single adapter
kernels.run_step(&ring, &mut io_buffers)?;
```

### k=3 (Multiple Adapters)
Standard multi-adapter routing:
```rust
// Router selects 3 adapters with weights
let decision = router.route(&features, &priors);
// decision.indices = [0, 2, 5]
// decision.gates_q15 = [16384, 10923, 5460] (normalized to sum ≈ 32767)

// Pipeline converts to RouterRing
let ring = RouterRing::from(&decision);
// ring.k = 3
// Kernel fuses all 3 adapters with their gates

kernels.run_step(&ring, &mut io_buffers)?;
```

## Integration with Lifecycle Manager

### Coordinated Loading
```rust
// Lifecycle manager promotes adapter to Hot state
lifecycle_manager.promote_to_hot("adapter-id").await?;

// Load into pipeline for inference
let idx = pipeline.load_adapter(
    "adapter-id",
    Path::new(&adapter_path)
).await?;

// Now adapter is both Hot in lifecycle and loaded in pipeline
```

### Coordinated Unloading
```rust
// Evict from pipeline first
pipeline.unload_adapter("adapter-id").await?;

// Then demote in lifecycle manager
lifecycle_manager.demote_to_cold("adapter-id").await?;
```

## Configuration Examples

### Development Setup
```rust
let mut config = InferencePipelineConfig::default();
config.temperature = 0.7;
config.top_k = Some(50);
config.top_p = Some(0.95);
config.adapter_base_path = Some(PathBuf::from("./dev/adapters"));
config.initial_adapter_ids = vec![
    "python-debug".to_string(),
    "rust-debug".to_string(),
];
```

### Production Setup
```rust
let mut config = InferencePipelineConfig::default();
config.temperature = 0.2;  // Lower temperature for consistency
config.top_k = Some(40);
config.top_p = Some(0.9);
config.adapter_base_path = Some(PathBuf::from("/var/adapteros/adapters"));
config.initial_adapter_ids = vec![
    "tier-0-general".to_string(),
    "tier-1-specialized".to_string(),
];
```

### Minimal Setup (Base Model Only)
```rust
let config = InferencePipelineConfig {
    model_name: "Qwen2.5-7B-Instruct".to_string(),
    vocab_size: 152064,
    max_seq_len: 32768,
    temperature: 0.7,
    top_k: Some(50),
    top_p: Some(0.95),
    adapter_base_path: None,  // No adapters
    initial_adapter_ids: Vec::new(),
};
```

## Telemetry and Observability

### Router Decisions
```rust
// Automatically logged for each inference step
// Event: router.decision
// Fields:
// - step: usize
// - input_token_id: Option<u32>
// - candidate_adapters: Vec<RouterCandidate>
// - entropy: f32
// - stack_id: Option<String>
```

### Adapter Loading
```rust
// Logged via tracing::info!
info!(
    adapter_id = %adapter_id,
    adapter_idx = adapter_idx,
    hash = %adapter_hash.to_short_hex(),
    path = %adapter_path.display(),
    "Adapter loaded successfully"
);
```

### Kernel Execution
```rust
// Debug logging for each step
debug!(
    step = step,
    k = router_ring.len(),
    indices = ?router_ring.active_indices(),
    "Router selected {} adapter(s)", router_ring.len()
);
```

## Performance Tips

1. **Preload Frequently Used Adapters**
   ```rust
   config.initial_adapter_ids = vec![
       "tier-0-frequent".to_string(),
       "tier-1-frequent".to_string(),
   ];
   ```

2. **Batch Similar Requests**
   ```rust
   let responses = pipeline.infer_batch(requests).await?;
   ```

3. **Monitor Adapter Usage**
   ```rust
   let indices = pipeline.get_loaded_adapter_indices().await;
   if indices.len() > max_loaded {
       // Unload least recently used
   }
   ```

## Common Patterns

### Lazy Loading
```rust
// Start with no adapters
let pipeline = InferencePipeline::new(...).await?;

// Load on first use
async fn ensure_adapter_loaded(
    pipeline: &mut InferencePipeline,
    adapter_id: &str,
    adapter_path: &Path,
) -> Result<u16> {
    match pipeline.get_loaded_adapter_indices().await.iter()
        .find(|&&idx| /* check if adapter is loaded */) {
        Some(&idx) => Ok(idx),
        None => pipeline.load_adapter(adapter_id, adapter_path).await,
    }
}
```

### Adapter Rotation
```rust
// Unload old, load new
pipeline.unload_adapter("old-adapter").await?;
pipeline.load_adapter(
    "new-adapter",
    Path::new("./adapters/new-adapter.aos")
).await?;
```

### Conditional Loading
```rust
// Load based on prompt analysis
let prompt_lang = detect_language(&request.prompt);
let adapter_id = format!("{}-general", prompt_lang);
let adapter_path = base_path.join(format!("{}.aos", adapter_id));

if adapter_path.exists() {
    pipeline.load_adapter(&adapter_id, &adapter_path).await?;
}
```

## Troubleshooting

### "Adapter file not found"
```
WARN adapter_id="python-general" path="./adapters/python-general.aos"
     "Adapter file not found, skipping"
```
**Solution:** Check file path and ensure .aos file exists

### "Backend failed to load adapter"
```
ERROR AosError::Worker("Backend failed to load adapter: Invalid safetensors")
```
**Solution:** Verify .aos file is valid (correct format, not corrupted)

### "Adapter index overflow"
```
ERROR AosError::Worker("Adapter index overflow")
```
**Solution:** Too many adapters loaded (>65535), unload some before loading more

### "Kernel failed at step N"
```
ERROR step=42 error="GPU memory allocation failed" k=3 "Kernel execution failed"
```
**Solution:** Reduce number of loaded adapters or model size

---

**Reference:** See [PIPELINE_UPDATE_SUMMARY.md](./PIPELINE_UPDATE_SUMMARY.md) for detailed technical documentation.
