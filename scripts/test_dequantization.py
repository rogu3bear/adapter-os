#!/usr/bin/env python3
"""
Test script to verify dequantization works correctly.
"""

import sys
from pathlib import Path
import numpy as np

# Add scripts directory to path
sys.path.insert(0, str(Path(__file__).parent))

from coreml_moe_ops import WeightLoader

def test_dequantization():
    """Test that quantized weights can be dequantized without errors."""
    model_path = Path("./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit")

    if not model_path.exists():
        print(f"ERROR: Model not found at {model_path}")
        return False

    print(f"Testing dequantization from: {model_path}")
    print("=" * 70)

    try:
        loader = WeightLoader(model_path)
        print(f"✓ WeightLoader initialized successfully")

        # Test dequantizing a few weights
        test_weights = [
            ("model.embed_tokens", "Embedding layer"),
            ("model.layers.0.self_attn.q_proj", "Query projection"),
            ("model.layers.0.self_attn.k_proj", "Key projection"),
        ]

        for key_prefix, desc in test_weights:
            try:
                print(f"\nTesting: {desc} ({key_prefix})")

                # Load quantized components
                weight, scales, biases = loader.get_quantized_weight(key_prefix)
                print(f"  ✓ Loaded quantized components:")
                print(f"    weight: {weight.shape}, dtype: {weight.dtype}")
                print(f"    scales: {scales.shape}, dtype: {scales.dtype}")
                print(f"    biases: {biases.shape}, dtype: {biases.dtype}")

                # Verify no bfloat16
                if 'bfloat16' in str(scales.dtype).lower() or 'bfloat16' in str(biases.dtype).lower():
                    print(f"  ERROR: scales or biases still have bfloat16!")
                    return False

                # Dequantize
                dequantized = loader.dequantize_weight(
                    weight, scales, biases,
                    group_size=64, bits=4
                )
                print(f"  ✓ Dequantized successfully:")
                print(f"    shape: {dequantized.shape}, dtype: {dequantized.dtype}")

                # Verify shape is correct
                expected_rows = scales.shape[0]
                expected_cols = scales.shape[1] * 64  # group_size
                if dequantized.shape != (expected_rows, expected_cols):
                    print(f"  ERROR: Wrong shape! Expected ({expected_rows}, {expected_cols})")
                    return False

                # Verify dtype is float16
                if dequantized.dtype != np.float16:
                    print(f"  ERROR: Wrong dtype! Expected float16")
                    return False

                print(f"  ✓ Shape and dtype are correct")

            except Exception as e:
                print(f"✗ Failed to dequantize {key_prefix}: {e}")
                import traceback
                traceback.print_exc()
                return False

        print("\n" + "=" * 70)
        print("✓ All dequantization tests passed!")
        print("✓ bfloat16 handling is working correctly")
        return True

    except Exception as e:
        print(f"✗ Test failed: {e}")
        import traceback
        traceback.print_exc()
        return False

if __name__ == "__main__":
    success = test_dequantization()
    sys.exit(0 if success else 1)
