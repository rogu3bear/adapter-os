#!/bin/bash
# adapterOS Interactive Environment Setup Script
# Guides developers through environment configuration

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
MAGENTA='\033[0;35m'
CYAN='\033[0;36m'
NC='\033[0m'

# Utilities
print_header() {
    echo ""
    echo -e "${BLUE}╔════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║ adapterOS Environment Setup${NC}${BLUE} $(printf '%-40s' "$1")║${NC}"
    echo -e "${BLUE}╚════════════════════════════════════════════════════════════════╝${NC}"
}

print_section() {
    echo ""
    echo -e "${CYAN}→ $1${NC}"
}

print_step() {
    echo -e "${MAGENTA}  ✓${NC} $1"
}

print_info() {
    echo -e "${YELLOW}ℹ${NC} $1"
}

prompt_yes_no() {
    local prompt="$1"
    local default="${2:-n}"

    while true; do
        if [ "$default" = "y" ]; then
            read -p "$(echo -e ${CYAN}$prompt${NC}) [Y/n] " -r response
            [ -z "$response" ] && response="y"
        else
            read -p "$(echo -e ${CYAN}$prompt${NC}) [y/N] " -r response
            [ -z "$response" ] && response="n"
        fi

        case "$response" in
            [Yy]) return 0 ;;
            [Nn]) return 1 ;;
            *) echo "Please answer yes (y) or no (n)." ;;
        esac
    done
}

prompt_choice() {
    local prompt="$1"
    shift
    local options=("$@")

    echo ""
    echo -e "${CYAN}$prompt${NC}"
    for i in "${!options[@]}"; do
        echo "  $((i+1)). ${options[$i]}"
    done

    while true; do
        read -p "$(echo -e ${YELLOW}Select (1-${#options[@]})${NC}: " -r choice
        if [[ "$choice" =~ ^[0-9]+$ ]] && [ "$choice" -ge 1 ] && [ "$choice" -le "${#options[@]}" ]; then
            echo "${options[$((choice-1))]}"
            return 0
        fi
        echo "Invalid selection. Please try again."
    done
}

# ═══════════════════════════════════════════════════════════════════════════════
# MAIN FLOW
# ═══════════════════════════════════════════════════════════════════════════════

clear

print_header "Setup Guide"
echo -e "${MAGENTA}Creating optimized environment configuration for your workflow${NC}"
echo ""
echo "This script will:"
echo "  1. Copy .env.example to .env"
echo "  2. Ask about your setup (development, training, or production)"
echo "  3. Configure the environment for your use case"
echo "  4. Validate your configuration"

echo ""
if ! prompt_yes_no "Continue with environment setup?"; then
    echo "Setup cancelled."
    exit 0
fi

# Check if .env already exists
if [ -f .env ]; then
    if prompt_yes_no ".env already exists. Overwrite it?"; then
        rm .env
        print_info "Existing .env removed"
    else
        print_info "Using existing .env file"
        read -p "Press Enter to continue..."
        exit 0
    fi
fi

# ═══════════════════════════════════════════════════════════════════════════════
# COPY TEMPLATE
# ═══════════════════════════════════════════════════════════════════════════════

print_section "Creating .env file"
cp .env.example .env
print_step ".env file created from .env.example"

# ═══════════════════════════════════════════════════════════════════════════════
# PROFILE SELECTION
# ═══════════════════════════════════════════════════════════════════════════════

print_section "Selecting setup profile"
echo ""
echo "Choose your use case:"
echo ""
echo -e "  ${GREEN}1. Development${NC} - Local testing, all features, debug logging"
echo -e "  ${GREEN}2. Training${NC}     - MLX backend, GPU acceleration, fine-tuning"
echo -e "  ${GREEN}3. Production${NC}   - CoreML backend, maximum security, auditing"
echo -e "  ${GREEN}4. Custom${NC}       - Manual configuration"
echo ""

PROFILE=$(prompt_choice "Select profile (1-4):")

case "$PROFILE" in
    Development)
        PROFILE_TYPE="dev"
        print_step "Development profile selected"
        ;;
    Training)
        PROFILE_TYPE="training"
        print_step "Training profile selected"
        ;;
    Production)
        PROFILE_TYPE="prod"
        print_step "Production profile selected"
        ;;
    Custom)
        PROFILE_TYPE="custom"
        print_step "Custom profile selected (you'll configure manually)"
        ;;
esac

# ═══════════════════════════════════════════════════════════════════════════════
# CONFIGURATION BASED ON PROFILE
# ═══════════════════════════════════════════════════════════════════════════════

