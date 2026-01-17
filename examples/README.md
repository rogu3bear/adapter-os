# adapterOS Examples

This directory contains example code and data files demonstrating how to use adapterOS features.

## Files

### Training Examples
- `basic_inference.rs` - Basic inference with a single adapter
- `lora_routing.rs` - Multi-adapter routing example
- `patch_proposal_basic.rs` - Basic patch proposal workflow
- `patch_proposal_advanced.rs` - Advanced patch proposal with validation
- `patch_proposal_api.rs` - Patch proposal API integration

### Calibration Examples
- `router_calibration_example.json` - Sample calibration dataset for router weight optimization

### Diagnostics Examples
- `diagnostics/README.md` - Diagnostic bundle and export workflows

### Manifests
- `manifests/qwen7b.json` - Qwen 2.5-7B model manifest example

## Router Calibration Example

The `router_calibration_example.json` file demonstrates the format for router calibration datasets.

### Feature Vector Structure (21 dimensions)

**Language (indices 0-7):** One-hot encoding
- 0: Python
- 1: JavaScript
- 2: Rust
- 3: Go
- 4: Java
- 5: C++
- 6: TypeScript
- 7: Other

**Framework (indices 8-10):** Scores (0.0-1.0)
- 8: Framework 1 (e.g., Django, React)
- 9: Framework 2 (e.g., Flask, Vue)
- 10: Framework 3 (e.g., FastAPI, Angular)

**Code Context (indices 11-12):** Continuous values
- 11: Symbol hits (0.0-1.0)
- 12: Path tokens (0.0-1.0)

**Prompt Verb (indices 13-20):** One-hot encoding
- 13: Write
- 14: Fix
- 15: Refactor
- 16: Add
- 17: Remove
- 18: Update
- 19: Optimize
- 20: Explain

### Usage

```bash
# Calibrate router weights
aosctl router calibrate \
  --dataset examples/router_calibration_example.json \
  --output my_weights.json \
  --method grid-search \
  --k 3

# Validate on test set
aosctl router validate \
  --dataset examples/test_data.json \
  --weights my_weights.json

# Show calibrated weights
aosctl router show --weights my_weights.json
```

### Creating Your Own Dataset

1. Collect prompts with known optimal adapter selections
2. Extract features using `adapteros_lora_router::CodeFeatures`
3. Record ground truth adapter IDs
4. Save in JSON format following the example structure

Example code:

```rust
use adapteros_lora_router::{CodeFeatures, CalibrationSample, CalibrationDataset};

let mut samples = Vec::new();

// For each training example
let features = CodeFeatures::from_prompt_and_context(&prompt, &context);
let feature_vec = features.to_vector();

samples.push(CalibrationSample {
    features: feature_vec,
    ground_truth_adapters: vec![0, 2], // Adapters that worked best
    metadata: serde_json::json!({
        "prompt": prompt,
        "language": "python",
        "framework": "django",
        "task": "write"
    }),
});

// Save dataset
let dataset = CalibrationDataset { samples };
dataset.save("my_dataset.json")?;
```

## Running Examples

### Basic Inference

```bash
cargo run --example basic_inference
```

### LoRA Routing

```bash
cargo run --example lora_routing
```

### Patch Proposals

```bash
# Basic
cargo run --example patch_proposal_basic

# Advanced with validation
cargo run --example patch_proposal_advanced

# API integration
cargo run --example patch_proposal_api
```

## Additional Resources

- **Usage Guide:** `docs/P3_USAGE_GUIDE.md` - Comprehensive feature usage documentation
- **Implementation:** `P3_IMPLEMENTATION_COMPLETE.md` - Complete implementation details
- **Architecture:** `docs/ARCHITECTURE.md` - System architecture overview
- **Production:** `docs/PRODUCTION_READINESS.md` - Deployment guidelines

## Contributing Examples

When adding new examples:

1. Follow the existing naming convention
2. Include comprehensive comments
3. Add to this README
4. Ensure examples compile and run
5. Use realistic data and scenarios

## Questions?

See the main documentation in `/docs/` or the P3 usage guide for detailed information on all features.
