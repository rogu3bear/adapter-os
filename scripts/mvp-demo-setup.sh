#!/usr/bin/env bash
# =============================================================================
# AdapterOS MVP Demo Setup
# One-command setup for demo-ready state
# =============================================================================

set -euo pipefail

# Script location
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Configuration
MODEL_DIR="${REPO_ROOT}/var/model-cache/models"
MODEL_NAME="qwen2.5-7b-mlx"
MODEL_PATH="${MODEL_DIR}/${MODEL_NAME}"
DATABASE_PATH="${REPO_ROOT}/var/aos-cp.sqlite3"
API_URL="http://localhost:${AOS_SERVER_PORT:-8080}"

# =============================================================================
# Helper Functions
# =============================================================================

log_step() {
    echo -e "${BLUE}==>${NC} $1"
}

log_success() {
    echo -e "${GREEN}✓${NC} $1"
}

log_warning() {
    echo -e "${YELLOW}!${NC} $1"
}

log_error() {
    echo -e "${RED}✗${NC} $1"
}

check_command() {
    if command -v "$1" &>/dev/null; then
        log_success "$1 found"
        return 0
    else
        log_error "$1 not found"
        return 1
    fi
}

# =============================================================================
# Step 1: Check Dependencies
# =============================================================================

check_dependencies() {
    log_step "Checking dependencies..."

    local failed=0

    # Rust toolchain
    if check_command rustc; then
        local rust_version
        rust_version=$(rustc --version | awk '{print $2}')
        echo "    Rust version: $rust_version"
    else
        log_error "Install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        failed=1
    fi

    # Cargo
    check_command cargo || failed=1

    # pnpm (for UI)
    if check_command pnpm; then
        local pnpm_version
        pnpm_version=$(pnpm --version)
        echo "    pnpm version: $pnpm_version"
    else
        log_warning "pnpm not found - UI will not be available"
        log_warning "Install with: npm install -g pnpm"
    fi

    # huggingface-cli (for model download)
    if check_command huggingface-cli; then
        log_success "huggingface-cli found"
    else
        log_warning "huggingface-cli not found - model download may fail"
        log_warning "Install with: pip install huggingface-hub[cli]"
    fi

    # mlx (required for multi-backend)
    if [ -d "/opt/homebrew/opt/mlx" ] || [ -d "/usr/local/opt/mlx" ] || pkg-config --modversion mlx &>/dev/null; then
        log_success "MLX library found"
    else
        log_warning "MLX library not found - required for multi-backend feature"
        log_warning "Install with: brew install mlx"
    fi

    # sqlite3
    check_command sqlite3 || {
        log_error "sqlite3 required for database operations"
        failed=1
    }

    if [ $failed -eq 1 ]; then
        log_error "Missing required dependencies. Please install them and retry."
        exit 1
    fi

    log_success "All required dependencies found"
}

# =============================================================================
# Step 2: Setup Environment
# =============================================================================

setup_environment() {
    log_step "Setting up environment..."

    # Create required directories
    mkdir -p "${REPO_ROOT}/var/model-cache/models"
    mkdir -p "${REPO_ROOT}/var/logs"
    mkdir -p "${REPO_ROOT}/var/run"
    mkdir -p "${REPO_ROOT}/var/artifacts"
    mkdir -p "${REPO_ROOT}/var/bundles"
    mkdir -p "${REPO_ROOT}/var/alerts"
    mkdir -p "${REPO_ROOT}/var/demo-data"

    # Check if .env exists, create from example if not
    if [ ! -f "${REPO_ROOT}/.env" ]; then
        if [ -f "${REPO_ROOT}/.env.example" ]; then
            cp "${REPO_ROOT}/.env.example" "${REPO_ROOT}/.env"
            log_success "Created .env from .env.example"
        fi
    fi

    # Ensure critical environment variables are set
    if ! grep -q "AOS_MODEL_CACHE_MAX_MB" "${REPO_ROOT}/.env" 2>/dev/null; then
        echo "" >> "${REPO_ROOT}/.env"
        echo "# MVP Demo settings" >> "${REPO_ROOT}/.env"
        echo "AOS_MODEL_CACHE_MAX_MB=8192" >> "${REPO_ROOT}/.env"
        log_success "Added AOS_MODEL_CACHE_MAX_MB to .env"
    fi

    # Source environment
    if [ -f "${REPO_ROOT}/.env" ]; then
        set -a
        source "${REPO_ROOT}/.env"
        set +a
    fi

    log_success "Environment configured"
}

