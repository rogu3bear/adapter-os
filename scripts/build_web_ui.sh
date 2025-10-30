#!/bin/bash
set -e

echo "Building AdapterOS Web UI..."

# Check if pnpm is installed
if ! command -v pnpm &> /dev/null; then
    echo "Error: pnpm is not installed"
    echo "Install with: npm install -g pnpm"
    exit 1
fi

# Navigate to ui directory
cd "$(dirname "$0")/../ui"

# Install dependencies if needed
if [ ! -d "node_modules" ]; then
    echo "Installing dependencies..."
    pnpm install
fi

# Build in production mode
echo "Building React application..."
pnpm build

echo "Web UI built successfully!"
echo "Output: crates/adapteros-server/static/"
