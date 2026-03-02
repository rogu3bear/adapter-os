#!/usr/bin/env bash
#
# Build UI WASM with CI-equivalent optimization
#
# Usage: ./scripts/build-ui.sh [--skip-opt]
#
# Requirements:
#   - trunk (cargo install trunk)
#   - wasm-opt (brew install binaryen OR apt install binaryen)
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
UI_DIR="$ROOT_DIR/crates/adapteros-ui"
STATIC_DIR="$ROOT_DIR/crates/adapteros-server/static"

# Build ID + asset versioning (local builds)
read_source_date_epoch() {
    if [[ -n "${SOURCE_DATE_EPOCH:-}" ]]; then
        echo "$SOURCE_DATE_EPOCH"
        return
    fi

    local cargo_config="$ROOT_DIR/.cargo/config.toml"
    if [[ -f "$cargo_config" ]]; then
        local epoch
        epoch=$(awk -F'"' '/SOURCE_DATE_EPOCH/ {print $2; exit}' "$cargo_config")
        if [[ -n "$epoch" ]]; then
            echo "$epoch"
            return
        fi
    fi
}

format_timestamp_compact() {
    local epoch="$1"
    if [[ -n "$epoch" ]]; then
        date -u -r "$epoch" +%Y%m%d%H%M%S 2>/dev/null || date -u -d "@$epoch" +%Y%m%d%H%M%S
    else
        date -u +%Y%m%d%H%M%S
    fi
}

format_timestamp_iso() {
    local epoch="$1"
    if [[ -n "$epoch" ]]; then
        date -u -r "$epoch" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u -d "@$epoch" +%Y-%m-%dT%H:%M:%SZ
    else
        date -u +%Y-%m-%dT%H:%M:%SZ
    fi
}

get_git_hash() {
    local desc=""
    if desc=$(git describe --tags --always --dirty=-dirty 2>/dev/null); then
        desc="$(echo "$desc" | tr -d '\n')"
        if [[ "$desc" =~ ^[0-9a-f]{40}$ ]]; then
            echo "${desc:0:7}"
        else
            echo "$desc"
        fi
        return
    fi

    if desc=$(git rev-parse --short=7 HEAD 2>/dev/null); then
        echo "$(echo "$desc" | tr -d '\n')"
        return
    fi

    echo "unknown"
}

sanitize_for_filename() {
    local raw="$1"
    echo "$raw" | tr -c 'A-Za-z0-9._-' '_' | tr -s '_'
}

escape_perl_replacement() {
    local raw="$1"
    printf '%s' "$raw" | sed -e 's/[\\/&$]/\\&/g'
}

get_build_id() {
    if [[ -n "${AOS_BUILD_ID:-}" ]]; then
        echo "$AOS_BUILD_ID"
        return
    fi

    local build_id_file="$ROOT_DIR/target/build_id.txt"
    if [[ -f "$build_id_file" ]]; then
        local from_file
        from_file="$(cat "$build_id_file" 2>/dev/null | tr -d '\n')"
        if [[ -n "$from_file" ]]; then
            echo "$from_file"
            return
        fi
    fi

    local epoch
    epoch="$(read_source_date_epoch || true)"
    local git_hash
    git_hash="$(get_git_hash)"
    local ts
    ts="$(format_timestamp_compact "$epoch")"
    echo "${git_hash}-${ts}"
}