# =============================================================================
# Step 3: Download Model
# =============================================================================

download_model() {
    log_step "Checking model availability..."

    if [ -d "$MODEL_PATH" ] && [ -f "$MODEL_PATH/config.json" ]; then
        log_success "Model already downloaded at $MODEL_PATH"
        return 0
    fi

    log_step "Downloading model (~3.8GB)..."

    if [ -x "${REPO_ROOT}/scripts/download-model.sh" ]; then
        "${REPO_ROOT}/scripts/download-model.sh"
        log_success "Model downloaded successfully"
    else
        log_error "download-model.sh not found or not executable"
        log_warning "Please manually download Qwen2.5-7B-Instruct model to: $MODEL_PATH"
        return 1
    fi
}

# =============================================================================
# Step 4: Build Project
# =============================================================================

build_project() {
    log_step "Building project..."

    cd "$REPO_ROOT"

    # Build main workspace
    log_step "Building Rust workspace (this may take a few minutes)..."
    # Build with default features (deterministic-only + coreml-backend)
    # Use mlx-backend feature only if MLX python package is available
    # Exclude fuzz crate (requires special cargo-fuzz setup)
    local build_features=""
    if python3 -c "import mlx.core" &>/dev/null; then
        build_features="--features mlx-backend"
    fi
    cargo build --release --workspace --exclude adapteros-fuzz $build_features 2>&1 | tail -5

    # Build CLI with TUI
    log_step "Building CLI..."
    cargo build --release -p adapteros-cli --features tui 2>&1 | tail -5
    ln -sf target/release/aosctl ./aosctl

    # Build UI if pnpm available
    if command -v pnpm &>/dev/null && [ -d "${REPO_ROOT}/ui" ]; then
        log_step "Installing UI dependencies..."
        cd "${REPO_ROOT}/ui"
        pnpm install --frozen-lockfile 2>&1 | tail -3
        cd "$REPO_ROOT"
    fi

    log_success "Build completed"
}

# =============================================================================
# Step 5: Setup Database
# =============================================================================

setup_database() {
    log_step "Setting up database..."

    cd "$REPO_ROOT"

    # Run migrations
    if [ -x "${REPO_ROOT}/aosctl" ]; then
        log_step "Running migrations..."
        "${REPO_ROOT}/aosctl" db migrate 2>&1 | tail -3
    elif cargo run --release -p adapteros-cli -- db migrate 2>&1 | tail -3; then
        :
    else
        log_warning "Could not run migrations via CLI, trying direct"
    fi

    # Initialize tenant if not exists
    if [ -x "${REPO_ROOT}/aosctl" ]; then
        log_step "Initializing default tenant..."
        "${REPO_ROOT}/aosctl" init --yes 2>&1 || true
    fi

    log_success "Database setup completed"
}

# =============================================================================
# Step 6: Create Demo Data
# =============================================================================

create_demo_data() {
    log_step "Creating demo data..."

    # Create demo training dataset
    local demo_data_dir="${REPO_ROOT}/var/demo-data"
    local demo_dataset="${demo_data_dir}/training-sample.jsonl"

    if [ ! -f "$demo_dataset" ]; then
        cat > "$demo_dataset" << 'EOF'
{"prompt": "What is AdapterOS?", "completion": "AdapterOS is an ML inference platform with offline-capable, UMA-optimized orchestration for multi-LoRA systems on Apple Silicon. It provides deterministic inference, hot-swap adapters, and multi-tenant isolation."}
{"prompt": "How do I train an adapter?", "completion": "To train an adapter: 1) Navigate to Training > Datasets, 2) Upload your training data in JSONL format, 3) Click 'Start Training' to create an adapter from your dataset, 4) Once complete, add the adapter to a stack for inference."}
{"prompt": "What is deterministic inference?", "completion": "Deterministic inference ensures that given the same input, model, and configuration, you will always get the exact same output. AdapterOS achieves this through fixed seeds, Q15 quantization, and reproducible routing decisions."}
{"prompt": "How does the K-sparse router work?", "completion": "The K-sparse router selects the top-K adapters based on feature scoring. It extracts 22-dimensional features from the input context (language, framework, symbols, etc.) and scores each adapter. The top K adapters are selected with Q15 quantized gates."}
{"prompt": "What is a stack?", "completion": "A stack is a collection of adapters configured for inference. Stacks allow you to combine multiple specialized adapters and route requests to the most appropriate ones based on the input context."}
{"prompt": "How do I upload a dataset?", "completion": "Navigate to Training > Datasets and click 'Upload Dataset'. Select your JSONL file containing prompt/completion pairs. The system will validate the format and compute a BLAKE3 hash for integrity verification."}
{"prompt": "What backends are supported?", "completion": "AdapterOS supports multiple backends: CoreML (primary, uses Apple Neural Engine), Metal (GPU fallback), and MLX (experimental). The default is CoreML for optimal performance on Apple Silicon."}
{"prompt": "How is tenant isolation enforced?", "completion": "Tenant isolation is enforced at multiple layers: handler-level validation, database FK constraints, tenant-scoped queries, and 29+ composite triggers that prevent cross-tenant data access."}
{"prompt": "What is Q15 quantization?", "completion": "Q15 quantization uses 16-bit fixed-point representation with a denominator of 32767.0 (not 32768). This ensures deterministic gate values for adapter routing and reproducible inference results."}
{"prompt": "How do I start the system?", "completion": "Run ./start from the repository root. This boots the backend API (port 8080), UI server (port 3200), and optionally the inference worker. Health checks verify each service is ready before proceeding."}
EOF
        log_success "Created demo training dataset at $demo_dataset"
    else
        log_success "Demo training dataset already exists"
    fi

    log_success "Demo data created"
}

