#!/bin/bash
# adapterOS Environment Profile Switcher
# Quick switch between development, training, and production profiles

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# Check if .env exists
if [ ! -f .env ]; then
    echo -e "${RED}Error: .env file not found${NC}"
    echo "Run: cp .env.example .env"
    exit 1
fi

# Function to update .env variable
update_env_var() {
    local key="$1"
    local value="$2"

    if grep -q "^$key=" .env; then
        # Update existing value
        sed -i.bak "s|^$key=.*|$key=$value|g" .env
    else
        # Add new variable
        echo "$key=$value" >> .env
    fi
    rm -f .env.bak
}

# Function to comment out variable
comment_env_var() {
    local key="$1"
    if grep -q "^$key=" .env; then
        sed -i.bak "s|^$key=|# $key=|g" .env
    fi
    rm -f .env.bak
}

print_header() {
    echo ""
    echo -e "${BLUE}╔════════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${BLUE}║ Switching to $1${NC}"
    echo -e "${BLUE}╚════════════════════════════════════════════════════════════════╝${NC}"
}

print_config() {
    echo -e "${GREEN}Configuration updated:${NC}"
    echo "$1"
}

# Parse command line argument
PROFILE="${1:-}"

case "$PROFILE" in
    dev|development)
        print_header "DEVELOPMENT PROFILE"

        update_env_var "RUST_LOG" "debug,adapteros=trace"
        update_env_var "AOS_SERVER_PRODUCTION_MODE" "false"
        update_env_var "AOS_SECURITY_JWT_MODE" "hs256"
        update_env_var "AOS_MODEL_BACKEND" "auto"

        print_config "
  • Log level: DEBUG + TRACE
  • Security: Development (insecure)
  • Backend: Auto-select
  • Mode: Development (all features)

Good for: Local testing, debugging, feature development"

        echo ""
        echo -e "${YELLOW}Next: cargo run --release -p adapteros-server-api${NC}"
        ;;

    training)
        print_header "TRAINING PROFILE"

        update_env_var "RUST_LOG" "info,adapteros_lora_mlx_ffi=debug"
        update_env_var "AOS_SERVER_PRODUCTION_MODE" "false"
        update_env_var "AOS_MODEL_BACKEND" "mlx"
        update_env_var "AOS_MLX_PRECISION" "float16"
        update_env_var "AOS_MLX_MEMORY_POOL_ENABLED" "true"

        print_config "
  • Backend: MLX (GPU-accelerated)
  • Precision: float16 (optimized)
  • Memory: Pool enabled
  • Log level: DEBUG for MLX

Good for: Training LoRA adapters, ML experiments"

        echo ""
        echo -e "${YELLOW}Next: cargo run --release -p adapteros-server-api${NC}"
        ;;

    prod|production)
        print_header "PRODUCTION PROFILE"

        update_env_var "AOS_SERVER_PRODUCTION_MODE" "true"
        update_env_var "AOS_SECURITY_JWT_MODE" "eddsa"
        update_env_var "AOS_SECURITY_PF_DENY" "true"
        update_env_var "AOS_MODEL_BACKEND" "coreml"
        update_env_var "RUST_LOG" "warn,adapteros=info"
        update_env_var "AOS_TELEMETRY_ENABLED" "true"

        print_config "
  • Backend: CoreML (ANE acceleration)
  • Security: Ed25519 JWT
  • PF Deny Rules: Enabled
  • Telemetry: Enabled
  • Mode: Production (enforced security)

Good for: Production serving, compliance, auditing"

        echo ""
        echo -e "${YELLOW}⚠ IMPORTANT: Production setup checklist:${NC}"
        echo "  1. Set AOS_SECURITY_JWT_SECRET: $(openssl rand -base64 32)"
        echo "  2. Set AOS_SERVER_UDS_SOCKET (required for production)"
        echo "  3. Create socket directory: sudo mkdir -p /var/run/aos"
        echo "  4. Update AOS_DATABASE_URL to production path"
        echo "  5. Secure .env: chmod 600 .env"
        echo "  6. Verify: ./scripts/validate_env.sh"
        ;;

    show)
        echo ""
        echo -e "${BLUE}Current Environment Settings:${NC}"
        echo ""
        echo "Production Mode:"
        grep "^AOS_SERVER_PRODUCTION_MODE=" .env || echo "  (not set)"
        echo ""
        echo "Security:"
        grep "^AOS_SECURITY_JWT_MODE=" .env || echo "  (not set)"
        grep "^AOS_SECURITY_PF_DENY=" .env || echo "  (not set)"
        echo ""
        echo "Backend:"
        grep "^AOS_MODEL_BACKEND=" .env || echo "  (not set)"
        echo ""
        echo "Logging:"
        grep "^RUST_LOG=" .env || echo "  (not set)"
        echo ""
        echo "Database:"
        grep "^AOS_DATABASE_URL=" .env || echo "  (not set)"
        echo ""
        ;;

    *)
        echo ""
        echo -e "${BLUE}adapterOS Environment Profile Switcher${NC}"
        echo ""
        echo "Usage: ./scripts/switch_env_profile.sh <profile>"
        echo ""
        echo "Available profiles:"
        echo ""
        echo -e "${GREEN}1. dev${NC}        - Development mode (debug logging, all features)"
        echo -e "${GREEN}2. training${NC}   - Training mode (MLX backend, GPU acceleration)"
        echo -e "${GREEN}3. prod${NC}       - Production mode (CoreML, maximum security)"
        echo -e "${GREEN}4. show${NC}       - Show current settings"
        echo ""
        echo "Examples:"
        echo "  ./scripts/switch_env_profile.sh dev"
        echo "  ./scripts/switch_env_profile.sh training"
        echo "  ./scripts/switch_env_profile.sh prod"
        echo "  ./scripts/switch_env_profile.sh show"
        echo ""
        echo "Then start the server:"
        echo "  cargo run --release -p adapteros-server-api"
        echo ""
        ;;
esac

# Show validation
if [ "$PROFILE" != "show" ] && [ -n "$PROFILE" ]; then
    echo ""
    echo -e "${YELLOW}Validating configuration...${NC}"
    if [ -f ./scripts/validate_env.sh ]; then
        ./scripts/validate_env.sh | head -20
        echo ""
        echo -e "${YELLOW}Run './scripts/validate_env.sh' for full validation${NC}"
    fi
fi
