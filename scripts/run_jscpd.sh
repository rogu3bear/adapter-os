#!/usr/bin/env bash
set -euo pipefail

# Run jscpd across the repo and store timestamped reports under var/reports/jscpd
# Usage:
#   bash scripts/run_jscpd.sh            # local run (logs path + summary)
#   bash scripts/run_jscpd.sh --ci       # CI-safe run (no prompts)
#   bash scripts/run_jscpd.sh --batched  # batched scan for lower peak memory
#   JSCPD_MIN_TOKENS=80 bash scripts/run_jscpd.sh  # override min tokens

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")"/.. && pwd)"
OUT_BASE_DIR="$ROOT_DIR/var/reports/jscpd"
TS="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="$OUT_BASE_DIR/$TS"
CONFIG="$ROOT_DIR/configs/jscpd.config.json"

mkdir -p "$OUT_DIR"

CI_MODE=false
BATCHED_MODE=false
PASSTHROUGH_ARGS=()

print_usage() {
  cat <<'EOF'
Usage: bash scripts/run_jscpd.sh [--ci] [--batched] [extra-jscpd-args...]

Options:
  --ci       CI-safe mode (silent output)
  --batched  Run in batches to reduce peak Node.js memory use
  --help     Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --ci)
      CI_MODE=true
      ;;
    --batched)
      BATCHED_MODE=true
      ;;
    --help|-h)
      print_usage
      exit 0
      ;;
    *)
      PASSTHROUGH_ARGS+=("$1")
      ;;
  esac
  shift
done

# Allow overriding minTokens without editing config
if [[ -n "${JSCPD_MIN_TOKENS:-}" ]]; then
  # Build a temp config merging override
  TMP_CFG="$OUT_DIR/jscpd.config.json"
  node -e '
    const fs = require("fs");
    const cfg = JSON.parse(fs.readFileSync(process.argv[1], "utf8"));
    cfg.minTokens = parseInt(process.env.JSCPD_MIN_TOKENS, 10) || cfg.minTokens;
    fs.writeFileSync(process.argv[2], JSON.stringify(cfg, null, 2));
  ' "$CONFIG" "$TMP_CFG"
  CONFIG="$TMP_CFG"
fi

# Use npx to avoid adding permanent deps
NPX_CMD=(npx --yes jscpd)

