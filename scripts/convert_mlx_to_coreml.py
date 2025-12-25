#!/usr/bin/env python3
"""
MLX to CoreML MoE Model Converter
==================================

Converts MLX/safetensors MoE models to CoreML .mlpackage format for ANE execution.

This script handles the full conversion pipeline:
1. Load MLX safetensors model weights
2. Build CoreML MIL (Model Intermediate Language) program
3. Handle MoE expert routing with top-k gating
4. Convert 4-bit quantized weights to FP16
5. Export to .mlpackage optimized for ANE

Target: Qwen3-Coder-30B-A3B-Instruct-MLX-4bit
- 128 experts
- 8 experts per token (top-k routing)
- 48 layers
- 2048 hidden size
- 768 moe_intermediate_size
- 4-bit quantization (group_size=64)

Requirements:
    pip install -r scripts/requirements-convert.txt

Usage:
    # Convert full model
    python scripts/convert_mlx_to_coreml.py \\
        --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \\
        --output ./var/models/Qwen3-Coder-30B-CoreML.mlpackage

    # Convert single layer (for testing)
    python scripts/convert_mlx_to_coreml.py \\
        --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \\
        --output ./var/models/Qwen3-30B-layer0.mlpackage \\
        --single-layer 0

    # Smaller sequence length for faster conversion
    python scripts/convert_mlx_to_coreml.py \\
        --input ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit \\
        --output ./var/models/Qwen3-30B-CoreML-512.mlpackage \\
        --seq-len 512
"""

import argparse
import json
import sys
import time
from pathlib import Path
from typing import Dict, List, Optional, Tuple

import numpy as np

try:
    import coremltools as ct
    from coremltools.converters.mil import Builder as mb
    from coremltools.converters.mil.mil import types, Program
except ImportError:
    print("ERROR: coremltools not installed. Run: pip install coremltools>=9.0")
    sys.exit(1)

# Import our utilities
from coreml_moe_ops import (
    WeightLoader,
    build_moe_layer_dense,
    build_rms_norm,
    validate_ane_compatibility,
    print_conversion_summary,
    estimate_model_size,
)


def print_banner(msg: str):
    """Print a section banner."""
    print("\n" + "=" * 70)
    print(msg)
    print("=" * 70)


def load_config(model_path: Path) -> Dict:
    """Load model configuration."""
    config_path = model_path / "config.json"
    if not config_path.exists():
        raise FileNotFoundError(f"Config not found: {config_path}")

    with open(config_path) as f:
        config = json.load(f)

    return config


def build_rope_embeddings(seq_len: int, head_dim: int, theta: float = 10000000.0, name: str = "rope"):
    """Build RoPE (Rotary Position Embedding) frequency constants.

    RoPE applies rotary positional embeddings by rotating pairs of elements in the
    embedding dimension. The rotation angle depends on the position and frequency band.

    For Qwen3: theta=10000000 (10M), head_dim=128, max_seq_len=262144

    Args:
        seq_len: Sequence length for the model
        head_dim: Head dimension (must be even)
        theta: RoPE theta parameter (base frequency, default: 10M for Qwen3)
        name: Operation name prefix

    Returns:
        Tuple of (cos_cached, sin_cached) tensors with shape (seq_len, head_dim)

    Implementation:
        1. Compute inverse frequencies: inv_freq[i] = 1 / (theta^(2i/d)) for i in [0, d/2)
        2. Compute position-frequency products: freqs[t,i] = t * inv_freq[i]
        3. Duplicate frequencies to match pairs: [f0,f0,f1,f1,...]
        4. Precompute cos and sin for each position and dimension
    """
    # head_dim must be even for RoPE
    assert head_dim % 2 == 0, f"head_dim must be even for RoPE, got {head_dim}"

    # Compute inverse frequencies: 1 / (theta^(2i/d)) for i in [0, d/2)
    # This creates frequency bands that decay exponentially
    inv_freq = 1.0 / (theta ** (np.arange(0, head_dim, 2, dtype=np.float32) / head_dim))

    # Position indices: [0, 1, 2, ..., seq_len-1]
    t = np.arange(seq_len, dtype=np.float32)

    # Compute position-frequency products: outer product of positions and frequency bands
    # Shape: (seq_len, head_dim/2)
    freqs = np.outer(t, inv_freq)

    # Duplicate each frequency to match the pairing structure
    # [f0, f0, f1, f1, ...] ensures each pair rotates with the same angle
    # Shape: (seq_len, head_dim)
    freqs_expanded = np.repeat(freqs, 2, axis=-1)

    # Precompute cos and sin for efficient application
    cos_cached = np.cos(freqs_expanded).astype(np.float16)
    sin_cached = np.sin(freqs_expanded).astype(np.float16)

    return cos_cached, sin_cached


