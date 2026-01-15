#!/usr/bin/env python3
"""
Export Qwen2.5 model to CoreML .mlpackage format for ANE acceleration.

This is a ONE-TIME model preparation script, not runtime code.
The exported .mlpackage can then be used by the CoreML backend.

Requirements:
    pip install coremltools torch transformers

Usage:
    python scripts/export_coreml_model.py

    # Or with custom paths:
    python scripts/export_coreml_model.py \
        --model Qwen/Qwen2.5-7B-Instruct \
        --output models/qwen2.5-7b-instruct.mlpackage

Memory Requirements:
    - 7B model: ~32GB RAM recommended
    - 3B model: ~16GB RAM
    - 1.5B model: ~8GB RAM

Time Estimates:
    - 7B model: 30-60 minutes
    - 3B model: 15-30 minutes
    - 1.5B model: 5-15 minutes
"""

import argparse
import sys
from pathlib import Path

# Import torch at module level for LogitsOnlyWrapper class
try:
    import torch
except ImportError:
    torch = None  # Will fail at check_dependencies


def check_dependencies():
    """Check if required packages are installed."""
    missing = []

    try:
        import coremltools
        print(f"✓ coremltools {coremltools.__version__}")
    except ImportError:
        missing.append("coremltools")

    try:
        import torch
        print(f"✓ torch {torch.__version__}")
    except ImportError:
        missing.append("torch")

    try:
        import transformers
        print(f"✓ transformers {transformers.__version__}")
    except ImportError:
        missing.append("transformers")

    if missing:
        print(f"\n❌ Missing packages: {', '.join(missing)}")
        print("\nInstall with:")
        print(f"    pip install {' '.join(missing)}")
        sys.exit(1)

    print()


class LogitsOnlyWrapper(torch.nn.Module):
    """Wrapper that returns only logits tensor, not the full CausalLMOutput."""
    def __init__(self, model):
        super().__init__()
        self.model = model

    def forward(self, input_ids):
        # Disable KV cache for tracing (use_cache=False)
        outputs = self.model(input_ids, use_cache=False)
        return outputs.logits


def export_to_coreml(model_id: str, output_path: Path, seq_len: int = 128):
    """
    Export a Hugging Face model to CoreML .mlpackage format.

    Args:
        model_id: HuggingFace model ID (e.g., "Qwen/Qwen2.5-7B-Instruct")
        output_path: Output path for .mlpackage
        seq_len: Sequence length for tracing (default: 128)
    """
    import coremltools as ct
    from transformers import AutoModelForCausalLM, AutoTokenizer
    import torch

    print(f"Loading model: {model_id}")
    print("This may take several minutes for large models...")

    # Load model in float16 to reduce memory
    model = AutoModelForCausalLM.from_pretrained(
        model_id,
        torch_dtype=torch.float16,
        trust_remote_code=True,
        low_cpu_mem_usage=True,
    )
    tokenizer = AutoTokenizer.from_pretrained(model_id, trust_remote_code=True)

    print(f"✓ Model loaded: {model.config.num_hidden_layers} layers, "
          f"{model.config.hidden_size} hidden size")

    # Set to eval mode
    model.eval()

    # Wrap model to return only logits (avoids DynamicCache tracing issues)
    wrapped_model = LogitsOnlyWrapper(model)
    wrapped_model.eval()

    # Create example input
    vocab_size = tokenizer.vocab_size
    print(f"Creating trace input: batch=1, seq_len={seq_len}, vocab={vocab_size}")
    input_ids = torch.randint(0, vocab_size, (1, seq_len), dtype=torch.long)

    # Trace model
    print("Tracing model (this takes a while for 7B)...")
    print("  - Using LogitsOnlyWrapper to avoid DynamicCache issues")
    with torch.no_grad():
        traced_model = torch.jit.trace(wrapped_model, (input_ids,))
    print("✓ Model traced")

    # Convert to CoreML
    print("Converting to CoreML (this is the slow part)...")
    print("  - Optimizing for ANE (Neural Engine)")
    print("  - Target: macOS 13+")

    mlmodel = ct.convert(
        traced_model,
        inputs=[
            ct.TensorType(
                name="input_ids",
                shape=(1, seq_len),
                dtype=ct.int32
            )
        ],
        outputs=[
            ct.TensorType(name="logits", dtype=ct.float16)
        ],
        minimum_deployment_target=ct.target.macOS13,
        compute_units=ct.ComputeUnit.ALL,  # Enable ANE
        convert_to="mlprogram",  # ML Program format (required for ANE)
    )

    print("✓ Conversion complete")

    # Add metadata
    mlmodel.author = "adapterOS"
    mlmodel.license = "Apache-2.0"
    mlmodel.short_description = f"{model_id} - CoreML for adapterOS LoRA inference"
    mlmodel.version = "1.0"

    # Save
    output_path = Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)

    print(f"Saving to: {output_path}")
    mlmodel.save(str(output_path))

    # Report size
    if output_path.is_dir():
        total_size = sum(f.stat().st_size for f in output_path.rglob('*') if f.is_file())
    else:
        total_size = output_path.stat().st_size
    size_gb = total_size / (1024**3)

    print(f"\n✅ Export complete!")
    print(f"   Output: {output_path}")
    print(f"   Size: {size_gb:.2f} GB")
    print(f"\nTo use with adapterOS:")
    print(f"   AOS_MODEL_BACKEND=coreml ./target/release/aos-worker \\")
    print(f"       --model-path {output_path}")


def main():
    parser = argparse.ArgumentParser(
        description="Export Hugging Face model to CoreML for adapterOS",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog=__doc__
    )
    parser.add_argument(
        "--model", "-m",
        default="Qwen/Qwen2.5-7B-Instruct",
        help="HuggingFace model ID (default: Qwen/Qwen2.5-7B-Instruct)"
    )
    parser.add_argument(
        "--output", "-o",
        default=None,
        help="Output .mlpackage path (default: models/<model-name>.mlpackage)"
    )
    parser.add_argument(
        "--seq-len",
        type=int,
        default=128,
        help="Sequence length for tracing (default: 128)"
    )
    parser.add_argument(
        "--check-only",
        action="store_true",
        help="Only check dependencies, don't convert"
    )

    args = parser.parse_args()

    print("=" * 60)
    print("adapterOS CoreML Model Exporter")
    print("=" * 60)
    print()

    # Check dependencies
    print("Checking dependencies...")
    check_dependencies()

    if args.check_only:
        print("✓ All dependencies installed")
        return

    # Determine output path
    if args.output:
        output_path = Path(args.output)
    else:
        model_name = args.model.split("/")[-1].lower()
        output_path = Path(f"models/{model_name}.mlpackage")

    print(f"Model:  {args.model}")
    print(f"Output: {output_path}")
    print(f"SeqLen: {args.seq_len}")
    print()

    # Confirm
    print("⚠️  This will:")
    print("   1. Download the model from HuggingFace (~14GB for 7B)")
    print("   2. Load it into memory (~32GB RAM for 7B)")
    print("   3. Convert to CoreML (30-60 min for 7B)")
    print()

    response = input("Continue? [y/N] ").strip().lower()
    if response != 'y':
        print("Aborted.")
        return

    print()
    export_to_coreml(args.model, output_path, args.seq_len)


if __name__ == "__main__":
    main()
