#!/bin/bash
set -euo pipefail

# AdapterOS SDK Generation Script
# Generates TypeScript and Python client SDKs from OpenAPI spec

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

# Check if openapi-generator-cli is available
check_generator() {
    if command -v openapi-generator-cli &> /dev/null; then
        GENERATOR="openapi-generator-cli"
    elif command -v openapi-generator &> /dev/null; then
        GENERATOR="openapi-generator"
    elif [ -f "$PROJECT_ROOT/node_modules/.bin/openapi-generator-cli" ]; then
        GENERATOR="$PROJECT_ROOT/node_modules/.bin/openapi-generator-cli"
    else
        log_error "openapi-generator-cli not found"
        log_info "Install it with: npm install -g @openapitools/openapi-generator-cli"
        log_info "Or: pnpm add -D @openapitools/openapi-generator-cli"
        exit 1
    fi
    log_info "Using generator: $GENERATOR"
}

# Generate OpenAPI spec from Rust code
generate_spec() {
    log_info "Generating OpenAPI specification..."

    # Ensure output directory exists
    mkdir -p "$(dirname "$OPENAPI_SPEC")"

    # Run the export binary
    if ! cargo run --bin export-openapi -- "$OPENAPI_SPEC"; then
        log_error "Failed to generate OpenAPI spec"
        exit 1
    fi

    if [ ! -f "$OPENAPI_SPEC" ]; then
        log_error "OpenAPI spec was not created at $OPENAPI_SPEC"
        exit 1
    fi

    log_info "OpenAPI spec generated at $OPENAPI_SPEC"
}

# Generate TypeScript SDK
generate_typescript() {
    log_info "Generating TypeScript SDK..."

    local output_dir="$PROJECT_ROOT/ui/src/api/generated"
    mkdir -p "$output_dir"

    $GENERATOR generate \
        -c "$CODEGEN_DIR/typescript-fetch.json" \
        --skip-validate-spec \
        || { log_error "TypeScript SDK generation failed"; return 1; }

    log_info "TypeScript SDK generated at $output_dir"

    # Add .gitignore to generated directory
    echo "# Generated files - do not edit manually" > "$output_dir/.gitignore"
    echo "# Regenerate with: ./scripts/generate-sdks.sh --typescript" >> "$output_dir/.gitignore"
}

# Generate Python SDK
generate_python() {
    log_info "Generating Python SDK..."

    local output_dir="$PROJECT_ROOT/sdk/python"
    mkdir -p "$output_dir"

    $GENERATOR generate \
        -c "$CODEGEN_DIR/python.json" \
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

    if command -v swagger-cli &> /dev/null; then
        swagger-cli validate "$OPENAPI_SPEC" || log_warn "Spec validation had warnings"
    else
        log_warn "swagger-cli not found, skipping validation"
    fi
}

# Print usage
usage() {
    echo "Usage: $0 [OPTIONS]"
    echo ""
    echo "Options:"
    echo "  --spec-only      Only generate the OpenAPI spec"
    echo "  --typescript     Generate TypeScript SDK only"
    echo "  --python         Generate Python SDK only"
    echo "  --all            Generate spec and all SDKs (default)"
    echo "  --validate       Validate spec after generation"
    echo "  -h, --help       Show this help message"
}

# Main
main() {
    local generate_spec_flag=true
    local generate_ts=false
    local generate_py=false
    local validate=false

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

    # Default to all if no specific SDK selected
    if ! $generate_ts && ! $generate_py; then
        generate_ts=true
        generate_py=true
    fi

    cd "$PROJECT_ROOT"

    # Generate spec first
    generate_spec

    # Validate if requested
    if $validate; then
        validate_spec
    fi

    # Check for generator only if we need to generate SDKs
    if $generate_ts || $generate_py; then
        check_generator
    fi

    # Generate SDKs
    if $generate_ts; then
        generate_typescript
    fi

    if $generate_py; then
        generate_python
    fi

    log_info "SDK generation complete!"
}

main "$@"
