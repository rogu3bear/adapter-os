#!/bin/bash
# AdapterOS Infrastructure Health Check
# Prevents the issues that caused the recent rectification

set -e

echo "🔍 AdapterOS Infrastructure Health Check"
echo "========================================"

# Get list of excluded crates
excluded_crates=$(grep -A 50 "exclude = \[" Cargo.toml | grep -E '^\s*"crates/' | sed 's/.*"crates\/\([^"]*\)".*/\1/' | tr '\n' ' ')

# 1. Check tokio configuration across all crates
echo "📦 Checking tokio configurations..."
for cargo_toml in crates/*/Cargo.toml; do
    crate_name=$(basename "$(dirname "$cargo_toml")")

    # Skip excluded crates
    if echo "$excluded_crates" | grep -q "$crate_name"; then
        echo "⏭️  $crate_name: Skipped (excluded from workspace)"
        continue
    fi

    # Check if crate has tokio tests
    if grep -r "#\[tokio::test\]" "crates/$crate_name/src/" >/dev/null 2>&1; then
        # Crate has tokio tests - check configuration
        if grep -q "workspace.*true" "$cargo_toml" && grep -q "tokio" "$cargo_toml"; then
            # Uses workspace tokio - should be fine
            echo "✅ $crate_name: Uses workspace tokio (inherits proper configuration)"
        elif grep -q "tokio.*=" "$cargo_toml"; then
            # Explicit tokio dependency
            if ! grep -q "macros" "$cargo_toml"; then
                echo "❌ $crate_name: Missing tokio macros feature (will break #[tokio::test])"
                exit 1
            fi
            if ! grep -q "rt-multi-thread" "$cargo_toml"; then
                echo "⚠️  $crate_name: Missing tokio rt-multi-thread feature (recommended for tests)"
                # Not a fatal error, just a warning
            fi
            echo "✅ $crate_name: Tokio explicitly configured"
        else
            echo "❌ $crate_name: Has tokio tests but no tokio dependency"
            exit 1
        fi
    fi
done

# 2. Check for missing dependencies in workspace
echo ""
echo "🔗 Checking workspace dependencies..."
for cargo_toml in crates/*/Cargo.toml; do
    crate_name=$(basename "$(dirname "$cargo_toml")")

    # Check for adapteros-types dependency if importing from it
    if grep -q "adapteros_types::" "crates/$crate_name/src/" 2>/dev/null; then
        if ! grep -q "adapteros-types" "$cargo_toml" && ! grep -q "adapteros_types" "$cargo_toml"; then
            echo "❌ $crate_name: Imports from adapteros_types but missing dependency"
            exit 1
        fi
    fi

    echo "✅ $crate_name: Dependencies verified"
done

# 3. Check workspace member consistency
echo ""
echo "📋 Checking workspace member consistency..."
workspace_members=$(grep -A 100 "members = \[" Cargo.toml | grep -E '^\s*"crates/' | sed 's/.*"crates\/\([^"]*\)".*/\1/' | sort)
existing_crates=$(ls crates/ | sort)

missing_from_workspace=""
extra_in_workspace=""

while IFS= read -r crate; do
    if ! echo "$workspace_members" | grep -q "^$crate$"; then
        missing_from_workspace="$missing_from_workspace $crate"
    fi
done <<< "$existing_crates"

while IFS= read -r member; do
    if ! echo "$existing_crates" | grep -q "^$member$"; then
        extra_in_workspace="$extra_in_workspace $member"
    fi
done <<< "$workspace_members"

if [ -n "$missing_from_workspace" ]; then
    echo "❌ Missing from workspace members:$missing_from_workspace"
    exit 1
fi

if [ -n "$extra_in_workspace" ]; then
    echo "❌ Extra in workspace members (crates don't exist):$extra_in_workspace"
    exit 1
fi

echo "✅ Workspace members consistent"

# 4. Test compilation across workspace
echo ""
echo "🔨 Testing compilation across workspace..."
if ! cargo check --workspace --quiet; then
    echo "❌ Workspace compilation failed"
    exit 1
fi
echo "✅ Workspace compiles successfully"

# 5. Test tokio functionality
echo ""
echo "🧪 Testing tokio test infrastructure..."
# Run a quick test on adapteros-core to verify tokio works
if ! cargo test --package adapteros-core --lib --quiet 2>/dev/null | grep -q "test result: ok"; then
    echo "❌ Tokio test infrastructure broken"
    exit 1
fi
echo "✅ Tokio test infrastructure functional"

echo ""
echo "🎉 All infrastructure checks passed!"
echo "🚫 Issues like the recent rectification should be caught by this script"