def apply_rope(x, cos_cached: np.ndarray, sin_cached: np.ndarray, name: str = "rope"):
    """Apply RoPE (Rotary Position Embedding) to query or key tensor.

    RoPE applies a rotation to pairs of elements in the head dimension based on their
    position in the sequence. This provides relative positional information without
    adding explicit position embeddings.

    The rotation is applied as:
        x_rotated = x * cos + rotate_half(x) * sin

    where rotate_half swaps and negates halves:
        rotate_half([x1, x2, ..., x_d/2, x_d/2+1, ..., x_d]) = [-x_d/2+1, ..., -x_d, x1, ..., x_d/2]

    Args:
        x: Input tensor (batch, num_heads, seq_len, head_dim)
        cos_cached: Cosine values (seq_len, head_dim) from build_rope_embeddings
        sin_cached: Sine values (seq_len, head_dim) from build_rope_embeddings
        name: Operation name prefix

    Returns:
        Rotated tensor with same shape as input

    Implementation:
        1. Split x into first half and second half along head_dim
        2. Create rotate_half by concatenating [-second_half, first_half]
        3. Apply formula: x * cos + rotate_half(x) * sin
    """
    # Reshape cos/sin to broadcast: (1, 1, seq_len, head_dim)
    cos_reshaped = mb.expand_dims(x=cos_cached, axes=[0, 0], name=f"{name}_cos_expand")
    sin_reshaped = mb.expand_dims(x=sin_cached, axes=[0, 0], name=f"{name}_sin_expand")

    # Implement RoPE rotation: x_rotated = x * cos + rotate_half(x) * sin
    # where rotate_half([x0, x1, x2, x3, ...]) = [-x1, x0, -x3, x2, ...]

    # Get head_dim from cos_cached shape
    head_dim = cos_cached.shape[-1]

    # Split x into first half and second half along head_dim
    # x shape: (batch, num_heads, seq_len, head_dim)
    # Split into x1: [..., :head_dim//2] and x2: [..., head_dim//2:]
    half_dim = head_dim // 2

    # Slice first half
    x1 = mb.slice_by_index(
        x=x,
        begin=[0, 0, 0, 0],
        end=[0, 0, 0, half_dim],
        begin_mask=[True, True, True, False],
        end_mask=[True, True, True, False],
        name=f"{name}_x1"
    )

    # Slice second half
    x2 = mb.slice_by_index(
        x=x,
        begin=[0, 0, 0, half_dim],
        end=[0, 0, 0, head_dim],
        begin_mask=[True, True, True, False],
        end_mask=[True, True, True, False],
        name=f"{name}_x2"
    )

    # Create rotate_half by concatenating [-x2, x1]
    # This implements the rotation: [x1, x2] -> [-x2, x1]
    x2_neg = mb.mul(x=x2, y=np.float16(-1.0), name=f"{name}_x2_neg")

    # Concatenate: [-x2, x1]
    x_rotated = mb.concat(
        values=[x2_neg, x1],
        axis=-1,
        name=f"{name}_rotated"
    )

    # Apply RoPE formula: x * cos + rotate_half(x) * sin
    x_cos = mb.mul(x=x, y=cos_reshaped, name=f"{name}_x_cos")
    x_rotated_sin = mb.mul(x=x_rotated, y=sin_reshaped, name=f"{name}_x_rotated_sin")

    # Final result
    result = mb.add(x=x_cos, y=x_rotated_sin, name=f"{name}_result")

    return result