run_standard() {
  ARGS=(
    "--config" "$CONFIG"
    "--output" "$OUT_DIR"
  )

  if [[ "$CI_MODE" == "true" ]]; then
    # CI mode: use silent mode for clean logs
    ARGS+=("--silent")
  else
    # Local mode: show progress and clear messages
    echo "🔍 [jscpd] Scanning repository for code duplication..."
    echo "        This analyzes Rust, TypeScript, Swift, and other code files."
    echo "        Progress will be shown below. Large repos may take 30-60 seconds."
    echo ""
  fi

  if [[ ${#PASSTHROUGH_ARGS[@]} -gt 0 ]]; then
    ARGS+=("${PASSTHROUGH_ARGS[@]}")
  fi

  "${NPX_CMD[@]}" "${ARGS[@]}" "$ROOT_DIR"

  JSON_REPORT=""
  MD_REPORT=""
  for cand in "$OUT_DIR"/jscpd-report.json "$OUT_DIR"/report.json "$OUT_DIR"/*report.json; do
    if [[ -f "$cand" ]]; then
      JSON_REPORT="$cand"
      break
    fi
  done
  for cand in "$OUT_DIR"/jscpd-report.md "$OUT_DIR"/report.md "$OUT_DIR"/*report.md; do
    if [[ -f "$cand" ]]; then
      MD_REPORT="$cand"
      break
    fi
  done

  HTML_REPORT=""
  if [[ -f "$OUT_DIR/html/index.html" ]]; then
    HTML_REPORT="$OUT_DIR/html/index.html"
  elif [[ -f "$OUT_DIR/index.html" ]]; then
    HTML_REPORT="$OUT_DIR/index.html"
  fi

  if [[ -n "$JSON_REPORT" ]] && command -v node >/dev/null 2>&1; then
    # Extract summary statistics
    node - "$JSON_REPORT" <<'NODE'
const fs = require('fs');
const p = process.argv[1];
try {
  const r = JSON.parse(fs.readFileSync(p, 'utf8'));
  const stat = r.statistics || {};
  const total = stat.total || {};
  const dupPct = total.percentage ?? stat.duplication?.percentage ?? stat.duplicated?.percentage ?? 0;
  const clones = total.clones ?? (r.clones && r.clones.length) ?? 0;
  const dupLines = total.duplicatedLines ?? stat.duplicated?.lines ?? 0;
  const files = total.sources ?? (stat.sources && stat.sources.length) ?? 0;
  const tokens = total.tokens ?? (stat.tokens && stat.tokens.length) ?? 0;

  console.log(`✅ [jscpd] Scan complete!`);
  console.log(`   📊 Files scanned: ${files.toLocaleString()}`);
  console.log(`   🔍 Code tokens analyzed: ${tokens.toLocaleString()}`);
  console.log(`   📋 Duplication clones found: ${clones.toLocaleString()}`);
  console.log(`   📝 Duplicated lines: ${dupLines.toLocaleString()}`);
  console.log(`   📈 Duplication percentage: ${dupPct}%`);

  if (clones > 0) {
    console.log(`   ⚠️  Review the HTML report for detailed clone locations`);
  } else {
    console.log(`   🎉 No significant code duplication detected!`);
  }
} catch (e) {
  console.log('✅ [jscpd] Scan completed successfully');
}
NODE
  else
    echo "✅ [jscpd] Scan completed successfully"
  fi

  if [[ "$CI_MODE" != "true" ]]; then
    echo ""
    echo "📁 Reports generated:"
    if [[ -n "$JSON_REPORT" ]]; then
      echo "   📄 JSON:     $JSON_REPORT"
    fi
    if [[ -n "$MD_REPORT" ]]; then
      echo "   📝 Markdown: $MD_REPORT"
    fi
    if [[ -n "$HTML_REPORT" ]]; then
      echo "   🌐 HTML:     $HTML_REPORT"
    fi
    echo ""
    echo "💡 Tip: Open the HTML report in your browser for interactive clone exploration"
  fi
}

run_batched() {
  if [[ "${NODE_OPTIONS:-}" == *"--max-old-space-size"* ]]; then
    :
  elif [[ -n "${NODE_OPTIONS:-}" ]]; then
    export NODE_OPTIONS="--max-old-space-size=4096 ${NODE_OPTIONS}"
  else
    export NODE_OPTIONS="--max-old-space-size=4096"
  fi

  local ci_arg=""
  if [[ "$CI_MODE" == "true" ]]; then
    ci_arg="--silent"
  fi

  local total_clones=0
  local total_dup_lines=0
  local total_files=0

  echo "🔍 [jscpd] Scanning repository for code duplication (batched mode)..."
  echo ""

  run_batch() {
    local batch_name="$1"
    shift
    local batch_out="$OUT_DIR/batch-$batch_name"
    mkdir -p "$batch_out"

    local paths=()
    local dir
    for dir in "$@"; do
      local full_path="$ROOT_DIR/$dir"
      if [[ -d "$full_path" ]]; then
        paths+=("$full_path")
      fi
    done

    if [[ ${#paths[@]} -eq 0 ]]; then
      echo "⏭️  Skipping $batch_name (no directories found)"
      return
    fi

    echo "📦 Scanning batch: $batch_name..."

    local run_args=(--config "$CONFIG" --output "$batch_out")
    if [[ -n "$ci_arg" ]]; then
      run_args+=("$ci_arg")
    fi
    if [[ ${#PASSTHROUGH_ARGS[@]} -gt 0 ]]; then
      run_args+=("${PASSTHROUGH_ARGS[@]}")
    fi

    if "${NPX_CMD[@]}" "${run_args[@]}" "${paths[@]}" 2>/dev/null; then
      local json_report
      json_report="$(ls -1 "$batch_out"/*report.json 2>/dev/null | head -n1 || true)"
      if [[ -n "$json_report" ]]; then
        local clones dup_lines files
        clones=$(node -e 'const r=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")); const t=(r.statistics&&r.statistics.total)||{}; console.log(t.clones ?? ((r.clones&&r.clones.length)||0))' "$json_report" 2>/dev/null || echo "0")
        dup_lines=$(node -e 'const r=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")); const t=(r.statistics&&r.statistics.total)||{}; console.log(t.duplicatedLines ?? 0)' "$json_report" 2>/dev/null || echo "0")
        files=$(node -e 'const r=JSON.parse(require("fs").readFileSync(process.argv[1],"utf8")); const t=(r.statistics&&r.statistics.total)||{}; console.log(t.sources ?? 0)' "$json_report" 2>/dev/null || echo "0")

        total_clones=$((total_clones + clones))
        total_dup_lines=$((total_dup_lines + dup_lines))
        total_files=$((total_files + files))

        echo "   ✅ $batch_name: $clones clones, $dup_lines dup lines, $files files"
      fi
    else
      echo "   ⚠️  $batch_name: scan completed"
    fi
  }

  run_batch "core" "crates/adapteros-core" "crates/adapteros-types" "crates/adapteros-api-types" "crates/adapteros-config"
  run_batch "db" "crates/adapteros-db" "crates/adapteros-registry"
  run_batch "crypto" "crates/adapteros-crypto" "crates/adapteros-secure-fs" "crates/adapteros-secd"
  run_batch "lora" "crates/adapteros-lora-router" "crates/adapteros-lora-worker" "crates/adapteros-lora-lifecycle" "crates/adapteros-lora-kernel-api"
  run_batch "kernels" "crates/adapteros-lora-kernel-mtl" "crates/adapteros-lora-kernel-coreml" "crates/adapteros-lora-mlx-ffi"
  run_batch "server" "crates/adapteros-server" "crates/adapteros-server-api" "crates/adapteros-api" "crates/adapteros-client"
  run_batch "policy" "crates/adapteros-policy" "crates/adapteros-manifest" "crates/adapteros-aos"
  run_batch "infra" "crates/adapteros-memory" "crates/adapteros-telemetry" "crates/adapteros-profiler" "crates/adapteros-deterministic-exec"
  run_batch "tools" "crates/adapteros-cli" "crates/adapteros-orchestrator" "crates/adapteros-ingest-docs"
  run_batch "ui" "crates/adapteros-ui/src"
  run_batch "tests" "tests"

  echo ""
  echo "═══════════════════════════════════════════════════════════════"
  echo "📊 TOTAL SUMMARY"
  echo "═══════════════════════════════════════════════════════════════"
  echo "   📁 Files scanned:        $total_files"
  echo "   📋 Duplication clones:   $total_clones"
  echo "   📝 Duplicated lines:     $total_dup_lines"
  echo "═══════════════════════════════════════════════════════════════"
  echo ""
  echo "📁 Batch reports saved to: $OUT_DIR/"
  echo ""

  cat > "$OUT_DIR/summary.json" <<EOF
{
  "timestamp": "$TS",
  "total_clones": $total_clones,
  "total_duplicated_lines": $total_dup_lines,
  "total_files": $total_files
}
EOF

  echo "📄 Summary: $OUT_DIR/summary.json"
}

if [[ "$BATCHED_MODE" == "true" ]]; then
  run_batched
else
  run_standard
fi
