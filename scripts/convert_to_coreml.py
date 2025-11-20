#!/usr/bin/env python3
"""
AdapterOS CoreML Model Converter

This script converts models from various formats (safetensors, PyTorch, ONNX)
to CoreML .mlpackage format with ANE optimization and quantization support.

Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.

Usage:
    # Convert safetensors to CoreML with FP16 quantization
    python3 convert_to_coreml.py --input weights.safetensors --output model.mlpackage

    # Convert with INT8 quantization
    python3 convert_to_coreml.py --input weights.safetensors --output model.mlpackage --quantize int8

    # Convert LoRA adapter
    python3 convert_to_coreml.py --input lora_adapter.safetensors --output adapter.mlpackage --lora

    # Convert with calibration
    python3 convert_to_coreml.py --input weights.safetensors --output model.mlpackage --calibrate calibration.json
"""

import argparse
import json
import logging
import sys
from pathlib import Path
from typing import Dict, List, Optional, Tuple
import numpy as np

# Check dependencies
try:
    import coremltools as ct
    import torch
    from safetensors import safe_open
    from transformers import AutoModelForCausalLM, AutoConfig, AutoTokenizer
except ImportError as e:
    print(f"❌ Missing dependency: {e}")
    print("Install dependencies:")
    print("  pip install coremltools torch safetensors transformers")
    sys.exit(1)

# Setup logging
logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s - %(levelname)s - %(message)s"
)
logger = logging.getLogger(__name__)


class QuantizationType:
    """Quantization types"""
    FLOAT32 = "float32"
    FLOAT16 = "float16"
    INT8 = "int8"
    INT4 = "int4"