def build_attention_layer(
    hidden_states,
    layer_idx: int,
    loader: WeightLoader,
    config: Dict,
    seq_len: int,
    name: str = "attn"
):
    """Build self-attention layer with Grouped Query Attention (GQA).

    Implements Qwen3 architecture with:
    - Grouped Query Attention (GQA): num_kv_heads < num_heads
    - QK normalization (RMSNorm on Q and K projections)
    - RoPE positional embeddings
    - Proper multi-head reshaping and attention computation

    Args:
        hidden_states: Input tensor (batch, seq_len, hidden_size)
        layer_idx: Layer index
        loader: Weight loader
        config: Model config
        seq_len: Sequence length for RoPE embeddings
        name: Operation name prefix

    Returns:
        Attention output tensor
    """
    hidden_size = config["hidden_size"]
    num_heads = config["num_attention_heads"]
    num_kv_heads = config["num_key_value_heads"]
    head_dim = config["head_dim"]

    # For GQA: each KV head is shared across multiple Q heads
    # num_heads=32, num_kv_heads=4 -> 8 Q heads per KV head
    assert num_heads % num_kv_heads == 0, "num_heads must be divisible by num_kv_heads for GQA"
    num_q_per_kv = num_heads // num_kv_heads

    prefix = f"model.layers.{layer_idx}.self_attn"

    # Load weights (dequantize from 4-bit)
    print(f"  Loading attention weights for layer {layer_idx}...")
    print(f"    Config: heads={num_heads}, kv_heads={num_kv_heads}, head_dim={head_dim}")
    print(f"    GQA: {num_q_per_kv} query heads per KV head")

    q_weight, q_scales, q_biases = loader.get_quantized_weight(f"{prefix}.q_proj")
    q_weight_fp16 = loader.dequantize_weight(q_weight, q_scales, q_biases,
                                              group_size=config["quantization"]["group_size"],
                                              bits=config["quantization"]["bits"])

    k_weight, k_scales, k_biases = loader.get_quantized_weight(f"{prefix}.k_proj")
    k_weight_fp16 = loader.dequantize_weight(k_weight, k_scales, k_biases,
                                              group_size=config["quantization"]["group_size"],
                                              bits=config["quantization"]["bits"])

    v_weight, v_scales, v_biases = loader.get_quantized_weight(f"{prefix}.v_proj")
    v_weight_fp16 = loader.dequantize_weight(v_weight, v_scales, v_biases,
                                              group_size=config["quantization"]["group_size"],
                                              bits=config["quantization"]["bits"])

    o_weight, o_scales, o_biases = loader.get_quantized_weight(f"{prefix}.o_proj")
    o_weight_fp16 = loader.dequantize_weight(o_weight, o_scales, o_biases,
                                              group_size=config["quantization"]["group_size"],
                                              bits=config["quantization"]["bits"])

    # QK norm weights
    q_norm_weight = loader.get_weight(f"{prefix}.q_norm.weight").astype(np.float16)
    k_norm_weight = loader.get_weight(f"{prefix}.k_norm.weight").astype(np.float16)

    print(f"    Weight shapes: Q={q_weight_fp16.shape}, K={k_weight_fp16.shape}, V={v_weight_fp16.shape}")

    # Q, K, V projections
    # Input: (batch, seq_len, hidden_size)
    # Q output: (batch, seq_len, num_heads * head_dim)
    # K/V output: (batch, seq_len, num_kv_heads * head_dim)
    q = mb.linear(x=hidden_states, weight=q_weight_fp16, bias=None, name=f"{name}_q")
    k = mb.linear(x=hidden_states, weight=k_weight_fp16, bias=None, name=f"{name}_k")
    v = mb.linear(x=hidden_states, weight=v_weight_fp16, bias=None, name=f"{name}_v")

    # Reshape for multi-head attention BEFORE applying QK norm
    # Q: (batch, seq_len, num_heads * head_dim) -> (batch, seq_len, num_heads, head_dim)
    # K/V: (batch, seq_len, num_kv_heads * head_dim) -> (batch, seq_len, num_kv_heads, head_dim)
    q = mb.reshape(x=q, shape=[1, -1, num_heads, head_dim], name=f"{name}_q_reshape")
    k = mb.reshape(x=k, shape=[1, -1, num_kv_heads, head_dim], name=f"{name}_k_reshape")
    v = mb.reshape(x=v, shape=[1, -1, num_kv_heads, head_dim], name=f"{name}_v_reshape")

    # Apply QK normalization (RMSNorm) per head
    # QK norm is applied on the last dimension (head_dim), which is already separated
    # The norm weight has shape (head_dim,) and should broadcast across (batch, seq_len, num_heads, head_dim)
    q = build_rms_norm(q, q_norm_weight, eps=config["rms_norm_eps"], name=f"{name}_q_norm")
    k = build_rms_norm(k, k_norm_weight, eps=config["rms_norm_eps"], name=f"{name}_k_norm")

    # Transpose to (batch, num_heads, seq_len, head_dim) for attention
    q = mb.transpose(x=q, perm=[0, 2, 1, 3], name=f"{name}_q_transpose")
    k = mb.transpose(x=k, perm=[0, 2, 1, 3], name=f"{name}_k_transpose")
    v = mb.transpose(x=v, perm=[0, 2, 1, 3], name=f"{name}_v_transpose")

    # Apply RoPE positional embeddings to Q and K
    # Note: RoPE is applied after reshaping to (batch, num_heads, seq_len, head_dim)
    rope_theta = config.get("rope_theta", 10000000.0)
    print(f"    Applying RoPE with theta={rope_theta}, seq_len={seq_len}, head_dim={head_dim}")

    # Build RoPE embeddings for the model's sequence length
    cos_cached, sin_cached = build_rope_embeddings(seq_len, head_dim, rope_theta, name=f"{name}_rope")

    # Apply RoPE to Q and K
    q = apply_rope(q, cos_cached, sin_cached, name=f"{name}_q_rope")
    k = apply_rope(k, cos_cached, sin_cached, name=f"{name}_k_rope")

    # Expand K and V for GQA: repeat each KV head to match Q heads
    # K, V: (batch, num_kv_heads, seq_len, head_dim) -> (batch, num_heads, seq_len, head_dim)
    # Each KV head is repeated num_q_per_kv times

    if num_kv_heads != num_heads:
        # Expand KV heads by repeating along the heads dimension
        # CoreML doesn't have a direct repeat operation, so we use tile

        # Reshape to add repeat dimension: (batch, num_kv_heads, 1, seq_len, head_dim)
        k = mb.expand_dims(x=k, axes=[2], name=f"{name}_k_expand")
        v = mb.expand_dims(x=v, axes=[2], name=f"{name}_v_expand")

        # Tile along the new dimension: (batch, num_kv_heads, num_q_per_kv, seq_len, head_dim)
        k = mb.tile(x=k, reps=[1, 1, num_q_per_kv, 1, 1], name=f"{name}_k_tile")
        v = mb.tile(x=v, reps=[1, 1, num_q_per_kv, 1, 1], name=f"{name}_v_tile")

        # Reshape to merge: (batch, num_heads, seq_len, head_dim)
        k = mb.reshape(x=k, shape=[1, num_heads, -1, head_dim], name=f"{name}_k_expand_reshape")
        v = mb.reshape(x=v, shape=[1, num_heads, -1, head_dim], name=f"{name}_v_expand_reshape")

    # Scaled dot-product attention
    # attn_weights = softmax(Q @ K^T / sqrt(head_dim))
    # output = attn_weights @ V

    # Transpose K for matmul: (batch, num_heads, seq_len, head_dim) -> (batch, num_heads, head_dim, seq_len)
    k_t = mb.transpose(x=k, perm=[0, 1, 3, 2], name=f"{name}_k_t")

    # Q @ K^T: (batch, num_heads, seq_len, seq_len)
    attn_scores = mb.matmul(x=q, y=k_t, name=f"{name}_attn_scores")

    # Scale by 1/sqrt(head_dim)
    scale = np.float16(1.0 / np.sqrt(head_dim))
    attn_scores = mb.mul(x=attn_scores, y=scale, name=f"{name}_attn_scaled")

    # Softmax over last dimension (key dimension)
    attn_weights = mb.softmax(x=attn_scores, axis=-1, name=f"{name}_attn_softmax")

    # attn_weights @ V: (batch, num_heads, seq_len, head_dim)
    attn_output = mb.matmul(x=attn_weights, y=v, name=f"{name}_attn_v")

    # Transpose back: (batch, num_heads, seq_len, head_dim) -> (batch, seq_len, num_heads, head_dim)
    attn_output = mb.transpose(x=attn_output, perm=[0, 2, 1, 3], name=f"{name}_attn_transpose")

    # Reshape to (batch, seq_len, num_heads * head_dim) for output projection
    # Note: num_heads * head_dim = hidden_size in standard transformers,
    # but for Qwen3, we need to use the actual projected dimension
    attn_output_dim = num_heads * head_dim
    attn_output = mb.reshape(x=attn_output, shape=[1, -1, attn_output_dim], name=f"{name}_attn_reshape")

    # Output projection: (batch, seq_len, num_heads * head_dim) -> (batch, seq_len, hidden_size)
    output = mb.linear(x=attn_output, weight=o_weight_fp16, bias=None, name=f"{name}_o_proj")

    return output


