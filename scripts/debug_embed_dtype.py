#!/usr/bin/env python3
"""
Debug script to check embed_tokens dtype.
"""

import sys
from pathlib import Path
from safetensors import safe_open
import torch

def debug_dtype():
    """Check embed_tokens dtype."""
    model_path = Path("./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit")

    # Load the index to find a shard
    import json
    index_path = model_path / "model.safetensors.index.json"
    with open(index_path) as f:
        index = json.load(f)

    test_key = "model.embed_tokens.weight"
    shard_name = index["weight_map"][test_key]
    shard_path = model_path / shard_name

    print(f"Testing: {test_key}")
    print(f"Shard: {shard_name}")
    print("=" * 70)

    with safe_open(str(shard_path), framework="pt") as f:
        tensor = f.get_tensor(test_key)

        print(f"Type: {type(tensor)}")
        print(f"dtype: {tensor.dtype}")
        print(f"Shape: {tensor.shape}")

        # Check if it's quantized
        print(f"\nIs this a quantized tensor?")
        print(f"dtype str: {str(tensor.dtype)}")

if __name__ == "__main__":
    debug_dtype()
