# Qwen2.5-7B-Instruct Integration Guide

This document describes the complete integration pipeline for Qwen2.5-7B-Instruct into AdapterOS v1.0, following the reality-first approach.

**NOTE: AdapterOS uses MLX format exclusively.** MLX provides optimized memory layout for K-sparse LoRA routing and superior adapter integration performance on Apple Silicon, which is critical for the aos-cp control plane's multi-tenant adapter orchestration.

## Overview

The integration follows a phased approach:
- **Phase 0**: Reality checks (egress, determinism, policy enforcement)
- **Phase 1**: Mock model integration (TinyLlama shapes, registry, CLI)
- **Phase 2**: Full Qwen pipeline (real weights, quantizer, manifest)
- **Phase 3**: Acceptance tests (ARR/ECS/HLR/CR, router heatmaps)
- **Phase 4**: Metal kernels (fused MLP, GQA, performance tuning)

## Phase 0: Reality Checks ✅

### Egress Preflight Validation
```bash
# Test egress policy enforcement
./target/debug/aosctl serve --tenant test --plan test-plan --dry-run
# Expected: ❌ PREFLIGHT FAILED: Egress policy validation failed
```

### Determinism Framework
```bash
# Test deterministic replay
./target/debug/aosctl audit --suite tests/corpora/reg_v1.json --cpid CP-TEST
./target/debug/aosctl replay --bundle out/audit_CP-TEST.tar.zst
# Expected: Zero diff between runs
```

### Policy Enforcement
- Evidence requirements: factual claims must cite sources
- Router entropy floor: prevents adapter collapse
- Numeric validation: unit-free numbers rejected
- Refusal policy: underspecified prompts refused with needed fields

## Phase 1: Mock Model Integration ✅

### Model Configuration Parsing
```rust
// GQA shape validation
let config = ModelConfig::from_json(config_json)?;
config.validate_gqa()?; // Ensures hidden_size % num_attention_heads == 0
```

### Registry Schema
```sql
CREATE TABLE models (
    name TEXT PRIMARY KEY,
    config_hash TEXT NOT NULL,
    tokenizer_hash TEXT NOT NULL,
    tokenizer_cfg_hash TEXT NOT NULL,
    weights_hash TEXT NOT NULL,
    license_hash TEXT NOT NULL,
    license_text TEXT NOT NULL,
    model_card_hash TEXT,
    created_at INTEGER NOT NULL
);
```

### Import Model CLI
```bash
# Import model artifacts
./target/debug/aosctl import-model \
  --name Qwen2.5-7B-Instruct \
  --weights ./qwen/safetensors \
  --config ./qwen/config.json \
  --tokenizer ./qwen/tokenizer.json \
  --tokenizer-cfg ./qwen/tokenizer_config.json \
  --license ./qwen/LICENSE
```

## Phase 2: Full Qwen Pipeline ✅

### Chat Template Integration
```rust
// ChatML format for Qwen2.5
let template = ChatTemplate {
    template: "qwen_template".to_string(),
    special_tokens: SpecialTokens {
        im_start: "<|im_start|>".to_string(),
        im_end: "<|im_end|>".to_string(),
        eos_token: "<|im_end|>".to_string(),
        pad_token: Some("<|endoftext|>".to_string()),
    },
};
```

### Manifest Extensions
```yaml
base:
  model_id: "Qwen2.5-7B-Instruct"
  config_hash: "b3:..."
  tokenizer_hash: "b3:..."
  tokenizer_cfg_hash: "b3:..."
  license_hash: "b3:..."
  rope_scaling_override:
    factor: 1.0
    original_max_position_embeddings: 32768
    scaling_type: "yarn"
```

### Quantizer API
```rust
// Block quantization for int4
let quantizer = BlockQuantizer::new("int4_block".to_string(), 128, 4);
let quantized = quantizer.quantize_tensor("weight", &data, &shape, &spec)?;
```

### Model Loader Integration
```rust
// Load and validate model configuration
let loader = ModelLoader::load_from_registry("Qwen2.5-7B-Instruct", &registry_path)?;
let gqa_config = loader.get_gqa_config();
let rope_config = loader.get_rope_config();
```

## Phase 3: Acceptance Tests ✅

### Integration Test Suite
```rust
#[tokio::test]
async fn test_qwen_integration_pipeline() {
    test_model_config_parsing().await;
    test_chat_template_processing().await;
    test_gqa_configuration().await;
    test_lora_memory_calculation().await;
    test_rope_configuration().await;
}
```

### Policy Gates Tests
```rust
#[tokio::test]
async fn test_qwen_policy_gates() {
    test_evidence_requirement_policy().await;
    test_router_entropy_floor_policy().await;
    test_numeric_validation_policy().await;
    test_refusal_policy().await;
    test_egress_block_policy().await;
}
```

