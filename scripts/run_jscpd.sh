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
  "--silent"
)

if [[ "${1:-}" == "--ci" ]]; then
  # CI mode: no interactive output; console report is still captured by runner logs
  :
fi

echo "[jscpd] Scanning repository for duplication..."
"${NPX_CMD[@]}" "${ARGS[@]}" "$ROOT_DIR" >/dev/null

JSON_REPORT="$OUT_DIR/report.json"
MD_REPORT="$OUT_DIR/report.md"

if [[ -f "$JSON_REPORT" ]] && command -v node >/dev/null 2>&1; then
  # Brief summary to stdout (if Node is available)
  node - <<'NODE'
const fs = require('fs');
const p = process.argv[1];
try {
  const r = JSON.parse(fs.readFileSync(p, 'utf8'));
  const stat = r.statistics || {};
  const dup = (stat.duplicated && stat.duplicated.percentage) || (stat.duplication && stat.duplication.percentage) || 0;
  const clones = (r.clones && r.clones.length) || 0;
  console.log(`[jscpd] Duplicated percentage: ${dup}% | Clones: ${clones}`);
} catch (e) {
  console.log('[jscpd] Completed. (summary unavailable)');
}
NODE
else
  echo "[jscpd] Completed. Report written to: $OUT_DIR"
fi

echo "[jscpd] Reports:"
echo "  - JSON:     $JSON_REPORT"
echo "  - Markdown: $MD_REPORT"
echo "  - HTML dir: $OUT_DIR"