apply_asset_versioning() {
    local index_file="$STATIC_DIR/index.html"
    if [[ ! -f "$index_file" ]]; then
        echo "Skipping asset versioning (missing index.html)"
        return
    fi
    if ! command -v perl >/dev/null 2>&1; then
        echo "Skipping asset versioning (perl not available)"
        return
    fi

    local build_id_raw
    build_id_raw="$(get_build_id)"
    local build_id_safe
    build_id_safe="$(sanitize_for_filename "$build_id_raw")"
    local epoch
    epoch="$(read_source_date_epoch || true)"
    local build_time_iso
    build_time_iso="$(format_timestamp_iso "$epoch")"
    local build_id_escaped
    build_id_escaped="$(escape_perl_replacement "$build_id_raw")"
    local build_time_escaped
    build_time_escaped="$(escape_perl_replacement "$build_time_iso")"

    echo "Build ID: $build_id_raw"
    echo "Asset version: $build_id_safe"
    echo "Build time: $build_time_iso"

    # Update build metadata placeholders (and enforce current values)
    perl -0pi -e 's/__TRUNK_HASH__/'"$build_id_escaped"'/g; s/__BUILD_TIME__/'"$build_time_escaped"'/g' "$index_file"
    perl -0pi -e 's|(meta name="aos-build-hash" content=")[^"]*(")|${1}'"$build_id_escaped"'${2}|g; s|(meta name="aos-build-time" content=")[^"]*(")|${1}'"$build_time_escaped"'${2}|g' "$index_file"

    # Collect assets referenced by index.html and append build id to their names
    while IFS= read -r asset; do
        case "$asset" in
            sw.js|*/sw.js) continue ;;
        esac

        local dir base ext name new_base new_asset new_asset_escaped
        dir="$(dirname "$asset")"
        base="$(basename "$asset")"
        ext="${base##*.}"
        name="${base%.*}"

        if [[ "$name" == *"-${build_id_safe}" ]]; then
            continue
        fi

        new_base="${name}-${build_id_safe}.${ext}"
        if [[ "$dir" == "." ]]; then
            new_asset="$new_base"
        else
            new_asset="$dir/$new_base"
        fi

        if [[ ! -f "$STATIC_DIR/$asset" ]]; then
            echo "Skipping rename (missing asset): $asset"
            continue
        fi

        mv "$STATIC_DIR/$asset" "$STATIC_DIR/$new_asset"
        new_asset_escaped="$(escape_perl_replacement "$new_asset")"
        perl -0pi -e 's|/\Q'"$asset"'\E|/'"$new_asset_escaped"'|g' "$index_file"
    done < <(perl -ne 'while (m{/([^"'"'"' ]+\.(?:js|css|wasm))}g){ print "$1\n" }' "$index_file" | sort -u)
}