if [ "$PROFILE_TYPE" = "dev" ]; then
    print_section "Configuring for development"

    # Uncomment development settings in .env
    cat >> .env.dev_patch << 'EOF'

# ═══════════════════════════════════════════════════════════════════════════════
# DEVELOPMENT PROFILE SETTINGS
# ═══════════════════════════════════════════════════════════════════════════════
# Enabled: All features, debug logging, insecure defaults for convenience
# Use for: Local testing and development

RUST_LOG=debug,adapteros=trace
AOS_SERVER_PRODUCTION_MODE=false
AOS_SECURITY_JWT_MODE=hs256
AOS_MODEL_BACKEND=auto
EOF

    cat .env.dev_patch >> .env
    rm .env.dev_patch
    print_step "Development settings configured"

    echo ""
    echo "Configuration summary:"
    echo "  • Debug logging enabled (RUST_LOG=debug)"
    echo "  • Development mode (AOS_SERVER_PRODUCTION_MODE=false)"
    echo "  • HMAC-SHA256 JWT (HS256)"
    echo "  • Auto backend selection (CoreML > Metal > MLX)"

elif [ "$PROFILE_TYPE" = "training" ]; then
    print_section "Configuring for training"

    cat >> .env.training_patch << 'EOF'

# ═══════════════════════════════════════════════════════════════════════════════
# TRAINING PROFILE SETTINGS
# ═══════════════════════════════════════════════════════════════════════════════
# Enabled: MLX backend with GPU acceleration, float16 precision
# Use for: Training new LoRA adapters, research/experiments

AOS_MODEL_BACKEND=mlx
AOS_MLX_PRECISION=float16
AOS_MLX_MEMORY_POOL_ENABLED=true
AOS_MLX_MAX_MEMORY=0
RUST_LOG=info,adapteros_lora_mlx_ffi=debug
AOS_DATABASE_URL=sqlite:var/aos-cp.sqlite3
EOF

    cat .env.training_patch >> .env
    rm .env.training_patch
    print_step "Training settings configured"

    echo ""
    echo "Configuration summary:"
    echo "  • MLX backend enabled"
    echo "  • Float16 precision (GPU-optimized)"
    echo "  • Memory pool enabled for efficiency"
    echo "  • Backend debug logging"

elif [ "$PROFILE_TYPE" = "prod" ]; then
    print_section "Configuring for production"

    # Prompt for sensitive values
    echo ""
    echo -e "${YELLOW}⚠ Production requires secure credentials${NC}"
    echo ""

    read -p "Enter database path [/var/lib/aos/cp.db]: " DB_PATH
    DB_PATH="${DB_PATH:-/var/lib/aos/cp.db}"

    # Generate JWT secret
    JWT_SECRET=$(openssl rand -base64 32)
    print_step "Generated JWT secret"

    read -p "Enter UDS socket path [/var/run/aos/aos.sock]: " UDS_SOCKET
    UDS_SOCKET="${UDS_SOCKET:-/var/run/aos/aos.sock}"

    cat >> .env.prod_patch << EOF

# ═══════════════════════════════════════════════════════════════════════════════
# PRODUCTION PROFILE SETTINGS
# ═══════════════════════════════════════════════════════════════════════════════
# Enabled: CoreML backend, maximum security, Ed25519 JWT, auditing
# Use for: Production serving, compliance-required environments

AOS_SERVER_PRODUCTION_MODE=true
AOS_MODEL_BACKEND=coreml
AOS_SECURITY_JWT_MODE=eddsa
AOS_SECURITY_JWT_SECRET=$JWT_SECRET
AOS_SECURITY_PF_DENY=true
AOS_SERVER_UDS_SOCKET=$UDS_SOCKET
AOS_DATABASE_URL=sqlite:$DB_PATH
RUST_LOG=warn,adapteros=info
AOS_TELEMETRY_ENABLED=true
EOF

    cat .env.prod_patch >> .env
    rm .env.prod_patch
    print_step "Production settings configured"

    echo ""
    echo "Configuration summary:"
    echo "  • Production mode enabled"
    echo "  • CoreML backend (ANE acceleration)"
    echo "  • Ed25519 JWT (secure signing)"
    echo "  • PF deny rules enforced"
    echo "  • UDS socket: $UDS_SOCKET"
    echo "  • Database: $DB_PATH"
    echo "  • Telemetry enabled"

    print_info "JWT Secret saved to .env"
    print_info "Please secure this .env file - it contains credentials"

elif [ "$PROFILE_TYPE" = "custom" ]; then
    print_section "Custom configuration"
    echo ""
    echo "Edit .env manually with your preferred editor:"
    echo "  vim .env"
    echo "  nano .env"
    echo ""
    echo "See docs/ENVIRONMENT_SETUP.md for detailed variable reference"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# MODEL SETUP
