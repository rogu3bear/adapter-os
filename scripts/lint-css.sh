#!/usr/bin/env bash
# lint-css.sh — Dead-CSS detection for adapterOS UI
#
# Usage:
#   ./scripts/lint-css.sh                      # run with defaults
#   DEAD_CSS_THRESHOLD=100 ./scripts/lint-css.sh  # override threshold
#
# Scans CSS class selectors in dist/ and cross-references with class usage
# in .rs source files and index.html. Reports dead (unused) selectors and
# missing (referenced but undefined) classes.
#
# Dependencies: grep, sed, sort, comm, tr, wc (POSIX)

set -euo pipefail

# ─── Configuration ───────────────────────────────────────────────────────────

DEAD_CSS_THRESHOLD="${DEAD_CSS_THRESHOLD:-200}"

# Project root (script lives in scripts/)
ROOT="$(cd "$(dirname "$0")/.." && pwd)"

CSS_DIR="$ROOT/crates/adapteros-ui/dist"
SRC_DIR="$ROOT/crates/adapteros-ui/src"
INDEX_HTML="$ROOT/crates/adapteros-ui/index.html"

# Skip list: design-system utility classes that may not all be referenced yet.
# These are intentionally defined as part of the API surface.
SKIP_CLASSES="
flex
flex-col
flex-row
flex-1
flex-wrap
grid
hidden
block
inline
inline-flex
inline-block
items-center
items-start
items-end
justify-center
justify-between
justify-end
justify-start
gap-1
gap-2
gap-3
gap-4
text-xs
text-sm
text-base
text-lg
text-xl
text-2xl
font-mono
font-medium
font-semibold
font-bold
w-full
h-full
min-h-screen
overflow-hidden
overflow-auto
overflow-x-auto
relative
absolute
fixed
sticky
inset-0
truncate
sr-only
pointer-events-none
cursor-pointer
select-none
dark
light
theme-glass
"

# ─── Temporary files ─────────────────────────────────────────────────────────

TMPDIR_LINT="${TMPDIR:-/tmp}/lint-css-$$"
mkdir -p "$TMPDIR_LINT"
trap 'rm -rf "$TMPDIR_LINT"' EXIT

CSS_CLASSES_RAW="$TMPDIR_LINT/css_classes_raw.txt"
CSS_CLASSES="$TMPDIR_LINT/css_classes.txt"
USED_CLASSES_RAW="$TMPDIR_LINT/used_classes_raw.txt"
USED_CLASSES="$TMPDIR_LINT/used_classes.txt"
SKIP_FILE="$TMPDIR_LINT/skip.txt"
DEAD_FILE="$TMPDIR_LINT/dead.txt"
MISSING_FILE="$TMPDIR_LINT/missing.txt"

# ─── Build skip list ─────────────────────────────────────────────────────────

echo "$SKIP_CLASSES" | sed '/^$/d' | sed 's/^[[:space:]]*//' | sort -u > "$SKIP_FILE"

# ─── Step 1: Extract class selectors from CSS ────────────────────────────────

# Strategy:
#   1. Strip block comments
#   2. Remove @keyframes blocks (names are not class selectors)
#   3. Remove @media / @supports wrappers (but keep contents)
#   4. Extract lines containing '.' class selectors
#   5. Isolate each .class-name token
#   6. Exclude pseudo-selectors, custom properties, and attribute selectors

# Concatenate all CSS files, strip comments, strip @keyframes blocks
css_content() {
    # Find all .css files under dist/
    find "$CSS_DIR" -name '*.css' -type f -print0 \
        | xargs -0 cat
}

css_content \
    | sed 's|/\*[^*]*\*\+\([^/*][^*]*\*\+\)*/||g' \
    | sed '/@keyframes/,/^}/d' \
    | grep '\.' \
    | sed 's/[{}]/ /g' \
    | tr ',' '\n' \
    | grep -oE '\.[a-zA-Z_-][a-zA-Z0-9_-]*' \
    | sed 's/^\.//' \
    | grep -v '^-' \
    | sort -u \
    > "$CSS_CLASSES_RAW"

# Remove skip-list entries
comm -23 "$CSS_CLASSES_RAW" "$SKIP_FILE" > "$CSS_CLASSES"

# ─── Step 2: Extract class references from source ────────────────────────────

# 2a. From .rs files: class="..." values, split on whitespace
#     Also handles class=format!("...") by extracting the string literal part
if [ -d "$SRC_DIR" ]; then
    # Extract class="..." attribute values
    grep -rhoE 'class="[^"]*"' "$SRC_DIR" --include='*.rs' 2>/dev/null \
        | sed 's/^class="//;s/"$//' \
        | tr ' ' '\n' \
        >> "$USED_CLASSES_RAW" || true

    # Extract class=format!("...") — grab the string literal inside
    grep -rhoE 'class=format!\("[^"]*"' "$SRC_DIR" --include='*.rs' 2>/dev/null \
        | sed 's/^class=format!("//;s/"$//' \
        | tr ' ' '\n' \
        | sed 's/{[^}]*}//g' \
        >> "$USED_CLASSES_RAW" || true

    # Extract class:name reactive bindings (Leptos: class:foo-bar=...)
    grep -rhoE 'class:[a-zA-Z_-][a-zA-Z0-9_-]*' "$SRC_DIR" --include='*.rs' 2>/dev/null \
        | sed 's/^class://' \
        >> "$USED_CLASSES_RAW" || true

    # Extract bare string literals that look like CSS class names (catches dynamic
    # class bindings like `"hud-card--ready"` assigned to variables or match arms)
    grep -rhoE '"[a-zA-Z][a-zA-Z0-9_ -]*--?[a-zA-Z0-9_-]+"' "$SRC_DIR" --include='*.rs' 2>/dev/null \
        | sed 's/^"//;s/"$//' \
        | tr ' ' '\n' \
        >> "$USED_CLASSES_RAW" || true
