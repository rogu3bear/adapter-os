#!/bin/bash
# Download models for AdapterOS
#
# NOTE: AdapterOS requires models in MLX format for optimal adapter performance
# MLX format provides better memory layout for K-sparse LoRA routing on Apple Silicon
#
# This script downloads TinyLlama-1.1B-Chat in MLX format for testing
# For production, use Qwen2.5-7B-Instruct from mlx-community

set -e

MODEL_DIR="models/tinyllama-1.1b-mlx"
MODEL_URL="https://huggingface.co/mlx-community/TinyLlama-1.1B-Chat-v1.0/resolve/main/model.safetensors"
TOKENIZER_URL="https://huggingface.co/mlx-community/TinyLlama-1.1B-Chat-v1.0/resolve/main/tokenizer.json"

echo "Creating model directory..."
mkdir -p "$MODEL_DIR"

echo "Downloading TinyLlama-1.1B model (MLX format)..."
if command -v curl > /dev/null; then
    curl -L "$MODEL_URL" -o "$MODEL_DIR/model.safetensors"
    curl -L "$TOKENIZER_URL" -o "$MODEL_DIR/tokenizer.json"
elif command -v wget > /dev/null; then
    wget "$MODEL_URL" -O "$MODEL_DIR/model.safetensors"
    wget "$TOKENIZER_URL" -O "$MODEL_DIR/tokenizer.json"
else
    echo "Error: Neither curl nor wget found. Please install one of them."
    exit 1
fi

echo "Verifying download..."
if [ -f "$MODEL_DIR/model.safetensors" ] && [ -f "$MODEL_DIR/tokenizer.json" ]; then
    MODEL_SIZE=$(stat -f%z "$MODEL_DIR/model.safetensors" 2>/dev/null || stat -c%s "$MODEL_DIR/model.safetensors")
    echo "✓ Model downloaded: $(numfmt --to=iec $MODEL_SIZE 2>/dev/null || echo $MODEL_SIZE bytes)"
    
    # Compute BLAKE3 hash if b3sum is available
    if command -v b3sum > /dev/null; then
        echo "Computing BLAKE3 hash..."
        b3sum "$MODEL_DIR/model.safetensors" | tee "$MODEL_DIR/model.hash"
    fi
    
    echo "✓ Download complete!"
    echo ""
    echo "Model location: $MODEL_DIR/model.safetensors"
    echo "Tokenizer location: $MODEL_DIR/tokenizer.json"
    echo ""
    echo "NOTE: For production use with Qwen2.5-7B-Instruct (MLX format):"
    echo "  huggingface-cli download mlx-community/Qwen2.5-7B-Instruct \\"
    echo "    --include 'config.json,tokenizer.json,tokenizer_config.json,model.safetensors' \\"
    echo "    --local-dir models/qwen2.5-7b-mlx"
else
    echo "✗ Download failed"
    exit 1
fi