# ═══════════════════════════════════════════════════════════════════════════════

print_section "Setting up model"
echo ""

if prompt_yes_no "Download model now? (required for inference)"; then
    if [ -f ./scripts/download_model.sh ]; then
        ./scripts/download_model.sh
        print_step "Model download complete"
    else
        print_info "download_model.sh not found"
        echo "Manually download a model:"
        echo "  huggingface-cli download mlx-community/Qwen2.5-7B-Instruct \\"
        echo "      --include '*.safetensors' '*.json' \\"
        echo "      --local-dir models/qwen2.5-7b-mlx"
    fi
else
    print_info "Model setup skipped"
    print_info "You can download later using: ./scripts/download_model.sh"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# DATABASE SETUP
# ═══════════════════════════════════════════════════════════════════════════════

print_section "Setting up database"
echo ""

if prompt_yes_no "Initialize database now?"; then
    mkdir -p var/artifacts var/bundles var/alerts

    print_step "Created data directories"

    if command -v cargo &> /dev/null; then
        print_info "Running migrations..."
        cargo run --release -p adapteros-orchestrator -- db migrate 2>&1 | tail -5
        print_step "Database migrations complete"

        print_info "Initializing default tenant..."
        cargo run --release -p adapteros-orchestrator -- init-tenant \
            --id default --uid 1000 --gid 1000 2>&1 | tail -3
        print_step "Default tenant created"
    else
        print_info "Cargo not found - skipping automatic migrations"
        echo "Run migrations manually when ready:"
        echo "  cargo run --release -p adapteros-orchestrator -- db migrate"
    fi
else
    print_info "Database setup skipped"
    print_info "Run migrations later:"
    echo "  cargo run --release -p adapteros-orchestrator -- db migrate"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# VALIDATION
# ═══════════════════════════════════════════════════════════════════════════════

print_section "Validating configuration"
echo ""

if [ -f ./scripts/validate_env.sh ]; then
    ./scripts/validate_env.sh
    VALIDATION_STATUS=$?

    if [ $VALIDATION_STATUS -eq 0 ]; then
        print_step "Configuration validation successful"
    elif [ $VALIDATION_STATUS -eq 1 ]; then
        echo ""
        echo -e "${RED}Configuration has errors. Please fix them before proceeding.${NC}"
        exit 1
    fi
else
    print_info "validate_env.sh not found - skipping validation"
fi

# ═══════════════════════════════════════════════════════════════════════════════
# NEXT STEPS
# ═══════════════════════════════════════════════════════════════════════════════

print_header "Setup Complete"
echo ""
echo "Your environment is configured! Next steps:"
echo ""

if [ "$PROFILE_TYPE" = "dev" ]; then
    echo "1. Start the backend server:"
    echo "   cargo run --release -p adapteros-server-api"
    echo ""
    echo "2. Build and serve the UI (Leptos WASM):"
    echo "   cd crates/adapteros-ui && trunk build --release"
    echo ""
    echo "3. Access the UI at http://localhost:${AOS_SERVER_PORT:-8080}"

elif [ "$PROFILE_TYPE" = "training" ]; then
    echo "1. Verify MLX is available:"
    echo "   brew install mlx"
    echo ""
    echo "2. Start the server:"
    echo "   cargo run --release -p adapteros-server-api"
    echo ""
    echo "3. Access training interface at http://localhost:${AOS_UI_PORT:-3200}/training"

elif [ "$PROFILE_TYPE" = "prod" ]; then
    echo "1. Secure your credentials:"
    echo "   chmod 600 .env"
    echo ""
    echo "2. Create UDS socket directory:"
    echo "   sudo mkdir -p $(dirname $UDS_SOCKET)"
    echo "   sudo chmod 755 $(dirname $UDS_SOCKET)"
    echo ""
    echo "3. Build release binary:"
    echo "   cargo build --release -p adapteros-server-api"
    echo ""
    echo "4. Deploy and start service"

else
    echo "1. Edit .env with your configuration:"
    echo "   vim .env"
    echo ""
    echo "2. Validate your setup:"
    echo "   ./scripts/validate_env.sh"
    echo ""
    echo "3. Follow the quick start guide:"
    echo "   cat QUICKSTART.md"
fi

echo ""
echo "Documentation:"
echo "  • Environment guide: docs/ENVIRONMENT_SETUP.md"
echo "  • Quick start: QUICKSTART.md"
echo "  • Full architecture: docs/ARCHITECTURE_INDEX.md"
echo ""
echo -e "${GREEN}✅ Setup complete. Happy coding!${NC}"
echo ""
