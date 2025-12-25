# CoreML Backend for MoE Models

## Problem Statement

The MLX FFI layer in AdapterOS doesn't support Mixture of Experts (MoE) architectures.
The Qwen3-Coder-30B-A3B model uses 128 experts with top-8 routing, which fails at inference
with "Failed to run model forward with hidden states".

**mlx-lm works directly** - the model generates correctly when called via Python's mlx-lm,
proving the model itself is valid.

## Proposed Solution: CoreML Backend with MoE Support

Use CoreML's Neural Engine (ANE) backend instead of the custom MLX FFI. CoreML handles
MoE routing natively when the model is properly converted.

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                        Worker Process                            │
├─────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐       │
│  │ UDS Server   │───▶│ Backend      │───▶│ CoreML       │       │
│  │              │    │ Coordinator  │    │ Backend      │       │
│  └──────────────┘    └──────────────┘    └──────┬───────┘       │
│                                                  │               │
│                                                  ▼               │
│                                          ┌──────────────┐       │
│                                          │ .mlpackage   │       │
│                                          │ (MoE Model)  │       │
│                                          └──────────────┘       │
│                                                  │               │
│                                                  ▼               │
│                                          ┌──────────────┐       │
│                                          │ ANE / GPU    │       │
│                                          │ Execution    │       │
│                                          └──────────────┘       │
└─────────────────────────────────────────────────────────────────┘
```

## Implementation Steps

### Step 1: Convert MLX Model to CoreML Format

Create a conversion script using coremltools:

```python
#!/usr/bin/env python3
"""Convert MLX MoE model to CoreML .mlpackage format."""

import mlx.core as mx
from mlx_lm import load
import coremltools as ct
from coremltools.converters.mil import Builder as mb
import numpy as np

def convert_qwen3_moe_to_coreml(
    mlx_model_path: str,
    output_path: str,
    compute_units: str = "ALL"  # CPU_AND_NE, CPU_AND_GPU, ALL
):
    """
    Convert Qwen3 MoE model from MLX safetensors to CoreML .mlpackage.

    The MoE routing is handled by:
    1. Extracting expert weights from safetensors
    2. Building CoreML MIL graph with conditional expert dispatch
    3. Compiling to .mlpackage with ANE optimization
    """
    # Load MLX model to get config
    model, tokenizer = load(mlx_model_path)
    config = model.config

    # MoE parameters
    num_experts = config.num_experts  # 128
    num_experts_per_tok = config.num_experts_per_tok  # 8
    hidden_size = config.hidden_size  # 2048
    moe_intermediate_size = config.moe_intermediate_size  # 768

    # Build CoreML model using MIL
    @mb.program(
        input_specs=[
            mb.TensorSpec(shape=(1, "seq_len"), dtype=ct.int32),
        ]
    )
    def qwen3_moe_program(input_ids):
        # ... MIL implementation of MoE forward pass
        # This requires implementing:
        # 1. Embedding lookup
        # 2. RMSNorm layers
        # 3. Attention (GQA with RoPE)
        # 4. MoE routing + expert dispatch
        # 5. Final LM head
        pass

    # Convert to CoreML
    mlmodel = ct.convert(
        qwen3_moe_program,
        compute_units=getattr(ct.ComputeUnit, compute_units),
        minimum_deployment_target=ct.target.macOS15,
    )

    # Save as .mlpackage
    mlmodel.save(output_path)
    return output_path
```

### Step 2: Alternative - Use mlx-lm as Subprocess Bridge

A faster approach that avoids full model conversion:

```rust
// crates/adapteros-lora-worker/src/mlx_subprocess_bridge.rs

use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader, Write};

/// Bridge to mlx-lm Python process for MoE inference
pub struct MlxSubprocessBridge {
    child: std::process::Child,
    stdin: std::process::ChildStdin,
    stdout: BufReader<std::process::ChildStdout>,
}

