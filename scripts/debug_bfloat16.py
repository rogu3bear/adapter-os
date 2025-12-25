#!/usr/bin/env python3
"""
Debug script to understand how safetensors represents bfloat16.
"""

import sys
from pathlib import Path
from safetensors import safe_open
import numpy as np

def debug_dtype():
    """Debug dtype representation in safetensors."""
    model_path = Path("./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit")

    # Load the index to find a shard
    import json
    index_path = model_path / "model.safetensors.index.json"
    with open(index_path) as f:
        index = json.load(f)

    # Get a weight that should be bfloat16
    test_key = "model.layers.0.input_layernorm.weight"
    shard_name = index["weight_map"][test_key]
    shard_path = model_path / shard_name

    print(f"Testing: {test_key}")
    print(f"Shard: {shard_name}")
    print("=" * 70)

    with safe_open(str(shard_path), framework="numpy") as f:
        tensor = f.get_tensor(test_key)

        print(f"Type: {type(tensor)}")
        print(f"Has dtype attr: {hasattr(tensor, 'dtype')}")

        if hasattr(tensor, 'dtype'):
            print(f"dtype: {tensor.dtype}")
            print(f"dtype as str: {str(tensor.dtype)}")
            print(f"dtype repr: {repr(tensor.dtype)}")
            print(f"dtype name: {tensor.dtype.name if hasattr(tensor.dtype, 'name') else 'N/A'}")

        print(f"\nShape: {tensor.shape if hasattr(tensor, 'shape') else 'N/A'}")

        # Try to understand the actual type
        print("\nTrying to access data...")
        try:
            arr = np.array(tensor)
            print(f"Conversion to np.array succeeded: dtype={arr.dtype}")
        except Exception as e:
            print(f"Conversion failed: {e}")
            print(f"Error type: {type(e).__name__}")

        # Try direct float32 conversion
        print("\nTrying direct float32 conversion...")
        try:
            arr = np.array(tensor, dtype=np.float32)
            print(f"✓ Direct float32 conversion succeeded: shape={arr.shape}, dtype={arr.dtype}")
        except Exception as e:
            print(f"✗ Direct float32 conversion failed: {e}")

if __name__ == "__main__":
    debug_dtype()
