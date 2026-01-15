#!/usr/bin/env bash
# generate_route_map.sh - Generate ROUTE_MAP.md from routes.rs
#
# Usage: ./scripts/dev/generate_route_map.sh
#
# This script parses routes.rs and generates a deterministic route map document.
# Output is sorted by path for stable diffs.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
ROUTES_DIR="$REPO_ROOT/crates/adapteros-server-api/src"
OUTPUT_FILE="$REPO_ROOT/docs/api/ROUTE_MAP.md"

# Route files to scan (main routes.rs + submodules)
ROUTE_FILES=(
    "$ROUTES_DIR/routes.rs"
    "$ROUTES_DIR/routes/adapters.rs"
)

# Ensure routes.rs exists
if [[ ! -f "${ROUTE_FILES[0]}" ]]; then
    echo "Error: routes.rs not found at ${ROUTE_FILES[0]}" >&2
    exit 1
fi

# Count routes across all files
ROUTE_COUNT=0
for f in "${ROUTE_FILES[@]}"; do
    if [[ -f "$f" ]]; then
        count=$(grep -c '\.route(' "$f" 2>/dev/null || echo "0")
        ROUTE_COUNT=$((ROUTE_COUNT + count))
    fi
done

# Extract routes using perl with improved regex
# This handles both single-line and multi-line route definitions
extract_routes() {
    for route_file in "${ROUTE_FILES[@]}"; do
        [[ -f "$route_file" ]] || continue
        perl -0777 -ne '
        # Remove comments
        s|//[^\n]*||g;

        # Slurp the entire file and find route blocks
        # First, normalize whitespace in .route() calls
        my $content = $_;

        # Match both forms:
        # .route("/path", get(handler))
        # .route(\n    "/path",\n    get(handler)\n)
        while ($content =~ /\.route\s*\(\s*"([^"]+)"\s*,\s*((?:[^()]+|\((?:[^()]+|\([^()]*\))*\))+)\s*\)/gs) {
            my $path = $1;
            my $handlers_block = $2;

            # Extract handlers from the block
            # Handles: get(handler), post(handler), get(handler).post(handler2)
            while ($handlers_block =~ /\b(get|post|put|delete|patch)\s*\(\s*([^()]+(?:\([^()]*\))?)\s*\)/gi) {
                my $method = uc($1);
                my $handler = $2;

                # Clean up handler
                $handler =~ s/^\s+|\s+$//g;
                $handler =~ s/\s+/ /g;

                # Skip empty or closure handlers (|| async { ... })
                next if $handler eq "" || $handler =~ /^\|/;

                print "$method|$path|$handler\n";
            }
        }
    ' "$route_file"
    done
}

# Map handler path to test file (best effort)
find_test_file() {
    local handler="$1"

    # Extract module name from handler path
    local module
    module=$(echo "$handler" | sed -E 's/.*handlers::([^:]+).*/\1/' | sed 's/::.*//' | tr -d ' ')

    # Check common test locations
    local test_candidates=(
        "$REPO_ROOT/crates/adapteros-server-api/tests/${module}_test.rs"
        "$REPO_ROOT/crates/adapteros-server-api/tests/${module}.rs"
        "$REPO_ROOT/crates/adapteros-e2e/tests/${module}_e2e.rs"
        "$REPO_ROOT/crates/adapteros-e2e/tests/e2e_${module}.rs"
        "$REPO_ROOT/tests/e2e/${module}_test.rs"
    )

    for candidate in "${test_candidates[@]}"; do
        if [[ -f "$candidate" ]]; then
            echo "${candidate#$REPO_ROOT/}"
            return
        fi
    done

    echo "UNKNOWN"
}

# Generate the markdown document
generate_doc() {
    local timestamp
    timestamp=$(date -u +"%Y-%m-%d %H:%M UTC")
    local extracted_count
    extracted_count=$(extract_routes | wc -l | tr -d ' ')

    cat << EOF
# adapterOS API Route Map

> **Auto-generated:** Do not edit manually.
> Run \`./scripts/dev/generate_route_map.sh\` to regenerate.
>
> Generated: $timestamp

## Overview

| Metric | Count |
|--------|-------|
| **Total Route Registrations** | $ROUTE_COUNT |
| **Extracted Handler Mappings** | $extracted_count |

## Route Table

| Method | Path | Handler | Test File |
|--------|------|---------|-----------|
EOF

    # Parse and sort routes deterministically
    extract_routes | LC_ALL=C sort -t'|' -k2,2 -k1,1 | while IFS='|' read -r method path handler; do
        # Skip empty lines
        [[ -z "$method" || -z "$path" ]] && continue

        # Find test file
        test_file=$(find_test_file "$handler")

        # Output table row
        echo "| \`$method\` | \`$path\` | \`$handler\` | \`$test_file\` |"
    done

    cat << 'EOF'

## Route Categories

### Health & System
- `/healthz`, `/readyz` - Liveness and readiness probes
- `/v1/status`, `/v1/system/*` - System status and configuration

### Authentication
- `/v1/auth/*` - Login, logout, MFA, sessions

### Adapters
- `/v1/adapters/*` - Adapter CRUD, lifecycle, versions
- `/v1/adapter-repositories/*` - Adapter repository management
- `/v1/adapter-stacks/*` - Stack composition

### Training
- `/v1/training/*` - Training jobs, datasets, templates

### Inference
- `/v1/infer/*` - Synchronous and streaming inference
- `/v1/chat/*` - Chat sessions and messages

### Data
- `/v1/datasets/*` - Dataset management and uploads
- `/v1/documents/*` - Document management
- `/v1/collections/*` - Collection management

### Diagnostics
- `/v1/diag/*` - Diagnostic runs and bundles
- `/v1/traces/*` - Trace search and retrieval

### Admin
- `/v1/tenants/*` - Tenant management
- `/v1/workspaces/*` - Workspace management
- `/v1/policies/*` - Policy management

## Maintenance

### Regenerating This Document

```bash
./scripts/dev/generate_route_map.sh
```

### CI Freshness Check

This document is verified by CI. If routes.rs changes without updating
this document, CI will fail with instructions to regenerate.

---

*See [docs/engineering/HANDLER_HYGIENE.md](../engineering/HANDLER_HYGIENE.md) for handler size audit.*
EOF
}

# Main
echo "Generating route map from ${#ROUTE_FILES[@]} route files..." >&2
generate_doc > "$OUTPUT_FILE"
echo "Route map written to $OUTPUT_FILE" >&2
echo "Found approximately $ROUTE_COUNT route registrations" >&2
