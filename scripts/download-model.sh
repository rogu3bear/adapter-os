#!/usr/bin/env bash
# Download a configured base model for adapterOS
#
# This script downloads the configured base model from Hugging Face
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

source "$REPO_ROOT/scripts/lib/model-config.sh"

# Resolve model id from env/.env so local lane changes don't drift from tooling defaults.
ENV_MODEL_NAME=""
ENV_CACHE_DIR=""
if [ -f "$REPO_ROOT/.env" ]; then
    ENV_MODEL_NAME="$(grep -E '^AOS_BASE_MODEL_ID=' "$REPO_ROOT/.env" | tail -1 | cut -d'=' -f2- || true)"
    ENV_CACHE_DIR="$(grep -E '^AOS_MODEL_CACHE_DIR=' "$REPO_ROOT/.env" | tail -1 | cut -d'=' -f2- || true)"
fi
MODEL_NAME="${AOS_BASE_MODEL_ID:-${ENV_MODEL_NAME:-Qwen3.5-27B}}"
CACHE_DIR="${AOS_MODEL_CACHE_DIR:-${ENV_CACHE_DIR:-var/models}}"
MODEL_DIR="$(aos_expand_path "$CACHE_DIR" "$REPO_ROOT")"
MODEL_PATH="${AOS_MODEL_PATH:-$MODEL_DIR/$MODEL_NAME}"
MODEL_PATH="$(aos_expand_path "$MODEL_PATH" "$REPO_ROOT")"
if [ -n "${AOS_MODEL_PATH:-}" ] && [ -z "${AOS_BASE_MODEL_ID:-}" ]; then
    MODEL_NAME="$(basename "$MODEL_PATH")"
fi

# Hugging Face model ID (override with AOS_HF_MODEL_ID when needed).
if [[ "$MODEL_NAME" == *"-MLX-"* || "$MODEL_NAME" == *"-mlx-"* ]]; then
    DEFAULT_HF_MODEL_ID="mlx-community/${MODEL_NAME}"
else
    DEFAULT_HF_MODEL_ID="Qwen/${MODEL_NAME}"
fi
HF_MODEL_ID="${AOS_HF_MODEL_ID:-$DEFAULT_HF_MODEL_ID}"

echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}  adapterOS Model Downloader${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo ""

# Check if Hugging Face CLI is installed (newer releases expose `hf`)
HF_CLI_CMD=""
if command -v huggingface-cli &> /dev/null; then
    HF_CLI_CMD="huggingface-cli"
elif command -v hf &> /dev/null; then
    HF_CLI_CMD="hf"
else
    echo -e "${RED}✗ Error: Hugging Face CLI not found (expected 'huggingface-cli' or 'hf')${NC}"
    echo ""
    echo "Please install the Hugging Face CLI:"
    echo "  pip install huggingface-hub[cli]"
    echo ""
    echo "Or using pipx (recommended):"
    echo "  pipx install huggingface-hub[cli]"
    exit 1
fi

echo -e "${GREEN}✓ Found Hugging Face CLI: $HF_CLI_CMD${NC}"

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
        REQUIRED_FILES=("config.json" "tokenizer.json" "tokenizer_config.json")
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

        # Verify model has at least one weight file
        WEIGHT_FILES=$(find "$MODEL_PATH" -type f \( -name "*.safetensors" -o -name "*.bin" \) 2>/dev/null | wc -l)
        if [ "$WEIGHT_FILES" -eq 0 ]; then
            echo -e "${RED}✗ Missing model weights (*.safetensors or *.bin)${NC}"
            echo -e "${YELLOW}→ Re-run and choose re-download (y) to fetch full model weights${NC}"
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
echo -e "${YELLOW}Note: This will download a large base model (multi-GB)${NC}"
echo -e "${YELLOW}      Download time depends on your internet connection${NC}"
echo ""

# Download the model
echo -e "${GREEN}→ Starting download...${NC}"
echo ""

"$HF_CLI_CMD" download "$HF_MODEL_ID" \
    --include "*.safetensors" \
    --include "*.json" \
    --local-dir "$MODEL_PATH"

# Verify download
echo ""
echo -e "${BLUE}→ Verifying download...${NC}"

REQUIRED_FILES=("config.json" "tokenizer.json" "tokenizer_config.json")
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
WEIGHT_FILES=$(find "$MODEL_PATH" -type f \( -name "*.safetensors" -o -name "*.bin" \) 2>/dev/null | wc -l)
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

# Determine manifest for this model lane (if available in repo).
MODEL_MANIFEST_PATH="$(aos_guess_manifest_path "$REPO_ROOT" "$MODEL_NAME" "$MODEL_PATH" || true)"
MODEL_MANIFEST_ENV=""
if [ -n "$MODEL_MANIFEST_PATH" ]; then
    MODEL_MANIFEST_ENV="${MODEL_MANIFEST_PATH#"$REPO_ROOT"/}"
    if [[ "$MODEL_MANIFEST_ENV" != /* ]] && [[ "$MODEL_MANIFEST_ENV" != ./* ]]; then
        MODEL_MANIFEST_ENV="./$MODEL_MANIFEST_ENV"
    fi
fi

# Check if .env needs updating
ENV_FILE="$REPO_ROOT/.env"
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

        local_model_path="$CACHE_DIR/$MODEL_NAME"
        if [[ "$local_model_path" != /* ]] && [[ "$local_model_path" != ./* ]]; then
            local_model_path="./$local_model_path"
        fi
        if [ -n "${AOS_MODEL_PATH:-}" ]; then
            local_model_path="$AOS_MODEL_PATH"
        fi
        # Update or add AOS_MODEL_PATH (legacy alias for backwards compatibility)
        if grep -q "^AOS_MODEL_PATH=" "$ENV_FILE"; then
            sed -i.bak "s|^AOS_MODEL_PATH=.*|AOS_MODEL_PATH=$local_model_path|" "$ENV_FILE"
            rm -f "$ENV_FILE.bak"
        else
            echo "AOS_MODEL_PATH=$local_model_path" >> "$ENV_FILE"
        fi

        if [ -n "$MODEL_MANIFEST_ENV" ]; then
            if grep -q "^AOS_MANIFEST_PATH=" "$ENV_FILE"; then
                sed -i.bak "s|^AOS_MANIFEST_PATH=.*|AOS_MANIFEST_PATH=$MODEL_MANIFEST_ENV|" "$ENV_FILE"
                rm -f "$ENV_FILE.bak"
            else
                echo "AOS_MANIFEST_PATH=$MODEL_MANIFEST_ENV" >> "$ENV_FILE"
            fi

            if grep -q "^AOS_WORKER_MANIFEST=" "$ENV_FILE"; then
                sed -i.bak "s|^AOS_WORKER_MANIFEST=.*|AOS_WORKER_MANIFEST=$MODEL_MANIFEST_ENV|" "$ENV_FILE"
                rm -f "$ENV_FILE.bak"
            else
                echo "AOS_WORKER_MANIFEST=$MODEL_MANIFEST_ENV" >> "$ENV_FILE"
            fi
        fi

        echo -e "${GREEN}✓ .env updated${NC}"
    fi
else
    echo -e "${YELLOW}⚠ No .env file found${NC}"
    echo -e "${BLUE}→ Create .env with:${NC}"
    echo "    AOS_MODEL_CACHE_DIR=$CACHE_DIR"
    echo "    AOS_BASE_MODEL_ID=$MODEL_NAME"
    if [ -n "$MODEL_MANIFEST_ENV" ]; then
        echo "    AOS_MANIFEST_PATH=$MODEL_MANIFEST_ENV"
        echo "    AOS_WORKER_MANIFEST=$MODEL_MANIFEST_ENV"
    fi
fi

echo ""
echo -e "${GREEN}Next steps:${NC}"
echo -e "${BLUE}  1.${NC} Build the project: ${YELLOW}cargo build --release${NC}"
echo -e "${BLUE}  2.${NC} Start the server:  ${YELLOW}cargo run --release -p adapteros-server-api${NC}"
echo -e "${BLUE}  3.${NC} Run inference:     ${YELLOW}curl -X POST http://localhost:18080/v1/infer \\${NC}"
echo -e "                         ${YELLOW}-H \"Content-Type: application/json\" \\${NC}"
echo -e "                         ${YELLOW}-d '{\"prompt\": \"Hello\", \"max_tokens\": 50}'${NC}"
echo ""
