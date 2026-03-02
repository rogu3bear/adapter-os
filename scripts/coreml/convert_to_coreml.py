#!/usr/bin/env python3
"""
Convert trained MLX synthesis model to CoreML package.

This script takes the fine-tuned MLX model and converts it to a CoreML
package optimized for Apple Neural Engine inference.

Usage:
    python convert_to_coreml.py \
        --input output/synthesis_model_mlx/final \
        --output output/synthesis_model.mlpackage \
        --compute-units cpu_and_ne
"""

import argparse
import json
import os
import shutil
import sys
from pathlib import Path

try:
    import coremltools as ct
    import mlx.core as mx
    from mlx_lm import load
except ImportError as e:
    print(f"Missing dependency: {e}")
    print("Please install: pip install coremltools mlx mlx-lm")
    sys.exit(1)

import numpy as np


def merge_lora_weights(base_model, lora_path: Path):
    """Merge LoRA weights into base model."""
    print(f"Loading LoRA weights from: {lora_path}")
    
    lora_weights = mx.load(str(lora_path / "adapter.safetensors"))
    
    # Group LoRA weights by target layer
    lora_pairs = {}  # layer_name -> (lora_a, lora_b)
    
    for name, weight in lora_weights.items():
        # Parse LoRA weight name
        # Format: model.layers.0.self_attn.q_proj.lora_a
        parts = name.rsplit(".", 2)
        if len(parts) >= 2:
            base_name = parts[0]
            lora_type = parts[-1]  # "lora_a" or "lora_b"
            
            if base_name not in lora_pairs:
                lora_pairs[base_name] = {}
            lora_pairs[base_name][lora_type] = weight
    
    # Merge into base model
    merged_count = 0
    for name, lora in lora_pairs.items():
        if "lora_a" in lora and "lora_b" in lora:
            # Compute merged weight: W' = W + B @ A
            lora_a = lora["lora_a"]
            lora_b = lora["lora_b"]
            delta = mx.matmul(lora_b, lora_a)
            
            # Find and update base weight
            # Navigate to the weight in the model
            parts = name.split(".")
            module = base_model
            for part in parts[:-1]:
                if part.isdigit():
                    module = module[int(part)]
                else:
                    module = getattr(module, part)
            
            weight_name = parts[-1]
            base_weight = getattr(module, weight_name)
            merged_weight = base_weight + delta
            setattr(module, weight_name, merged_weight)
            merged_count += 1
            print(f"  Merged: {name}")
    
    print(f"Merged {merged_count} LoRA layers")
    return base_model


def convert_to_coreml(
    model,
    tokenizer,
    output_path: Path,
    seq_length: int = 2048,
    compute_units: str = "cpu_and_ne"
):
    """Convert MLX model to CoreML package."""
    print(f"\nConverting to CoreML...")
    print(f"  Sequence length: {seq_length}")
    print(f"  Compute units: {compute_units}")
    
    # Get model config
    vocab_size = tokenizer.vocab_size
    hidden_size = model.config.hidden_size if hasattr(model, "config") else 896
    
    # Export to intermediate format first
    # MLX doesn't have direct CoreML export, so we'll use a traced approach
    
    print("  Tracing model...")
    
    # Create dummy input
    dummy_input = mx.zeros((1, seq_length), dtype=mx.int32)
    
    # Trace through the model
    # For now, we'll export the merged weights and reconstruct in CoreML
    
    # Save merged weights to safetensors
    merged_weights_path = output_path.parent / "merged_weights.safetensors"
    weights_dict = dict(model.named_parameters())
    mx.save_safetensors(str(merged_weights_path), weights_dict)
    print(f"  Saved merged weights to: {merged_weights_path}")
    
    # Note: Full CoreML conversion requires model architecture definition
    # For production, use the adapterOS conversion pipeline:
    #   scripts/convert_mlx_to_coreml.py
    
    print(f"""
  =========================================================
  Weights exported. To complete CoreML conversion, run:
  
  python ../../scripts/convert_mlx_to_coreml.py \\
      --input {merged_weights_path} \\
      --output {output_path} \\
      --seq-len {seq_length} \\
      --compute-units {compute_units}
  =========================================================
""")
    
    # Create a metadata file for the conversion
    metadata = {
        "model_type": "synthesis",
        "base_model": "Qwen/Qwen2.5-0.5B-Instruct",
        "vocab_size": vocab_size,
        "hidden_size": hidden_size,
        "seq_length": seq_length,
        "compute_units": compute_units,
        "merged_weights": str(merged_weights_path),
    }
    
    metadata_path = output_path.parent / "coreml_conversion_config.json"
    with open(metadata_path, "w") as f:
        json.dump(metadata, f, indent=2)
    print(f"  Saved conversion config to: {metadata_path}")


def main():
    parser = argparse.ArgumentParser(
        description="Convert MLX synthesis model to CoreML"
    )
    parser.add_argument(
        "--input", "-i",
        type=str,
        required=True,
        help="Path to trained MLX model directory"
    )
    parser.add_argument(
        "--output", "-o",
        type=str,
        default="output/synthesis_model.mlpackage",
        help="Output CoreML package path"
    )
    parser.add_argument(
        "--base-model",
        type=str,
        default="Qwen/Qwen2.5-0.5B-Instruct",
        help="Base model name/path"
    )
    parser.add_argument(
        "--seq-length",
        type=int,
        default=2048,
        help="Sequence length for CoreML model"
    )
    parser.add_argument(
        "--compute-units",
        type=str,
        choices=["cpu_only", "cpu_and_gpu", "cpu_and_ne", "all"],
        default="cpu_and_ne",
        help="CoreML compute units"
    )
    args = parser.parse_args()
    
    input_path = Path(args.input)
    output_path = Path(args.output)
    
    # Load base model
    print(f"Loading base model: {args.base_model}")
    model, tokenizer = load(args.base_model)
    
    # Merge LoRA weights if they exist
    if (input_path / "adapter.safetensors").exists():
        model = merge_lora_weights(model, input_path)
    else:
        print(f"No LoRA weights found at {input_path}, using base model")
    
    # Create output directory
    output_path.parent.mkdir(parents=True, exist_ok=True)
    
    # Convert to CoreML
    convert_to_coreml(
        model,
        tokenizer,
        output_path,
        seq_length=args.seq_length,
        compute_units=args.compute_units,
    )
    
    print("\nConversion preparation complete!")


if __name__ == "__main__":
    main()