def estimate_peak_memory_gb(num_layers: int, num_experts: int, config: Dict) -> float:
    """Estimate peak memory usage during conversion.

    Args:
        num_layers: Number of layers to convert
        num_experts: Number of experts per layer
        config: Model configuration

    Returns:
        Estimated peak memory in GB
    """
    hidden_size = config.get("hidden_size", 2048)
    moe_intermediate_size = config.get("moe_intermediate_size", 768)

    # Per-expert size: gate + up + down projections (FP16 = 2 bytes)
    expert_params = (
        hidden_size * moe_intermediate_size +  # gate_proj
        hidden_size * moe_intermediate_size +  # up_proj
        moe_intermediate_size * hidden_size    # down_proj
    )
    bytes_per_expert = expert_params * 2  # FP16

    # Memory for loaded experts (need to hold all during layer build)
    expert_memory = num_experts * bytes_per_expert * num_layers

    # Overhead for intermediate tensors, CoreML graph, etc (~2x)
    total_bytes = expert_memory * 2

    return total_bytes / (1024 ** 3)


def load_experts_batched(
    loader: WeightLoader,
    prefix: str,
    config: Dict,
    num_experts: int,
    batch_size: int = 16
):
    """Generator that yields expert weights in batches for memory efficiency.

    Args:
        loader: Weight loader
        prefix: Expert weight prefix (e.g., "model.layers.0.mlp.switch_mlp")
        config: Model config
        num_experts: Number of experts to load
        batch_size: Number of experts per batch

    Yields:
        List of expert weight dicts for each batch
    """
    import gc

    # Load all experts' raw weights (these are still quantized, smaller)
    all_gate_w, all_gate_s, all_gate_b = loader.get_quantized_weight(f"{prefix}.gate_proj")
    all_up_w, all_up_s, all_up_b = loader.get_quantized_weight(f"{prefix}.up_proj")
    all_down_w, all_down_s, all_down_b = loader.get_quantized_weight(f"{prefix}.down_proj")

    for batch_start in range(0, num_experts, batch_size):
        batch_end = min(batch_start + batch_size, num_experts)
        batch_experts = []

        for expert_idx in range(batch_start, batch_end):
            # Extract this expert's weights
            gate_w = all_gate_w[expert_idx]
            gate_s = all_gate_s[expert_idx]
            gate_b = all_gate_b[expert_idx]

            up_w = all_up_w[expert_idx]
            up_s = all_up_s[expert_idx]
            up_b = all_up_b[expert_idx]

            down_w = all_down_w[expert_idx]
            down_s = all_down_s[expert_idx]
            down_b = all_down_b[expert_idx]

            # Dequantize
            gate_weight_fp16 = loader.dequantize_weight(
                gate_w, gate_s, gate_b,
                group_size=config["quantization"]["group_size"],
                bits=config["quantization"]["bits"]
            )

            up_weight_fp16 = loader.dequantize_weight(
                up_w, up_s, up_b,
                group_size=config["quantization"]["group_size"],
                bits=config["quantization"]["bits"]
            )

            down_weight_fp16 = loader.dequantize_weight(
                down_w, down_s, down_b,
                group_size=config["quantization"]["group_size"],
                bits=config["quantization"]["bits"]
            )

            batch_experts.append({
                "gate_proj": gate_weight_fp16,
                "up_proj": up_weight_fp16,
                "down_proj": down_weight_fp16,
            })

        yield batch_experts

        # Force garbage collection between batches
        gc.collect()


