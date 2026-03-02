#!/usr/bin/env bash
# Download Qwen2.5-3B-Instruct for adapterOS / JKCA CoreML
#
# This script downloads the Qwen2.5-3B-Instruct model from Hugging Face
# (32K context, suitable for CoreML conversion via jkca-agent prepare_qwen3b_offline.sh).
#
# Copyright: © 2025 JKCA / James KC Auchterlonie. All rights reserved.

set -euo pipefail

# Color codes for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Script directory (repo root)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MODEL_DIR="$REPO_ROOT/var/models"
MODEL_NAME="Qwen2.5-3B-Instruct"
MODEL_PATH="$MODEL_DIR/$MODEL_NAME"

# Hugging Face model ID
HF_MODEL_ID="Qwen/Qwen2.5-3B-Instruct"

echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}  adapterOS Model Downloader: Qwen2.5-3B-Instruct (32K context)${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo ""

# Check if huggingface-cli is installed
if ! command -v huggingface-cli &> /dev/null; then
    echo -e "${RED}✗ Error: huggingface-cli not found${NC}"
    echo ""
    echo "Please install the Hugging Face CLI:"
    echo "  pip install huggingface-hub[cli]"
    echo ""
    echo "Or using pipx (recommended):"
    echo "  pipx install huggingface-hub[cli]"
    exit 1
fi

echo -e "${GREEN}✓ Found huggingface-cli${NC}"

# Create models directory if it doesn't exist
if [ ! -d "$MODEL_DIR" ]; then
    echo -e "${YELLOW}→ Creating models directory: $MODEL_DIR${NC}"
    mkdir -p "$MODEL_DIR"
fi

# Check if model already exists
if [ -d "$MODEL_PATH" ]; then
    echo -e "${YELLOW}⚠ Model directory already exists: $MODEL_PATH${NC}"
    echo ""
    read -p "Do you want to re-download? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo -e "${BLUE}→ Skipping download. Verifying existing model...${NC}"

        # Verify model has required files
        REQUIRED_FILES=("config.json" "tokenizer.json")
        MISSING_FILES=()

        for file in "${REQUIRED_FILES[@]}"; do
            if [ ! -f "$MODEL_PATH/$file" ]; then
                MISSING_FILES+=("$file")
            fi
        done

        if [ ${#MISSING_FILES[@]} -gt 0 ]; then
            echo -e "${RED}✗ Missing required files: ${MISSING_FILES[*]}${NC}"
            echo -e "${YELLOW}→ Please delete the directory and re-run this script${NC}"
            exit 1
        fi

        echo -e "${GREEN}✓ Model verified${NC}"
        echo ""
        echo -e "${GREEN}Model ready at: $MODEL_PATH${NC}"
        echo ""
        echo -e "${BLUE}JKCA CoreML conversion (32K context):${NC}"
        echo "  cd /path/to/jkca-agent"
        echo "  COREML_HF_SNAPSHOT_DIR=$MODEL_PATH make coreml-prepare-3b"
        exit 0
    fi

    # Remove existing directory
    echo -e "${YELLOW}→ Removing existing model directory${NC}"
    rm -rf "$MODEL_PATH"
fi

# Display download information
echo ""
echo -e "${BLUE}Downloading: $HF_MODEL_ID${NC}"
echo -e "${BLUE}Destination: $MODEL_PATH${NC}"
echo ""
echo -e "${YELLOW}Note: ~6GB download. 32K context for CoreML conversion.${NC}"
echo ""

# Download the model
echo -e "${GREEN}→ Starting download...${NC}"
echo ""

huggingface-cli download "$HF_MODEL_ID" \
    --include "*.safetensors" "*.json" "*.txt" \
    --local-dir "$MODEL_PATH" \
    --local-dir-use-symlinks False

# Verify download
echo ""
echo -e "${BLUE}→ Verifying download...${NC}"

REQUIRED_FILES=("config.json" "tokenizer.json")
MISSING_FILES=()

for file in "${REQUIRED_FILES[@]}"; do
    if [ -f "$MODEL_PATH/$file" ]; then
        echo -e "${GREEN}  ✓ Found: $file${NC}"
    else
        echo -e "${RED}  ✗ Missing: $file${NC}"
        MISSING_FILES+=("$file")
    fi
done

# Check for model weights
WEIGHT_FILES=$(find "$MODEL_PATH" -name "*.safetensors" -o -name "*.bin" 2>/dev/null | wc -l)
if [ "$WEIGHT_FILES" -gt 0 ]; then
    echo -e "${GREEN}  ✓ Found $WEIGHT_FILES weight file(s)${NC}"
else
    echo -e "${RED}  ✗ No weight files found${NC}"
    MISSING_FILES+=("model weights")
fi

if [ ${#MISSING_FILES[@]} -gt 0 ]; then
    echo ""
    echo -e "${RED}✗ Download incomplete or corrupted${NC}"
    echo -e "${RED}  Missing: ${MISSING_FILES[*]}${NC}"
    exit 1
fi

# Calculate total size
TOTAL_SIZE=$(du -sh "$MODEL_PATH" | cut -f1)

echo ""
echo -e "${GREEN}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${GREEN}✓ Model downloaded successfully!${NC}"
echo -e "${GREEN}═══════════════════════════════════════════════════════════════${NC}"
echo ""
echo -e "${BLUE}Model Location:${NC} $MODEL_PATH"
echo -e "${BLUE}Total Size:${NC}     $TOTAL_SIZE"
echo ""
echo -e "${BLUE}JKCA CoreML conversion (32K context):${NC}"
echo "  cd /path/to/jkca-agent"
echo "  COREML_HF_SNAPSHOT_DIR=$MODEL_PATH make coreml-prepare-3b"
echo ""
