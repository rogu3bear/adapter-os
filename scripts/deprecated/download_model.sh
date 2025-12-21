#!/usr/bin/env bash
#
# download_model.sh - Download Qwen2.5 models for AdapterOS
#
# Copyright (c) 2025 JKCA / James KC Auchterlonie. All rights reserved.
#
# Usage:
#   ./scripts/download_model.sh [OPTIONS]
#
# Options:
#   --format mlx|safetensors  Model format (default: mlx)
#   --model MODEL_NAME        Model name (default: Qwen2.5)
#   --size 7b|3b|1.5b|0.5b    Model size (default: 7b)
#   --quantized               Download 4-bit quantized version (MLX only, default)
#   --no-quantized            Download full precision version
#   --output DIR              Output directory (default: models/)
#   --help                    Show this help message
#
# Examples:
#   ./scripts/download_model.sh                           # MLX 4-bit 7B (recommended)
#   ./scripts/download_model.sh --format safetensors      # Full precision SafeTensors
#   ./scripts/download_model.sh --size 3b --quantized     # Smaller 3B quantized model
#   ./scripts/download_model.sh --size 0.5b               # Tiny model for testing
#

set -euo pipefail

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
FORMAT="mlx"
MODEL_BASE="Qwen2.5"
SIZE="7b"
QUANTIZED=true
OUTPUT_DIR="models"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

# Print colored output
info() { echo -e "${BLUE}[INFO]${NC} $1"; }
success() { echo -e "${GREEN}[OK]${NC} $1"; }
warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1" >&2; }

# Show usage
show_help() {
    cat << 'EOF'
download_model.sh - Download Qwen2.5 models for AdapterOS

Usage:
  ./scripts/download_model.sh [OPTIONS]

Options:
  --format mlx|safetensors  Model format (default: mlx)
  --model MODEL_NAME        Model name (default: Qwen2.5)
  --size 7b|3b|1.5b|0.5b    Model size (default: 7b)
  --quantized               Download 4-bit quantized version (MLX only, default)
  --no-quantized            Download full precision version
  --output DIR              Output directory (default: models/)
  --help                    Show this help message

Examples:
  ./scripts/download_model.sh                           # MLX 4-bit 7B (recommended)
  ./scripts/download_model.sh --format safetensors      # Full precision SafeTensors
  ./scripts/download_model.sh --size 3b --quantized     # Smaller 3B quantized model
  ./scripts/download_model.sh --size 0.5b               # Tiny model for testing
EOF
    exit 0
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --format)
            FORMAT="$2"
            shift 2
            ;;
        --model)
            MODEL_BASE="$2"
            shift 2
            ;;
        --size)
            SIZE="$2"
            shift 2
            ;;
        --quantized)
            QUANTIZED=true
            shift
            ;;
        --no-quantized)
            QUANTIZED=false
            shift
            ;;
        --output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        --help|-h)
            show_help
            ;;
        *)
            error "Unknown option: $1"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Validate format
if [[ "$FORMAT" != "mlx" && "$FORMAT" != "safetensors" ]]; then
    error "Invalid format: $FORMAT. Use 'mlx' or 'safetensors'"
    exit 1
fi

# Validate size
case $SIZE in
    7b|3b|1.5b|0.5b) ;;
    *)
        error "Invalid size: $SIZE. Use 7b, 3b, 1.5b, or 0.5b"
        exit 1
        ;;
esac

# Determine repository based on format and options
get_repo_name() {
    local size_upper
    # Convert size to uppercase format used by Qwen repos
    case $SIZE in
        7b) size_upper="7B" ;;
        3b) size_upper="3B" ;;
        1.5b) size_upper="1.5B" ;;
        0.5b) size_upper="0.5B" ;;
    esac

    if [[ "$FORMAT" == "mlx" ]]; then
        if [[ "$QUANTIZED" == true ]]; then
            echo "mlx-community/${MODEL_BASE}-${size_upper}-Instruct-4bit"
        else
            echo "mlx-community/${MODEL_BASE}-${size_upper}-Instruct-bf16"
        fi
    else
        echo "Qwen/${MODEL_BASE}-${size_upper}-Instruct"
    fi
}

get_model_dir() {
    local suffix=""
    if [[ "$FORMAT" == "mlx" ]]; then
        if [[ "$QUANTIZED" == true ]]; then
            suffix="-4bit-mlx"
        else
            suffix="-mlx"
        fi
    else
        suffix="-safetensors"
    fi
    # Use tr for lowercase (bash 3.2 compatible)
    local base_lower
    base_lower=$(echo "$MODEL_BASE" | tr '[:upper:]' '[:lower:]')
    echo "${base_lower}-${SIZE}-instruct${suffix}"
}

REPO=$(get_repo_name)
MODEL_DIR=$(get_model_dir)
OUTPUT_PATH="$PROJECT_ROOT/$OUTPUT_DIR/$MODEL_DIR"

info "Download Configuration:"
echo "  Format:     $FORMAT"
echo "  Repository: $REPO"
echo "  Quantized:  $QUANTIZED"
echo "  Size:       $SIZE"
echo "  Output:     $OUTPUT_PATH"
echo ""

# Check for required tools
check_huggingface_cli() {
    if command -v huggingface-cli &> /dev/null; then
        return 0
    elif command -v hf &> /dev/null; then
        return 0
    else
        return 1
    fi
}

# Check for git-lfs
check_git_lfs() {
    if command -v git-lfs &> /dev/null; then
        return 0
    else
        return 1
    fi
}