def build_moe_layer(
    hidden_states,
    layer_idx: int,
    loader: WeightLoader,
    config: Dict,
    max_experts: Optional[int] = None,
    expert_batch_size: int = 16,
    name: str = "moe"
):
    """Build MoE (Mixture of Experts) layer.

    Args:
        hidden_states: Input tensor (batch, seq_len, hidden_size)
        layer_idx: Layer index
        loader: Weight loader
        config: Model config
        max_experts: Maximum number of experts to load (None = all)
        expert_batch_size: Number of experts to dequantize per batch
        name: Operation name prefix

    Returns:
        MoE output tensor
    """
    num_experts = config["num_experts"]
    num_experts_per_tok = config["num_experts_per_tok"]
    hidden_size = config["hidden_size"]
    moe_intermediate_size = config["moe_intermediate_size"]

    # Limit experts if specified
    experts_to_load = min(num_experts, max_experts) if max_experts else num_experts

    prefix = f"model.layers.{layer_idx}.mlp"

    print(f"  Loading MoE weights for layer {layer_idx} ({experts_to_load}/{num_experts} experts)...")

    # Load router weight
    router_weight, router_scales, router_biases = loader.get_quantized_weight(f"{prefix}.gate")
    router_weight_fp16 = loader.dequantize_weight(
        router_weight, router_scales, router_biases,
        group_size=config["quantization"]["group_size"],
        bits=config["quantization"]["bits"]
    )

    # Load expert weights using batched loading for memory efficiency
    expert_prefix = f"{prefix}.switch_mlp"
    expert_weights = []

    print(f"    Loading experts in batches of {expert_batch_size}...")
    batch_num = 0
    for batch in load_experts_batched(loader, expert_prefix, config, experts_to_load, expert_batch_size):
        batch_num += 1
        start_idx = (batch_num - 1) * expert_batch_size
        end_idx = start_idx + len(batch)
        print(f"    Batch {batch_num}: experts {start_idx}-{end_idx-1}")
        expert_weights.extend(batch)

    print(f"    Loaded {len(expert_weights)} expert weight sets")

    # Build MoE layer
    output = build_moe_layer_dense(
        hidden_states=hidden_states,
        router_weight=router_weight_fp16,
        expert_weights=expert_weights,
        num_experts_per_tok=num_experts_per_tok,
        hidden_size=hidden_size,
        moe_intermediate_size=moe_intermediate_size,
        name=name
    )

    return output


