#!/usr/bin/env python3
"""
CoreML MoE Operations and Utilities
====================================

Helper functions for converting MoE (Mixture-of-Experts) models to CoreML.
Handles weight loading, quantization, and MoE-specific operations.

Key MoE components:
- Router/gate network: projects hidden states to expert logits
- Top-k selection: selects top 8 experts per token
- Expert MLPs: each expert has gate_proj, up_proj, down_proj
- Weighted combination: combines expert outputs by routing weights
"""

import json
import numpy as np
from pathlib import Path
from typing import Dict, List, Tuple, Optional
import safetensors
from safetensors import safe_open

try:
    import torch
except ImportError:
    print("ERROR: torch not installed. Run: pip install torch")
    raise

try:
    import coremltools as ct
    from coremltools.converters.mil import Builder as mb
    from coremltools.converters.mil.mil import types
except ImportError:
    print("ERROR: coremltools not installed. Run: pip install coremltools>=9.0")
    raise


class WeightLoader:
    """Load and manage weights from MLX safetensors format."""

    def __init__(self, model_path: Path):
        """Initialize weight loader for a model directory.

        Args:
            model_path: Path to directory containing safetensors shards and index
        """
        self.model_path = Path(model_path)
        self.index_path = self.model_path / "model.safetensors.index.json"

        if not self.index_path.exists():
            raise FileNotFoundError(f"Index file not found: {self.index_path}")

        with open(self.index_path) as f:
            self.index = json.load(f)

        self.weight_map = self.index["weight_map"]
        self.metadata = self.index.get("metadata", {})

        # Cache for open safetensors files (using PyTorch framework)
        self._shard_cache: Dict[str, any] = {}

    def _get_tensor_dtype(self, shard_path: Path, key: str) -> str:
        """Get the dtype of a tensor from safetensors header.

        Args:
            shard_path: Path to safetensors file
            key: Tensor key

        Returns:
            dtype string (e.g., 'BF16', 'U32', 'F32')
        """
        import struct
        import json

        try:
            with open(shard_path, 'rb') as f:
                # Read header size (first 8 bytes)
                header_size = struct.unpack('<Q', f.read(8))[0]
                # Read header JSON
                header_json = f.read(header_size).decode('utf-8')
                header = json.loads(header_json)

            if key in header:
                return header[key].get('dtype', 'unknown')

            # Fallback: use heuristic based on key name
            return self._heuristic_dtype(key)
        except Exception:
            # If header parsing fails, use heuristic
            return self._heuristic_dtype(key)

    def _heuristic_dtype(self, key: str) -> str:
        """Heuristic dtype detection for MLX models.

        MLX commonly uses bfloat16 for scales/biases and certain weights.

        Args:
            key: Tensor key name

        Returns:
            Likely dtype string
        """
        key_lower = key.lower()

        # Scales and biases in MLX are commonly bfloat16
        if '.scales' in key_lower or '.biases' in key_lower:
            return 'BF16'

        # Layer norm and RMS norm weights are typically bfloat16
        if 'norm' in key_lower and 'weight' in key_lower:
            return 'BF16'

        # Quantized weights are uint32 (packed 4-bit values)
        if '.weight' in key_lower and not ('norm' in key_lower or 'embed' in key_lower):
            # Could be quantized uint32 or BF16, check for associated scales
            return 'unknown'

        return 'unknown'

    def get_weight(self, key: str) -> np.ndarray:
        """Load a weight tensor by key.

        Args:
            key: Weight key from the model (e.g., "model.layers.0.mlp.gate.weight")

        Returns:
            numpy array containing the weight (float16 if originally bfloat16)
        """
        if key not in self.weight_map:
            raise KeyError(f"Weight key not found: {key}")

        shard_name = self.weight_map[key]
        shard_path = self.model_path / shard_name

        # Check dtype to determine which framework to use
        dtype_str = self._get_tensor_dtype(shard_path, key)

        # Use different frameworks based on dtype:
        # - PyTorch for bfloat16 (BF16) - numpy doesn't support it
        # - Numpy for uint types (U8, U16, U32) - PyTorch doesn't support uint32
        use_pytorch = dtype_str == 'BF16'

        # Create cache key with framework info
        cache_key = f"{shard_name}:{'pt' if use_pytorch else 'np'}"

        # Open shard file (with caching)
        if cache_key not in self._shard_cache:
            framework = "pt" if use_pytorch else "numpy"
            self._shard_cache[cache_key] = safe_open(str(shard_path), framework=framework)

        shard = self._shard_cache[cache_key]
        tensor = shard.get_tensor(key)

        # Handle bfloat16 conversion - CoreML doesn't support bfloat16
        # Convert to float16 via float32 to preserve range
        if use_pytorch and isinstance(tensor, torch.Tensor):
            if tensor.dtype == torch.bfloat16:
                # Convert: bfloat16 -> float32 -> numpy float32 -> float16
                tensor = tensor.to(torch.float32).numpy().astype(np.float16)
            else:
                # For other dtypes, convert directly to numpy
                tensor = tensor.numpy()

        return np.array(tensor)

    def get_quantized_weight(self, key_prefix: str) -> Tuple[np.ndarray, np.ndarray, np.ndarray]:
        """Load quantized weight components (weight, scales, biases).

        For MLX 4-bit quantized weights, we need:
        - weight: packed 4-bit values
        - scales: per-group scaling factors
        - biases: per-group bias offsets

        Args:
            key_prefix: Weight key prefix (e.g., "model.layers.0.mlp.gate")

        Returns:
            Tuple of (weight, scales, biases) as numpy arrays
        """
        weight = self.get_weight(f"{key_prefix}.weight")
        scales = self.get_weight(f"{key_prefix}.scales")
        biases = self.get_weight(f"{key_prefix}.biases")

        return weight, scales, biases

    def dequantize_weight(
        self,
        weight: np.ndarray,
        scales: np.ndarray,
        biases: np.ndarray,
        group_size: int = 64,
        bits: int = 4
    ) -> np.ndarray:
        """Dequantize a 4-bit weight to float16.

        MLX uses grouped quantization where each group of `group_size` values
        shares a scale and bias.

        Args:
            weight: Packed 4-bit weight values (stored as uint32)
            scales: Per-group scales
            biases: Per-group biases
            group_size: Size of each quantization group
            bits: Number of bits per value (4 for 4-bit)

        Returns:
            Dequantized weight as float16 numpy array
        """
        # Ensure scales and biases are float32 for computation
        # Note: get_weight() already handles bfloat16 conversion
        scales = scales.astype(np.float32)
        biases = biases.astype(np.float32)

        # Scales shape tells us the actual dimensions
        # scales shape is (out_features, in_features // group_size)
        out_features = scales.shape[0]
        num_groups = scales.shape[1]
        in_features = num_groups * group_size

        # MLX stores 4-bit weights packed in uint32
        # Each uint32 holds 8 values (32 bits / 4 bits per value)
        # So we need to unpack them

        # Convert weight to uint32 if not already
        weight = weight.astype(np.uint32)

        # Flatten and unpack 4-bit values from uint32
        weight_flat = weight.reshape(-1)

        # Each uint32 contains 8 4-bit values
        values_per_elem = 32 // bits  # 8 for 4-bit
        total_values = weight_flat.size * values_per_elem

        unpacked = np.zeros(total_values, dtype=np.uint8)

        # Extract 4-bit values from each uint32
        for i in range(values_per_elem):
            shift = i * bits
            mask = (1 << bits) - 1  # 0x0F for 4-bit
            unpacked[i::values_per_elem] = (weight_flat >> shift) & mask

        # Trim to exact size and reshape
        unpacked = unpacked[:out_features * in_features].reshape(out_features, in_features)

        # Convert to signed int (4-bit range: -8 to 7)
        unpacked = unpacked.astype(np.int8)
        unpacked = np.where(unpacked > 7, unpacked - 16, unpacked)

        # Apply scales and biases per group using vectorized operations
        # Reshape unpacked to (out_features, num_groups, group_size) for broadcasting
        unpacked_grouped = unpacked.reshape(out_features, num_groups, group_size).astype(np.float32)

        # scales and biases shape: (out_features, num_groups)
        # Expand dims for broadcasting: (out_features, num_groups, 1)
        # Dequantize: value = (quantized * scale) + bias
        dequantized = (
            unpacked_grouped * scales[:, :, np.newaxis] + biases[:, :, np.newaxis]
        )

        # Reshape back to (out_features, in_features) and convert to float16
        dequantized = dequantized.reshape(out_features, in_features).astype(np.float16)

        return dequantized

    def __del__(self):
        """Close all cached shard files."""
        self._shard_cache.clear()


