#!/bin/bash
# Test script for MLX bridge server

set -e

MODEL_PATH="${1:-./var/models/Qwen3-Coder-30B-A3B-Instruct-MLX-4bit}"

if [ ! -d "$MODEL_PATH" ]; then
    echo "Error: Model path does not exist: $MODEL_PATH"
    exit 1
fi

echo "Testing MLX bridge with model: $MODEL_PATH"

# Export model path
export MLX_MODEL_PATH="$MODEL_PATH"

# Start the bridge and send test requests via stdin
(
    # Wait for ready message
    sleep 3

    # Send a health check
    echo '{"type": "health_check"}'
    sleep 1

    # Send a generation request
    echo '{"type": "generate", "prompt": "def hello():", "max_tokens": 10, "temperature": 0.7, "top_p": 0.9, "stream": false}'
    sleep 5

    # Send shutdown
    echo '{"type": "shutdown"}'
) | python3 scripts/mlx_bridge_server.py

echo ""
echo "Test completed"