def build_transformer_layer(
    hidden_states,
    layer_idx: int,
    loader: WeightLoader,
    config: Dict,
    seq_len: int,
    max_experts: Optional[int] = None,
    expert_batch_size: int = 16,
    name: str = "layer"
):
    """Build a full transformer layer (attention + MoE).

    Args:
        hidden_states: Input tensor
        layer_idx: Layer index
        loader: Weight loader
        config: Model config
        seq_len: Sequence length for RoPE embeddings
        max_experts: Maximum number of experts to load (None = all)
        expert_batch_size: Number of experts to dequantize per batch
        name: Operation name prefix

    Returns:
        Layer output tensor
    """
    print(f"\nBuilding layer {layer_idx}...")

    prefix = f"model.layers.{layer_idx}"

    # Load layer norm weights
    input_ln_weight = loader.get_weight(f"{prefix}.input_layernorm.weight").astype(np.float16)
    post_attn_ln_weight = loader.get_weight(f"{prefix}.post_attention_layernorm.weight").astype(np.float16)

    # Pre-attention norm
    normed = build_rms_norm(
        hidden_states,
        input_ln_weight,
        eps=config["rms_norm_eps"],
        name=f"{name}_input_ln"
    )

    # Attention
    attn_out = build_attention_layer(
        normed,
        layer_idx,
        loader,
        config,
        seq_len,
        name=f"{name}_attn"
    )

    # Residual connection
    hidden_states = mb.add(x=hidden_states, y=attn_out, name=f"{name}_attn_residual")

    # Post-attention norm
    normed = build_rms_norm(
        hidden_states,
        post_attn_ln_weight,
        eps=config["rms_norm_eps"],
        name=f"{name}_post_attn_ln"
    )

    # MoE
    moe_out = build_moe_layer(
        normed,
        layer_idx,
        loader,
        config,
        max_experts=max_experts,
        expert_batch_size=expert_batch_size,
        name=f"{name}_moe"
    )

    # Residual connection
    hidden_states = mb.add(x=hidden_states, y=moe_out, name=f"{name}_moe_residual")

    return hidden_states


def build_coreml_model(
    model_path: Path,
    seq_len: int,
    single_layer: Optional[int] = None,
    max_layers: Optional[int] = None,
    max_experts: Optional[int] = None,
    expert_batch_size: int = 16,
    output_hidden_states: bool = False
) -> Tuple[Program, Optional[np.ndarray]]:
    """Build CoreML MIL program for the model.

    Args:
        model_path: Path to MLX model directory
        seq_len: Sequence length
        single_layer: If set, only build this layer (for testing)
        max_layers: Maximum number of layers to convert (None = all)
        max_experts: Maximum number of experts per layer (None = all)
        expert_batch_size: Number of experts to process per batch
        output_hidden_states: If True, output hidden states instead of logits

    Returns:
        Tuple of (CoreML MIL Program, LM head weights if output_hidden_states else None)
    """
    print_banner("Loading Model Configuration")

    config = load_config(model_path)
    loader = WeightLoader(model_path)

    hidden_size = config["hidden_size"]
    total_layers = config["num_hidden_layers"]
    vocab_size = config["vocab_size"]
    total_experts = config["num_experts"]

    # Determine actual conversion parameters
    layers_to_build = min(total_layers, max_layers) if max_layers else total_layers
    experts_to_load = min(total_experts, max_experts) if max_experts else total_experts

    print(f"Model: {config.get('architectures', ['unknown'])[0]}")
    print(f"Total layers: {total_layers} (converting: {layers_to_build})")
    print(f"Hidden size: {hidden_size}")
    print(f"Vocab size: {vocab_size}")
    print(f"Total experts: {total_experts} (loading: {experts_to_load})")
    print(f"Experts per token: {config['num_experts_per_tok']}")
    print(f"MoE intermediate size: {config['moe_intermediate_size']}")
    print(f"Sequence length: {seq_len}")
    print(f"Expert batch size: {expert_batch_size}")

    # Validate ANE compatibility
    warnings = validate_ane_compatibility(seq_len, hidden_size)
    if warnings:
        print("\nANE Compatibility Warnings:")
        for w in warnings:
            print(f"  - {w}")

    # Estimate size
    size_gb = estimate_model_size(config)
    print(f"\nEstimated model size (full): {size_gb:.2f} GB")

    # Estimate peak memory for conversion
    peak_memory_gb = estimate_peak_memory_gb(layers_to_build, experts_to_load, config)
    print(f"Estimated peak memory: {peak_memory_gb:.2f} GB")

    if peak_memory_gb > 64:
        print("\nWARNING: Estimated peak memory exceeds 64GB!")
        print("Consider reducing --num-layers or --num-experts")
        print("Recommended for 64GB: --num-layers 24 --num-experts 64")

    print_banner("Building CoreML MIL Program")

    # Build the model using CoreML MIL
    @mb.program(
        input_specs=[mb.TensorSpec(shape=(1, seq_len), dtype=types.int32)],
        opset_version=ct.target.iOS17
    )
    def qwen3_moe_model(input_ids):
        """CoreML MIL program for Qwen3 MoE model."""

        # 1. Embedding layer
        print("Building embedding layer...")
        embed_weight, embed_scales, embed_biases = loader.get_quantized_weight("model.embed_tokens")
        embed_weight_fp16 = loader.dequantize_weight(
            embed_weight, embed_scales, embed_biases,
            group_size=config["quantization"]["group_size"],
            bits=config["quantization"]["bits"]
        )

        # Gather embeddings for input token IDs
        hidden_states = mb.gather(
            x=embed_weight_fp16,
            indices=input_ids,
            axis=0,
            name="embed"
        )

        # 2. Transformer layers
        if single_layer is not None:
            # Build only specified layer
            print(f"\nBuilding single layer {single_layer} (testing mode)...")
            hidden_states = build_transformer_layer(
                hidden_states,
                single_layer,
                loader,
                config,
                seq_len,
                max_experts=experts_to_load,
                expert_batch_size=expert_batch_size,
                name=f"layer_{single_layer}"
            )
        else:
            # Build requested number of layers
            print(f"\nBuilding {layers_to_build} of {total_layers} layers...")

            for layer_idx in range(layers_to_build):
                hidden_states = build_transformer_layer(
                    hidden_states,
                    layer_idx,
                    loader,
                    config,
                    seq_len,
                    max_experts=experts_to_load,
                    expert_batch_size=expert_batch_size,
                    name=f"layer_{layer_idx}"
                )

        # 3. Final layer norm
        print("\nBuilding final layer norm...")
        final_ln_weight = loader.get_weight("model.norm.weight").astype(np.float16)
        hidden_states = build_rms_norm(
            hidden_states,
            final_ln_weight,
            eps=config["rms_norm_eps"],
            name="final_ln"
        )

        # 4. LM head (output projection to vocab) - skip if outputting hidden states
        lm_head_weight_fp16 = None
        if output_hidden_states:
            print("Skipping LM head (outputting hidden states for hybrid inference)...")
            # Load LM head weights to return separately
            lm_head_weight, lm_head_scales, lm_head_biases = loader.get_quantized_weight("lm_head")
            lm_head_weight_fp16 = loader.dequantize_weight(
                lm_head_weight, lm_head_scales, lm_head_biases,
                group_size=config["quantization"]["group_size"],
                bits=config["quantization"]["bits"]
            )
            # Return hidden states directly
            return hidden_states
        else:
            print("Building LM head...")
            lm_head_weight, lm_head_scales, lm_head_biases = loader.get_quantized_weight("lm_head")
            lm_head_weight_fp16 = loader.dequantize_weight(
                lm_head_weight, lm_head_scales, lm_head_biases,
                group_size=config["quantization"]["group_size"],
                bits=config["quantization"]["bits"]
            )

            logits = mb.linear(
                x=hidden_states,
                weight=lm_head_weight_fp16,
                bias=None,
                name="logits"
            )

            return logits

    print("\nCompiling MIL program...")
    program = qwen3_moe_model

    # Load LM head weights separately if outputting hidden states
    # (loader is still in scope from the outer function)
    lm_head_weights = None
    if output_hidden_states:
        print("Saving LM head weights for hybrid inference...")
        lm_head_weight, lm_head_scales, lm_head_biases = loader.get_quantized_weight("lm_head")
        lm_head_weights = loader.dequantize_weight(
            lm_head_weight, lm_head_scales, lm_head_biases,
            group_size=config["quantization"]["group_size"],
            bits=config["quantization"]["bits"]
        )
        print(f"  LM head weight shape: {lm_head_weights.shape}")

    return program, lm_head_weights


