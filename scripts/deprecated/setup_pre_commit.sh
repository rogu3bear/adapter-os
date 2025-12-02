#!/bin/bash
# Setup script for pre-commit hook installation

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOOK_SOURCE="$SCRIPT_DIR/pre-commit-template"
HOOK_TARGET="$REPO_ROOT/.git/hooks/pre-commit"

echo "Installing pre-commit hook..."

# Check if .git/hooks directory exists
if [ ! -d "$REPO_ROOT/.git/hooks" ]; then
    echo "Error: .git/hooks directory not found. Are you in a git repository?"
    exit 1
fi

# Check if hook template exists
if [ ! -f "$HOOK_SOURCE" ]; then
    echo "Error: Pre-commit template not found at $HOOK_SOURCE"
    exit 1
fi

# Copy template to hooks directory
cp "$HOOK_SOURCE" "$HOOK_TARGET"

# Make hook executable
chmod +x "$HOOK_TARGET"

# Verify installation
if [ -f "$HOOK_TARGET" ] && [ -x "$HOOK_TARGET" ]; then
    echo "✅ Pre-commit hook installed successfully"
    echo ""
    echo "The hook will run automatically on every commit."
    echo ""
    echo "To bypass the hook (not recommended), use:"
    echo "  git commit --no-verify"
    echo ""
    echo "To uninstall, run:"
    echo "  rm .git/hooks/pre-commit"
else
    echo "❌ Failed to install pre-commit hook"
    exit 1
fi

