#!/usr/bin/env bash
# check_css_token_consistency.sh
# Ensures all transition durations in component CSS use design tokens
# (--duration-fast, --duration-normal, --duration-slow) rather than
# hardcoded millisecond values.
#
# Allowlist: animation-duration and @keyframes can use raw ms values.
# Scope: crates/adapteros-ui/dist/**/*.css

set -euo pipefail

CSS_DIR="crates/adapteros-ui/dist"
VIOLATIONS=0

echo "=== CSS Token Consistency Check ==="
echo ""

# Pattern: transition property with hardcoded ms value
# Matches: transition: foo 150ms, transition-duration: 200ms
# Ignores: animation-duration, @keyframes, comments
while IFS= read -r line; do
  file="${line%%:*}"
  rest="${line#*:}"
  lineno="${rest%%:*}"
  content="${rest#*:}"

  # Skip animation-related lines (allowed to use raw ms)
  if echo "$content" | grep -qE '^\s*(animation|animation-duration)'; then
    continue
  fi

  echo "  VIOLATION: $file:$lineno"
  echo "    $content"
  VIOLATIONS=$((VIOLATIONS + 1))
done < <(grep -rnE 'transition[^:]*:\s*[^;]*\b(100|120|150|200|250|300)ms' "$CSS_DIR" \
  --include='*.css' || true)

echo ""
if [ "$VIOLATIONS" -eq 0 ]; then
  echo "PASS: All transition durations use design tokens."
  exit 0
else
  echo "FAIL: $VIOLATIONS transition(s) use hardcoded ms values."
  echo "Fix: Replace with var(--duration-fast|normal|slow)."
  exit 1
fi