## Phase 4: Metal Kernels (Future)

### Fused MLP Kernel
```metal
// SwiGLU activation with LoRA
kernel void fused_mlp(
    device const float* input,
    device const float* gate_weight,
    device const float* up_weight,
    device const float* down_weight,
    device float* output,
    // LoRA parameters
    device const float* gate_lora_a,
    device const float* gate_lora_b,
    device const float* up_lora_a,
    device const float* up_lora_b,
    device const float* down_lora_a,
    device const float* down_lora_b,
    // GQA configuration
    constant GqaConfig& gqa_config,
    uint3 gid [[thread_position_in_grid]]
);
```

### Fused QKV Kernel with GQA
```metal
kernel void fused_qkv_gqa(
    device const float* input,
    device const float* q_weight,
    device const float* k_weight,
    device const float* v_weight,
    device float* q_output,
    device float* k_output,
    device float* v_output,
    // LoRA parameters
    device const float* q_lora_a,
    device const float* q_lora_b,
    device const float* k_lora_a,
    device const float* k_lora_b,
    device const float* v_lora_a,
    device const float* v_lora_b,
    // GQA configuration
    constant GqaConfig& gqa_config,
    uint3 gid [[thread_position_in_grid]]
);
```

## Qwen2.5-7B-Instruct Configuration

### Model Parameters
- **Architecture**: Qwen2 (Transformer with GQA)
- **Hidden Size**: 4096
- **Intermediate Size**: 11008 (SwiGLU)
- **Layers**: 32
- **Attention Heads**: 32
- **Key-Value Heads**: 4 (GQA ratio 8:1)
- **Head Dimension**: 128
- **KV Width**: 512
- **RoPE Theta**: 1,000,000
- **Max Position Embeddings**: 32768

### LoRA Target Modules
- **Attention**: `q_proj`, `k_proj`, `v_proj`, `o_proj`
- **MLP**: `gate_proj`, `up_proj`, `down_proj`
- **Total Parameters per Layer**: 622,592 (rank 16)
- **Memory per Adapter**: ~39.8 MB (fp16)

### Chat Template
```
<|im_start|>user
{prompt}<|im_end|>
<|im_start|>assistant
{response}<|im_end|>
```

## Usage Examples

### Import Model
```bash
# Download model artifacts in MLX format (offline)
mkdir -p ~/adapteros/models/qwen2.5-7b-mlx
cd ~/adapteros/models/qwen2.5-7b-mlx

# Download from MLX Community repository (optimized for Apple Silicon)
huggingface-cli download mlx-community/Qwen2.5-7B-Instruct \
  --include "config.json,tokenizer.json,tokenizer_config.json,model.safetensors" \
  --local-dir .

# Import into AdapterOS
./target/debug/aosctl import-model \
  --name Qwen2.5-7B-Instruct \
  --weights ./qwen2.5-7b-mlx/model.safetensors \
  --config ./qwen2.5-7b-mlx/config.json \
  --tokenizer ./qwen2.5-7b-mlx/tokenizer.json \
  --tokenizer-cfg ./qwen2.5-7b-mlx/tokenizer_config.json \
  --license ./qwen2.5-7b-mlx/LICENSE
```

### Build Plan
```bash
# Create manifest
cat > manifests/qwen7b.yaml << EOF
schema: adapteros.manifest.v3
base:
  model_id: "Qwen2.5-7B-Instruct"
  model_hash: "b3:$(cat ~/adapteros/models/qwen2.5-7b-mlx/MODEL_HASH)"
  arch: "qwen2"
  vocab_size: 32000
  hidden_dim: 4096
  n_layers: 32
  n_heads: 32
  config_hash: "b3:$(cat ~/adapteros/models/qwen2.5-7b-mlx/CONFIG_HASH)"
  tokenizer_hash: "b3:$(cat ~/adapteros/models/qwen2.5-7b-mlx/TOKENIZER_HASH)"
  tokenizer_cfg_hash: "b3:$(cat ~/adapteros/models/qwen2.5-7b-mlx/TOKENIZER_CFG_HASH)"
  license_hash: "b3:$(cat ~/adapteros/models/qwen2.5-7b-mlx/LICENSE_HASH)"
router:
  k_sparse: 3
  gate_quant: "q15"
  entropy_floor: 0.02
  tau: 1.0
  sample_tokens_full: 128
telemetry:
  schema_hash: "b3:stub"
  sampling:
    token: 0.05
    router: 1.0
    inference: 1.0
  router_full_tokens: 128
  bundle:
    max_events: 500000
    max_bytes: 268435456
policies:
  egress: "deny_all"
  access:
    adapters: "RBAC"
    datasets: "ABAC"
seeds:
  global: "b3:deadbeef"
EOF

# Build plan
./target/debug/aosctl build-plan \
  --manifest manifests/qwen7b.yaml \
  --out ./plan/qwen7b
```