# =============================================================================
# Step 7: Verify Setup
# =============================================================================

verify_setup() {
    log_step "Verifying setup..."

    local checks_passed=0
    local checks_total=4

    # Check model
    if [ -d "$MODEL_PATH" ] && [ -f "$MODEL_PATH/config.json" ]; then
        log_success "Model: OK"
        ((checks_passed++))
    else
        log_error "Model: NOT FOUND at $MODEL_PATH"
    fi

    # Check database
    if [ -f "$DATABASE_PATH" ]; then
        log_success "Database: OK"
        ((checks_passed++))
    else
        log_warning "Database: Will be created on first run"
        ((checks_passed++))
    fi

    # Check binaries
    if [ -x "${REPO_ROOT}/target/release/adapteros-server" ]; then
        log_success "Backend binary: OK"
        ((checks_passed++))
    else
        log_error "Backend binary: NOT FOUND"
    fi

    # Check demo data
    if [ -f "${REPO_ROOT}/var/demo-data/training-sample.jsonl" ]; then
        log_success "Demo data: OK"
        ((checks_passed++))
    else
        log_error "Demo data: NOT FOUND"
    fi

    echo ""
    log_step "Verification: $checks_passed/$checks_total checks passed"

    if [ $checks_passed -lt $checks_total ]; then
        log_warning "Some checks failed but you may still be able to proceed"
    fi
}

# =============================================================================
# Print Instructions
# =============================================================================

print_instructions() {
    echo ""
    echo "=============================================="
    echo -e "${GREEN}AdapterOS MVP Demo Ready!${NC}"
    echo "=============================================="
    echo ""
    echo "To start the demo:"
    echo ""
    echo "  1. Start services:"
    echo -e "     ${BLUE}./start${NC}"
    echo ""
    echo "  2. Open browser:"
    echo -e "     ${BLUE}http://localhost:\${AOS_UI_PORT:-3200}${NC}"
    echo ""
    echo "Demo Flow:"
    echo "  /dashboard     - System overview"
    echo "  /adapters      - View registered adapters"
    echo "  /chat          - Chat with AI (select stack)"
    echo "  /training      - Upload dataset, train adapter"
    echo ""
    echo "Environment variables for debugging:"
    echo "  AOS_BOOT_VERBOSE=1    - Show logs on startup failures"
    echo "  AOS_HEALTH_TIMEOUT=30 - Increase health check timeout"
    echo ""
    echo "Demo training data available at:"
    echo "  var/demo-data/training-sample.jsonl"
    echo ""
}

# =============================================================================
# Main
# =============================================================================

main() {
    echo ""
    echo "=============================================="
    echo "AdapterOS MVP Demo Setup"
    echo "=============================================="
    echo ""

    check_dependencies
    echo ""

    setup_environment
    echo ""

    download_model
    echo ""

    build_project
    echo ""

    setup_database
    echo ""

    create_demo_data
    echo ""

    verify_setup

    print_instructions
}

# Run main
main "$@"