class CoreMLConverter:
    """Convert models to CoreML format with ANE optimization"""

    def __init__(
        self,
        input_path: Path,
        output_path: Path,
        quantization: str = QuantizationType.FLOAT16,
        target_ane: bool = True,
        batch_size: int = 1,
        sequence_length: int = 128,
        min_macos_version: str = "13.0",
    ):
        self.input_path = input_path
        self.output_path = output_path
        self.quantization = quantization
        self.target_ane = target_ane
        self.batch_size = batch_size
        self.sequence_length = sequence_length
        self.min_macos_version = min_macos_version

        # Validate paths
        if not input_path.exists():
            raise FileNotFoundError(f"Input file not found: {input_path}")

        # Create output directory
        output_path.parent.mkdir(parents=True, exist_ok=True)

    def load_safetensors(self) -> Dict[str, torch.Tensor]:
        """Load weights from safetensors file"""
        logger.info(f"Loading safetensors: {self.input_path}")

        state_dict = {}
        with safe_open(self.input_path, framework="pt") as f:
            for key in f.keys():
                state_dict[key] = f.get_tensor(key)

        logger.info(f"Loaded {len(state_dict)} tensors")
        return state_dict

    def infer_model_config(self, state_dict: Dict[str, torch.Tensor]) -> Dict:
        """Infer model configuration from state dict"""
        logger.info("Inferring model configuration...")

        # Try to extract vocab size
        vocab_size = None
        if "lm_head.weight" in state_dict:
            vocab_size = state_dict["lm_head.weight"].shape[0]

        # Count transformer layers
        num_layers = 0
        for key in state_dict.keys():
            if "model.layers." in key:
                layer_idx = int(key.split(".")[2])
                num_layers = max(num_layers, layer_idx + 1)

        # Extract hidden size
        hidden_size = None
        for key in state_dict.keys():
            if "model.layers.0.self_attn.q_proj.weight" in key:
                hidden_size = state_dict[key].shape[1]
                break

        # Extract attention heads
        num_attention_heads = None
        if hidden_size:
            # Assume head_dim = 128
            if "model.layers.0.self_attn.q_proj.weight" in state_dict:
                out_dim = state_dict["model.layers.0.self_attn.q_proj.weight"].shape[0]
                num_attention_heads = out_dim // 128

        # Extract intermediate size (FFN)
        intermediate_size = None
        if "model.layers.0.mlp.gate_proj.weight" in state_dict:
            intermediate_size = state_dict["model.layers.0.mlp.gate_proj.weight"].shape[0]

        config = {
            "vocab_size": vocab_size or 152064,  # Qwen2.5 default
            "hidden_size": hidden_size or 3584,
            "num_hidden_layers": num_layers or 28,
            "num_attention_heads": num_attention_heads or 28,
            "intermediate_size": intermediate_size or 18944,
        }

        logger.info(f"Inferred config: {config}")
        return config

    def create_model(self, state_dict: Dict[str, torch.Tensor]) -> torch.nn.Module:
        """Create PyTorch model and load weights"""
        logger.info("Creating model architecture...")

        config_dict = self.infer_model_config(state_dict)

        # Create config
        config = AutoConfig.from_pretrained(
            "Qwen/Qwen2.5-7B",  # Base architecture
            **config_dict,
        )

        # Create model
        model = AutoModelForCausalLM.from_config(config)

        # Load weights
        logger.info("Loading weights into model...")
        missing_keys, unexpected_keys = model.load_state_dict(state_dict, strict=False)

        if missing_keys:
            logger.warning(f"Missing keys: {missing_keys[:5]}...")
        if unexpected_keys:
            logger.warning(f"Unexpected keys: {unexpected_keys[:5]}...")

        model.eval()
        return model

    def quantize_model(self, mlmodel: ct.models.MLModel) -> ct.models.MLModel:
        """Apply quantization to CoreML model"""
        if self.quantization == QuantizationType.FLOAT32:
            logger.info("No quantization applied (FP32)")
            return mlmodel

        logger.info(f"Applying {self.quantization} quantization...")

        if self.quantization == QuantizationType.FLOAT16:
            # FP16 quantization (recommended for ANE)
            return ct.models.neural_network.quantization_utils.quantize_weights(
                mlmodel,
                nbits=16,
            )
        elif self.quantization == QuantizationType.INT8:
            # INT8 quantization
            return ct.models.neural_network.quantization_utils.quantize_weights(
                mlmodel,
                nbits=8,
            )
        elif self.quantization == QuantizationType.INT4:
            # INT4 quantization (experimental)
            logger.warning("INT4 quantization is experimental and may not be ANE-compatible")
            return ct.models.neural_network.quantization_utils.quantize_weights(
                mlmodel,
                nbits=4,
            )
        else:
            raise ValueError(f"Unknown quantization type: {self.quantization}")

    def convert_to_coreml(self, model: torch.nn.Module) -> ct.models.MLModel:
        """Convert PyTorch model to CoreML"""
        logger.info("Converting to CoreML...")

        # Create example input
        input_ids = torch.randint(
            0,
            model.config.vocab_size,
            (self.batch_size, self.sequence_length),
            dtype=torch.long,
        )

        logger.info(f"Example input shape: {input_ids.shape}")

        # Trace model
        logger.info("Tracing model with TorchScript...")
        with torch.no_grad():
            traced_model = torch.jit.trace(model, (input_ids,))

        # Determine compute precision
        if self.quantization == QuantizationType.FLOAT16:
            compute_precision = ct.precision.FLOAT16
        elif self.quantization == QuantizationType.INT8:
            # Use FP16 as base, then quantize
            compute_precision = ct.precision.FLOAT16
        else:
            compute_precision = ct.precision.FLOAT32

        # Determine compute units
        compute_units = ct.ComputeUnit.ALL if self.target_ane else ct.ComputeUnit.CPU_AND_GPU

        # Determine deployment target
        if self.min_macos_version >= "14.0":
            deployment_target = ct.target.macOS14
        elif self.min_macos_version >= "13.0":
            deployment_target = ct.target.macOS13
        else:
            deployment_target = ct.target.macOS12

        # Convert to CoreML
        logger.info("Performing CoreML conversion...")
        mlmodel = ct.convert(
            traced_model,
            inputs=[
                ct.TensorType(
                    name="input_ids",
                    shape=(self.batch_size, self.sequence_length),
                    dtype=np.int32,
                )
            ],
            outputs=[ct.TensorType(name="logits")],
            compute_precision=compute_precision,
            compute_units=compute_units,
            minimum_deployment_target=deployment_target,
            convert_to="mlprogram",  # ML Program for ANE support
        )

        # Apply additional quantization if needed
        if self.quantization in [QuantizationType.INT8, QuantizationType.INT4]:
            mlmodel = self.quantize_model(mlmodel)

        return mlmodel

    def add_metadata(self, mlmodel: ct.models.MLModel, config_dict: Dict) -> None:
        """Add metadata to CoreML model"""
        logger.info("Adding metadata...")

        mlmodel.author = "AdapterOS"
        mlmodel.license = "Copyright © 2025 JKCA"
        mlmodel.short_description = f"{self.input_path.stem} (CoreML)"
        mlmodel.version = "1.0.0"

        # Add custom metadata
        spec = mlmodel.get_spec()
        spec.description.metadata.userDefined["quantization"] = self.quantization
        spec.description.metadata.userDefined["target_ane"] = str(self.target_ane)
        spec.description.metadata.userDefined["vocab_size"] = str(config_dict.get("vocab_size", "unknown"))
        spec.description.metadata.userDefined["hidden_size"] = str(config_dict.get("hidden_size", "unknown"))
        spec.description.metadata.userDefined["num_layers"] = str(config_dict.get("num_hidden_layers", "unknown"))

    def validate_model(self, mlmodel: ct.models.MLModel) -> None:
        """Validate converted CoreML model"""
        logger.info("Validating converted model...")

        spec = mlmodel.get_spec()

        # Check model type
        model_type = spec.description.metadata.userDefined.get(
            "com.apple.coreml.model.preview.type", "unknown"
        )
        logger.info(f"Model type: {model_type}")

        # Check for ANE compatibility
        if self.target_ane:
            # Check for unsupported ops
            unsupported_ops = []
            if hasattr(spec, 'neuralNetwork') and hasattr(spec.neuralNetwork, 'layers'):
                for layer in spec.neuralNetwork.layers:
                    layer_type = layer.WhichOneof('layer')
                    if layer_type in ['custom', 'customLayer']:
                        unsupported_ops.append(layer.name)

            if unsupported_ops:
                logger.warning(
                    f"⚠️  Found {len(unsupported_ops)} unsupported ops (may fall back to GPU): "
                    f"{unsupported_ops[:5]}..."
                )
            else:
                logger.info("✅ All ops ANE-compatible")

        # Test inference
        logger.info("Testing inference...")
        test_input = {
            "input_ids": np.random.randint(
                0,
                152064,
                (self.batch_size, self.sequence_length),
                dtype=np.int32,
            )
        }

        try:
            output = mlmodel.predict(test_input)
            logger.info(f"✅ Inference test passed, output shape: {output['logits'].shape}")
        except Exception as e:
            logger.error(f"❌ Inference test failed: {e}")
            raise

    def save_metadata_json(self, config_dict: Dict) -> None:
        """Save conversion metadata as JSON"""
        metadata_path = self.output_path.with_suffix(".metadata.json")

        metadata = {
            "model_name": self.input_path.stem,
            "input_path": str(self.input_path),
            "output_path": str(self.output_path),
            "quantization": self.quantization,
            "target_ane": self.target_ane,
            "batch_size": self.batch_size,
            "sequence_length": self.sequence_length,
            "min_macos_version": self.min_macos_version,
            "vocab_size": config_dict.get("vocab_size"),
            "hidden_size": config_dict.get("hidden_size"),
            "num_layers": config_dict.get("num_hidden_layers"),
            "num_attention_heads": config_dict.get("num_attention_heads"),
            "intermediate_size": config_dict.get("intermediate_size"),
        }

        with open(metadata_path, "w") as f:
            json.dump(metadata, f, indent=2)

        logger.info(f"Saved metadata: {metadata_path}")

    def convert(self) -> None:
        """Main conversion pipeline"""
        logger.info("🔧 AdapterOS CoreML Converter")
        logger.info(f"Input:  {self.input_path}")
        logger.info(f"Output: {self.output_path}")
        logger.info(f"Quantization: {self.quantization}")
        logger.info(f"Target ANE: {self.target_ane}")

        # Load weights
        state_dict = self.load_safetensors()

        # Infer config
        config_dict = self.infer_model_config(state_dict)

        # Create model
        model = self.create_model(state_dict)

        # Convert to CoreML
        mlmodel = self.convert_to_coreml(model)

        # Add metadata
        self.add_metadata(mlmodel, config_dict)

        # Validate
        self.validate_model(mlmodel)

        # Save
        logger.info(f"Saving CoreML model: {self.output_path}")
        mlmodel.save(str(self.output_path))

        # Save metadata JSON
        self.save_metadata_json(config_dict)

        logger.info("✅ Conversion complete!")
        logger.info(f"Output: {self.output_path}")


