#!/bin/bash
# Setup git hooks for AdapterOS development
# This ensures consistent code quality across the team

set -e

echo "🔧 Setting up git hooks for AdapterOS..."

# Configure git to use our custom hooks directory
git config core.hooksPath .githooks

# Test that the hook works
echo "🧪 Testing pre-commit hook..."
if .githooks/pre-commit; then
    echo "✅ Git hooks configured successfully!"
    echo ""
    echo "💡 Tips:"
    echo "  - The pre-commit hook runs on every commit"
    echo "  - To skip hooks: git commit --no-verify"
    echo "  - To test hooks manually: .githooks/pre-commit"
else
    echo "❌ Hook setup failed!"
    exit 1
fi




