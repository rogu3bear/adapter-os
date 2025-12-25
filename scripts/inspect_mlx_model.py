#!/usr/bin/env python3
"""
MLX Model Inspector
===================

Inspect an MLX model directory and display architecture details.
Useful for understanding model structure before conversion.

Usage:
    python scripts/inspect_mlx_model.py ./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit
"""

import argparse
import json
from pathlib import Path
import sys


def format_bytes(bytes_val):
    """Format bytes as human-readable string."""
    for unit in ['B', 'KB', 'MB', 'GB', 'TB']:
        if bytes_val < 1024.0:
            return f"{bytes_val:.2f} {unit}"
        bytes_val /= 1024.0
    return f"{bytes_val:.2f} PB"


def inspect_model(model_path: Path):
    """Inspect MLX model directory."""

    print("=" * 70)
    print("MLX Model Inspector")
    print("=" * 70)
    print()

    # Check directory exists
    if not model_path.exists():
        print(f"ERROR: Path does not exist: {model_path}")
        sys.exit(1)

    if not model_path.is_dir():
        print(f"ERROR: Path is not a directory: {model_path}")
        sys.exit(1)

    print(f"Model path: {model_path}")
    print()

    # Load config
    config_path = model_path / "config.json"
    if not config_path.exists():
        print("ERROR: config.json not found")
        sys.exit(1)

    with open(config_path) as f:
        config = json.load(f)

    # Display config
    print("Configuration")
    print("-" * 70)

    arch = config.get("architectures", ["unknown"])[0]
    print(f"Architecture:            {arch}")
    print(f"Model type:              {config.get('model_type', 'unknown')}")
    print(f"Hidden size:             {config.get('hidden_size', 0)}")
    print(f"Number of layers:        {config.get('num_hidden_layers', 0)}")
    print(f"Attention heads:         {config.get('num_attention_heads', 0)}")
    print(f"KV heads:                {config.get('num_key_value_heads', 0)}")
    print(f"Head dimension:          {config.get('head_dim', 0)}")
    print(f"Vocab size:              {config.get('vocab_size', 0)}")
    print(f"Max position:            {config.get('max_position_embeddings', 0)}")

    # MoE specific
    if 'num_experts' in config:
        print()
        print("MoE Configuration")
        print("-" * 70)
        print(f"Number of experts:       {config['num_experts']}")
        print(f"Experts per token:       {config['num_experts_per_tok']}")
        print(f"MoE intermediate size:   {config['moe_intermediate_size']}")
        print(f"Intermediate size:       {config.get('intermediate_size', 0)}")
        print(f"Shared expert size:      {config.get('shared_expert_intermediate_size', 0)}")

    # Quantization
    if 'quantization' in config or 'quantization_config' in config:
        quant = config.get('quantization', config.get('quantization_config', {}))
        print()
        print("Quantization")
        print("-" * 70)
        print(f"Bits:                    {quant.get('bits', 'unknown')}")
        print(f"Group size:              {quant.get('group_size', 'unknown')}")

    # Check safetensors files
    print()
    print("Model Files")
    print("-" * 70)

    safetensors_files = list(model_path.glob("*.safetensors"))
    index_file = model_path / "model.safetensors.index.json"

    if index_file.exists():
        with open(index_file) as f:
            index = json.load(f)

        metadata = index.get("metadata", {})
        total_size = metadata.get("total_size", 0)
        total_params = metadata.get("total_parameters", 0)

        print(f"Total parameters:        {total_params:,}")
        print(f"Total size:              {format_bytes(total_size)}")
        print(f"Safetensors shards:      {len(safetensors_files)}")

        print()
        print("Shards:")
        for shard_file in sorted(safetensors_files):
            size = shard_file.stat().st_size
            print(f"  {shard_file.name:40s} {format_bytes(size)}")
    else:
        print("No model.safetensors.index.json found")
        if safetensors_files:
            print(f"Found {len(safetensors_files)} safetensors files:")
            for f in safetensors_files:
                size = f.stat().st_size
                print(f"  {f.name:40s} {format_bytes(size)}")

    # Estimate conversion requirements
    print()
    print("Conversion Estimates")
    print("-" * 70)

    hidden_size = config.get("hidden_size", 0)
    num_layers = config.get("num_hidden_layers", 0)
    vocab_size = config.get("vocab_size", 0)
    num_experts = config.get("num_experts", 1)
    moe_intermediate_size = config.get("moe_intermediate_size", hidden_size * 4)

    # Estimate parameters per layer
    # Embedding
    embed_params = vocab_size * hidden_size

    # Attention (Q, K, V, O projections)
    attn_params = 4 * hidden_size * hidden_size

    # MoE
    router_params = hidden_size * num_experts
    expert_mlp_params = (
        hidden_size * moe_intermediate_size +  # gate_proj
        hidden_size * moe_intermediate_size +  # up_proj
        moe_intermediate_size * hidden_size     # down_proj
    )
    total_expert_params = num_experts * expert_mlp_params

    # Norms
    norm_params = 2 * hidden_size

    layer_params = attn_params + router_params + total_expert_params + norm_params

    print(f"Parameters per layer:    {layer_params:,}")
    print(f"  - Attention:           {attn_params:,}")
    print(f"  - Router:              {router_params:,}")
    print(f"  - Experts ({num_experts}):       {total_expert_params:,}")
    print(f"  - Norms:               {norm_params:,}")

    print()
    print(f"Total model parameters:  {(embed_params + num_layers * layer_params):,}")

    # Size in FP16
    total_params_calc = embed_params + num_layers * layer_params
    bytes_fp16 = total_params_calc * 2  # 2 bytes per FP16 param

    print(f"Estimated FP16 size:     {format_bytes(bytes_fp16)}")

    # Memory requirements
    # During conversion, we need:
    # - Original quantized weights loaded
    # - Dequantized FP16 weights
    # - CoreML graph in memory
    conversion_memory_gb = bytes_fp16 / (1024 ** 3) * 2.5  # 2.5x multiplier

    print()
    print(f"RAM needed (estimated):  {conversion_memory_gb:.1f} GB")
    print()

    # MVP estimates (4 layers, 16 experts)
    mvp_layers = min(num_layers, 4)
    mvp_experts = min(num_experts, 16)

    mvp_expert_params = mvp_experts * expert_mlp_params
    mvp_layer_params = attn_params + router_params + mvp_expert_params + norm_params
    mvp_total_params = embed_params + mvp_layers * mvp_layer_params
    mvp_bytes = mvp_total_params * 2

    print("MVP Conversion (4 layers, 16 experts)")
    print("-" * 70)
    print(f"Layers:                  {mvp_layers} of {num_layers}")
    print(f"Experts per layer:       {mvp_experts} of {num_experts}")
    print(f"Parameters:              {mvp_total_params:,}")
    print(f"Size (FP16):             {format_bytes(mvp_bytes)}")
    print(f"RAM needed:              {mvp_bytes / (1024 ** 3) * 2:.1f} GB")
    print()

    # Recommendations
    print("Recommendations")
    print("-" * 70)

    if conversion_memory_gb < 16:
        print("  - Should work on most modern Macs (16GB+)")
    elif conversion_memory_gb < 32:
        print("  - Requires Mac with 32GB RAM")
    else:
        print("  - Requires Mac with 64GB+ RAM")
        print("  - Consider MVP conversion (4 layers, 16 experts)")

    print(f"  - Start with seq_len=512 for testing")
    print(f"  - Use --single-layer 0 for initial validation")
    print(f"  - Expected conversion time: 20-30 minutes (MVP)")
    print()

    print("=" * 70)


def main():
    parser = argparse.ArgumentParser(
        description="Inspect MLX model directory and estimate conversion requirements"
    )
    parser.add_argument(
        "model_path",
        type=Path,
        help="Path to MLX model directory"
    )

    args = parser.parse_args()
    inspect_model(args.model_path)


if __name__ == "__main__":
    main()