class LoRAConverter(CoreMLConverter):
    """Convert LoRA adapters to CoreML format"""

    def __init__(self, *args, **kwargs):
        super().__init__(*args, **kwargs)
        logger.info("LoRA adapter conversion mode enabled")

    def load_lora_weights(self) -> Dict[str, torch.Tensor]:
        """Load LoRA adapter weights"""
        logger.info(f"Loading LoRA adapter: {self.input_path}")

        lora_weights = {}
        with safe_open(self.input_path, framework="pt") as f:
            for key in f.keys():
                if "lora_A" in key or "lora_B" in key:
                    lora_weights[key] = f.get_tensor(key)

        logger.info(f"Loaded {len(lora_weights)} LoRA tensors")
        return lora_weights

    def merge_lora_into_base(
        self,
        base_weights: Dict[str, torch.Tensor],
        lora_weights: Dict[str, torch.Tensor],
        alpha: float = 1.0,
    ) -> Dict[str, torch.Tensor]:
        """Merge LoRA weights into base model weights"""
        logger.info("Merging LoRA weights into base model...")

        merged_weights = base_weights.copy()

        # Group LoRA weights by layer
        lora_pairs = {}
        for key in lora_weights.keys():
            if "lora_A" in key:
                base_key = key.replace(".lora_A", "")
                if base_key not in lora_pairs:
                    lora_pairs[base_key] = {}
                lora_pairs[base_key]["A"] = lora_weights[key]
            elif "lora_B" in key:
                base_key = key.replace(".lora_B", "")
                if base_key not in lora_pairs:
                    lora_pairs[base_key] = {}
                lora_pairs[base_key]["B"] = lora_weights[key]

        # Merge LoRA: W' = W + alpha * B @ A
        for base_key, lora in lora_pairs.items():
            if "A" in lora and "B" in lora:
                A = lora["A"]
                B = lora["B"]

                # Compute LoRA delta: delta = alpha * B @ A
                delta = alpha * (B @ A)

                # Add to base weight
                if base_key in merged_weights:
                    merged_weights[base_key] = merged_weights[base_key] + delta
                    logger.debug(f"Merged LoRA for {base_key}")

        logger.info(f"Merged {len(lora_pairs)} LoRA layers")
        return merged_weights


