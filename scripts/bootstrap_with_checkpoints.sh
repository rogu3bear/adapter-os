#!/bin/bash
# adapterOS Bootstrap with Checkpoint Recovery
# Wraps installation steps with state persistence for resume capability

set -e

echo "DEPRECATION: scripts/bootstrap_with_checkpoints.sh is legacy. Use ./start."
echo "A prompt will auto-cancel after 15s (default: No)."
echo ""
read -r -t 15 -p "Proceed with legacy bootstrap (with checkpoints)? [y/N]: " REPLY || REPLY=""
echo ""
if [[ ! "$REPLY" =~ ^[Yy]$ ]]; then
  echo "Aborting. Use ./start instead."
  exit 1
fi

# Configuration
DEFAULT_CHECKPOINT_FILE="$HOME/Library/Application Support/adapterOS/installer/adapteros_install.state"
CHECKPOINT_FILE="${1:-$DEFAULT_CHECKPOINT_FILE}"
MODE="${2:-full}"
AIRGAPPED="${3:-false}"
JSON_OUTPUT="${4:-false}"

WORKSPACE_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$WORKSPACE_ROOT"

# Keep legacy bootstrap builds on the canonical cache root unless explicitly overridden.
: "${CARGO_TARGET_DIR:=target}"
export CARGO_TARGET_DIR

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# JSON output helper
json_progress() {
    local step="$1"
    local progress="$2"
    local message="$3"
    local status="$4"
    
    if [[ "$JSON_OUTPUT" == "true" ]]; then
        echo "{\"step\":\"$step\",\"progress\":$progress,\"message\":\"$message\",\"status\":\"$status\"}"
    fi
}

# Load checkpoint state
load_checkpoint() {
    if [[ -f "$CHECKPOINT_FILE" ]]; then
        source "$CHECKPOINT_FILE"
        echo -e "${YELLOW}Resuming from checkpoint: $LAST_COMPLETED${NC}"
        return 0
    fi
    return 1
}

# Save checkpoint state
save_checkpoint() {
    local step="$1"
    mkdir -p "$(dirname "$CHECKPOINT_FILE")"
    cat > "$CHECKPOINT_FILE" << EOF
LAST_COMPLETED=$step
LAST_TIMESTAMP=$(date -u +"%Y-%m-%dT%H:%M:%SZ")
MODE=$MODE
AIRGAPPED=$AIRGAPPED
EOF
}

# Check if step should be skipped (already completed)
should_skip_step() {
    local step="$1"
    if [[ -n "$LAST_COMPLETED" ]]; then
        case "$LAST_COMPLETED" in
            "create_dirs")
                [[ "$step" == "create_dirs" ]] && return 0
                ;;
            "build_binaries")
                [[ "$step" == "create_dirs" || "$step" == "build_binaries" ]] && return 0
                ;;
            "init_db")
                [[ "$step" == "create_dirs" || "$step" == "build_binaries" || "$step" == "init_db" ]] && return 0
                ;;
            "build_metal")
                [[ "$step" =~ ^(create_dirs|build_binaries|init_db|build_metal)$ ]] && return 0
                ;;
            "download_model")
                [[ "$step" =~ ^(create_dirs|build_binaries|init_db|build_metal|download_model)$ ]] && return 0
                ;;
            "create_tenant")
                [[ "$step" =~ ^(create_dirs|build_binaries|init_db|build_metal|download_model|create_tenant)$ ]] && return 0
                ;;
            "smoke_test")
                [[ "$step" =~ ^(create_dirs|build_binaries|init_db|build_metal|download_model|create_tenant|smoke_test)$ ]] && return 0
                ;;
        esac
    fi
    return 1
}

# Run a step with checkpoint guards
run_step() {
    local step_name="$1"
    local step_func="$2"
    local progress="$3"
    
    if should_skip_step "$step_name"; then
        echo -e "${GREEN}✓ Skipping $step_name (already completed)${NC}"
        json_progress "$step_name" "$progress" "Skipped (already completed)" "skipped"
        return 0
    fi
    
    echo -e "${YELLOW}Running: $step_name${NC}"
    json_progress "$step_name" "$progress" "Starting $step_name" "running"
    
    if $step_func; then
        save_checkpoint "$step_name"
        echo -e "${GREEN}✓ Completed: $step_name${NC}"
        json_progress "$step_name" "$progress" "Completed $step_name" "completed"
        return 0
    else
        echo -e "${RED}✗ Failed: $step_name${NC}"
        json_progress "$step_name" "$progress" "Failed: $step_name" "failed"
        return 1
    fi
}

