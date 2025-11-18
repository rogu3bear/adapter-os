#!/usr/bin/env bash
set -euo pipefail

# Run jscpd across the repo and store timestamped reports under var/reports/jscpd
# Usage:
#   bash scripts/run_jscpd.sh            # local run (logs path + summary)
#   bash scripts/run_jscpd.sh --ci       # CI-safe run (no prompts)
#   JSCPD_MIN_TOKENS=80 bash scripts/run_jscpd.sh  # override min tokens

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")"/.. && pwd)"
OUT_BASE_DIR="$ROOT_DIR/var/reports/jscpd"
TS="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="$OUT_BASE_DIR/$TS"
CONFIG="$ROOT_DIR/configs/jscpd.config.json"

mkdir -p "$OUT_DIR"

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

ARGS=(
  "--config" "$CONFIG"
  "--output" "$OUT_DIR"
)

if [[ "${1:-}" == "--ci" ]]; then
  # CI mode: use silent mode for clean logs
  ARGS+=("--silent")
else
  # Local mode: show progress and clear messages
  echo "🔍 [jscpd] Scanning repository for code duplication..."
  echo "        This analyzes Rust, TypeScript, Swift, and other code files."
  echo "        Progress will be shown below. Large repos may take 30-60 seconds."
  echo ""
fi

"${NPX_CMD[@]}" "${ARGS[@]}" "$ROOT_DIR"

JSON_REPORT="$OUT_DIR/report.json"
MD_REPORT="$OUT_DIR/report.md"

if [[ -f "$JSON_REPORT" ]] && command -v node >/dev/null 2>&1; then
  # Extract summary statistics
  node - <<'NODE'
const fs = require('fs');
const p = process.argv[1];
try {
  const r = JSON.parse(fs.readFileSync(p, 'utf8'));
  const stat = r.statistics || {};
  const dup = (stat.duplicated && stat.duplicated.percentage) || (stat.duplication && stat.duplication.percentage) || 0;
  const clones = (r.clones && r.clones.length) || 0;
  const files = (stat.sources && stat.sources.length) || 0;
  const tokens = (stat.tokens && stat.tokens.length) || 0;

  console.log(`✅ [jscpd] Scan complete!`);
  console.log(`   📊 Files scanned: ${files.toLocaleString()}`);
  console.log(`   🔍 Code tokens analyzed: ${tokens.toLocaleString()}`);
  console.log(`   📋 Duplication clones found: ${clones}`);
  console.log(`   📈 Duplication percentage: ${dup}%`);

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

if [[ "${1:-}" != "--ci" ]]; then
  echo ""
  echo "📁 Reports generated:"
  echo "   📄 JSON:     $JSON_REPORT"
  echo "   📝 Markdown: $MD_REPORT"
  echo "   🌐 HTML:     $OUT_DIR/index.html"
  echo ""
  echo "💡 Tip: Open the HTML report in your browser for interactive clone exploration"
fi
