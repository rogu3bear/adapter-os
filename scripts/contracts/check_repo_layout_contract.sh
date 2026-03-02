#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

fail() {
  echo "FAIL: $1" >&2
  exit 1
}

require_match() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  rg -n --quiet -- "$pattern" "$file" || fail "$message ($file)"
}

require_no_match() {
  local pattern="$1"
  local file="$2"
  local message="$3"
  if rg -n --quiet -- "$pattern" "$file"; then
    fail "$message ($file)"
  fi
}

require_absent_path() {
  local path="$1"
  local message="$2"
  [[ ! -e "$path" ]] || fail "$message ($path)"
}

# Canonical runtime path language in policy anchors.
require_match 'Runtime data: `var/` only' CONTRIBUTING.md \
  "Path hygiene contract in CONTRIBUTING.md must use canonical var/ form"
require_match 'Canonical runtime root.*`var/`' docs/SECURITY.md \
  "SECURITY.md must document canonical var/ runtime root"
require_match 'Runtime data: `var/` only' .cursor/rules/adapteros.mdc \
  ".cursor rules must enforce canonical var/ runtime root"

# Generated output directories must remain ignored.
for pattern in "**/target/" "output/" "reports/" "/test-results" "**/test-results/" "**/var/"; do
  rg -n -F --quiet "$pattern" .gitignore \
    || fail ".gitignore missing required generated-artifact ignore pattern: $pattern"
done

# Golden baseline storage should not drift into var/golden_runs in production code.
golden_split_hits="$(
  rg -n --no-heading -S 'var/golden_runs' crates scripts tests \
    --glob '!scripts/contracts/check_repo_layout_contract.sh' \
    --glob '!**/*_test.rs' \
    || true
)"
if [[ -n "$golden_split_hits" ]]; then
  echo "$golden_split_hits" >&2
  fail "Found split-path golden baseline usage (var/golden_runs). Use golden_runs/ consistently."
fi

# Keep canonical golden root usage present in CLI and API.
require_match 'Path::new\("golden_runs"\)' crates/adapteros-cli/src/commands/golden.rs \
  "CLI golden commands must anchor to golden_runs/"
require_match 'Path::new\("golden_runs"\)' crates/adapteros-server-api/src/handlers/golden.rs \
  "API golden handlers must anchor to golden_runs/"

# Legacy codegen quickstart references should not reappear.
if [[ -f ".github/API_TYPES_QUICKSTART.md" ]]; then
  require_no_match 'scripts/generate-sdks\.sh' .github/API_TYPES_QUICKSTART.md \
    "API types quickstart must not reference removed scripts/generate-sdks.sh"
  require_no_match '`codegen/`' .github/API_TYPES_QUICKSTART.md \
    "API types quickstart must not treat root codegen/ as active config source"
  require_match 'target/codegen/openapi\.json' .github/API_TYPES_QUICKSTART.md \
    "API types quickstart must point to target/codegen/openapi.json"
fi

# Legacy root directories are retired; docs are tracked under docs/legacy/.
for legacy_root in baselines codegen commands skills etc; do
  require_absent_path "$legacy_root" \
    "Legacy root directory must remain retired; use docs/legacy/ instead"
done

require_match 'Status: legacy docs-only directory\.' docs/legacy/codegen/README.md \
  "docs/legacy/codegen/README.md must mark codegen as legacy docs-only"
require_match 'target/codegen/' docs/legacy/codegen/README.md \
  "docs/legacy/codegen/README.md must point to target/codegen/ as active generated path"

require_match 'Status: legacy docs-only directory\.' docs/legacy/commands/README.md \
  "docs/legacy/commands/README.md must mark commands as legacy docs-only"
require_match 'crates/adapteros-cli/src/commands/' docs/legacy/commands/README.md \
  "docs/legacy/commands/README.md must point to canonical CLI command module path"
require_match '\.agents/workflows/' docs/legacy/commands/README.md \
  "docs/legacy/commands/README.md must point to .agents/workflows/ (not .agent/workflows/)"

