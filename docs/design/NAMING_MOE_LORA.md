# MoE-LoRA Naming Contract

## Purpose

Single source of truth for naming conventions in MoE (Mixture of Experts) and LoRA (Low-Rank Adaptation) code. All new code and refactors must follow these conventions.

---

## Acronym Conventions

Acronyms preserve uppercase casing in type names and enum variants.

| Acronym | Types (PascalCase) | Fields/Methods (snake_case) | Enum Variants |
|---------|-------------------|----------------------------|---------------|
| MoE     | `MoE*`            | `moe_*`                    | `MoE`         |
| LoRA    | `LoRA*`           | `lora_*`                   | `LoRA`        |
| KV      | `KV*`             | `kv_*`                     | `KV`          |
| RoPE    | `RoPE*`           | `rope_*`                   | `RoPE`        |
| MLX     | `MLX*`            | `mlx_*`                    | `MLX`         |
| FFI     | `*FFI*`           | `ffi_*`                    | `FFI`         |
| GQA     | `GQA*`            | `gqa_*`                    | `GQA`         |

### Examples

```rust
// GOOD
struct MoEConfig { ... }
struct LoRAAdapter { ... }
struct KVCacheConfig { ... }
struct MLXFFIBackend { ... }
enum BackendHint { MLX, CoreML, Metal }
fn moe_forward(...) { ... }
let lora_rank = 16;

// BAD
struct MoeConfig { ... }      // Use MoE, not Moe
struct LoraAdapter { ... }    // Use LoRA, not Lora
struct KvCacheConfig { ... }  // Use KV, not Kv
enum BackendHint { Mlx, ... } // Use MLX, not Mlx
```

---

## Terminology Glossary

### Router & Gating

| Term | Definition | Type | Shape |
|------|------------|------|-------|
| `router_logits` | Pre-softmax router network output | `f32` | `[batch, seq_len, num_experts]` |
| `gates` | Post-softmax expert weights (array form) | `Vec<f32>` | `[num_experts_per_token]` |
| `gate_weight` | Single expert weight (scalar form) | `f32` | scalar, range `[0.0, 1.0]` |
| `gates_q15` | Q15-quantized gates | `Vec<i16>` | `[num_experts_per_token]` |
| `router_weight` | Router network parameters | `Tensor` | `[hidden_size, num_experts]` |

**Usage rule:** Use `gates` when referring to an array of weights, use `gate_weight` when referring to a single scalar weight value.

### Expert Selection

| Term | Definition |
|------|------------|
| `routing` | Selected expert IDs for a single token |
| `selection` | The top-k operation that produces routing |
| `expert_routing` | Full routing for all tokens in a batch |
| `expert_gates` | Gate weights corresponding to routing selections |

### Expert Network Weights

These are the MLP projection weights within each expert (not to be confused with `gate_weight` routing weights):

| Term | Definition | Shape |
|------|------------|-------|
| `gate_proj` / `gate_weight` (in expert context) | Gate projection in SwiGLU | `[intermediate, hidden]` |
| `up_proj` / `up_weight` | Up projection in SwiGLU | `[intermediate, hidden]` |
| `down_proj` / `down_weight` | Down projection | `[hidden, intermediate]` |

**Context rule:** When in MoE expert FFN context, `gate_weight` refers to the SwiGLU gate projection. When in routing context, `gate_weight` refers to the scalar routing weight.

### Shape Fields

| Field | Meaning |
|-------|---------|
| `num_experts` | Total number of experts in layer (e.g., 64, 128) |
| `num_experts_per_token` | Top-k experts selected per token (e.g., 8) |
| `num_shared_experts` | Shared experts that are always active |
| `hidden_size` | Model hidden dimension |
| `moe_intermediate_size` | Expert FFN intermediate dimension |

---

## Q15 Fixed-Point Constants

For quantized routing gates:

```rust
/// Q15 denominator for dequantization: i16 → f32
pub const ROUTER_GATE_Q15_DENOM: f32 = 32767.0;

/// Maximum Q15 value (2^15 - 1)
pub const ROUTER_GATE_Q15_MAX: i16 = 32767;
```

**Conversion formulas:**
- Quantize: `gate_q15 = (gate_f32 * 32767.0).round() as i16`
- Dequantize: `gate_f32 = gate_q15 as f32 / 32767.0`

---

## LoRA-Specific Terms

| Term | Definition |
|------|------------|
| `lora_a` | Down-projection matrix (A in LoRA) |
| `lora_b` | Up-projection matrix (B in LoRA) |
| `rank` | LoRA rank (r in paper) |
| `alpha` | LoRA scaling factor |
| `lora_strength` | Runtime multiplier for LoRA contribution |
| `target_modules` | Modules targeted for LoRA adaptation |

### LoRA Scaling Formula

```
output = base_output + (alpha / rank) * (x @ lora_a @ lora_b)
```

---

## MoE + LoRA Strategy Terms

| Term | Definition |
|------|------------|
| `MoELoRAStrategy::RoutingWeightedShared` | Single shared LoRA scaled by routing weights |
| `MoELoRAStrategy::PerExpertLoRA` | Separate LoRA per expert |
| `use_routing_weights` | Whether to apply routing weights to LoRA contribution |

### Routing-Weighted LoRA Formula

```
lora_contrib = (gate_q15 / 32767.0) * routing_weight * (alpha / rank) * (B @ A) @ x
```

---

## Changelog

| Date | Change |
|------|--------|
| 2025-12-25 | Initial contract established |