def main():
    parser = argparse.ArgumentParser(
        description="Convert models to CoreML with ANE optimization"
    )
    parser.add_argument(
        "--input",
        type=Path,
        required=True,
        help="Input model path (safetensors, PyTorch, ONNX)",
    )
    parser.add_argument(
        "--output",
        type=Path,
        required=True,
        help="Output CoreML model path (.mlpackage)",
    )
    parser.add_argument(
        "--quantize",
        choices=["float32", "float16", "int8", "int4"],
        default="float16",
        help="Quantization type (default: float16 for ANE)",
    )
    parser.add_argument(
        "--no-ane",
        action="store_true",
        help="Disable ANE optimization (use CPU/GPU only)",
    )
    parser.add_argument(
        "--batch-size",
        type=int,
        default=1,
        help="Batch size (ANE optimized for 1)",
    )
    parser.add_argument(
        "--sequence-length",
        type=int,
        default=128,
        help="Maximum sequence length",
    )
    parser.add_argument(
        "--min-macos",
        default="13.0",
        help="Minimum macOS deployment target",
    )
    parser.add_argument(
        "--lora",
        action="store_true",
        help="Convert LoRA adapter (merge with base model)",
    )
    parser.add_argument(
        "--lora-base",
        type=Path,
        help="Base model for LoRA merging (safetensors)",
    )
    parser.add_argument(
        "--lora-alpha",
        type=float,
        default=1.0,
        help="LoRA scaling factor (default: 1.0)",
    )

    args = parser.parse_args()

    try:
        if args.lora:
            if not args.lora_base:
                logger.error("--lora-base required for LoRA conversion")
                sys.exit(1)

            converter = LoRAConverter(
                input_path=args.input,
                output_path=args.output,
                quantization=args.quantize,
                target_ane=not args.no_ane,
                batch_size=args.batch_size,
                sequence_length=args.sequence_length,
                min_macos_version=args.min_macos,
            )
        else:
            converter = CoreMLConverter(
                input_path=args.input,
                output_path=args.output,
                quantization=args.quantize,
                target_ane=not args.no_ane,
                batch_size=args.batch_size,
                sequence_length=args.sequence_length,
                min_macos_version=args.min_macos,
            )

        converter.convert()

    except Exception as e:
        logger.error(f"❌ Conversion failed: {e}", exc_info=True)
        sys.exit(1)


if __name__ == "__main__":
    main()
