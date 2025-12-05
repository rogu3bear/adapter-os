#!/usr/bin/env bash
# Build helper for the real MLX backend (CoreML/Metal + MLX)
# Detects common MLX install locations and exports include/lib paths before invoking cargo.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
PROFILE="${PROFILE:-release}"
FEATURES="${FEATURES:-multi-backend,mlx}"
PACKAGE="${PACKAGE:-adapteros-lora-mlx-ffi}"
RUN_TESTS=0
RUN_BENCH=0

usage() {
  cat <<'EOF'
Usage: scripts/build-mlx.sh [options]

Options:
  --profile <name>   Cargo profile to use (default: release)
  --features <list>  Feature list (default: multi-backend,mlx)
  --package <name>   Crate/package to build (default: adapteros-lora-mlx-ffi)
  --tests            Run tests after building
  --bench            Run benchmarks after building
  -h, --help         Show this help message

Environment:
  MLX_PATH          Base install prefix (e.g., /opt/homebrew). If unset, common
                    prefixes and 'brew --prefix mlx' are checked.
  MLX_INCLUDE_DIR   Override include dir (default: $MLX_PATH/include)
  MLX_LIB_DIR       Override lib dir (default: $MLX_PATH/lib)
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --profile) PROFILE="$2"; shift 2;;
    --features) FEATURES="$2"; shift 2;;
    --package) PACKAGE="$2"; shift 2;;
    --tests) RUN_TESTS=1; shift;;
    --bench) RUN_BENCH=1; shift;;
    -h|--help) usage; exit 0;;
    *) echo "Unknown option: $1" >&2; usage; exit 1;;
  esac
done

discover_prefix() {
  if [[ -n "${MLX_PATH:-}" ]]; then
    echo "$MLX_PATH"
    return
  fi

  if command -v brew >/dev/null 2>&1; then
    if BREW_PREFIX=$(brew --prefix mlx 2>/dev/null); then
      echo "$BREW_PREFIX"
      return
    fi
  fi

  for prefix in /opt/homebrew /usr/local /usr; do
    if [[ -d "$prefix/include/mlx" ]] || [[ -f "$prefix/lib/libmlx.dylib" ]]; then
      echo "$prefix"
      return
    fi
  done

  echo ""
}

PREFIX="$(discover_prefix)"
if [[ -z "$PREFIX" ]]; then
  echo "⚠️  Could not detect MLX install prefix. Set MLX_PATH, MLX_INCLUDE_DIR, and MLX_LIB_DIR." >&2
else
  export MLX_PATH="$PREFIX"
  export MLX_INCLUDE_DIR="${MLX_INCLUDE_DIR:-$PREFIX/include}"
  export MLX_LIB_DIR="${MLX_LIB_DIR:-$PREFIX/lib}"
fi

if [[ -z "${MLX_INCLUDE_DIR:-}" || -z "${MLX_LIB_DIR:-}" ]]; then
  echo "❌ MLX_INCLUDE_DIR or MLX_LIB_DIR is not set. Aborting." >&2
  exit 1
fi

echo "🔧 Using MLX paths:"
echo "   MLX_INCLUDE_DIR=$MLX_INCLUDE_DIR"
echo "   MLX_LIB_DIR=$MLX_LIB_DIR"
echo "   PACKAGE=$PACKAGE"
echo "   FEATURES=$FEATURES"
echo "   PROFILE=$PROFILE"

if [[ ! -d "$MLX_INCLUDE_DIR/mlx" ]]; then
  echo "⚠️  Headers not found under $MLX_INCLUDE_DIR/mlx (continuing, but build may fail)." >&2
fi
if ! compgen -G "$MLX_LIB_DIR/libmlx.*" >/dev/null; then
  echo "⚠️  libmlx not found under $MLX_LIB_DIR (continuing, but build may fail)." >&2
fi

echo "🚀 Building real MLX backend..."
cargo build -p "$PACKAGE" --features "$FEATURES" --profile "$PROFILE" --manifest-path "$ROOT_DIR/Cargo.toml"

if [[ "$RUN_TESTS" -eq 1 ]]; then
  echo "🧪 Running MLX tests..."
  cargo test -p "$PACKAGE" --features "$FEATURES" --manifest-path "$ROOT_DIR/Cargo.toml"
fi

if [[ "$RUN_BENCH" -eq 1 ]]; then
  echo "📊 Running MLX benchmarks..."
  cargo bench -p "$PACKAGE" --features "$FEATURES" --manifest-path "$ROOT_DIR/Cargo.toml"
fi

echo "✅ MLX build helper completed."
