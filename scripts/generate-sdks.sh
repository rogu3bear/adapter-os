#!/bin/bash
set -euo pipefail

# AdapterOS SDK Generation Script
# Generates TypeScript types and Python SDK from OpenAPI spec

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
CODEGEN_DIR="$PROJECT_ROOT/codegen"
OPENAPI_SPEC="$PROJECT_ROOT/docs/api/openapi.json"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Check if required tools are available
check_typescript_tools() {
    if ! command -v pnpm &> /dev/null; then
        log_error "pnpm not found"
        log_info "Install it with: npm install -g pnpm"
        exit 1
    fi

    # Check if openapi-typescript is available in ui/node_modules
    if [ ! -f "$PROJECT_ROOT/ui/node_modules/.bin/openapi-typescript" ]; then
        log_warn "openapi-typescript not found, installing dependencies..."
        (cd "$PROJECT_ROOT/ui" && pnpm install --frozen-lockfile)
    fi
}

check_python_generator() {
    if command -v openapi-generator-cli &> /dev/null; then
        GENERATOR="openapi-generator-cli"
    elif command -v openapi-generator &> /dev/null; then
        GENERATOR="openapi-generator"
    else
        log_error "openapi-generator-cli not found"
        log_info "Install it with: npm install -g @openapitools/openapi-generator-cli"
        exit 1
    fi
    log_info "Using generator: $GENERATOR"
}

# Generate OpenAPI spec from Rust code
generate_spec() {
    log_info "Generating OpenAPI specification from Rust backend..."

    # Ensure output directory exists
    mkdir -p "$(dirname "$OPENAPI_SPEC")"

    # Run the export binary
    if ! cargo run -p adapteros-server-api --bin export-openapi -- "$OPENAPI_SPEC"; then
        log_error "Failed to generate OpenAPI spec"
        exit 1
    fi

    if [ ! -f "$OPENAPI_SPEC" ]; then
        log_error "OpenAPI spec was not created at $OPENAPI_SPEC"
        exit 1
    fi

    # Get spec stats
    local paths_count=$(jq '.paths | length' "$OPENAPI_SPEC" 2>/dev/null || echo "?")
    local schemas_count=$(jq '.components.schemas | length' "$OPENAPI_SPEC" 2>/dev/null || echo "?")

    log_info "OpenAPI spec generated at $OPENAPI_SPEC"
    log_info "  Paths: $paths_count"
    log_info "  Schemas: $schemas_count"
}

# Generate TypeScript types using openapi-typescript
generate_typescript() {
    log_info "Generating TypeScript types from OpenAPI spec..."

    check_typescript_tools

    local output_file="$PROJECT_ROOT/ui/src/api/generated.ts"

    # Generate types using openapi-typescript with same options as package.json
    (cd "$PROJECT_ROOT/ui" && pnpm exec openapi-typescript \
        "$OPENAPI_SPEC" \
        --output src/api/generated.ts \
        --export-type \
        --enum \
        --alphabetize \
        --empty-objects-unknown \
        --default-non-nullable=false) || {
        log_error "TypeScript type generation failed"
        return 1
    }

    log_info "TypeScript types generated at $output_file"

    # Verify the generated file
    if [ -f "$output_file" ]; then
        local file_size=$(wc -c < "$output_file")
        log_info "  Generated file size: $file_size bytes"
    else
        log_error "Generated types file not found at $output_file"
        return 1
    fi
}

# Generate Python SDK using openapi-generator
generate_python() {
    log_info "Generating Python SDK..."

    check_python_generator

    local output_dir="$PROJECT_ROOT/sdk/python"
    mkdir -p "$output_dir"

    $GENERATOR generate \
        -i "$OPENAPI_SPEC" \
        -g python \
        -o "$output_dir" \
        --package-name adapteros_client \
        --additional-properties=packageVersion=0.1.0,projectName=adapteros-client \
        --skip-validate-spec \
        || { log_error "Python SDK generation failed"; return 1; }

    log_info "Python SDK generated at $output_dir"

    # Create a basic pyproject.toml if it doesn't exist
    if [ ! -f "$output_dir/pyproject.toml" ]; then
        cat > "$output_dir/pyproject.toml" << 'EOF'
[project]
name = "adapteros-client"
version = "0.1.0"
description = "AdapterOS Python Client SDK"
readme = "README.md"
requires-python = ">=3.8"
dependencies = [
    "urllib3>=1.25.3",
    "python-dateutil>=2.8.0",
    "pydantic>=2.0.0",
]

[project.optional-dependencies]
dev = [
    "pytest>=7.0.0",
    "pytest-asyncio>=0.21.0",
]

[build-system]
requires = ["setuptools>=61.0"]
build-backend = "setuptools.build_meta"
EOF
        log_info "Created pyproject.toml"
    fi
}