# Step implementations
create_directories() {
    echo "Creating adapterOS directories..."
    mkdir -p var
    mkdir -p artifacts
    mkdir -p plan
    mkdir -p var/model-cache/models
    mkdir -p var/run
    mkdir -p var/tmp
    echo "Directories created"
}

build_all() {
    echo "Building adapterOS binaries (this may take 10-15 minutes)..."
    json_progress "build_binaries" 0.2 "Compiling Rust crates" "running"
    
    # Build with release profile
    cargo build --release --bin aosctl 2>&1 | while IFS= read -r line; do
        echo "$line"
        # Extract compilation progress if possible
        if [[ "$line" =~ Compiling ]]; then
            json_progress "build_binaries" 0.3 "$line" "running"
        fi
    done
    
    cargo build --release --bin adapteros-server 2>&1 | while IFS= read -r line; do
        echo "$line"
    done
    
    echo "Binaries built successfully"
}

initialize_cp_database() {
    echo "Initializing control plane database..."
    
    # Check if config exists
    if [[ ! -f "configs/cp.toml" ]]; then
        echo "Error: configs/cp.toml not found"
        return 1
    fi
    
    # Run migrations
    ./target/release/adapteros-server --config configs/cp.toml --migrate-only
    echo "Database initialized"
}

compile_metal_kernels() {
    echo "Compiling Metal kernels for deterministic execution..."
    
    if [[ -f "metal/build.sh" ]]; then
        cd metal
        bash build.sh
        cd "$WORKSPACE_ROOT"
        echo "Metal kernels compiled"
    else
        echo "Warning: metal/build.sh not found, skipping Metal compilation"
    fi
}

download_qwen() {
    if [[ "$AIRGAPPED" == "true" ]]; then
        echo "Skipping model download (air-gapped mode)"
        return 0
    fi
    
    echo "Downloading Qwen 2.5 7B model..."
    
    if [[ -f "scripts/download-model.sh" ]]; then
        bash scripts/download-model.sh
    else
        echo "Warning: scripts/download-model.sh not found"
        echo "You'll need to manually download and import models"
    fi
}

setup_default_tenant() {
    echo "Creating default tenant..."
    
    # Create default tenant with UID 501 (standard macOS user)
    ./target/release/aosctl init-tenant --id default --uid 501 --gid 20
    
    echo "Default tenant created"
}

run_smoke_tests() {
    echo "Running post-install smoke tests..."
    
    # Check if smoke test script exists
    if [[ -f "installer/smoke_test.sh" ]]; then
        bash installer/smoke_test.sh
        echo "Smoke tests completed"
    else
        echo "Warning: smoke_test.sh not found, skipping smoke tests"
    fi
}

# Main installation flow
main() {
    echo "================================================"
    echo "adapterOS Bootstrap Installer"
    echo "================================================"
    echo "Mode: $MODE"
    echo "Air-gapped: $AIRGAPPED"
    echo "Workspace: $WORKSPACE_ROOT"
    echo "================================================"
    echo ""
    
    # Load previous checkpoint if exists
    load_checkpoint || true
    
    # Core steps (always run)
    run_step "create_dirs" create_directories 0.1 || exit 1
    run_step "build_binaries" build_all 0.4 || exit 1
    run_step "init_db" initialize_cp_database 0.6 || exit 1
    run_step "build_metal" compile_metal_kernels 0.7 || exit 1
    
    # Full mode: download model and create tenant
    if [[ "$MODE" == "full" ]]; then
        if [[ "$AIRGAPPED" == "false" ]]; then
            run_step "download_model" download_qwen 0.85 || exit 1
        else
            echo -e "${YELLOW}Skipping model download (air-gapped mode)${NC}"
        fi
        run_step "create_tenant" setup_default_tenant 0.95 || exit 1
    fi
    
    # Run smoke tests (both full and minimal modes)
    run_step "smoke_test" run_smoke_tests 0.98 || exit 1
    
    # Complete
    echo ""
    echo "================================================"
    echo -e "${GREEN}Installation Complete!${NC}"
    echo "================================================"
    json_progress "complete" 1.0 "Installation completed successfully" "completed"
    
    # Clean up checkpoint file on success
    rm -f "$CHECKPOINT_FILE"
    
    echo ""
    echo "Next steps:"
    echo "  1. Start the control plane:"
    echo "     ./target/release/adapteros-server --config configs/cp.toml"
    echo ""
    echo "  2. Run your first inference:"
    echo "     ./target/release/aosctl serve --tenant default --plan qwen7b"
    echo ""
}

# Run main
main
