#!/usr/bin/env python3
"""
Production CoreML Export Script for Qwen2.5-7B
=============================================

Creates FP16-optimized CoreML models for ANE (Apple Neural Engine) deployment.

Features:
- FP16 precision for ~50% model size reduction and ANE optimization
- Multiple sequence length variants (512, 2048, 4096)
- Validation against PyTorch reference
- Batch size 1 (required for ANE determinism)

Requirements:
    pip install torch==2.4.0 coremltools==7.2 transformers numpy==1.26.4

Usage:
    python scripts/export_coreml_production.py --seq-len 2048 --validate
    python scripts/export_coreml_production.py --all-variants
"""

import argparse
import sys
import time
from pathlib import Path
from typing import Optional

import numpy as np
import torch
import coremltools as ct
from transformers import AutoModelForCausalLM, AutoTokenizer


class LogitsOnlyWrapper(torch.nn.Module):
    """Wrapper that returns only logits for clean tracing."""

    def __init__(self, model):
        super().__init__()
        self.model = model

    def forward(self, input_ids):
        outputs = self.model(input_ids, use_cache=False)
        return outputs.logits


def print_banner(title: str):
    """Print a section banner."""
    print()
    print("=" * 70)
    print(title)
    print("=" * 70)


def load_model(model_id: str, use_fp16: bool = True) -> tuple:
    """Load PyTorch model and tokenizer."""
    print(f"Loading model: {model_id}")
    print(f"Precision: {'FP16' if use_fp16 else 'FP32'}")

    start = time.time()

    dtype = torch.float16 if use_fp16 else torch.float32

    model = AutoModelForCausalLM.from_pretrained(
        model_id,
        torch_dtype=dtype,
        trust_remote_code=True,
        low_cpu_mem_usage=True,
        attn_implementation="eager",  # Required for clean tracing
    )
    tokenizer = AutoTokenizer.from_pretrained(model_id, trust_remote_code=True)

    print(f"Model loaded in {time.time()-start:.1f}s")
    print(f"  Layers: {model.config.num_hidden_layers}")
    print(f"  Hidden: {model.config.hidden_size}")
    print(f"  Vocab: {tokenizer.vocab_size}")

    return model, tokenizer


def trace_model(model: torch.nn.Module, seq_len: int, vocab_size: int) -> torch.jit.ScriptModule:
    """Trace model with torch.jit.trace."""
    print(f"\nTracing model with seq_len={seq_len}...")

    model.eval()
    wrapped = LogitsOnlyWrapper(model)
    wrapped.eval()

    example_input = torch.randint(0, vocab_size, (1, seq_len), dtype=torch.long)
    print(f"Example input shape: {example_input.shape}")

    start = time.time()
    with torch.no_grad():
        traced = torch.jit.trace(wrapped, (example_input,), strict=False)
    print(f"Trace completed in {time.time()-start:.1f}s")

    return traced, example_input


def convert_to_coreml(
    traced: torch.jit.ScriptModule,
    seq_len: int,
    output_path: Path,
    model_id: str,
    use_fp16: bool = True,
) -> ct.models.MLModel:
    """Convert traced model to CoreML."""
    print(f"\nConverting to CoreML (this may take 30-60 minutes)...")

    start = time.time()

    # Configure conversion
    inputs = [
        ct.TensorType(name="input_ids", shape=(1, seq_len), dtype=np.int32)
    ]
    outputs = [
        ct.TensorType(name="logits")
    ]

    # FP16 compute precision for ANE optimization
    compute_precision = ct.precision.FLOAT16 if use_fp16 else ct.precision.FLOAT32

    mlmodel = ct.convert(
        traced,
        inputs=inputs,
        outputs=outputs,
        minimum_deployment_target=ct.target.macOS13,
        compute_units=ct.ComputeUnit.ALL,  # Enable ANE
        convert_to="mlprogram",
        compute_precision=compute_precision,
    )

    elapsed = time.time() - start
    print(f"Conversion completed in {elapsed/60:.1f} minutes")

    # Add metadata
    mlmodel.author = "adapterOS"
    mlmodel.license = "Apache-2.0"
    mlmodel.short_description = f"{model_id} - CoreML FP16 for adapterOS (seq_len={seq_len})"
    mlmodel.version = "1.0"

    # Add custom metadata
    mlmodel.user_defined_metadata["seq_len"] = str(seq_len)
    mlmodel.user_defined_metadata["precision"] = "fp16" if use_fp16 else "fp32"
    mlmodel.user_defined_metadata["batch_size"] = "1"
    mlmodel.user_defined_metadata["source_model"] = model_id

    # Save
    output_path.parent.mkdir(parents=True, exist_ok=True)
    print(f"Saving to: {output_path}")
    mlmodel.save(str(output_path))

    # Report size
    if output_path.is_dir():
        total_size = sum(f.stat().st_size for f in output_path.rglob('*') if f.is_file())
    else:
        total_size = output_path.stat().st_size
    size_gb = total_size / (1024**3)
    print(f"Model size: {size_gb:.2f} GB")

    return mlmodel