# Install huggingface-cli if needed
install_hf_cli() {
    warn "huggingface-cli not found. Attempting to install..."

    if command -v pip &> /dev/null; then
        pip install "huggingface_hub[cli]" --quiet
        success "huggingface-cli installed successfully"
    elif command -v pip3 &> /dev/null; then
        pip3 install "huggingface_hub[cli]" --quiet
        success "huggingface-cli installed successfully"
    else
        error "pip not found. Please install huggingface-cli manually:"
        echo "  pip install 'huggingface_hub[cli]'"
        exit 1
    fi
}

# Create output directory
create_output_dir() {
    if [[ ! -d "$PROJECT_ROOT/$OUTPUT_DIR" ]]; then
        info "Creating models directory: $PROJECT_ROOT/$OUTPUT_DIR"
        mkdir -p "$PROJECT_ROOT/$OUTPUT_DIR"
    fi

    if [[ -d "$OUTPUT_PATH" ]]; then
        warn "Model directory already exists: $OUTPUT_PATH"
        read -p "Do you want to re-download? [y/N] " -n 1 -r
        echo
        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            info "Skipping download. Use existing model."
            exit 0
        fi
        rm -rf "$OUTPUT_PATH"
    fi

    mkdir -p "$OUTPUT_PATH"
}

# Download using huggingface-cli
download_with_hf_cli() {
    local cmd="huggingface-cli"
    if ! command -v huggingface-cli &> /dev/null; then
        cmd="hf"
    fi

    info "Downloading model from Hugging Face: $REPO"
    echo ""

    # Download with progress
    $cmd download "$REPO" \
        --local-dir "$OUTPUT_PATH" \
        --local-dir-use-symlinks False \
        --include "*.json" "*.safetensors" "*.txt" "*.jinja" "*.py" "*.model" "*.tiktoken" \
        --exclude ".git*" "*.md" "LICENSE*" "README*" "*.bin"

    return $?
}

# Download using git clone (fallback)
download_with_git() {
    if ! check_git_lfs; then
        error "git-lfs is required for git-based download"
        echo "Install with: brew install git-lfs && git lfs install"
        exit 1
    fi

    info "Downloading model via git clone: $REPO"
    echo ""

    git lfs install
    GIT_LFS_SKIP_SMUDGE=0 git clone --depth 1 "https://huggingface.co/$REPO" "$OUTPUT_PATH"

    # Clean up git metadata
    rm -rf "$OUTPUT_PATH/.git"

    return $?
}

# Verify download
verify_download() {
    info "Verifying download..."

    local required_files=("config.json")
    local has_weights=false

    # Check for tokenizer (various formats)
    if [[ -f "$OUTPUT_PATH/tokenizer.json" ]] || [[ -f "$OUTPUT_PATH/tokenizer.model" ]]; then
        success "Tokenizer found"
    else
        error "No tokenizer found (tokenizer.json or tokenizer.model)"
        return 1
    fi

    # Check for weight files
    if [[ -f "$OUTPUT_PATH/weights.safetensors" ]]; then
        has_weights=true
        success "Found weights.safetensors"
    elif [[ -f "$OUTPUT_PATH/model.safetensors" ]]; then
        has_weights=true
        success "Found model.safetensors"
    else
        # Check for sharded weights
        local shard_count
        shard_count=$(find "$OUTPUT_PATH" -name "model-*.safetensors" 2>/dev/null | wc -l | tr -d ' ')
        if [[ "$shard_count" -gt 0 ]]; then
            has_weights=true
            success "Found $shard_count sharded weight files"
        fi
    fi

    if [[ "$has_weights" != true ]]; then
        error "No weight files found"
        return 1
    fi

    # Check required config files
    for file in "${required_files[@]}"; do
        if [[ ! -f "$OUTPUT_PATH/$file" ]]; then
            error "Missing required file: $file"
            return 1
        fi
    done

    success "Download verified successfully!"
    return 0
}

# Compute BLAKE3 hash if available
compute_hash() {
    if command -v b3sum &> /dev/null; then
        info "Computing BLAKE3 hash..."
        local hash_file="$OUTPUT_PATH/model.b3sum"

        # Find weight files and hash them
        find "$OUTPUT_PATH" -name "*.safetensors" -type f | while read -r f; do
            b3sum "$f"
        done > "$hash_file"

        success "Hash saved to: $hash_file"
    fi
}

# Show download summary
show_summary() {
    echo ""
    success "Model downloaded successfully!"
    echo ""
    info "Model location: $OUTPUT_PATH"
    echo ""
    info "Directory contents:"
    ls -lh "$OUTPUT_PATH" | head -20
    echo ""

    # Calculate total size
    local total_size
    total_size=$(du -sh "$OUTPUT_PATH" | cut -f1)
    info "Total size: $total_size"
    echo ""

    info "To use this model with AdapterOS:"
    echo ""
    echo "  # Set environment variable"
    echo "  export AOS_MLX_FFI_MODEL=\"$OUTPUT_PATH\""
    echo ""
    echo "  # Or use CLI flag"
    echo "  ./target/release/aosctl serve --model-path \"$OUTPUT_PATH\""
    echo ""
    echo "  # Run inference example"
    echo "  cargo run --example chat_with_adapter -- --model-path \"$OUTPUT_PATH\""
    echo ""
}

# Main execution
main() {
    echo ""
    echo "=========================================="
    echo "  AdapterOS Model Downloader"
    echo "=========================================="
    echo ""

    # Check for huggingface-cli
    if ! check_huggingface_cli; then
        install_hf_cli
    fi

    # Create output directory
    create_output_dir

    # Try huggingface-cli first, fall back to git
    if check_huggingface_cli; then
        if ! download_with_hf_cli; then
            warn "huggingface-cli download failed, trying git clone..."
            download_with_git
        fi
    else
        download_with_git
    fi

    # Verify download
    if ! verify_download; then
        error "Download verification failed!"
        exit 1
    fi

    # Compute hash
    compute_hash

    # Show summary
    show_summary
}

main