require_match 'Status: legacy docs-only directory\.' docs/legacy/skills/README.md \
  "docs/legacy/skills/README.md must mark skills as legacy docs-only"
require_match '\$CODEX_HOME/skills/' docs/legacy/skills/README.md \
  "docs/legacy/skills/README.md must point to \$CODEX_HOME/skills/ as canonical runtime"

require_match 'Status: legacy docs-only directory\.' docs/legacy/baselines/README.md \
  "docs/legacy/baselines/README.md must mark baselines as legacy docs-only"
require_match 'golden_runs/baselines' docs/legacy/baselines/README.md \
  "docs/legacy/baselines/README.md must point to golden_runs/baselines as canonical golden baseline path"
require_match 'metal/baselines/kernel_hash\.txt' docs/legacy/baselines/README.md \
  "docs/legacy/baselines/README.md must point to metal/baselines/kernel_hash.txt for kernel hash baselines"

require_match 'Status: legacy docs-only directory\.' docs/legacy/etc/README.md \
  "docs/legacy/etc/README.md must mark etc as legacy docs-only"
require_match 'configs/cp\.toml' docs/legacy/etc/README.md \
  "docs/legacy/etc/README.md must point to configs/cp.toml as canonical control-plane config path"
require_match 'deploy/supervisor\.yaml' docs/legacy/etc/README.md \
  "docs/legacy/etc/README.md must point to deploy/supervisor.yaml as canonical supervisor config path"

# Keep deprecated paths from reappearing outside this contract.
legacy_ref_hits="$(
  rg -n --no-heading -S 'scripts/generate-sdks\.sh|codegen/python\.json|skills/codebase-investigation' \
    crates scripts tests docs .github \
    --glob '!scripts/contracts/check_repo_layout_contract.sh' \
    --glob '!docs/legacy/**' \
    || true
)"
if [[ -n "$legacy_ref_hits" ]]; then
  echo "$legacy_ref_hits" >&2
  fail "Found legacy dead-area references outside allowlist"
fi

# Script-generated reports should land in var/reports, not repo-root reports/.
if [[ -f "scripts/audit_api_endpoints.sh" ]]; then
  require_match 'REPORT_DIR="\$PROJECT_ROOT/var/reports/' scripts/audit_api_endpoints.sh \
    "audit_api_endpoints.sh must write reports under var/reports/"
fi

if [[ -f "scripts/ui_util_audit.py" ]]; then
  require_match 'REPORTS_DIR = Path\(__file__\)\.parent\.parent / "var" / "reports"' scripts/ui_util_audit.py \
    "ui_util_audit.py must write reports under var/reports/"
fi

if [[ -f "scripts/trim_allowlist.py" ]]; then
  require_match 'REPORTS_DIR = Path\(__file__\)\.parent\.parent / "var" / "reports" / "ui_util"' scripts/trim_allowlist.py \
    "trim_allowlist.py must consume reports from var/reports/ui_util/"
fi

if [[ -f "scripts/coreml/convert_to_coreml.py" ]]; then
  require_match '--output var/output/synthesis_model\.mlpackage' scripts/coreml/convert_to_coreml.py \
    "convert_to_coreml.py usage docs must point to var/output/"
  require_match 'default="var/output/synthesis_model\.mlpackage"' scripts/coreml/convert_to_coreml.py \
    "convert_to_coreml.py default output path must be under var/output/"
fi

# Adapter verifier should not reference retired mplora crate prefixes.
if [[ -d "xtask/src/verify_adapters" ]]; then
  verify_mplora_hits="$(
    rg -n --no-heading -S 'mplora-' xtask/src/verify_adapters || true
  )"
  if [[ -n "$verify_mplora_hits" ]]; then
    echo "$verify_mplora_hits" >&2
    fail "verify_adapters contains stale mplora-* path references"
  fi
fi

echo "=== Repo Layout Contract Check: PASSED ==="