def validate_output(
    pytorch_model: torch.nn.Module,
    coreml_model: ct.models.MLModel,
    tokenizer,
    seq_len: int,
) -> bool:
    """Validate CoreML output against PyTorch reference."""
    print("\nValidating CoreML output against PyTorch...")

    # Create test input
    test_prompt = "Hello, how are you today?"
    tokens = tokenizer.encode(test_prompt, return_tensors="pt")

    # Pad or truncate to seq_len
    if tokens.shape[1] < seq_len:
        padding = torch.zeros(1, seq_len - tokens.shape[1], dtype=torch.long)
        tokens = torch.cat([tokens, padding], dim=1)
    else:
        tokens = tokens[:, :seq_len]

    print(f"Test input shape: {tokens.shape}")

    # PyTorch inference
    pytorch_model.eval()
    wrapped = LogitsOnlyWrapper(pytorch_model)
    wrapped.eval()

    with torch.no_grad():
        pt_logits = wrapped(tokens)

    # CoreML inference
    input_dict = {"input_ids": tokens.numpy().astype(np.int32)}
    coreml_out = coreml_model.predict(input_dict)
    coreml_logits = coreml_out["logits"]

    # Compare - convert FP16 to FP32 first to avoid numpy conversion issues
    pt_float32 = pt_logits.float()  # Ensure FP32 before numpy
    pt_np = pt_float32.numpy()

    # Convert CoreML output to same dtype for comparison
    coreml_np = coreml_logits.astype(np.float32)

    # Check for NaN/Inf in outputs
    pt_has_nan = np.isnan(pt_np).any() or np.isinf(pt_np).any()
    coreml_has_nan = np.isnan(coreml_np).any() or np.isinf(coreml_np).any()

    if pt_has_nan:
        print("  WARNING: PyTorch output contains NaN/Inf")
    if coreml_has_nan:
        print("  WARNING: CoreML output contains NaN/Inf")

    # Calculate metrics (handle NaN gracefully)
    abs_diff = np.abs(pt_np - coreml_np)
    max_diff = np.nanmax(abs_diff) if not (pt_has_nan or coreml_has_nan) else float('nan')
    mean_diff = np.nanmean(abs_diff) if not (pt_has_nan or coreml_has_nan) else float('nan')

    # Check top-k agreement (most important for generation)
    pt_top5 = np.argsort(pt_np[0, -1, :])[-5:][::-1]
    coreml_top5 = np.argsort(coreml_np[0, -1, :])[-5:][::-1]
    top5_match = np.array_equal(pt_top5, coreml_top5)
    top1_match = pt_top5[0] == coreml_top5[0]

    print(f"  Max absolute difference: {max_diff:.6f}")
    print(f"  Mean absolute difference: {mean_diff:.6f}")
    print(f"  Top-1 token match: {top1_match}")
    print(f"  Top-5 tokens match: {top5_match}")

    # FP16 tolerance is higher than FP32
    tolerance = 0.1 if pt_logits.dtype == torch.float16 else 0.001

    if max_diff < tolerance and top1_match:
        print("  Validation: PASSED")
        return True
    else:
        print(f"  Validation: WARNING - differences detected (tolerance: {tolerance})")
        return False


