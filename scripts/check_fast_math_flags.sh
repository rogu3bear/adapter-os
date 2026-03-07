#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

patterns=(
  "-ffast-math"
  "-funsafe-math-optimizations"
)

include_globs=(
  "Cargo.toml"
  ".cargo/config"
  ".cargo/config.toml"
  "build.rs"
  "*.mk"
  "*.cmake"
  "CMakeLists.txt"
  "scripts/**/*.sh"
  "metal/**/*.metal"
)

exclude_globs=(
  "docs/**"
  "target/**"
  "node_modules/**"
  "sbom/**"
  "vendor/**"
  "scripts/check_fast_math_flags.sh"
)

if command -v rg >/dev/null 2>&1; then
  args=(--fixed-strings -n)
  for glob in "${include_globs[@]}"; do
    args+=(-g "$glob")
  done
  for glob in "${exclude_globs[@]}"; do
    args+=(-g "!$glob")
  done

  found=0
  for pattern in "${patterns[@]}"; do
    if rg "${args[@]}" -- "$pattern" .; then
      found=1
    fi
  done

  if [[ $found -ne 0 ]]; then
    echo "error: forbidden compiler flags detected (-ffast-math/-funsafe-math-optimizations)" >&2
    exit 1
  fi
else
  found=0
  while IFS= read -r -d '' file; do
    for pattern in "${patterns[@]}"; do
      if grep -n --fixed-strings "$pattern" "$file" >/dev/null 2>&1; then
        echo "$file: forbidden flag $pattern" >&2
        found=1
      fi
    done
  done < <(
    find . -type f \( \
      -name "Cargo.toml" -o \
      -name "build.rs" -o \
      -name "*.mk" -o \
      -name "*.cmake" -o \
      -name "CMakeLists.txt" -o \
      -path "./.cargo/config" -o \
      -path "./.cargo/config.toml" -o \
      -path "./scripts/*.sh" -o \
      -path "./scripts/**/*.sh" -o \
      -path "./metal/*.metal" -o \
      -path "./metal/**/*.metal" \
    \) \
    -not -path "./docs/*" \
    -not -path "./target/*" \
    -not -path "./node_modules/*" \
    -not -path "./sbom/*" \
    -not -path "./vendor/*" \
    -print0
  )

  if [[ $found -ne 0 ]]; then
    echo "error: forbidden compiler flags detected" >&2
    exit 1
  fi
fi

echo "fast-math flags: OK"
