#!/usr/bin/env bash
#
# CoreML Model Conversion CLI
#
# Quick wrapper for converting models to CoreML format
#
# Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

usage() {
    cat <<EOF
CoreML Model Conversion CLI

Usage: $0 [OPTIONS] INPUT OUTPUT

Arguments:
  INPUT           Input safetensors file path
  OUTPUT          Output CoreML .mlpackage path

Options:
  -q, --quantize TYPE      Quantization type (float32|float16|int8|int4)
                           Default: float16
  -b, --batch-size N       Batch size (default: 1, recommended for ANE)
  -s, --seq-length N       Sequence length (default: 128)
  --no-ane                 Disable ANE optimization
  --lora                   Convert LoRA adapter
  --lora-base PATH         Base model for LoRA merging
  --validate               Run validation after conversion
  -h, --help               Show this help message

Examples:
  # Convert model with FP16 quantization (ANE-optimized)
  $0 model.safetensors model.mlpackage

  # Convert with INT8 quantization
  $0 -q int8 model.safetensors model.mlpackage

  # Convert and validate
  $0 --validate model.safetensors model.mlpackage

  # Convert LoRA adapter
  $0 --lora --lora-base base.safetensors adapter.safetensors adapter.mlpackage

EOF
    exit 1
}

# Default options
QUANTIZE="float16"
BATCH_SIZE=1
SEQ_LENGTH=128
TARGET_ANE=true
LORA=false
LORA_BASE=""
VALIDATE=false

# Parse arguments
INPUT=""
OUTPUT=""

while [[ $# -gt 0 ]]; do
    case $1 in
        -q|--quantize)
            QUANTIZE="$2"
            shift 2
            ;;
        -b|--batch-size)
            BATCH_SIZE="$2"
            shift 2
            ;;
        -s|--seq-length)
            SEQ_LENGTH="$2"
            shift 2
            ;;
        --no-ane)
            TARGET_ANE=false
            shift
            ;;
        --lora)
            LORA=true
            shift
            ;;
        --lora-base)
            LORA_BASE="$2"
            shift 2
            ;;
        --validate)
            VALIDATE=true
            shift
            ;;
        -h|--help)
            usage
            ;;
        *)
            if [[ -z "$INPUT" ]]; then
                INPUT="$1"
            elif [[ -z "$OUTPUT" ]]; then
                OUTPUT="$1"
            else
                echo "Error: Unexpected argument: $1"
                usage
            fi
            shift
            ;;
    esac
done

# Validate arguments
if [[ -z "$INPUT" ]] || [[ -z "$OUTPUT" ]]; then
    echo "Error: INPUT and OUTPUT are required"
    usage
fi

if [[ ! -f "$INPUT" ]]; then
    echo "Error: Input file not found: $INPUT"
    exit 1
fi

if [[ "$LORA" == true ]] && [[ -z "$LORA_BASE" ]]; then
    echo "Error: --lora-base required when using --lora"
    exit 1
fi

# Check Python dependencies
if ! command -v python3 &> /dev/null; then
    echo "Error: python3 not found"
    exit 1
fi

if ! python3 -c "import coremltools" 2>/dev/null; then
    echo "Error: coremltools not installed"
    echo "Install: pip install coremltools torch safetensors transformers"
    exit 1
fi

# Build conversion command
CONVERT_CMD="python3 $SCRIPT_DIR/convert_to_coreml.py"
CONVERT_CMD="$CONVERT_CMD --input \"$INPUT\""
CONVERT_CMD="$CONVERT_CMD --output \"$OUTPUT\""
CONVERT_CMD="$CONVERT_CMD --quantize $QUANTIZE"
CONVERT_CMD="$CONVERT_CMD --batch-size $BATCH_SIZE"
CONVERT_CMD="$CONVERT_CMD --sequence-length $SEQ_LENGTH"

if [[ "$TARGET_ANE" == false ]]; then
    CONVERT_CMD="$CONVERT_CMD --no-ane"
fi

if [[ "$LORA" == true ]]; then
    CONVERT_CMD="$CONVERT_CMD --lora"
    CONVERT_CMD="$CONVERT_CMD --lora-base \"$LORA_BASE\""
fi

# Print configuration
echo "🔧 CoreML Model Conversion"
echo "=========================="
echo ""
echo "Input:       $INPUT"
echo "Output:      $OUTPUT"
echo "Quantize:    $QUANTIZE"
echo "Batch size:  $BATCH_SIZE"
echo "Seq length:  $SEQ_LENGTH"
echo "Target ANE:  $TARGET_ANE"
if [[ "$LORA" == true ]]; then
    echo "LoRA mode:   enabled"
    echo "LoRA base:   $LORA_BASE"
fi
echo ""

# Run conversion
echo "Starting conversion..."
eval "$CONVERT_CMD"

if [[ $? -eq 0 ]]; then
    echo ""
    echo "✅ Conversion completed successfully!"
    echo "Output: $OUTPUT"

    # Run validation if requested
    if [[ "$VALIDATE" == true ]]; then
        echo ""
        echo "Running validation..."

        # TODO: Add validation step
        echo "⚠️  Validation not yet implemented in CLI"
        echo "Manual validation: Load model in CoreML and test inference"
    fi

    echo ""
    echo "Next steps:"
    echo "  1. Load model in CoreML backend"
    echo "  2. Test inference with sample inputs"
    echo "  3. Verify ANE is being used (check logs)"
else
    echo ""
    echo "❌ Conversion failed"
    exit 1
fi