def export_variant(
    model_id: str,
    output_dir: Path,
    seq_len: int,
    validate: bool = False,
    use_fp16: bool = True,
) -> Optional[Path]:
    """Export a single variant."""
    print_banner(f"Exporting {seq_len} token variant")

    precision_suffix = "fp16" if use_fp16 else "fp32"
    output_path = output_dir / f"qwen2.5-7b-instruct-{precision_suffix}-{seq_len}.mlpackage"

    print(f"Output: {output_path}")
    print(f"Precision: {precision_suffix.upper()}")
    print(f"Sequence length: {seq_len}")

    try:
        # Load model
        model, tokenizer = load_model(model_id, use_fp16=use_fp16)

        # Trace
        traced, example_input = trace_model(model, seq_len, tokenizer.vocab_size)

        # Convert
        mlmodel = convert_to_coreml(traced, seq_len, output_path, model_id, use_fp16)

        # Validate if requested
        if validate:
            validate_output(model, mlmodel, tokenizer, seq_len)

        print_banner(f"SUCCESS: {output_path}")
        return output_path

    except Exception as e:
        print_banner(f"ERROR: {e}")
        import traceback
        traceback.print_exc()
        return None


def main():
    parser = argparse.ArgumentParser(
        description="Export Qwen2.5-7B to CoreML with FP16 optimization",
        formatter_class=argparse.RawDescriptionHelpFormatter,
        epilog="""
Examples:
    # Export single 2048-token variant with validation
    python scripts/export_coreml_production.py --seq-len 2048 --validate

    # Export all standard variants (512, 2048, 4096)
    python scripts/export_coreml_production.py --all-variants

    # Export FP32 for debugging (larger but more precise)
    python scripts/export_coreml_production.py --seq-len 2048 --fp32
        """
    )

    parser.add_argument(
        "--model-id",
        default="Qwen/Qwen2.5-7B-Instruct",
        help="HuggingFace model ID (default: Qwen/Qwen2.5-7B-Instruct)"
    )
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path("models"),
        help="Output directory (default: models)"
    )
    parser.add_argument(
        "--seq-len",
        type=int,
        default=2048,
        help="Sequence length (default: 2048, must be multiple of 8 for ANE)"
    )
    parser.add_argument(
        "--all-variants",
        action="store_true",
        help="Export all standard variants (512, 2048, 4096)"
    )
    parser.add_argument(
        "--validate",
        action="store_true",
        help="Validate output against PyTorch reference"
    )
    parser.add_argument(
        "--fp32",
        action="store_true",
        help="Use FP32 instead of FP16 (for debugging)"
    )

    args = parser.parse_args()

    # Validate seq_len is multiple of 8
    if args.seq_len % 8 != 0:
        print(f"ERROR: seq_len must be multiple of 8 for ANE optimization (got {args.seq_len})")
        sys.exit(1)

    print_banner("adapterOS CoreML Production Export")
    print(f"PyTorch: {torch.__version__}")
    print(f"NumPy: {np.__version__}")
    print(f"coremltools: {ct.__version__}")
    print(f"Model: {args.model_id}")
    print(f"Output dir: {args.output_dir}")

    use_fp16 = not args.fp32

    if args.all_variants:
        # Export all standard variants
        variants = [512, 2048, 4096]
        results = []

        for seq_len in variants:
            result = export_variant(
                args.model_id,
                args.output_dir,
                seq_len,
                validate=args.validate,
                use_fp16=use_fp16,
            )
            results.append((seq_len, result))

        # Summary
        print_banner("Export Summary")
        for seq_len, path in results:
            status = "SUCCESS" if path else "FAILED"
            print(f"  {seq_len} tokens: {status}")
            if path:
                print(f"    -> {path}")

        # Exit with error if any failed
        if any(r[1] is None for r in results):
            sys.exit(1)
    else:
        # Export single variant
        result = export_variant(
            args.model_id,
            args.output_dir,
            args.seq_len,
            validate=args.validate,
            use_fp16=use_fp16,
        )

        if result is None:
            sys.exit(1)

    print()
    print("To use with adapterOS:")
    print("  AOS_MODEL_BACKEND=coreml ./target/release/aos-worker \\")
    print("      --model-path models/qwen2.5-7b-instruct-fp16-2048.mlpackage")


if __name__ == "__main__":
    main()