fi

# 2b. From index.html: class="..." values
if [ -f "$INDEX_HTML" ]; then
    grep -oE 'class="[^"]*"' "$INDEX_HTML" 2>/dev/null \
        | sed 's/^class="//;s/"$//' \
        | tr ' ' '\n' \
        >> "$USED_CLASSES_RAW" || true

    # Also class names set via classList.add('...')
    grep -oE "classList\.add\('[^']*'\)" "$INDEX_HTML" 2>/dev/null \
        | sed "s/classList\.add('//;s/')$//" \
        | tr ' ' '\n' \
        >> "$USED_CLASSES_RAW" || true

    # className = '...' assignments
    grep -oE "className = '[^']*'" "$INDEX_HTML" 2>/dev/null \
        | sed "s/className = '//;s/'$//" \
        | tr ' ' '\n' \
        >> "$USED_CLASSES_RAW" || true
fi

# Clean up: remove empty lines, format placeholders, and sort unique
sed '/^$/d' "$USED_CLASSES_RAW" \
    | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' \
    | grep -E '^[a-zA-Z_-][a-zA-Z0-9_./:-]*$' \
    | sort -u \
    > "$USED_CLASSES"

# ─── Step 3: Cross-reference ─────────────────────────────────────────────────

# Dead CSS: defined in CSS but not referenced in source
comm -23 "$CSS_CLASSES" "$USED_CLASSES" > "$DEAD_FILE"

# Missing CSS: referenced in source but not defined in CSS (also exclude skip list)
comm -23 "$USED_CLASSES" "$CSS_CLASSES_RAW" \
    | comm -23 - "$SKIP_FILE" \
    > "$MISSING_FILE"

# ─── Step 4: Report ──────────────────────────────────────────────────────────

DEAD_COUNT=$(wc -l < "$DEAD_FILE" | tr -d ' ')
MISSING_COUNT=$(wc -l < "$MISSING_FILE" | tr -d ' ')

echo "═══════════════════════════════════════════════════════════"
echo "  adapterOS CSS Lint Report"
echo "═══════════════════════════════════════════════════════════"
echo ""

if [ "$DEAD_COUNT" -gt 0 ]; then
    echo "── DEAD CSS ($DEAD_COUNT selectors defined but never referenced) ──"
    while IFS= read -r cls; do
        # Show which CSS file defines it
        file=$(grep -rl "\\.$cls[^a-zA-Z0-9_-]" "$CSS_DIR" --include='*.css' 2>/dev/null | head -1 || true)
        if [ -n "$file" ]; then
            file="${file#"$ROOT"/}"
        else
            file="(unknown)"
        fi
        printf "  %-40s  %s\n" "$cls" "$file"
    done < "$DEAD_FILE"
    echo ""
fi

if [ "$MISSING_COUNT" -gt 0 ]; then
    echo "── MISSING CSS ($MISSING_COUNT classes referenced but not defined) ──"
    while IFS= read -r cls; do
        # Show where it's referenced
        file=$(grep -rl "$cls" "$SRC_DIR" --include='*.rs' 2>/dev/null | head -1 || true)
        if [ -z "$file" ] && [ -f "$INDEX_HTML" ]; then
            file=$(grep -l "$cls" "$INDEX_HTML" 2>/dev/null || true)
        fi
        if [ -n "$file" ]; then
            file="${file#"$ROOT"/}"
        else
            file="(unknown)"
        fi
        printf "  %-40s  %s\n" "$cls" "$file"
    done < "$MISSING_FILE"
    echo ""
fi

echo "── Summary ──"
echo "  CSS selectors scanned:  $(wc -l < "$CSS_CLASSES_RAW" | tr -d ' ')"
echo "  Classes used in source: $(wc -l < "$USED_CLASSES" | tr -d ' ')"
echo "  Skip list entries:      $(wc -l < "$SKIP_FILE" | tr -d ' ')"
echo "  Dead CSS:               $DEAD_COUNT"
echo "  Missing CSS:            $MISSING_COUNT"
echo "  Threshold:              $DEAD_CSS_THRESHOLD"
echo ""

if [ "$DEAD_COUNT" -gt "$DEAD_CSS_THRESHOLD" ]; then
    echo "FAIL: Dead CSS count ($DEAD_COUNT) exceeds threshold ($DEAD_CSS_THRESHOLD)"
    exit 1
else
    echo "PASS: Dead CSS count ($DEAD_COUNT) within threshold ($DEAD_CSS_THRESHOLD)"
    exit 0
fi
