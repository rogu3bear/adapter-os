#!/usr/bin/env bash
#
# Sign all database migrations with Ed25519 signatures
# Outputs signatures.json for verification during migration
#
# Per Artifacts Ruleset #13: All migrations must be signed
# Per Build Ruleset #15: Signatures gate CAB promotion

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"
SIGNATURES_FILE="$MIGRATIONS_DIR/signatures.json"
KEY_FILE="$PROJECT_ROOT/var/migration_signing_key.txt"
TMP_WORK_DIR="$PROJECT_ROOT/var/tmp"
mkdir -p "$TMP_WORK_DIR"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "adapterOS Migration Signing Tool"
echo "================================="
echo

# Check if openssl is available (prefer Homebrew version for Ed25519 support)
OPENSSL_BIN=""
if [ -f "/opt/homebrew/bin/openssl" ]; then
    OPENSSL_BIN="/opt/homebrew/bin/openssl"
elif command -v openssl &> /dev/null; then
    OPENSSL_BIN="openssl"
else
    echo -e "${RED}Error: openssl not found${NC}"
    echo "Install openssl to sign migrations: brew install openssl"
    exit 1
fi

# Generate or load signing key
if [ ! -f "$KEY_FILE" ]; then
    echo -e "${YELLOW}Generating new Ed25519 signing key...${NC}"
    mkdir -p "$(dirname "$KEY_FILE")"

    # Generate Ed25519 private key
    $OPENSSL_BIN genpkey -algorithm Ed25519 -out "$KEY_FILE" 2>/dev/null

    # Set restrictive permissions
    chmod 600 "$KEY_FILE"

    echo -e "${GREEN}✓ Key generated: $KEY_FILE${NC}"
    echo -e "${YELLOW}⚠  Keep this key secure - required for CAB promotion${NC}"
    echo
else
    echo -e "${GREEN}✓ Using existing signing key: $KEY_FILE${NC}"
    echo
fi

# Extract public key for verification
PUBLIC_KEY_FILE="$PROJECT_ROOT/var/migration_signing_key.pub"
$OPENSSL_BIN pkey -in "$KEY_FILE" -pubout -out "$PUBLIC_KEY_FILE" 2>/dev/null
echo -e "${GREEN}✓ Public key exported: $PUBLIC_KEY_FILE${NC}"
echo

# Start signatures JSON
echo "{" > "$SIGNATURES_FILE"
echo "  \"schema_version\": \"1.0\"," >> "$SIGNATURES_FILE"
echo "  \"signed_at\": \"$(date -u +%Y-%m-%dT%H:%M:%SZ)\"," >> "$SIGNATURES_FILE"
echo "  \"public_key\": \"$(base64 < "$PUBLIC_KEY_FILE" | tr -d '\n')\"," >> "$SIGNATURES_FILE"
echo "  \"signatures\": {" >> "$SIGNATURES_FILE"

# Sign each migration
migration_count=0
first=true

echo "Signing migrations..."
echo

for migration_file in "$MIGRATIONS_DIR"/*.sql; do
    if [ ! -f "$migration_file" ]; then
        continue
    fi

    filename=$(basename "$migration_file")

    # Compute BLAKE3 hash (fallback to SHA256 if blake3 not available)
    if command -v b3sum &> /dev/null; then
        file_hash=$(b3sum "$migration_file" | cut -d' ' -f1)
        hash_algo="blake3"
    else
        file_hash=$(shasum -a 256 "$migration_file" | cut -d' ' -f1)
        hash_algo="sha256"
    fi

    # Sign the file hash
    hash_input="$(mktemp "$TMP_WORK_DIR/hash_input.XXXXXX")"
    sig_file="$(mktemp "$TMP_WORK_DIR/migration.sig.XXXXXX")"
    echo -n "$file_hash" > "$hash_input"
    $OPENSSL_BIN pkeyutl -sign -rawin -inkey "$KEY_FILE" -in "$hash_input" -out "$sig_file" 2>/dev/null
    signature=$(base64 < "$sig_file" | tr -d '\n')
    rm -f "$sig_file" "$hash_input"

    # Add to JSON (with comma handling)
    if [ "$first" = true ]; then
        first=false
    else
        echo "," >> "$SIGNATURES_FILE"
    fi

    echo -n "    \"$filename\": {" >> "$SIGNATURES_FILE"
    echo -n "\"hash\": \"$file_hash\", \"signature\": \"$signature\", \"algorithm\": \"ed25519\", \"hash_algorithm\": \"$hash_algo\"}" >> "$SIGNATURES_FILE"

    echo -e "  ${GREEN}✓${NC} $filename"
    migration_count=$((migration_count + 1))
done

# Close signatures JSON
echo >> "$SIGNATURES_FILE"
echo "  }" >> "$SIGNATURES_FILE"
echo "}" >> "$SIGNATURES_FILE"

echo
echo -e "${GREEN}✓ Successfully signed $migration_count migrations${NC}"
echo -e "${GREEN}✓ Signatures written to: $SIGNATURES_FILE${NC}"
echo
echo "Next steps:"
echo "  1. Commit signatures.json to repository"
echo "  2. Implement verification in adapteros-db/src/migration_verify.rs"
echo "  3. Integrate into Db::migrate() method"
echo "  4. Test tamper detection with modified migration"
echo

# Verify signatures (quick check)
echo "Verifying signatures..."
verify_count=0

for migration_file in "$MIGRATIONS_DIR"/*.sql; do
    if [ ! -f "$migration_file" ]; then
        continue
    fi

    filename=$(basename "$migration_file")

    # Extract signature from JSON
    signature=$(grep "\"$filename\"" "$SIGNATURES_FILE" | sed -E 's/.*"signature": "([^"]+)".*/\1/')

    if [ -z "$signature" ]; then
        echo -e "${RED}✗ No signature found for $filename${NC}"
        continue
    fi

    # Recompute hash
    if command -v b3sum &> /dev/null; then
        file_hash=$(b3sum "$migration_file" | cut -d' ' -f1)
    else
        file_hash=$(shasum -a 256 "$migration_file" | cut -d' ' -f1)
    fi

    # Verify signature
    sig_file="$(mktemp "$TMP_WORK_DIR/migration.sig.XXXXXX")"
    hash_input="$(mktemp "$TMP_WORK_DIR/hash_input.XXXXXX")"
    echo "$signature" | base64 -d > "$sig_file"
    echo -n "$file_hash" > "$hash_input"
    if $OPENSSL_BIN pkeyutl -verify -rawin -pubin -inkey "$PUBLIC_KEY_FILE" -in "$hash_input" -sigfile "$sig_file" 2>/dev/null; then
        verify_count=$((verify_count + 1))
    else
        echo -e "${RED}✗ Signature verification failed for $filename${NC}"
    fi
    rm -f "$sig_file" "$hash_input"
done

echo -e "${GREEN}✓ Verified $verify_count/$migration_count signatures${NC}"
echo

if [ "$verify_count" -eq "$migration_count" ]; then
    echo -e "${GREEN}All migrations successfully signed and verified!${NC}"
    exit 0
else
    echo -e "${RED}Some signatures failed verification${NC}"
    exit 1
fi
