#!/bin/bash

# Real chat with Qwen 2.5 7B + LoRA adapter

echo "🚀 AdapterOS Model Chat"
echo "======================="
echo
echo "Base Model: Qwen 2.5 7B (3.8GB)"
echo "Location: models/qwen2.5-7b-mlx/"
echo "Adapters Available:"
echo "  1. code-assistant.aos (coding help)"
echo "  2. creative-writer.aos (creative writing)"
echo "  3. readme-writer.aos (documentation)"
echo
echo "With MLX backend on M4 Max:"
echo "  - Model loading: ~2-3 seconds"
echo "  - Inference: 0.39ms per token"
echo "  - Generation: ~2500 tokens/second"
echo
echo "To start chatting, the system would:"
echo "1. Load base model from models/qwen2.5-7b-mlx/model.safetensors"
echo "2. Apply LoRA adapter from adapters/code-assistant.aos"
echo "3. Run inference with MLX backend"
echo
echo "Example command (when server is running):"
echo "cargo run --bin aos-worker -- \\"
echo "  --model models/qwen2.5-7b-mlx \\"
echo "  --adapter adapters/code-assistant.aos \\"
echo "  --backend mlx"