#!/bin/bash
set -euo pipefail

echo "🔨 Building AdapterOS Web UI..."

# Check if trunk is installed
if ! command -v trunk &> /dev/null; then
    echo "❌ Error: trunk is not installed"
    echo "Install with: cargo install trunk"
    exit 1
fi

# Navigate to UI crate
cd "$(dirname "$0")/../crates/aos-ui-web"

# Clean previous build
if [ -d "target/site" ]; then
    echo "🧹 Cleaning previous build..."
    rm -rf target/site
fi

# Build for release
echo "📦 Building UI (release mode)..."
trunk build --release

echo "✅ UI build complete!"
echo "📂 Output: crates/aos-ui-web/target/site"

