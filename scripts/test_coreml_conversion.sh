#!/bin/bash
# Test CoreML MoE Conversion Pipeline
# =====================================
#
# This script tests the MLX to CoreML conversion pipeline with a single layer.
# It's designed to validate the conversion works before attempting full model conversion.

set -e  # Exit on error

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
MODEL_DIR="${PROJECT_ROOT}/var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit"
OUTPUT_DIR="${PROJECT_ROOT}/var/models"
TEST_OUTPUT="${OUTPUT_DIR}/Qwen3-30B-test-layer0.mlpackage"
SEQ_LEN=128  # Short sequence for faster testing

echo "========================================================================"
echo "CoreML MoE Conversion Test"
echo "========================================================================"
echo ""

# Check Python environment
echo -n "Checking Python... "
if ! command -v python3 &> /dev/null; then
    echo -e "${RED}FAILED${NC}"
    echo "Python 3 is required but not found"
    exit 1
fi
PYTHON_VERSION=$(python3 --version)
echo -e "${GREEN}OK${NC} ($PYTHON_VERSION)"

# Check if model exists
echo -n "Checking model directory... "
if [ ! -d "$MODEL_DIR" ]; then
    echo -e "${RED}FAILED${NC}"
    echo "Model not found at: $MODEL_DIR"
    echo ""
    echo "Please download the model first:"
    echo "  ./scripts/download-model.sh Qwen3-Coder-30B-A3B-Instruct-MLX-4bit"
    exit 1
fi
echo -e "${GREEN}OK${NC}"

# Check required files
echo -n "Checking config.json... "
if [ ! -f "$MODEL_DIR/config.json" ]; then
    echo -e "${RED}FAILED${NC}"
    echo "config.json not found in $MODEL_DIR"
    exit 1
fi
echo -e "${GREEN}OK${NC}"

echo -n "Checking safetensors files... "
SHARD_COUNT=$(find "$MODEL_DIR" -name "model-*.safetensors" | wc -l | tr -d ' ')
if [ "$SHARD_COUNT" -eq 0 ]; then
    echo -e "${RED}FAILED${NC}"
    echo "No safetensors files found in $MODEL_DIR"
    exit 1
fi
echo -e "${GREEN}OK${NC} ($SHARD_COUNT shards)"

# Check Python dependencies
echo ""
echo "Checking Python dependencies..."

check_package() {
    local package=$1
    echo -n "  $package... "
    if python3 -c "import $package" 2>/dev/null; then
        echo -e "${GREEN}OK${NC}"
        return 0
    else
        echo -e "${RED}MISSING${NC}"
        return 1
    fi
}

MISSING_DEPS=0

check_package "coremltools" || MISSING_DEPS=1
check_package "numpy" || MISSING_DEPS=1
check_package "safetensors" || MISSING_DEPS=1

if [ $MISSING_DEPS -eq 1 ]; then
    echo ""
    echo -e "${YELLOW}Some dependencies are missing.${NC}"
    echo "Install them with:"
    echo "  pip install -r $SCRIPT_DIR/requirements-convert.txt"
    exit 1
fi

# Check disk space
echo ""
echo -n "Checking disk space... "
AVAILABLE_GB=$(df -g "$OUTPUT_DIR" | awk 'NR==2 {print $4}')
if [ "$AVAILABLE_GB" -lt 5 ]; then
    echo -e "${YELLOW}WARNING${NC}"
    echo "  Only ${AVAILABLE_GB}GB available. Recommend 10GB+ for conversion."
else
    echo -e "${GREEN}OK${NC} (${AVAILABLE_GB}GB available)"
fi

# Run conversion
echo ""
echo "========================================================================"
echo "Running Single Layer Conversion Test"
echo "========================================================================"
echo ""
echo "Configuration:"
echo "  Model: $MODEL_DIR"
echo "  Output: $TEST_OUTPUT"
echo "  Sequence length: $SEQ_LEN"
echo "  Layer: 0 (single layer test)"
echo ""

# Clean up previous test output
if [ -d "$TEST_OUTPUT" ]; then
    echo "Removing previous test output..."
    rm -rf "$TEST_OUTPUT"
fi

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Run conversion
echo "Starting conversion..."
echo ""

START_TIME=$(date +%s)

cd "$PROJECT_ROOT"

python3 "$SCRIPT_DIR/convert_mlx_to_coreml.py" \
    --input "$MODEL_DIR" \
    --output "$TEST_OUTPUT" \
    --seq-len "$SEQ_LEN" \
    --single-layer 0

CONVERSION_EXIT_CODE=$?
END_TIME=$(date +%s)
ELAPSED=$((END_TIME - START_TIME))

echo ""
echo "========================================================================"

if [ $CONVERSION_EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}Conversion Successful!${NC}"
else
    echo -e "${RED}Conversion Failed!${NC}"
    exit 1
fi

echo "========================================================================"
echo ""

# Verify output
echo "Verifying output..."
echo ""

if [ ! -d "$TEST_OUTPUT" ]; then
    echo -e "${RED}ERROR: Output .mlpackage not found${NC}"
    exit 1
fi

# Check package structure
echo -n "  Package structure... "
if [ -f "$TEST_OUTPUT/Manifest.json" ]; then
    echo -e "${GREEN}OK${NC}"
else
    echo -e "${RED}FAILED${NC} (Manifest.json missing)"
    exit 1
fi

# Calculate size
PACKAGE_SIZE=$(du -sh "$TEST_OUTPUT" | awk '{print $1}')
echo "  Package size: $PACKAGE_SIZE"

# Show contents
echo ""
echo "Package contents:"
find "$TEST_OUTPUT" -type f | head -10 | sed 's/^/  /'
TOTAL_FILES=$(find "$TEST_OUTPUT" -type f | wc -l | tr -d ' ')
if [ "$TOTAL_FILES" -gt 10 ]; then
    echo "  ... and $((TOTAL_FILES - 10)) more files"
fi

echo ""
echo "========================================================================"
echo -e "${GREEN}Test Complete!${NC}"
echo "========================================================================"
echo ""
echo "Results:"
echo "  Conversion time: ${ELAPSED}s"
echo "  Output: $TEST_OUTPUT"
echo "  Size: $PACKAGE_SIZE"
echo ""
echo "Next steps:"
echo "  1. Inspect the model:"
echo "     python3 -c 'import coremltools as ct; m = ct.models.MLModel(\"$TEST_OUTPUT\"); print(m)'"
echo ""
echo "  2. Try full model conversion (4 layers, 16 experts):"
echo "     python3 $SCRIPT_DIR/convert_mlx_to_coreml.py \\"
echo "         --input $MODEL_DIR \\"
echo "         --output $OUTPUT_DIR/Qwen3-30B-CoreML.mlpackage \\"
echo "         --seq-len 512"
echo ""
echo "  3. Implement CoreML backend in Rust (Phase 3)"
echo ""