def build_moe_layer_dense(
    hidden_states,
    router_weight: np.ndarray,
    expert_weights: List[Dict[str, np.ndarray]],
    num_experts_per_tok: int,
    hidden_size: int,
    moe_intermediate_size: int,
    name: str = "moe_layer"
):
    """Build a MoE layer using dense computation (all experts).

    This is simpler but less efficient than sparse dispatch. All experts are computed
    and then masked/selected based on routing weights.

    Args:
        hidden_states: Input tensor (batch, seq_len, hidden_size)
        router_weight: Router linear projection weight (num_experts, hidden_size)
        expert_weights: List of dicts with 'gate_proj', 'up_proj', 'down_proj' for each expert
        num_experts_per_tok: Number of experts to select per token (top-k)
        hidden_size: Model hidden dimension
        moe_intermediate_size: Expert intermediate dimension
        name: Name prefix for operations

    Returns:
        Output tensor after MoE processing
    """

    # 1. Router: compute expert scores for each token
    # router_logits: (batch, seq_len, num_experts)
    router_logits = mb.linear(
        x=hidden_states,
        weight=router_weight,
        bias=None,
        name=f"{name}_router_logits"
    )

    # 2. Softmax to get routing probabilities
    routing_probs = mb.softmax(
        x=router_logits,
        axis=-1,
        name=f"{name}_routing_probs"
    )

    # 3. Top-k selection: get top num_experts_per_tok experts
    # topk returns (values, indices)
    # Note: CoreML's topk doesn't have a 'sorted' parameter - it always returns sorted results
    routing_weights, selected_experts = mb.topk(
        x=routing_probs,
        k=num_experts_per_tok,
        axis=-1,
        name=f"{name}_topk"
    )

    # 4. Normalize routing weights (optional but recommended)
    # Sum across the k dimension and divide
    weight_sum = mb.reduce_sum(
        x=routing_weights,
        axes=[-1],
        keep_dims=True,
        name=f"{name}_weight_sum"
    )
    routing_weights = mb.real_div(
        x=routing_weights,
        y=weight_sum,
        name=f"{name}_normalized_weights"
    )

    # 5. Compute all expert outputs (dense approach)
    # For each expert, compute: down_proj(silu(gate_proj(x)) * up_proj(x))
    expert_outputs = []

    for expert_idx, expert in enumerate(expert_weights):
        # Gate projection
        gate_out = mb.linear(
            x=hidden_states,
            weight=expert["gate_proj"],
            bias=None,
            name=f"{name}_expert{expert_idx}_gate"
        )

        # SiLU activation
        gate_out = mb.silu(x=gate_out, name=f"{name}_expert{expert_idx}_silu")

        # Up projection
        up_out = mb.linear(
            x=hidden_states,
            weight=expert["up_proj"],
            bias=None,
            name=f"{name}_expert{expert_idx}_up"
        )

        # Element-wise multiply
        mlp_hidden = mb.mul(
            x=gate_out,
            y=up_out,
            name=f"{name}_expert{expert_idx}_mul"
        )

        # Down projection
        expert_out = mb.linear(
            x=mlp_hidden,
            weight=expert["down_proj"],
            bias=None,
            name=f"{name}_expert{expert_idx}_down"
        )

        expert_outputs.append(expert_out)

    # 6. Stack expert outputs: (num_experts, batch, seq_len, hidden_size)
    stacked_expert_outputs = mb.stack(
        values=expert_outputs,
        axis=0,
        name=f"{name}_stacked_experts"
    )

    # 7. Gather selected expert outputs based on selected_experts indices
    # This is the tricky part - we need to select top-k experts per token
    # For now, use a simplified approach with gather operations

    # Reshape for easier indexing
    # selected_experts: (batch, seq_len, k)
    # We need to gather from stacked_expert_outputs using these indices

    # TODO: Implement proper sparse dispatch using gather/scatter
    # For MVP, average all expert outputs weighted by routing_probs
    # This is less efficient but easier to implement in CoreML

    # Weighted average approach:
    # output = sum(routing_prob[i] * expert_output[i] for i in range(num_experts))

    weighted_sum = None
    num_experts = len(expert_weights)

    for expert_idx in range(num_experts):
        # Get routing weight for this expert: (batch, seq_len, 1)
        expert_weight = mb.slice_by_index(
            x=routing_probs,
            begin=[0, 0, expert_idx],
            end=[0, 0, expert_idx + 1],
            begin_mask=[True, True, False],
            end_mask=[True, True, False],
            name=f"{name}_expert{expert_idx}_weight"
        )

        # Get expert output: (batch, seq_len, hidden_size)
        expert_out = expert_outputs[expert_idx]

        # Weight the expert output
        weighted_expert = mb.mul(
            x=expert_out,
            y=expert_weight,
            name=f"{name}_expert{expert_idx}_weighted"
        )

        # Accumulate
        if weighted_sum is None:
            weighted_sum = weighted_expert
        else:
            weighted_sum = mb.add(
                x=weighted_sum,
                y=weighted_expert,
                name=f"{name}_weighted_sum_{expert_idx}"
            )

    return weighted_sum