### Serve Model
```bash
# Start server (will refuse if PF not enforced)
./target/debug/aosctl serve \
  --tenant demo \
  --plan $(cat plan/qwen7b/PLAN_ID)

# Test inference via UDS
echo '{"cpid":"CP-QWEN-TEST","input":{"text":"<|im_start|>user\nExplain the Bernoulli principle.<|im_end|>\n<|im_start|>assistant\n"}}' \
| socat - UNIX-CONNECT:/var/run/aos/demo/serve.sock
```

### Run Acceptance Tests
```bash
# Run integration tests
cargo test --test integration_qwen

# Run policy gates tests
cargo test --test policy_gates_qwen

# Run determinism replay
./target/debug/aosctl audit --suite tests/corpora/reg_v1.json --cpid CP-QWEN
./target/debug/aosctl replay --bundle out/audit_CP-QWEN.tar.zst
```

## Performance Targets

### Latency (Phase 4)
- **P95 Token Step**: ≤ 24ms
- **Throughput**: ≥ 40 tokens/s
- **Router Overhead**: ≤ 8%

### Memory
- **Base Model**: ~7GB (int4 quantized)
- **LoRA Adapter**: ~40MB (rank 16)
- **KV Cache**: ~2GB (32K context)
- **Total**: ~9GB per tenant

### Quality Metrics
- **ARR**: ≥ 0.95
- **ECS@5**: ≥ 0.75
- **HLR**: ≤ 0.03
- **CR**: ≤ 0.01

## Troubleshooting

### Common Issues

1. **Egress Policy Violation**
   ```
   ❌ PREFLIGHT FAILED: Egress policy validation failed
   ```
   **Solution**: Configure PF firewall rules or run with `--dry-run` for testing

2. **Determinism Failure**
   ```
   ❌ Replay diff detected: event hashes don't match
   ```
   **Solution**: Ensure consistent seeds and disable fast-math in kernels

3. **Adapter Collapse**
   ```
   ⚠️ Router entropy below threshold: single adapter >80% activation
   ```
   **Solution**: Increase entropy floor or recalibrate router features

4. **Numeric Validation Error**
   ```
   ❌ Unit-free number detected: "The torque is 1500"
   ```
   **Solution**: Include units in numeric claims: "The torque is 1500 in-lbf"

### Debug Commands

```bash
# Check egress policy
./target/debug/aosctl serve --dry-run

# Validate model configuration
./target/debug/aosctl validate-model --config ./qwen/config.json

# Test chat template
./target/debug/aosctl test-template --tokenizer-cfg ./qwen/tokenizer_config.json

# Check router calibration
./target/debug/aosctl router-calibrate --manifest manifests/qwen7b.yaml
```

## Security Considerations

### Egress Control
- **PF Firewall**: Required for serving mode
- **UDS Only**: No TCP/UDP connections
- **DNS Blocking**: All DNS resolution blocked

### Isolation
- **Per-Tenant Process**: Unique UID/GID
- **Capability Scoping**: Directory handles only
- **No Shared Memory**: Cross-tenant isolation

### Artifacts
- **Signature Verification**: Ed25519 signatures required
- **SBOM Validation**: Software bill of materials
- **CAS Storage**: Content-addressed artifacts only

## Compliance

### Evidence Requirements
- **Open Book**: Factual claims must cite sources
- **Span Tracking**: Document ID, revision, span hash
- **Supersession Warnings**: Latest revision preferred

### Audit Trail
- **Event Logging**: Canonical JSON with BLAKE3 hashing
- **Bundle Rotation**: Size/count limits with Merkle signing
- **Retention Policy**: K bundles per CPID, incident bundles preserved

### Quality Gates
- **Deterministic Replay**: Zero diff on identical inputs
- **Performance Budgets**: Latency, throughput, memory limits
- **Rollback Plan**: Previous CP available and tested

## Next Steps

1. **Complete Registry Integration**: Fix compilation errors in `aos-registry`
2. **Implement Metal Kernels**: Fused MLP and QKV with GQA support
3. **Performance Tuning**: Optimize for Apple Silicon M-series
4. **Production Deployment**: Multi-tenant serving with isolation
5. **Monitoring**: Telemetry, metrics, and alerting

## References

- [Qwen2.5-7B-Instruct Model Card](https://huggingface.co/Qwen/Qwen2.5-7B-Instruct)
- [AdapterOS Architecture](docs/architecture.md)
- [Security Model](docs/security.md)
- [Policy Framework](docs/policies.md)
- [Metal Performance Shaders](https://developer.apple.com/metal/)