prune_unused_assets() {
    if [[ "${AOS_UI_ASSET_CLEAN:-1}" == "0" ]]; then
        echo "Skipping asset cleanup (AOS_UI_ASSET_CLEAN=0)"
        return
    fi

    local index_file="$STATIC_DIR/index.html"
    if [[ ! -f "$index_file" ]]; then
        echo "Skipping asset cleanup (missing index.html)"
        return
    fi
    if ! command -v perl >/dev/null 2>&1; then
        echo "Skipping asset cleanup (perl not available)"
        return
    fi

    local keep_count="${AOS_UI_ASSET_KEEP:-3}"
    if [[ ! "$keep_count" =~ ^[0-9]+$ ]]; then
        keep_count=3
    fi

    local keep_index
    keep_index="$(perl -ne 'while (m{/([^"'"'"' ]+\.(?:js|css|wasm))}g){ print "$1\n" }' "$index_file" | sort -u)"
    if [[ -z "$keep_index" ]]; then
        echo "Skipping asset cleanup (no assets found in index.html)"
        return
    fi

    # Determine most recent build IDs by timestamp suffix (YYYYMMDDHHMMSS).
    local id_entries=""
    for file in "$STATIC_DIR"/*.js "$STATIC_DIR"/*.css "$STATIC_DIR"/*.wasm; do
        [[ -e "$file" ]] || continue
        local base
        base="$(basename "$file")"
        if [[ "$base" =~ -([A-Za-z0-9._-]*[0-9]{14})\.(js|css|wasm)$ ]]; then
            local id="${BASH_REMATCH[1]}"
            if [[ "$id" =~ ([0-9]{14})$ ]]; then
                local ts="${BASH_REMATCH[1]}"
                id_entries+="${ts} ${id}"$'\n'
            fi
        fi
    done

    local keep_ids=""
    if [[ -n "$id_entries" && "$keep_count" -gt 0 ]]; then
        keep_ids="$(printf "%s" "$id_entries" | sort -u | sort -r | head -n "$keep_count" | awk '{print $2}')"
    fi

    local removed=0
    for file in "$STATIC_DIR"/*.js "$STATIC_DIR"/*.css "$STATIC_DIR"/*.wasm; do
        [[ -e "$file" ]] || continue
        local base
        base="$(basename "$file")"

        if [[ "$base" == "sw.js" ]]; then
            continue
        fi

        if grep -Fxq "$base" <<<"$keep_index"; then
            continue
        fi

        local build_id=""
        if [[ "$base" =~ -([A-Za-z0-9._-]*[0-9]{14})\.(js|css|wasm)$ ]]; then
            build_id="${BASH_REMATCH[1]}"
        fi

        if [[ -n "$build_id" ]] && grep -Fxq "$build_id" <<<"$keep_ids"; then
            continue
        fi

        rm -f "$file"
        removed=$((removed + 1))
    done

    echo "Pruned $removed stale asset(s) (kept last $keep_count build(s))"
}

index_asset_paths() {
    local index_file="$1"
    local ext="$2"
    if [[ ! -f "$index_file" ]]; then
        return
    fi
    perl -ne "while (m{/([^\"' ]+\\.${ext})}g){ print \"\$1\\n\" }" "$index_file"
}

index_asset_path() {
    local index_file="$1"
    local ext="$2"
    index_asset_paths "$index_file" "$ext" | head -1
}

# Parse args
SKIP_OPT=false
if [[ "${1:-}" == "--skip-opt" ]]; then
    SKIP_OPT=true
fi

echo "=== Building adapterOS UI (WASM) ==="
echo "Directory: $UI_DIR"

# Check trunk
if ! command -v trunk &> /dev/null; then
    echo "Error: trunk not found. Install with: cargo install trunk"
    exit 1
fi

# Build with trunk
cd "$UI_DIR"
echo "Running: trunk build --release"
# trunk treats NO_COLOR as a boolean; some environments set NO_COLOR=1 which is
# rejected by newer trunk (expects true/false).
NO_COLOR=true trunk build --release

# Ensure auxiliary static assets exist (service worker + fonts).
#
# Trunk does not reliably copy @font-face referenced font files into dist.
# When missing, browsers fetch `/fonts/*.woff2` and receive HTML (SPA fallback),
# triggering "Failed to decode downloaded font" warnings and degraded visuals.
#
# Service worker is also optional but expected by `index.html` boot code; keep it
# present when `dist/sw.js` exists.
if [[ -d "$UI_DIR/dist/fonts" ]]; then
    mkdir -p "$STATIC_DIR/fonts"
    for f in "$UI_DIR"/dist/fonts/*.woff2 "$UI_DIR"/dist/fonts/LICENSE.txt; do
        [[ -f "$f" ]] || continue
        cp -f "$f" "$STATIC_DIR/fonts/"
    done
fi
if [[ -f "$UI_DIR/dist/sw.js" ]]; then
    cp -f "$UI_DIR/dist/sw.js" "$STATIC_DIR/sw.js"
fi

# Apply build metadata + versioned asset naming
apply_asset_versioning

INDEX_FILE="$STATIC_DIR/index.html"

# Remove old build artifacts to avoid static/ bloat
prune_unused_assets

# Find the WASM file (prefer index.html reference)
WASM_ASSET="$(index_asset_path "$INDEX_FILE" "wasm")"
if [[ -n "$WASM_ASSET" ]]; then
    WASM_FILE="$STATIC_DIR/$WASM_ASSET"
else
    WASM_FILE=$(ls "$STATIC_DIR"/*.wasm 2>/dev/null | head -1)
fi
if [[ -z "$WASM_FILE" ]]; then
    echo "Error: WASM file not found in $STATIC_DIR"
    exit 1
fi

# Find JS glue file (skip service worker)
JS_ASSET="$(index_asset_paths "$INDEX_FILE" "js" | grep -v '/sw.js' | head -1 || true)"
if [[ -n "$JS_ASSET" ]]; then
    JS_FILE="$STATIC_DIR/$JS_ASSET"
else
    JS_FILE=$(ls "$STATIC_DIR"/*.js 2>/dev/null | grep -v '/sw.js' | head -1 || true)
fi

BEFORE_SIZE=$(stat -f%z "$WASM_FILE" 2>/dev/null || stat -c%s "$WASM_FILE" 2>/dev/null)
if command -v bc &> /dev/null; then
    echo "Before optimization: $BEFORE_SIZE bytes ($(echo "scale=2; $BEFORE_SIZE / 1048576" | bc) MB)"
else
    echo "Before optimization: $BEFORE_SIZE bytes"
fi

# Run wasm-opt if not skipped
if [[ "$SKIP_OPT" == "false" ]]; then
    if ! command -v wasm-opt &> /dev/null; then
        echo "Warning: wasm-opt not found. Install with: brew install binaryen"
        echo "Skipping optimization step."
    else
        echo "Running: wasm-opt -O4 --enable-bulk-memory"
        wasm-opt -O4 --enable-bulk-memory "$WASM_FILE" -o "${WASM_FILE}.opt"
        mv "${WASM_FILE}.opt" "$WASM_FILE"

        AFTER_SIZE=$(stat -f%z "$WASM_FILE" 2>/dev/null || stat -c%s "$WASM_FILE" 2>/dev/null)
        SAVINGS=$((BEFORE_SIZE - AFTER_SIZE))
        if command -v bc &> /dev/null; then
            PCT=$(echo "scale=1; $SAVINGS * 100 / $BEFORE_SIZE" | bc)
            echo "After optimization:  $AFTER_SIZE bytes ($(echo "scale=2; $AFTER_SIZE / 1048576" | bc) MB)"
            echo "Reduction: $SAVINGS bytes ($PCT%)"
        else
            echo "After optimization:  $AFTER_SIZE bytes"
            echo "Reduction: $SAVINGS bytes"
        fi
    fi
else
    echo "Skipping wasm-opt (--skip-opt)"
fi

# Bundle analysis with twiggy (optional)
if command -v twiggy &> /dev/null; then
    echo ""
    echo "=== Bundle Analysis ==="
    twiggy top -n 20 "$WASM_FILE"
fi

# Compressed sizes
FINAL_SIZE=$(stat -f%z "$WASM_FILE" 2>/dev/null || stat -c%s "$WASM_FILE" 2>/dev/null)

GZIP_SIZE=0
if command -v gzip &> /dev/null; then
    GZIP_SIZE=$(gzip -9 -c "$WASM_FILE" | wc -c | tr -d ' ')
fi

BROTLI_SIZE=0
if command -v brotli &> /dev/null; then
    brotli -9 -c "$WASM_FILE" > "${WASM_FILE}.br"
    BROTLI_SIZE=$(stat -f%z "${WASM_FILE}.br" 2>/dev/null || stat -c%s "${WASM_FILE}.br" 2>/dev/null)
    rm -f "${WASM_FILE}.br"
fi

echo ""
echo "=== Bundle Size Summary ==="
if command -v bc &> /dev/null; then
    echo "Raw:    $FINAL_SIZE bytes ($(echo "scale=2; $FINAL_SIZE / 1048576" | bc) MB)"
else
    echo "Raw:    $FINAL_SIZE bytes"
fi
if [[ "$BROTLI_SIZE" -gt 0 ]]; then
    if command -v bc &> /dev/null; then
        echo "Brotli: $BROTLI_SIZE bytes ($(echo "scale=2; $BROTLI_SIZE / 1048576" | bc) MB) [wire]"
    else
        echo "Brotli: $BROTLI_SIZE bytes [wire]"
    fi
fi
if [[ "$GZIP_SIZE" -gt 0 ]]; then
    if command -v bc &> /dev/null; then
        echo "Gzip:   $GZIP_SIZE bytes ($(echo "scale=2; $GZIP_SIZE / 1048576" | bc) MB) [fallback]"
    else
        echo "Gzip:   $GZIP_SIZE bytes [fallback]"
    fi
fi

# Gate warnings (match CI)
MAX_BROTLI=$((1200000))
STRETCH_GOAL=$((1000000))
if [[ "$BROTLI_SIZE" -gt 0 ]]; then
    if [[ "$BROTLI_SIZE" -gt "$MAX_BROTLI" ]]; then
        echo ""
        echo "⚠️  WARNING: Brotli size exceeds 1.2MB gate!"
    elif [[ "$BROTLI_SIZE" -gt "$STRETCH_GOAL" ]]; then
        echo ""
        echo "📊 Note: Brotli size exceeds 1.0MB stretch goal"
    else
        echo ""
        echo "✓ Within size budgets"
    fi
fi

# Recompute SRI hashes so index.html stays in sync after post-processing
update_integrity_attr() {
    local index_file="$1"
    local asset_path="$2"
    if [[ -z "$asset_path" ]]; then
        echo "Skipping SRI update for empty asset path"
        return
    fi
    local asset_name
    asset_name="$(basename "$asset_path")"

    if [[ ! -f "$asset_path" ]]; then
        echo "Skipping SRI update for missing asset: $asset_name"
        return
    fi

    local sri
    sri=$(openssl dgst -sha384 -binary "$asset_path" | base64)
    if perl -0pi -e 's|(href="/\Q'"$asset_name"'\E"[^>]*integrity=")sha384-[^"]+(")|${1}sha384-'"$sri"'${2}|' "$index_file"; then
        echo "Updated integrity for $asset_name (sha384-$sri)"
    else
        echo "Note: integrity attribute not found for $asset_name (skipped)"
    fi
}

if command -v openssl >/dev/null 2>&1 && command -v perl >/dev/null 2>&1 && [[ -f "$INDEX_FILE" ]]; then
    echo ""
    echo "=== Updating Subresource Integrity Digests ==="
    if [[ -n "$JS_FILE" ]]; then
        update_integrity_attr "$INDEX_FILE" "$JS_FILE"
    fi
    if [[ -n "$WASM_FILE" ]]; then
        update_integrity_attr "$INDEX_FILE" "$WASM_FILE"
    fi
    for css_file in "$STATIC_DIR"/*.css; do
        update_integrity_attr "$INDEX_FILE" "$css_file"
    done
else
    echo ""
    echo "Skipping SRI update (missing openssl/perl or index.html)"
fi

echo ""
echo "=== Build Complete ==="
echo "Output: $WASM_FILE"
