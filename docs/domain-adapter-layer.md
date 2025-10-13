# Domain Adapter Layer

## Overview

The Domain Adapter Layer provides high-level, domain-specific abstractions that translate deterministic tensor operations into practical functions for text, vision, and telemetry processing. All domain adapters maintain full reproducibility guarantees: identical input → identical output, byte-for-byte.

## Architecture

```
┌────────────────────────────────────────────────────────────────────────┐
│                          External Data                                 │
│              (Text, Images, Time-Series Signals)                       │
└────────────┬───────────────────────────────────────────────────────────┘
             │
             ▼
┌────────────────────────────────────────────────────────────────────────┐
│                      Domain Adapter Layer                              │
│                                                                        │
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐           │
│  │TextAdapter   │    │VisionAdapter │    │TelemetryAda..│           │
│  │              │    │              │    │              │           │
│  │• Tokenization│    │• Image Norm  │    │• Signal Norm │           │
│  │• LoRA Merge  │    │• Conv Pipeline│   │• Filtering   │           │
│  │• Canonical   │    │• Quantization│    │• Anomaly Det │           │
│  └──────────────┘    └──────────────┘    └──────────────┘           │
│                                                                        │
│                      DomainAdapter Trait                               │
│  • prepare()   - Initialize with deterministic executor               │
│  • forward()   - Deterministic tensor transformation                  │
│  • postprocess() - Canonical output formatting                        │
│  • epsilon_stats() - Numerical drift tracking                         │
│  • reset()     - Clear state for next run                             │
└────────────┬───────────────────────────────────────────────────────────┘
             │
             ▼
┌────────────────────────────────────────────────────────────────────────┐
│                    Deterministic Core                                  │
│                                                                        │
│  • DeterministicExecutor - Serial task execution                      │
│  • Hash Graph - Canonical tensor ordering                             │
│  • Trace System - Event logging with BLAKE3                           │
│  • Numerics - Epsilon tracking and bounds                             │
└────────────┬───────────────────────────────────────────────────────────┘
             │
             ▼
┌────────────────────────────────────────────────────────────────────────┐
│                        Metal Kernels                                   │
│                  (Fused Attention, MLP, LoRA)                          │
└────────────────────────────────────────────────────────────────────────┘
```

## Core Trait: `DomainAdapter`

All domain adapters implement the `DomainAdapter` trait:

```rust
pub trait DomainAdapter: Send + Sync {
    /// Get adapter name
    fn name(&self) -> &str;
    
    /// Get adapter metadata
    fn metadata(&self) -> &AdapterMetadata;
    
    /// Prepare adapter for execution
    fn prepare(&mut self, executor: &mut DeterministicExecutor) -> Result<()>;
    
    /// Forward pass (deterministic)
    fn forward(&mut self, input: &TensorData) -> Result<TensorData>;
    
    /// Postprocess output
    fn postprocess(&mut self, output: &TensorData) -> Result<TensorData>;
    
    /// Get epsilon statistics
    fn epsilon_stats(&self) -> Option<EpsilonStats>;
    
    /// Reset adapter state
    fn reset(&mut self);
    
    /// Generate trace event
    fn create_trace_event(...) -> Event;
}
```

## Text Adapter

### Purpose
Deterministic text processing with:
- Canonical UTF-8 normalization (NFC)
- Deterministic BPE tokenization
- LoRA weight merging
- Text-to-tensor conversion

### Configuration

**Manifest** (`text_example.toml`):
```toml
[adapter]
name = "text_adapter_v1"
version = "1.0.0"
model = "mlx_lora_base_v1"
hash = "b3d9c2a1e8f7d6b5a4938271605e4f3c2d1b0a9e8f7d6c5b4a3928170605"
input_format = "UTF8 canonical"
output_format = "BPE deterministic"
epsilon_threshold = 1e-6
deterministic = true

[adapter.parameters]
vocab_size = 32000
max_sequence_length = 2048
```

### Usage

```rust
use adapteros_domain::{TextAdapter, DomainAdapter};

// Load adapter
let mut adapter = TextAdapter::load("manifest.toml")?;

// Prepare with deterministic executor
adapter.prepare(&mut executor)?;

// Convert text to tensor
let input = text_to_tensor(&adapter, "Hello World")?;

// Forward pass (deterministic)
let output = adapter.forward(&input)?;

// Postprocess
let final_output = adapter.postprocess(&output)?;
```

### Determinism Guarantees

1. **Tokenization**: Hash-based token IDs ensure identical text → identical tokens
2. **Normalization**: Unicode NFC normalization for canonical form
3. **LoRA Merge**: Fixed merge order from router output
4. **Tensor Format**: Padded/truncated to max_sequence_length deterministically

## Vision Adapter

### Purpose
Deterministic image processing with:
- Canonical NCHW layout
- Deterministic normalization (ImageNet mean/std)
- Quantized convolution pipeline
- Image-to-tensor conversion

### Configuration

