#!/bin/bash
# Test if the CLI list_adapters implementation compiles

echo "Testing CLI list_adapters implementation..."

# Just check the syntax of list_adapters.rs
cargo check --message-format=short -p adapteros-cli 2>&1 | grep -i "list_adapters" || echo "No errors found in list_adapters.rs"

echo ""
echo "Checking for compilation errors in CLI..."
cargo check -p adapteros-cli 2>&1 | grep -E "^error" | head -10

if [ $? -eq 0 ]; then
    echo "Found errors in CLI compilation"
    exit 1
else
    echo "No errors found specifically in CLI code (other crates may have errors)"
    exit 0
fi