# Validate the generated spec
validate_spec() {
    log_info "Validating OpenAPI spec..."

    # Basic validation using jq
    if command -v jq &> /dev/null; then
        if ! jq -e '.paths | length > 0' "$OPENAPI_SPEC" > /dev/null; then
            log_error "OpenAPI spec has no paths defined"
            return 1
        fi
        if ! jq -e '.components.schemas | length > 0' "$OPENAPI_SPEC" > /dev/null; then
            log_error "OpenAPI spec has no schemas defined"
            return 1
        fi
        log_info "✓ Basic spec validation passed"
    fi

    # Optional: swagger-cli validation
    if command -v swagger-cli &> /dev/null; then
        swagger-cli validate "$OPENAPI_SPEC" || log_warn "Spec validation had warnings"
    else
        log_warn "swagger-cli not found, skipping advanced validation"
        log_info "Install with: npm install -g @apidevtools/swagger-cli"
    fi
}

# Check for drift in generated types
check_drift() {
    log_info "Checking for drift in generated types..."

    local temp_file="$PROJECT_ROOT/ui/src/api/generated.check.ts"
    local current_file="$PROJECT_ROOT/ui/src/api/generated.ts"

    # Generate types to temporary file
    (cd "$PROJECT_ROOT/ui" && pnpm exec openapi-typescript \
        "../$OPENAPI_SPEC" \
        --output src/api/generated.check.ts \
        --export-type \
        --enum \
        --alphabetize \
        --empty-objects-unknown \
        --default-non-nullable=false) > /dev/null 2>&1

    # Compare with current version
    if diff -q "$current_file" "$temp_file" > /dev/null 2>&1; then
        log_info "✓ No drift detected - types are in sync"
        rm -f "$temp_file"
        return 0
    else
        log_warn "⚠ Drift detected - generated types differ from committed version"
        log_info "Run: ./scripts/generate-sdks.sh --typescript"
        rm -f "$temp_file"
        return 1
    fi
}

# Print usage
usage() {
    cat << EOF
Usage: $0 [OPTIONS]

AdapterOS SDK Generation Script
Generates TypeScript types and Python SDK from OpenAPI specification.

Options:
  --spec-only      Only generate the OpenAPI spec
  --typescript     Generate TypeScript types only (uses openapi-typescript)
  --python         Generate Python SDK only (uses openapi-generator)
  --all            Generate spec and all SDKs (default)
  --validate       Validate spec after generation
  --check-drift    Check if generated types are in sync (exit 1 if drift)
  -h, --help       Show this help message

Examples:
  $0                           # Generate spec and all SDKs
  $0 --typescript              # Only update TypeScript types
  $0 --spec-only --validate    # Generate and validate spec
  $0 --check-drift             # Check for drift in CI

Notes:
  - TypeScript types are generated using openapi-typescript (faster, type-safe)
  - Python SDK uses openapi-generator (full client implementation)
  - Run from project root or scripts directory
  - Requires: cargo, pnpm (for TS), openapi-generator-cli (for Python)
EOF
}

# Main
main() {
    local generate_spec_flag=true
    local generate_ts=false
    local generate_py=false
    local validate=false
    local check_drift_flag=false

    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case $1 in
            --spec-only)
                generate_ts=false
                generate_py=false
                shift
                ;;
            --typescript)
                generate_ts=true
                shift
                ;;
            --python)
                generate_py=true
                shift
                ;;
            --all)
                generate_ts=true
                generate_py=true
                shift
                ;;
            --validate)
                validate=true
                shift
                ;;
            --check-drift)
                check_drift_flag=true
                generate_spec_flag=true
                shift
                ;;
            -h|--help)
                usage
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                usage
                exit 1
                ;;
        esac
    done

    # Default to all if no specific SDK selected and not just checking drift
    if ! $generate_ts && ! $generate_py && ! $check_drift_flag; then
        generate_ts=true
        generate_py=true
    fi

    cd "$PROJECT_ROOT"

    # Generate spec first
    if $generate_spec_flag; then
        generate_spec
    fi

    # Validate if requested
    if $validate; then
        validate_spec
    fi

    # Check drift and exit early if that's all we're doing
    if $check_drift_flag; then
        check_drift
        exit $?
    fi

    # Generate TypeScript types
    if $generate_ts; then
        generate_typescript
    fi

    # Generate Python SDK
    if $generate_py; then
        generate_python
    fi

    log_info "✓ SDK generation complete!"

    # Summary
    if $generate_ts || $generate_py; then
        echo ""
        log_info "Summary:"
        [ -f "$OPENAPI_SPEC" ] && log_info "  OpenAPI spec: $OPENAPI_SPEC"
        $generate_ts && log_info "  TypeScript types: ui/src/api/generated.ts"
        $generate_py && log_info "  Python SDK: sdk/python/"
        echo ""
        log_info "Next steps:"
        log_info "  - Review generated files"
        log_info "  - Run tests: cd ui && pnpm test"
        log_info "  - Commit changes if types are updated"
    fi
}

main "$@"
