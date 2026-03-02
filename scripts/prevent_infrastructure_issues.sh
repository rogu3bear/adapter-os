#!/bin/bash
# adapterOS Infrastructure Health Check
# Prevents the issues that caused the recent rectification

set -e

echo "🔍 adapterOS Infrastructure Health Check"
echo "========================================"

# Derive workspace members/excludes from Cargo.toml via tomllib to avoid
# brittle grep parsing that can drift with formatting/comments.
workspace_members_raw=$(
python3 - <<'PY'
import tomllib
from pathlib import Path

data = tomllib.loads(Path("Cargo.toml").read_text())
for path in data.get("workspace", {}).get("members", []):
    print(path)
PY
)

workspace_excludes_raw=$(
python3 - <<'PY'
import tomllib
from pathlib import Path

data = tomllib.loads(Path("Cargo.toml").read_text())
for path in data.get("workspace", {}).get("exclude", []):
    print(path)
PY
)

# Normalize crate names from "crates/<name>/..." entries.
workspace_member_crates=$(
echo "$workspace_members_raw" | awk -F/ '$1=="crates" && NF>=2 {print $2}' | sort -u
)
excluded_crates=$(
echo "$workspace_excludes_raw" | awk -F/ '$1=="crates" && NF>=2 {print $2}' | sort -u | tr '\n' ' '
)

is_excluded_crate() {
    local crate_name="$1"
    # Match whole crate names only.
    if echo " $excluded_crates " | grep -q " $crate_name "; then
        return 0
    fi
    return 1
}

# 1. Check tokio configuration across all crates
echo "📦 Checking tokio configurations..."
for cargo_toml in crates/*/Cargo.toml; do
    crate_name=$(basename "$(dirname "$cargo_toml")")

    # Skip excluded crates
    if is_excluded_crate "$crate_name"; then
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
workspace_members="$workspace_member_crates"
existing_crates=$(find crates -mindepth 2 -maxdepth 2 -name Cargo.toml | sed 's#^crates/\([^/]*\)/Cargo.toml$#\1#' | sort -u)

missing_from_workspace=""
extra_in_workspace=""

while IFS= read -r crate; do
    if is_excluded_crate "$crate"; then
        continue
    fi
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
