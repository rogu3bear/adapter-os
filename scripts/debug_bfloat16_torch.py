#!/usr/bin/env python3
"""
Debug script to load bfloat16 using PyTorch framework.
"""

import sys
from pathlib import Path
from safetensors import safe_open
import numpy as np

def debug_dtype():
    """Load using PyTorch framework which supports bfloat16."""
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

    # Try with PyTorch framework
    try:
        import torch
        print("✓ PyTorch available")
        print(f"PyTorch version: {torch.__version__}")

        with safe_open(str(shard_path), framework="pt") as f:
            tensor = f.get_tensor(test_key)

            print(f"\n✓ Loaded tensor with PyTorch framework")
            print(f"Type: {type(tensor)}")
            print(f"dtype: {tensor.dtype}")
            print(f"Shape: {tensor.shape}")

            # Convert to numpy via float32
            print(f"\nConverting to numpy...")
            if tensor.dtype == torch.bfloat16:
                print("  Detected bfloat16, converting via float32...")
                tensor_f32 = tensor.to(torch.float32)
                arr = tensor_f32.numpy()
                arr_f16 = arr.astype(np.float16)
                print(f"  ✓ Result: shape={arr_f16.shape}, dtype={arr_f16.dtype}")
            else:
                arr = tensor.numpy()
                print(f"  ✓ Result: shape={arr.shape}, dtype={arr.dtype}")

    except ImportError:
        print("✗ PyTorch not available")
        return

if __name__ == "__main__":
    debug_dtype()