def build_silu_activation(x, name: str = "silu"):
    """Build SiLU (Swish) activation: x * sigmoid(x).

    Args:
        x: Input tensor
        name: Operation name

    Returns:
        SiLU activated tensor
    """
    return mb.silu(x=x, name=name)


def build_rms_norm(x, weight: np.ndarray, eps: float = 1e-6, name: str = "rms_norm"):
    """Build RMSNorm operation.

    RMSNorm: x / sqrt(mean(x^2) + eps) * weight

    Args:
        x: Input tensor
        weight: Learned scale parameter
        eps: Small constant for numerical stability
        name: Operation name

    Returns:
        RMSNorm output tensor
    """
    # Compute x^2
    x_squared = mb.mul(x=x, y=x, name=f"{name}_squared")

    # Mean across last dimension
    mean_squared = mb.reduce_mean(
        x=x_squared,
        axes=[-1],
        keep_dims=True,
        name=f"{name}_mean"
    )

    # Add epsilon (cast to float16 to match input dtype)
    eps_f16 = np.float16(eps)
    mean_squared_eps = mb.add(
        x=mean_squared,
        y=eps_f16,
        name=f"{name}_eps"
    )

    # Sqrt
    rms = mb.sqrt(x=mean_squared_eps, name=f"{name}_rms")

    # Divide
    normalized = mb.real_div(x=x, y=rms, name=f"{name}_normalized")

    # Scale by weight
    output = mb.mul(x=normalized, y=weight, name=f"{name}_output")

    return output