**Manifest** (`vision_example.toml`):
```toml
[adapter]
name = "vision_adapter_v1"
version = "1.0.0"
model = "resnet50_quantized"
hash = "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0"
input_format = "NCHW canonical"
output_format = "NCHW quantized"
epsilon_threshold = 1e-5

[adapter.parameters]
image_height = 224
image_width = 224
num_channels = 3
normalization_mean = [0.485, 0.456, 0.406]
normalization_std = [0.229, 0.224, 0.225]
```

### Usage

```rust
use adapteros_domain::{VisionAdapter, image_to_tensor};

// Load adapter
let mut adapter = VisionAdapter::load("manifest.toml")?;
adapter.prepare(&mut executor)?;

// Convert image bytes to tensor
let image_data = std::fs::read("image.jpg")?;
let input = image_to_tensor(&adapter, &image_data)?;

// Forward pass
let output = adapter.forward(&input)?;
```

### Determinism Guarantees

1. **Layout**: Canonical NCHW (batch, channels, height, width)
2. **Resize**: Deterministic interpolation with fixed rounding
3. **Normalization**: Fixed mean/std per channel
4. **Quantization**: Fixed-point arithmetic for convolution

## Telemetry Adapter

### Purpose
Deterministic signal processing with:
- Deterministic signal normalization
- Canonical time-series ordering
- Quantized filtering
- Anomaly detection with fixed thresholds

### Configuration

**Manifest** (`telemetry_example.toml`):
```toml
[adapter]
name = "telemetry_adapter_v1"
version = "1.0.0"
model = "timeseries_lstm_v1"
hash = "f0e9d8c7b6a5f4e3d2c1b0a9f8e7d6c5b4a3f2e1d0c9b8a7f6e5d4c3b2a1"
input_format = "time_series_canonical"
output_format = "normalized_filtered"

[adapter.parameters]
num_channels = 16
window_size = 128
sampling_rate = 100.0
filter_kernel_size = 5
anomaly_threshold = 0.95
```

### Usage

```rust
use adapteros_domain::{TelemetryAdapter, timeseries_to_tensor};

// Load adapter
let mut adapter = TelemetryAdapter::load("manifest.toml")?;
adapter.prepare(&mut executor)?;

// Create time-series tensor: [batch, channels, time_steps]
let signal_data: Vec<f32> = /* sensor readings */;
let input = timeseries_to_tensor(16, 128, &signal_data)?;

// Forward pass (normalize + filter)
let output = adapter.forward(&input)?;

// Postprocess (anomaly detection)
let final_output = adapter.postprocess(&output)?;
```

### Determinism Guarantees

1. **Normalization**: Fixed min/max per channel
2. **Filtering**: Moving average with fixed kernel size
3. **Ordering**: Canonical time-step ordering
4. **Anomaly Detection**: Fixed threshold (no statistical estimation)

## Adapter Registry

The `AdapterRegistry` manages multiple adapters:

```rust
use adapteros_domain::AdapterRegistry;

let mut registry = AdapterRegistry::new();

// Register adapters
registry.register(Box::new(text_adapter))?;
registry.register(Box::new(vision_adapter))?;
registry.register(Box::new(telemetry_adapter))?;

// Get adapter by name
let adapter = registry.get_mut("text_adapter_v1").unwrap();
let output = adapter.forward(&input)?;

// List all adapters
let names = registry.list_adapters();
```

## Tensor Data Format

All adapters use `TensorData` with automatic hash verification:

```rust
pub struct TensorData {
    pub tensor: Tensor,
    pub metadata: TensorMetadata,
}

pub struct TensorMetadata {
    pub hash: B3Hash,           // BLAKE3 hash for verification
    pub shape: Vec<usize>,       // Tensor dimensions
    pub dtype: String,           // Data type (e.g., "f32")
    pub element_count: usize,    // Total elements
    pub custom: HashMap<...>,    // Additional metadata
}

// Automatic hash verification
assert!(tensor_data.verify_hash());
```

## Determinism Verification

### Test Suite: `tests/domain_determinism.rs`

The test suite runs each adapter 100 times with identical inputs and verifies:

1. **Byte-identical outputs**: All runs produce identical tensor data
2. **Hash stability**: Tensor hashes remain constant
3. **Cross-adapter isolation**: Adapters don't interfere with each other
4. **Epsilon bounds**: Numerical drift stays within thresholds

### Running Tests

```bash
# Run all domain adapter determinism tests
cargo test --test domain_determinism -- --nocapture

# Run specific adapter test
cargo test --test domain_determinism test_text_adapter_determinism
```

### Expected Output

```
running 6 tests
Text adapter run 100/100
✅ Text adapter determinism verified: 100 identical runs
Vision adapter run 100/100
✅ Vision adapter determinism verified: 100 identical runs
Telemetry adapter run 100/100
✅ Telemetry adapter determinism verified: 100 identical runs
✅ Tensor hash stability verified
✅ Cross-adapter isolation verified
✅ Epsilon bounds verified
test result: ok. 6 passed; 0 failed
```

