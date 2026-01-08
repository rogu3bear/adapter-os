#!/usr/bin/env bash
set -Eeuo pipefail

# AdapterOS UI Build Smoke Checks
# Static validation that component contracts are maintained.
# Run as part of CI to catch UI regressions early.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

UI_CRATE="crates/adapteros-ui"

RED='\033[0;31m'
GREEN='\033[0;32m'
NC='\033[0m'

ok() { printf "${GREEN}✓${NC} %s\n" "$*"; }
err() { printf "${RED}✗${NC} %s\n" "$*" >&2; }

FAILED=0

# -----------------------------------------------------------------------------
# 1. WASM compilation check
# -----------------------------------------------------------------------------
echo "=== WASM Compilation Check ==="
if cargo check -p adapteros-ui --target wasm32-unknown-unknown 2>/dev/null; then
  ok "WASM compilation succeeds"
else
  err "WASM compilation failed"
  FAILED=1
fi

# -----------------------------------------------------------------------------
# 2. Component exports check
# -----------------------------------------------------------------------------
echo ""
echo "=== Component Exports Check ==="

COMPONENTS_MOD="$UI_CRATE/src/components/mod.rs"

# Check SplitPanel is exported
if grep -q "pub use split_panel::SplitPanel" "$COMPONENTS_MOD" 2>/dev/null || \
   grep -q "SplitPanel" "$COMPONENTS_MOD" 2>/dev/null; then
  ok "SplitPanel is exported from components"
else
  err "SplitPanel not found in component exports"
  FAILED=1
fi

# Check VirtualList is exported
if grep -q "pub use virtual_list" "$COMPONENTS_MOD" 2>/dev/null || \
   grep -q "VirtualList" "$COMPONENTS_MOD" 2>/dev/null; then
  ok "VirtualList is exported from components"
else
  err "VirtualList not found in component exports"
  FAILED=1
fi

# -----------------------------------------------------------------------------
# 3. SplitPanel usage check (standard pages)
# -----------------------------------------------------------------------------
echo ""
echo "=== SplitPanel Usage Check ==="

PAGES_DIR="$UI_CRATE/src/pages"

# Training should use SplitPanel
if grep -q "SplitPanel" "$PAGES_DIR/training/mod.rs" 2>/dev/null || \
   grep -q "SplitPanel" "$PAGES_DIR/training.rs" 2>/dev/null; then
  ok "Training page uses SplitPanel"
else
  err "Training page does not use SplitPanel"
  FAILED=1
fi

# Models should use SplitPanel
if grep -q "SplitPanel" "$PAGES_DIR/models.rs" 2>/dev/null; then
  ok "Models page uses SplitPanel"
else
  err "Models page does not use SplitPanel"
  FAILED=1
fi

# Policies should use SplitPanel
if grep -q "SplitPanel" "$PAGES_DIR/policies.rs" 2>/dev/null; then
  ok "Policies page uses SplitPanel"
else
  err "Policies page does not use SplitPanel"
  FAILED=1
fi

# -----------------------------------------------------------------------------
# 4. Performance pattern check (no repeated .get() in render)
# -----------------------------------------------------------------------------
echo ""
echo "=== Performance Pattern Check ==="

# Check for Memo usage in dashboard (chart data should be memoized)
if grep -q "Memo::new" "$PAGES_DIR/dashboard.rs" 2>/dev/null; then
  ok "Dashboard uses Memo for derived data"
else
  err "Dashboard missing Memo usage for derived data"
  FAILED=1
fi

# -----------------------------------------------------------------------------
# Summary
# -----------------------------------------------------------------------------
echo ""
if [[ "$FAILED" -gt 0 ]]; then
  echo "=== FAILED: UI build smoke checks did not pass ==="
  exit 1
else
  echo "=== PASSED: All UI build smoke checks passed ==="
  exit 0
fi
