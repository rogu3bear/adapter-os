#!/bin/bash
# Deterministic audit archive compression script
# Uses zstd with deterministic settings for reproducible archives

set -euo pipefail

AUDIT_BUNDLE="${1:-audit_bundle.json}"
OUTPUT_FILE="${2:-audit_bundle.zst}"

if [[ ! -f "$AUDIT_BUNDLE" ]]; then
    echo "Error: Audit bundle file '$AUDIT_BUNDLE' not found" >&2
    exit 1
fi

echo "Compressing audit bundle with deterministic zstd..."
zstd --deterministic -19 -o "$OUTPUT_FILE" "$AUDIT_BUNDLE"

echo "Archive created: $OUTPUT_FILE"
echo "Original size: $(wc -c < "$AUDIT_BUNDLE") bytes"
echo "Compressed size: $(wc -c < "$OUTPUT_FILE") bytes"

# Verify deterministic compression
echo "Verifying deterministic compression..."
HASH1=$(zstd -d "$OUTPUT_FILE" | sha256sum | cut -d' ' -f1)
HASH2=$(cat "$AUDIT_BUNDLE" | sha256sum | cut -d' ' -f1)

if [[ "$HASH1" == "$HASH2" ]]; then
    echo "✓ Deterministic compression verified"
else
    echo "✗ Compression verification failed" >&2
    exit 1
fi

