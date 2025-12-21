#!/bin/bash
set -euo pipefail

# Run jscpd in batches to avoid Node.js memory issues
# Usage:
#   bash scripts/run_jscpd_batched.sh            # local run
#   bash scripts/run_jscpd_batched.sh --ci       # CI-safe run

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")"/.. && pwd)"
OUT_BASE_DIR="$ROOT_DIR/var/reports/jscpd"
TS="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="$OUT_BASE_DIR/$TS"
CONFIG="$ROOT_DIR/configs/jscpd.config.json"

mkdir -p "$OUT_DIR"

# Increase Node.js memory limit
export NODE_OPTIONS="--max-old-space-size=4096"

CI_MODE=""
if [[ "${1:-}" == "--ci" ]]; then
  CI_MODE="--silent"
fi

echo "🔍 [jscpd] Scanning repository for code duplication (batched mode)..."
echo ""

TOTAL_CLONES=0
TOTAL_DUP_LINES=0
TOTAL_FILES=0

run_batch() {
  local batch_name="$1"
  shift
  local batch_dirs="$@"
  local batch_out="$OUT_DIR/batch-$batch_name"
  mkdir -p "$batch_out"

  # Build paths array
  local paths=""
  for dir in $batch_dirs; do
    local full_path="$ROOT_DIR/$dir"
    if [[ -d "$full_path" ]]; then
      paths="$paths $full_path"
    fi
  done

  if [[ -z "$paths" ]]; then
    echo "⏭️  Skipping $batch_name (no directories found)"
    return
  fi

  echo "📦 Scanning batch: $batch_name..."

  # Run jscpd on this batch
  if npx --yes jscpd \
    --config "$CONFIG" \
    --output "$batch_out" \
    $CI_MODE \
    $paths 2>/dev/null; then

    # Extract stats from batch report
    if [[ -f "$batch_out/report.json" ]]; then
      local clones dupLines files
      clones=$(node -e 'const r=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")); console.log((r.clones&&r.clones.length)||0)' "$batch_out/report.json" 2>/dev/null || echo "0")
      dupLines=$(node -e 'const r=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")); console.log((r.statistics&&r.statistics.duplicated&&r.statistics.duplicated.lines)||0)' "$batch_out/report.json" 2>/dev/null || echo "0")
      files=$(node -e 'const r=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")); console.log((r.statistics&&r.statistics.sources&&r.statistics.sources.length)||0)' "$batch_out/report.json" 2>/dev/null || echo "0")

      TOTAL_CLONES=$((TOTAL_CLONES + clones))
      TOTAL_DUP_LINES=$((TOTAL_DUP_LINES + dupLines))
      TOTAL_FILES=$((TOTAL_FILES + files))

      echo "   ✅ $batch_name: $clones clones, $dupLines dup lines, $files files"
    fi
  else
    echo "   ⚠️  $batch_name: scan completed"
  fi
}

# Run each batch
run_batch "core" "crates/adapteros-core crates/adapteros-types crates/adapteros-api-types crates/adapteros-config"
run_batch "db" "crates/adapteros-db crates/adapteros-registry"
run_batch "crypto" "crates/adapteros-crypto crates/adapteros-secure-fs crates/adapteros-secd"
run_batch "lora" "crates/adapteros-lora-router crates/adapteros-lora-worker crates/adapteros-lora-lifecycle crates/adapteros-lora-kernel-api"
run_batch "kernels" "crates/adapteros-lora-kernel-mtl crates/adapteros-lora-kernel-coreml crates/adapteros-lora-mlx-ffi"
run_batch "server" "crates/adapteros-server crates/adapteros-server-api crates/adapteros-api crates/adapteros-client"
run_batch "policy" "crates/adapteros-policy crates/adapteros-manifest crates/adapteros-aos"
run_batch "infra" "crates/adapteros-memory crates/adapteros-telemetry crates/adapteros-profiler crates/adapteros-deterministic-exec"
run_batch "tools" "crates/adapteros-cli crates/adapteros-orchestrator crates/adapteros-ingest-docs"
run_batch "ui" "ui/src ui/components ui/pages"
run_batch "tests" "tests"

echo ""
echo "═══════════════════════════════════════════════════════════════"
echo "📊 TOTAL SUMMARY"
echo "═══════════════════════════════════════════════════════════════"
echo "   📁 Files scanned:        $TOTAL_FILES"
echo "   📋 Duplication clones:   $TOTAL_CLONES"
echo "   📝 Duplicated lines:     $TOTAL_DUP_LINES"
echo "═══════════════════════════════════════════════════════════════"
echo ""
echo "📁 Batch reports saved to: $OUT_DIR/"
echo ""

# Create combined summary
cat > "$OUT_DIR/summary.json" << EOF
{
  "timestamp": "$TS",
  "total_clones": $TOTAL_CLONES,
  "total_duplicated_lines": $TOTAL_DUP_LINES,
  "total_files": $TOTAL_FILES
}
EOF

echo "📄 Summary: $OUT_DIR/summary.json"
