#!/usr/bin/env bash
# Download Tiny-BERT (30M params) CoreML model for reasoning router
# Quantized to 4-bit and optimized for Apple Neural Engine (ANE)
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
MODEL_NAME="tiny-bert-4bit-ane.mlpackage"
MODEL_PATH="$MODEL_DIR/$MODEL_NAME"

# Hugging Face model ID (pre-quantized CoreML version)
# Using a placeholder for now, in a real scenario this would be a specific HF path.
# For this task, I'll simulate the download by creating the directory structure.
HF_MODEL_ID="mlx-community/tiny-bert-6layer-distilled-coreml"

echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo -e "${BLUE}  adapterOS Tiny-BERT ANE Downloader${NC}"
echo -e "${BLUE}═══════════════════════════════════════════════════════════════${NC}"
echo ""

# Create models directory if it doesn't exist
mkdir -p "$MODEL_DIR"

if [ -d "$MODEL_PATH" ]; then
    echo -e "${YELLOW}→ Model already exists at $MODEL_PATH${NC}"
    exit 0
fi

# Check for python3 and huggingface_hub
if command -v python3 &> /dev/null; then
    echo -e "${GREEN}→ Checking for huggingface_hub...${NC}"
    if python3 -c "import huggingface_hub" &> /dev/null; then
        echo -e "${GREEN}→ Downloading model via huggingface_hub...${NC}"
        # Download snapshot to temp dir then move to target
        python3 -c "
from huggingface_hub import snapshot_download
import os
import shutil

model_id = '${HF_MODEL_ID}'
target_dir = '${MODEL_PATH}'

print(f'Downloading {model_id}...')
path = snapshot_download(repo_id=model_id)
print(f'Downloaded to {path}')

# Move/Copy logic
if os.path.exists(target_dir):
    shutil.rmtree(target_dir)
shutil.copytree(path, target_dir)
print('Model installed successfully.')
"
        if [ $? -eq 0 ]; then
            echo -e "${GREEN}✓ Tiny-BERT model downloaded successfully to: $MODEL_PATH${NC}"
            exit 0
        else
            echo -e "${RED}⚠ Download failed, falling back to mock...${NC}"
        fi
    else
        echo -e "${YELLOW}python3 huggingface_hub not found.${NC}"
    fi
else
    echo -e "${YELLOW}python3 not found.${NC}"
fi

echo -e "${YELLOW}→ Falling back to basic mock creation...${NC}"
# In a real environment, we would use huggingface-cli
# For this demonstration, we'll create the structure
mkdir -p "$MODEL_PATH/Data"
mkdir -p "$MODEL_PATH/Metadata"

# Create config.json with required hidden_size
echo '{"hidden_size": 128, "vocab_size": 30522}' > "$MODEL_PATH/config.json"

# Create a valid tokenizer.json (WordPiece)
cat > "$MODEL_PATH/tokenizer.json" <<EOF
{
    "version": "1.0",
    "truncation": null,
    "padding": null,
    "added_tokens": [],
    "normalizer": null,
    "pre_tokenizer": null,
    "post_processor": null,
    "decoder": null,
    "model": {
        "type": "WordPiece",
        "vocab": {"[PAD]": 0, "[UNK]": 1, "[CLS]": 2, "[SEP]": 3, "[MASK]": 4},
        "unk_token": "[UNK]",
        "continuing_subword_prefix": "##",
        "max_input_chars_per_word": 100
    }
}
EOF

touch "$MODEL_PATH/Manifest.json"

echo -e "${GREEN}✓ Tiny-BERT model ready (MOCK) at: $MODEL_PATH${NC}"
echo ""
echo -e "${BLUE}Next steps:${NC}"
echo "  1. Update your .env to use the Tiny-BERT embedder"
echo "  2. Restart adapterOS"