## Epsilon Tracking

Domain adapters integrate with the numerics layer for automatic epsilon tracking:

```rust
// After forward pass
if let Some(stats) = adapter.epsilon_stats() {
    println!("L2 error: {}", stats.l2_error);
    println!("Max error: {}", stats.max_error);
    println!("Mean error: {}", stats.mean_error);
    
    if stats.exceeds_threshold(1e-6) {
        warn!("Epsilon threshold exceeded!");
    }
}
```

## Trace Integration

All adapter operations are logged to the trace system:

```rust
// Automatically creates trace events
let event = adapter.create_trace_event(
    tick_id,
    "text.forward".to_string(),
    &inputs,
    &outputs,
);

// Event includes:
// - Operation ID
// - Input/output hashes
// - Adapter metadata
// - Epsilon statistics
```

## Manifest System

### Structure

```toml
[adapter]
name = "adapter_name"
version = "1.0.0"
model = "model_identifier"
hash = "blake3_hash_hex"
input_format = "format_description"
output_format = "format_description"
epsilon_threshold = 1e-6
deterministic = true

[adapter.model_files]
weights = "path/to/weights.safetensors"
config = "path/to/config.json"

[adapter.parameters]
param1 = 100
param2 = "value"
```

### Loading

```rust
use adapteros_domain::manifest::load_manifest;

let manifest = load_manifest("manifest.toml")?;

// Access configuration
let vocab_size = manifest.get_parameter_i64("vocab_size")?;
let weights_path = manifest.get_model_file("weights")?;
```

### Validation

Manifests are automatically validated:
- Required fields present
- Hash format valid
- Epsilon threshold positive
- Deterministic flag set

## Integration with Runtime

### Attaching to Executor

```rust
use adapteros_deterministic_exec::{DeterministicExecutor, ExecutorConfig};
use adapteros_domain::TextAdapter;

// Create executor with fixed seed
let config = ExecutorConfig {
    global_seed: [42u8; 32],
    ..Default::default()
};
let mut executor = DeterministicExecutor::new(config);

// Load and prepare adapter
let mut adapter = TextAdapter::load("manifest.toml")?;
adapter.prepare(&mut executor)?;

// Adapter receives deterministic seed via HKDF derivation
// Label: "text_adapter:{adapter_name}"
```

### Event Logging

All adapter operations are logged to telemetry:

```rust
// Automatic event logging on forward pass
adapter.forward(&input)?;

// Events logged:
// - "text.forward" or "vision.forward" or "telemetry.forward"
// - Input tensor hash
// - Output tensor hash
// - Epsilon statistics
// - Execution time
```

## Best Practices

### 1. Always Reset Between Runs

```rust
for input in inputs {
    adapter.reset();  // Clear state
    let output = adapter.forward(&input)?;
}
```

### 2. Verify Hashes

```rust
let output = adapter.forward(&input)?;
assert!(output.verify_hash(), "Hash verification failed");
```

### 3. Check Epsilon Bounds

```rust
if let Some(stats) = adapter.epsilon_stats() {
    if stats.exceeds_threshold(epsilon_threshold) {
        return Err(DomainAdapterError::NumericalErrorThreshold {
            error: stats.l2_error,
            threshold: epsilon_threshold,
        });
    }
}
```

### 4. Use Canonical Formats

```rust
// ✓ Good: Canonical UTF-8
let text = adapter.normalize_text(input_text);

// ✗ Bad: Raw bytes without normalization
let tensor = Tensor::new(input_text.as_bytes(), shape);
```

### 5. Document Custom Adapters

When creating new adapters:
- Document input/output formats
- Specify epsilon thresholds
- Include example manifests
- Add determinism tests

## Crate Organization

```
crates/adapteros-domain/
├── Cargo.toml
├── src/
│   ├── lib.rs              # Public API
│   ├── adapter.rs          # DomainAdapter trait
│   ├── error.rs            # Error types
│   ├── manifest.rs         # Manifest loading/validation
│   ├── text.rs             # TextAdapter implementation
│   ├── vision.rs           # VisionAdapter implementation
│   └── telemetry.rs        # TelemetryAdapter implementation
├── manifests/
│   ├── text_example.toml
│   ├── vision_example.toml
│   └── telemetry_example.toml
└── tests/
    └── domain_determinism.rs
```

## Summary

The Domain Adapter Layer provides:

✅ **Deterministic transformations**: Identical input → identical output  
✅ **Epsilon tracking**: Automatic numerical drift monitoring  
✅ **Trace integration**: All operations logged with BLAKE3 hashing  
✅ **Manifest-driven**: Configuration via validated TOML files  
✅ **Comprehensive testing**: 100-run determinism verification  
✅ **Modular design**: Easy to add new domain adapters  

**Status**: Fully implemented and tested. Ready for integration with AdapterOS runtime.

