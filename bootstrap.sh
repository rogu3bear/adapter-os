#!/usr/bin/env bash
# bootstrap.sh -- Idempotent dependency installer for AdapterOS
#
# Usage:
#   ./bootstrap.sh           Check and install all build dependencies
#   ./bootstrap.sh --verify  Install deps, then validate full build
#   ./bootstrap.sh --help    Show this help message
#
# Requires: Homebrew (https://brew.sh), Rust toolchain via rustup (https://rustup.rs)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ---------------------------------------------------------------------------
# Colors (disabled when stdout is not a terminal)
# ---------------------------------------------------------------------------
if [[ -t 1 ]]; then
  GREEN='\033[0;32m'
  YELLOW='\033[0;33m'
  RED='\033[0;31m'
  BOLD='\033[1m'
  RESET='\033[0m'
else
  GREEN='' YELLOW='' RED='' BOLD='' RESET=''
fi

info()  { printf "${GREEN}[ok]${RESET} %s\n" "$1"; }
warn()  { printf "${YELLOW}[installing]${RESET} %s\n" "$1"; }
fail()  { printf "${RED}[error]${RESET} %s\n" "$1" >&2; exit 1; }

# ---------------------------------------------------------------------------
# Help
# ---------------------------------------------------------------------------
show_help() {
  cat <<HELP
${BOLD}AdapterOS Bootstrap${RESET}

Usage:
  ./bootstrap.sh           Check and install all build dependencies
  ./bootstrap.sh --verify  Install deps, then validate the full build
  ./bootstrap.sh --help    Show this help message

Prerequisites:
  - Homebrew   https://brew.sh
  - Rust       https://rustup.rs
HELP
  exit 0
}

# ---------------------------------------------------------------------------
# Prerequisite checks (fail-fast)
# ---------------------------------------------------------------------------
check_prerequisites() {
  if ! command -v brew >/dev/null 2>&1; then
    fail "Homebrew is required. Install from https://brew.sh"
  fi
  info "brew"

  if ! command -v rustup >/dev/null 2>&1; then
    fail "Rust toolchain (rustup) is required. Install from https://rustup.rs"
  fi
  info "rustup"

  if ! command -v cargo >/dev/null 2>&1; then
    fail "cargo not found. Run: rustup install stable"
  fi
  info "cargo"
}

# ---------------------------------------------------------------------------
# Brew package helper (idempotent)
# ---------------------------------------------------------------------------
check_brew_pkg() {
  local pkg="$1"
  if brew ls --versions "$pkg" >/dev/null 2>&1; then
    info "$pkg"
  else
    warn "$pkg..."
    brew install "$pkg"
    info "$pkg (installed)"
  fi
}

# ---------------------------------------------------------------------------
# Rust target/tool helpers (idempotent)
# ---------------------------------------------------------------------------
ensure_wasm_target() {
  if rustup target list --installed | grep -q wasm32-unknown-unknown; then
    info "wasm32-unknown-unknown target"
  else
    warn "wasm32-unknown-unknown target..."
    rustup target add wasm32-unknown-unknown
    info "wasm32-unknown-unknown target (installed)"
  fi
}

ensure_cargo_tool() {
  local tool="$1"
  local crate="${2:-$tool}"
  if command -v "$tool" >/dev/null 2>&1; then
    info "$tool"
  else
    warn "$tool..."
    cargo install "$crate"
    info "$tool (installed)"
  fi
}

# ---------------------------------------------------------------------------
# Install all dependencies
# ---------------------------------------------------------------------------
install_deps() {
  printf "\n${BOLD}=== Prerequisites ===${RESET}\n"
  check_prerequisites

  printf "\n${BOLD}=== Brew Packages ===${RESET}\n"
  check_brew_pkg "mlx"

  printf "\n${BOLD}=== Rust Targets ===${RESET}\n"
  ensure_wasm_target

  printf "\n${BOLD}=== Cargo Tools ===${RESET}\n"
  ensure_cargo_tool "wasm-bindgen" "wasm-bindgen-cli"
  ensure_cargo_tool "trunk" "trunk"
}

# ---------------------------------------------------------------------------
# Verify build (only with --verify)
# ---------------------------------------------------------------------------
verify_build() {
  local ok=true

  printf "\n${BOLD}=== Verifying workspace ===${RESET}\n"
  if cargo check --workspace; then
    info "cargo check --workspace"
  else
    printf "${RED}[fail]${RESET} cargo check --workspace\n"
    ok=false
  fi

  printf "\n${BOLD}=== Verifying UI (WASM) ===${RESET}\n"
  if "$SCRIPT_DIR/scripts/ui-check.sh"; then
    info "scripts/ui-check.sh"
  else
    printf "${RED}[fail]${RESET} scripts/ui-check.sh\n"
    ok=false
  fi

  if $ok; then
    printf "\n${GREEN}${BOLD}All checks passed.${RESET}\n"
  else
    printf "\n${RED}${BOLD}Some checks failed. See output above.${RESET}\n"
    exit 1
  fi
}

# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
VERIFY=false

for arg in "$@"; do
  case "$arg" in
    --help|-h) show_help ;;
    --verify)  VERIFY=true ;;
    *)         fail "Unknown argument: $arg. Use --help for usage." ;;
  esac
done

install_deps

if $VERIFY; then
  verify_build
else
  printf "\n${BOLD}Dependencies ready.${RESET} Run with ${BOLD}--verify${RESET} to validate the full build.\n"
fi