def convert_to_mlpackage(
    model_path: Path,
    output_path: Path,
    seq_len: int = 512,
    single_layer: Optional[int] = None,
    max_layers: Optional[int] = None,
    max_experts: Optional[int] = None,
    expert_batch_size: int = 16,
    output_hidden_states: bool = False
):
    """Convert MLX model to CoreML .mlpackage.

    Args:
        model_path: Path to MLX model directory
        output_path: Output .mlpackage path
        seq_len: Sequence length
        single_layer: If set, only convert this layer (for testing)
        max_layers: Maximum number of layers to convert (None = all)
        max_experts: Maximum number of experts per layer (None = all)
        expert_batch_size: Number of experts to process per batch
        output_hidden_states: If True, output hidden states instead of logits
    """
    import shutil

    start_time = time.time()

    try:
        # Build MIL program
        mil_program, lm_head_weights = build_coreml_model(
            model_path, seq_len, single_layer,
            max_layers=max_layers,
            max_experts=max_experts,
            expert_batch_size=expert_batch_size,
            output_hidden_states=output_hidden_states
        )

        print_banner("Converting to CoreML")

        # Convert to CoreML
        mlmodel = ct.convert(
            mil_program,
            minimum_deployment_target=ct.target.macOS14,
            compute_units=ct.ComputeUnit.ALL,  # Enable ANE
            convert_to="mlprogram",
            compute_precision=ct.precision.FLOAT16,
        )

        # Add metadata
        config = load_config(model_path)

        # Determine actual values used
        total_layers = config["num_hidden_layers"]
        total_experts = config["num_experts"]
        layers_converted = min(total_layers, max_layers) if max_layers else total_layers
        experts_loaded = min(total_experts, max_experts) if max_experts else total_experts

        mlmodel.author = "AdapterOS"
        mlmodel.license = "Apache-2.0"
        mlmodel.short_description = f"Qwen3-MoE-30B CoreML ({seq_len} tokens, {layers_converted}L/{experts_loaded}E)"
        mlmodel.version = "1.0"

        mlmodel.user_defined_metadata["seq_len"] = str(seq_len)
        mlmodel.user_defined_metadata["num_experts"] = str(experts_loaded)
        mlmodel.user_defined_metadata["num_experts_total"] = str(total_experts)
        mlmodel.user_defined_metadata["num_experts_per_tok"] = str(config["num_experts_per_tok"])
        mlmodel.user_defined_metadata["num_layers"] = str(layers_converted)
        mlmodel.user_defined_metadata["num_layers_total"] = str(total_layers)
        mlmodel.user_defined_metadata["source_model"] = str(model_path.name)

        if single_layer is not None:
            mlmodel.user_defined_metadata["single_layer"] = str(single_layer)

        # Save
        print_banner("Saving Model")
        output_path.parent.mkdir(parents=True, exist_ok=True)
        print(f"Output path: {output_path}")

        mlmodel.save(str(output_path))

        # Copy config.json into .mlpackage for MoE detection by Rust backend
        config_src = model_path / "config.json"
        if config_src.exists() and output_path.is_dir():
            config_dst = output_path / "config.json"
            shutil.copy2(config_src, config_dst)
            print(f"Copied config.json to {config_dst}")

        # Save LM head weights separately if outputting hidden states
        if output_hidden_states and lm_head_weights is not None:
            from safetensors.numpy import save_file
            lm_head_path = output_path / "lm_head_weights.safetensors"
            save_file({"lm_head.weight": lm_head_weights}, str(lm_head_path))
            print(f"Saved LM head weights to {lm_head_path}")
            print(f"  Shape: {lm_head_weights.shape}")
            print(f"  Size: {lm_head_weights.nbytes / 1e9:.2f} GB")

        elapsed = time.time() - start_time

        # Print summary
        print_conversion_summary(model_path, output_path, config, elapsed)

        print("\nConversion successful!")
        print(f"\nTo use with AdapterOS:")
        print(f"  ./target/release/aos-worker --backend coreml \\")
        print(f"      --model-path {output_path}")

    except Exception as e:
        print_banner(f"ERROR: {e}")
        import traceback
        traceback.print_exc()
        sys.exit(1)


