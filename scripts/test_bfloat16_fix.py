#!/usr/bin/env python3
"""
Test script to verify bfloat16 handling in weight loader.
"""

import sys
from pathlib import Path
import numpy as np

# Add scripts directory to path
sys.path.insert(0, str(Path(__file__).parent))

from coreml_moe_ops import WeightLoader

def test_weight_loading():
    """Test that weights load without bfloat16 errors."""
    model_path = Path("./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit")

    if not model_path.exists():
        print(f"ERROR: Model not found at {model_path}")
        return False

    print(f"Testing weight loading from: {model_path}")
    print("=" * 70)

    try:
        loader = WeightLoader(model_path)
        print(f"✓ WeightLoader initialized successfully")

        # Test loading a few different weight types
        test_weights = [
            "model.embed_tokens.weight",
            "model.layers.0.input_layernorm.weight",
            "model.layers.0.self_attn.q_proj.weight",
            "model.layers.0.self_attn.q_proj.scales",
            "model.layers.0.self_attn.q_proj.biases",
            "model.norm.weight",
        ]

        for key in test_weights:
            try:
                weight = loader.get_weight(key)
                dtype_str = str(weight.dtype)
                print(f"✓ Loaded {key}")
                print(f"  Shape: {weight.shape}, dtype: {dtype_str}")

                # Verify no bfloat16 in result
                if 'bfloat16' in dtype_str.lower():
                    print(f"  ERROR: Weight still has bfloat16 dtype!")
                    return False

                # Verify it's a valid numpy array
                if not isinstance(weight, np.ndarray):
                    print(f"  ERROR: Not a numpy array!")
                    return False

            except Exception as e:
                print(f"✗ Failed to load {key}: {e}")
                return False

        print("\n" + "=" * 70)
        print("✓ All weights loaded successfully!")
        print("✓ No bfloat16 errors detected")
        return True

    except Exception as e:
        print(f"✗ Test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

if __name__ == "__main__":
    success = test_weight_loading()
    sys.exit(0 if success else 1)