impl MlxSubprocessBridge {
    pub fn new(model_path: &str) -> Result<Self> {
        let mut child = Command::new("python3")
            .args(["-c", include_str!("mlx_bridge_server.py")])
            .env("MLX_MODEL_PATH", model_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()?;

        let stdin = child.stdin.take().unwrap();
        let stdout = BufReader::new(child.stdout.take().unwrap());

        Ok(Self { child, stdin, stdout })
    }

    pub fn generate(&mut self, prompt: &str, max_tokens: usize) -> Result<String> {
        // Send request as JSON
        let request = serde_json::json!({
            "prompt": prompt,
            "max_tokens": max_tokens
        });
        writeln!(self.stdin, "{}", request)?;
        self.stdin.flush()?;

        // Read response
        let mut response = String::new();
        self.stdout.read_line(&mut response)?;
        let parsed: serde_json::Value = serde_json::from_str(&response)?;
        Ok(parsed["text"].as_str().unwrap_or("").to_string())
    }
}
```

### Step 3: Native CoreML MoE Support (Long-term)

Extend `crates/adapteros-lora-kernel-coreml` to support MoE:

```rust
// crates/adapteros-lora-kernel-coreml/src/moe.rs

/// MoE configuration parsed from config.json
#[derive(Debug, Clone)]
pub struct MoEConfig {
    pub num_experts: usize,
    pub num_experts_per_tok: usize,
    pub hidden_size: usize,
    pub moe_intermediate_size: usize,
}

/// Expert routing result
#[derive(Debug)]
pub struct ExpertRouting {
    /// Selected expert indices per token [batch, seq_len, k]
    pub expert_indices: Vec<usize>,
    /// Routing weights per token [batch, seq_len, k]
    pub routing_weights: Vec<f32>,
}

impl CoreMLBackend {
    /// Route tokens to experts using learned gating
    pub fn moe_route(&self, hidden_states: &MLTensor, config: &MoEConfig) -> Result<ExpertRouting> {
        // Gate projection: [batch, seq, hidden] -> [batch, seq, num_experts]
        let gate_logits = self.linear_projection(hidden_states, "gate")?;

        // Softmax and top-k selection
        let (weights, indices) = self.topk_softmax(&gate_logits, config.num_experts_per_tok)?;

        Ok(ExpertRouting {
            expert_indices: indices,
            routing_weights: weights,
        })
    }

    /// Execute MoE forward pass
    pub fn moe_forward(
        &self,
        hidden_states: &MLTensor,
        routing: &ExpertRouting,
        config: &MoEConfig,
    ) -> Result<MLTensor> {
        // For each token, dispatch to selected experts and combine
        let mut output = MLTensor::zeros_like(hidden_states)?;

        for (token_idx, (expert_idx, weight)) in
            routing.expert_indices.iter().zip(&routing.routing_weights).enumerate()
        {
            let expert_prefix = format!("experts.{}", expert_idx);
            let expert_out = self.mlp_forward(
                &hidden_states.slice(token_idx)?,
                &expert_prefix,
                config.moe_intermediate_size,
            )?;
            output.add_scaled(&expert_out, *weight)?;
        }

        Ok(output)
    }
}
```

## Recommended Approach: Phased Implementation

### Phase 1: Quick Win (1-2 days)
Use mlx-lm subprocess bridge for immediate MoE support:
- No model conversion needed
- Works with existing MLX models
- ~10-20% latency overhead from IPC

### Phase 2: CoreML Conversion (1 week)
Convert Qwen3-30B to CoreML .mlpackage:
- Use coremltools with custom MIL ops for MoE
- Optimize for ANE execution
- 2-3x faster than subprocess approach

### Phase 3: Native Integration (2-3 weeks)
Extend CoreML backend with native MoE support:
- Implement expert routing in Objective-C++/Swift
- Add MoE weight loading to safetensors loader
- Full parity with MLX FFI for dense models

## File Changes Required

### Phase 1 (Subprocess Bridge)
```
crates/adapteros-lora-worker/
├── src/
│   ├── mlx_subprocess_bridge.rs  # NEW: Python subprocess wrapper
│   ├── backend_factory.rs        # MOD: Add MlxSubprocess backend type
│   └── lib.rs                    # MOD: Integrate bridge into inference
```

### Phase 2 (CoreML Conversion)
```
scripts/
├── convert_mlx_to_coreml.py      # NEW: Model conversion script
├── requirements-convert.txt       # NEW: coremltools, mlx dependencies

manifests/
├── qwen3-30b-coreml.yaml         # NEW: CoreML manifest with MoE config
```

### Phase 3 (Native MoE)
```
crates/adapteros-lora-kernel-coreml/
├── src/
│   ├── moe.rs                    # NEW: MoE routing and dispatch
│   ├── lib.rs                    # MOD: Add MoE forward path
│   └── coreml_bridge.mm          # MOD: Add MoE Swift/ObjC calls
```

## Quick Test: Verify CoreML ANE Availability

```bash
# Check if ANE is available
./target/release/aos-worker --backend coreml --check-capabilities

# Expected output:
# CoreML: Available
# Neural Engine: Available (Apple Silicon)
# GPU: Available
# Compute Units: ALL
```

## Decision Point

Which phase should we implement first?

1. **Phase 1** - Subprocess bridge (fastest to working inference)
2. **Phase 2** - CoreML conversion (better performance, more work)
3. **Phase 3** - Native MoE (best performance, significant engineering)

Recommendation: **Start with Phase 1** to unblock benchmarking, then move to Phase 2.