def main():
    parser = argparse.ArgumentParser(
        description="Convert MLX MoE model to CoreML",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )

    parser.add_argument(
        "--input",
        type=Path,
        required=True,
        help="Path to MLX model directory (with safetensors and config.json)"
    )

    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="Output .mlpackage path"
    )

    parser.add_argument(
        "--seq-len",
        type=int,
        default=512,
        help="Sequence length (default: 512). Use 512/1024/2048/4096 for ANE optimization"
    )

    parser.add_argument(
        "--single-layer",
        type=int,
        default=None,
        help="Convert only a single layer for testing (default: all layers)"
    )

    parser.add_argument(
        "--num-layers",
        type=int,
        default=None,
        help="Number of layers to convert (default: all). For 64GB RAM, use 24"
    )

    parser.add_argument(
        "--num-experts",
        type=int,
        default=None,
        help="Number of experts to load per layer (default: all). For 64GB RAM, use 64"
    )

    parser.add_argument(
        "--expert-batch-size",
        type=int,
        default=16,
        help="Number of experts to process per batch for memory efficiency (default: 16)"
    )

    parser.add_argument(
        "--output-hidden-states",
        action="store_true",
        help="Output hidden states instead of logits (for hybrid LoRA inference)"
    )

    args = parser.parse_args()

    # Validate inputs
    if not args.input.exists():
        print(f"ERROR: Input path does not exist: {args.input}")
        sys.exit(1)

    config_path = args.input / "config.json"
    if not config_path.exists():
        print(f"ERROR: config.json not found in {args.input}")
        sys.exit(1)

    # Validate seq_len
    if args.seq_len % 8 != 0:
        print(f"WARNING: seq_len={args.seq_len} is not a multiple of 8")
        print("ANE optimization works best with multiples of 8")

    print_banner("MLX to CoreML MoE Converter")
    print(f"CoreML Tools: {ct.__version__}")
    print(f"Input: {args.input}")
    print(f"Output: {args.output}")
    print(f"Sequence length: {args.seq_len}")

    if args.single_layer is not None:
        print(f"Single layer mode: layer {args.single_layer}")
    if args.num_layers is not None:
        print(f"Limiting to {args.num_layers} layers")
    if args.num_experts is not None:
        print(f"Limiting to {args.num_experts} experts per layer")
    print(f"Expert batch size: {args.expert_batch_size}")
    if args.output_hidden_states:
        print(f"Output mode: hidden_states (for hybrid LoRA inference)")

    # Run conversion
    convert_to_mlpackage(
        args.input,
        args.output,
        args.seq_len,
        args.single_layer,
        max_layers=args.num_layers,
        max_experts=args.num_experts,
        expert_batch_size=args.expert_batch_size,
        output_hidden_states=args.output_hidden_states
    )


if __name__ == "__main__":
    main()
