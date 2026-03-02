#!/usr/bin/env bash
# bundle-css.sh — Concatenate component CSS sub-files into a single bundle
# to eliminate the @import waterfall in components.css.
#
# Idempotent: safe to run multiple times; always overwrites the output file.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
COMPONENTS_DIR="$REPO_ROOT/crates/adapteros-ui/dist/components"
OUTPUT="$REPO_ROOT/crates/adapteros-ui/dist/components-bundle.css"

# Ordered list matching the original @import sequence in components.css
FILES=(
    core.css
    utilities.css
    layout.css
    overlays.css
    pages.css
    hud.css
)

# Verify all source files exist before writing output
for f in "${FILES[@]}"; do
    if [[ ! -f "$COMPONENTS_DIR/$f" ]]; then
        echo "bundle-css.sh: ERROR — missing $COMPONENTS_DIR/$f" >&2
        exit 1
    fi
done

# Build the bundle
{
    echo "/* AdapterOS Component Styles — bundled by scripts/bundle-css.sh"
    echo " * Do not edit directly; edit the source files in dist/components/ instead."
    echo " */"
    echo ""
    for f in "${FILES[@]}"; do
        echo "/* === $f === */"
        cat "$COMPONENTS_DIR/$f"
        echo ""
    done
} > "$OUTPUT"

echo "bundle-css.sh: wrote $OUTPUT ($(wc -c < "$OUTPUT" | tr -d ' ') bytes)"
