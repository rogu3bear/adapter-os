#!/usr/bin/env bash
# Download Mistral 7B Instruct v0.3 model for adapterOS
#
# This script downloads the MLX-optimized Mistral 7B Instruct v0.3 model from Hugging Face
# and sets up the model directory structure required by adapterOS.
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
MODEL_NAME="Llama-3.2-3B-Instruct-4bit"
MODEL_PATH="$MODEL_DIR/$MODEL_NAME"

# Hugging Face model ID (4-bit quantized version - public, no auth required)
HF_MODEL_ID="mlx-community/Llama-3.2-3B-Instruct-4bit"

echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}  adapterOS Model Downloader${NC}"
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
echo -e "${YELLOW}Note: This will download ~3.8GB of model weights${NC}"
echo -e "${YELLOW}      Download time depends on your internet connection${NC}"
echo ""

# Download the model
echo -e "${GREEN}→ Starting download...${NC}"
echo ""

huggingface-cli download "$HF_MODEL_ID" \
    --include "*.safetensors" "*.json" \
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

# Check if .env needs updating
ENV_FILE="$REPO_ROOT/.env"
CACHE_DIR="./var/model-cache/models"
if [ -f "$ENV_FILE" ]; then
    if grep -q "AOS_MODEL_CACHE_DIR=$CACHE_DIR" "$ENV_FILE" && grep -q "AOS_BASE_MODEL_ID=$MODEL_NAME" "$ENV_FILE"; then
        echo -e "${GREEN}✓ .env already configured correctly${NC}"
    else
        echo -e "${YELLOW}→ Updating .env with model configuration${NC}"

        # Update or add AOS_MODEL_CACHE_DIR (canonical)
        if grep -q "^AOS_MODEL_CACHE_DIR=" "$ENV_FILE"; then
            sed -i.bak "s|^AOS_MODEL_CACHE_DIR=.*|AOS_MODEL_CACHE_DIR=$CACHE_DIR|" "$ENV_FILE"
            rm -f "$ENV_FILE.bak"
        else
            echo "" >> "$ENV_FILE"
            echo "# Model configuration (added by download-model.sh)" >> "$ENV_FILE"
            echo "AOS_MODEL_CACHE_DIR=$CACHE_DIR" >> "$ENV_FILE"
        fi

        # Update or add AOS_BASE_MODEL_ID (canonical)
        if grep -q "^AOS_BASE_MODEL_ID=" "$ENV_FILE"; then
            sed -i.bak "s|^AOS_BASE_MODEL_ID=.*|AOS_BASE_MODEL_ID=$MODEL_NAME|" "$ENV_FILE"
            rm -f "$ENV_FILE.bak"
        else
            echo "AOS_BASE_MODEL_ID=$MODEL_NAME" >> "$ENV_FILE"
        fi

        # Update or add AOS_MODEL_PATH (legacy alias for backwards compatibility)
        if grep -q "^AOS_MODEL_PATH=" "$ENV_FILE"; then
            sed -i.bak "s|^AOS_MODEL_PATH=.*|AOS_MODEL_PATH=$CACHE_DIR/$MODEL_NAME|" "$ENV_FILE"
            rm -f "$ENV_FILE.bak"
        else
            echo "AOS_MODEL_PATH=$CACHE_DIR/$MODEL_NAME" >> "$ENV_FILE"
        fi

        echo -e "${GREEN}✓ .env updated${NC}"
    fi
else
    echo -e "${YELLOW}⚠ No .env file found${NC}"
    echo -e "${BLUE}→ Create .env with:${NC}"
    echo "    AOS_MODEL_CACHE_DIR=$CACHE_DIR"
    echo "    AOS_BASE_MODEL_ID=$MODEL_NAME"
fi

echo ""
echo -e "${GREEN}Next steps:${NC}"
echo -e "${BLUE}  1.${NC} Build the project: ${YELLOW}cargo build --release${NC}"
echo -e "${BLUE}  2.${NC} Start the server:  ${YELLOW}cargo run --release -p adapteros-server-api${NC}"
echo -e "${BLUE}  3.${NC} Run inference:     ${YELLOW}curl -X POST http://localhost:8080/v1/infer \\${NC}"
echo -e "                         ${YELLOW}-H \"Content-Type: application/json\" \\${NC}"
echo -e "                         ${YELLOW}-d '{\"prompt\": \"Hello\", \"max_tokens\": 50}'${NC}"
echo ""