def estimate_model_size(config: Dict) -> float:
    """Estimate CoreML model size in GB.

    Args:
        config: Model configuration dict

    Returns:
        Estimated size in GB
    """
    num_params = config.get("num_parameters", 0)

    if num_params == 0:
        # Estimate from architecture
        hidden_size = config["hidden_size"]
        num_layers = config["num_hidden_layers"]
        vocab_size = config["vocab_size"]
        num_experts = config.get("num_experts", 1)
        moe_intermediate_size = config.get("moe_intermediate_size", hidden_size * 4)

        # Embedding layers
        embed_params = vocab_size * hidden_size

        # Per-layer parameters
        # Attention: 4 * (hidden_size * hidden_size) for Q, K, V, O
        attn_params = 4 * hidden_size * hidden_size

        # MoE: router + num_experts * (gate + up + down)
        # Each expert MLP: gate_proj, up_proj, down_proj
        router_params = hidden_size * num_experts
        expert_params = num_experts * (
            hidden_size * moe_intermediate_size +  # gate_proj
            hidden_size * moe_intermediate_size +  # up_proj
            moe_intermediate_size * hidden_size    # down_proj
        )

        # Layer norms
        norm_params = 2 * hidden_size

        layer_params = attn_params + router_params + expert_params + norm_params
        total_params = embed_params + (num_layers * layer_params)
    else:
        total_params = num_params

    # Assume FP16 (2 bytes per param)
    bytes_per_param = 2
    total_bytes = total_params * bytes_per_param

    # Convert to GB
    size_gb = total_bytes / (1024 ** 3)

    return size_gb


def validate_ane_compatibility(seq_len: int, hidden_size: int) -> List[str]:
    """Validate model dimensions for ANE compatibility.

    Args:
        seq_len: Sequence length
        hidden_size: Hidden dimension size

    Returns:
        List of warning messages (empty if all checks pass)
    """
    warnings = []

    # ANE prefers multiples of 8
    if seq_len % 8 != 0:
        warnings.append(f"seq_len={seq_len} is not a multiple of 8 (ANE optimization)")

    if hidden_size % 8 != 0:
        warnings.append(f"hidden_size={hidden_size} is not a multiple of 8 (ANE optimization)")

    # ANE has limits on tensor dimensions
    max_seq_len = 8192
    if seq_len > max_seq_len:
        warnings.append(f"seq_len={seq_len} exceeds ANE recommended max ({max_seq_len})")

    return warnings


def print_conversion_summary(
    model_path: Path,
    output_path: Path,
    config: Dict,
    elapsed_seconds: float
):
    """Print a summary of the conversion.

    Args:
        model_path: Source model path
        output_path: Output .mlpackage path
        config: Model configuration
        elapsed_seconds: Conversion time in seconds
    """
    print("\n" + "=" * 70)
    print("Conversion Summary")
    print("=" * 70)
    print(f"Source: {model_path}")
    print(f"Output: {output_path}")
    print(f"Architecture: {config.get('model_type', 'unknown')}")
    print(f"Layers: {config.get('num_hidden_layers', 0)}")
    print(f"Hidden size: {config.get('hidden_size', 0)}")
    print(f"Num experts: {config.get('num_experts', 1)}")
    print(f"Experts per token: {config.get('num_experts_per_tok', 1)}")
    print(f"Conversion time: {elapsed_seconds / 60:.1f} minutes")

    # Size estimate
    size_gb = estimate_model_size(config)
    print(f"Estimated size: {size_gb:.2f} GB")

    # Check actual size if output exists
    if output_path.exists():
        if output_path.is_dir():
            total_size = sum(f.stat().st_size for f in output_path.rglob('*') if f.is_file())
        else:
            total_size = output_path.stat().st_size
        actual_size_gb = total_size / (1024 ** 3)
        print(f"Actual size: {actual_size_gb:.2f} GB")

    print("=" * 70)
