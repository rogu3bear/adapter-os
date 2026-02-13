#!/usr/bin/env bash
set -euo pipefail

# Demo bringup wrapper.
# Intentionally uses ./start (the high-level orchestrator) so model seed+load and
# readiness gates match what the UI expects.

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_ROOT"

usage() {
  cat <<'EOF'
Usage:
  scripts/demo-up.sh [--model-path PATH] [--backend coreml|mlx|metal] [--no-auth 0|1]

Defaults:
  --no-auth     1
  --backend     coreml
  --model-path  auto-detect under ./var/models/*.mlpackage (or use $AOS_MODEL_PATH)

Notes:
  - This is intended for demos on a single machine.
  - Optional services (node, secd) are skipped by default to reduce moving parts.
EOF
}

MODEL_PATH="${AOS_MODEL_PATH:-}"
BACKEND="${AOS_MODEL_BACKEND:-coreml}"
NO_AUTH="${AOS_DEV_NO_AUTH:-1}"

while [ $# -gt 0 ]; do
  case "$1" in
    --model-path)
      MODEL_PATH="${2:-}"
      shift 2
      ;;
    --backend)
      BACKEND="${2:-}"
      shift 2
      ;;
    --no-auth)
      NO_AUTH="${2:-}"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown arg: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [ -z "${MODEL_PATH:-}" ]; then
  # Prefer the "small demo model" if present; otherwise take the first mlpackage.
  if [ -d "$PROJECT_ROOT/var/models/qwen2.5-0.5b-coreml-128.mlpackage" ]; then
    MODEL_PATH="$PROJECT_ROOT/var/models/qwen2.5-0.5b-coreml-128.mlpackage"
  else
    MODEL_PATH="$(ls -1 "$PROJECT_ROOT"/var/models/*.mlpackage 2>/dev/null | head -n 1 || true)"
  fi
fi

if [ -z "${MODEL_PATH:-}" ] || [ ! -d "$MODEL_PATH" ]; then
  echo "Model not found. Provide --model-path or set AOS_MODEL_PATH. Got: ${MODEL_PATH:-<empty>}" >&2
  exit 1
fi

export AOS_MODEL_PATH="$MODEL_PATH"
export AOS_MODEL_BACKEND="$BACKEND"
export AOS_DEV_NO_AUTH="$NO_AUTH"

# Reduce moving parts for demo unless explicitly enabled by env.
export SKIP_NODE="${SKIP_NODE:-1}"
export SKIP_SECD="${SKIP_SECD:-1}"

exec ./start

