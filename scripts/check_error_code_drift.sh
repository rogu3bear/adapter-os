#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="docs/error-inventory"
TMP_DIR="var/tmp/error-inventory"
ALLOWLIST_CODES="scripts/ci/error_code_literal_allowlist.txt"
ALLOWLIST_NO_CODE="scripts/ci/error_response_no_with_code_allowlist.txt"

mkdir -p "$OUT_DIR" "$TMP_DIR" "scripts/ci"

CANONICAL_FILE="$TMP_DIR/canonical_codes.txt"
WITH_CODE_REFS_FILE="$TMP_DIR/with_code_literal_refs.txt"
WITH_CODE_CODES_FILE="$TMP_DIR/with_code_literal_codes.txt"
DYNAMIC_REFS_FILE="$TMP_DIR/with_code_dynamic_refs.txt"
NON_CANONICAL_CODES_FILE="$TMP_DIR/non_canonical_codes.txt"
NON_CANONICAL_REFS_FILE="$TMP_DIR/non_canonical_refs.txt"
NO_CODE_REFS_FILE="$TMP_DIR/error_response_no_with_code_refs.txt"
UNKNOWN_NON_CANONICAL_FILE="$TMP_DIR/unknown_non_canonical_codes.txt"
NEW_NO_CODE_FILE="$TMP_DIR/new_missing_with_code_refs.txt"
JSON_FILE="$OUT_DIR/error_codes_inventory.json"
MD_FILE="$OUT_DIR/ERROR_CODE_INVENTORY.md"
CSV_FILE="$OUT_DIR/error-code-disposition.csv"
METADATA_ENTRIES_FILE="$TMP_DIR/metadata_entries.json"

rg --no-filename 'pub const [A-Z0-9_]+: &str = "[A-Z0-9_]+";' crates/adapteros-core/src/error_codes.rs \
  | sed -E 's/.*"([A-Z0-9_]+)".*/\1/' \
  | sort -u > "$CANONICAL_FILE"

rg -n --no-heading -g '*.rs' 'with_code\(\s*"([A-Z0-9_]+)"\s*\)' crates > "$WITH_CODE_REFS_FILE" || true
sed -E 's/.*with_code\(\s*"([A-Z0-9_]+)"\s*\).*/\1/' "$WITH_CODE_REFS_FILE" | sort -u > "$WITH_CODE_CODES_FILE"

rg -n --no-heading -g '*.rs' 'with_code\(\s*[a-zA-Z_][a-zA-Z0-9_:\.\(\)]*\s*\)' crates \
  | rg -v 'with_code\(\s*"' > "$DYNAMIC_REFS_FILE" || true

comm -23 "$WITH_CODE_CODES_FILE" "$CANONICAL_FILE" > "$NON_CANONICAL_CODES_FILE"

awk 'NR==FNR{want[$1]=1;next}
{
  code=$0
  sub(/.*with_code\([[:space:]]*"/, "", code)
  sub(/".*/, "", code)
  if (code in want) { print }
}' "$NON_CANONICAL_CODES_FILE" "$WITH_CODE_REFS_FILE" > "$NON_CANONICAL_REFS_FILE"

: > "$NO_CODE_REFS_FILE"
while IFS=: read -r file line _; do
  snippet="$(sed -n "${line},$((line+8))p" "$file")"
  if ! printf '%s' "$snippet" | rg -q '\.with_code\('; then
    printf '%s:%s\n' "$file" "$line" >> "$NO_CODE_REFS_FILE"
  fi
done < <(rg -n --no-heading 'ErrorResponse::new\(' crates/adapteros-server-api crates/adapteros-server-api-admin crates/adapteros-server-api-models -g '*.rs')

if [[ ! -f "$ALLOWLIST_CODES" ]]; then
  cp "$NON_CANONICAL_CODES_FILE" "$ALLOWLIST_CODES"
fi
if [[ ! -f "$ALLOWLIST_NO_CODE" ]]; then
  cp "$NO_CODE_REFS_FILE" "$ALLOWLIST_NO_CODE"
fi

comm -23 "$NON_CANONICAL_CODES_FILE" <(sort -u "$ALLOWLIST_CODES") > "$UNKNOWN_NON_CANONICAL_FILE"
comm -23 <(sort -u "$NO_CODE_REFS_FILE") <(sort -u "$ALLOWLIST_NO_CODE") > "$NEW_NO_CODE_FILE"

const_count=$(wc -l < "$CANONICAL_FILE" | tr -d ' ')
literal_count=$(wc -l < "$WITH_CODE_CODES_FILE" | tr -d ' ')
non_canonical_count=$(wc -l < "$NON_CANONICAL_CODES_FILE" | tr -d ' ')
non_canonical_ref_count=$(wc -l < "$NON_CANONICAL_REFS_FILE" | tr -d ' ')
enum_count=$(wc -l < var/tmp/aos_error_enums_with_locations.tsv 2>/dev/null || echo 0)
dynamic_count=$(wc -l < "$DYNAMIC_REFS_FILE" | tr -d ' ')

{
  first=1
  while IFS= read -r code; do
    [[ -z "$code" ]] && continue
    status_family="5xx"
    owner_domain="platform"
    deprecated="false"
    alias_of="null"

    case "$code" in
      BAD_REQUEST|VALIDATION_ERROR|SERIALIZATION_ERROR|PARSE_ERROR|INVALID_HASH|INVALID_CPID|INVALID_MANIFEST|ADAPTER_NOT_IN_MANIFEST|ADAPTER_NOT_IN_EFFECTIVE_SET|KERNEL_LAYOUT_MISMATCH|CHAT_TEMPLATE_ERROR|MISSING_FIELD|INVALID_TENANT_ID|INVALID_SESSION_ID|INVALID_SEALED_DATA|FEATURE_DISABLED|PREFLIGHT_FAILED|INCOMPATIBLE_SCHEMA_VERSION|ADAPTER_BASE_MODEL_MISMATCH|INCOMPATIBLE_BASE_MODEL|UNSUPPORTED_BACKEND|HASH_INTEGRITY_FAILURE|VERSION_NOT_PROMOTABLE|DETERMINISM_ERROR)
        status_family="4xx"
        owner_domain="validation"
        ;;
      UNAUTHORIZED|TOKEN_MISSING|TOKEN_INVALID|TOKEN_SIGNATURE_INVALID|TOKEN_EXPIRED|TOKEN_REVOKED|INVALID_ISSUER|INVALID_AUDIENCE|INVALID_API_KEY|SESSION_EXPIRED|SESSION_LOCKED|DEVICE_MISMATCH|INVALID_CREDENTIALS)
        status_family="4xx"
        owner_domain="auth"
        ;;
      FORBIDDEN|PERMISSION_DENIED|TENANT_ISOLATION_ERROR|CSRF_ERROR|INSUFFICIENT_ROLE|MFA_REQUIRED|POLICY_VIOLATION|POLICY_ERROR|SIGNATURE_REQUIRED|SIGNATURE_INVALID|REPO_ARCHIVED|DETERMINISM_VIOLATION|EGRESS_VIOLATION|SSRF_BLOCKED|ISOLATION_VIOLATION|PERFORMANCE_VIOLATION|ANOMALY_DETECTED|SYSTEM_QUARANTINED|ADAPTER_TENANT_MISMATCH|INTEGRITY_VIOLATION|CHECKPOINT_INTEGRITY_FAILED)
        status_family="4xx"
        owner_domain="security-policy"
        ;;
      NOT_FOUND|ADAPTER_NOT_FOUND|MODEL_NOT_FOUND|CACHE_ENTRY_NOT_FOUND|REPO_NOT_FOUND|VERSION_NOT_FOUND)
        status_family="4xx"
        owner_domain="resource"
        ;;
      CONFLICT|ADAPTER_HASH_MISMATCH|ADAPTER_LAYER_HASH_MISMATCH|POLICY_HASH_MISMATCH|PROMOTION_ERROR|MODEL_ACQUISITION_IN_PROGRESS|DUPLICATE_REQUEST|ADAPTER_IN_FLIGHT|REPO_ALREADY_EXISTS)
        status_family="4xx"
        owner_domain="state"
        ;;
      REASONING_LOOP_DETECTED)
        status_family="4xx"
        owner_domain="inference"
        ;;
      TOO_MANY_REQUESTS|BACKPRESSURE|THUNDERING_HERD_REJECTED)
        status_family="4xx"
        owner_domain="throttling"
        ;;
      CLIENT_CLOSED_REQUEST)
        status_family="4xx"
        owner_domain="transport"
        ;;
      BAD_GATEWAY|NETWORK_ERROR|BASE_LLM_ERROR|UDS_CONNECTION_FAILED|INVALID_RESPONSE|DOWNLOAD_FAILED)
        status_family="5xx"
        owner_domain="upstream"
        ;;
      SERVICE_UNAVAILABLE|MEMORY_PRESSURE|WORKER_NOT_RESPONDING|CIRCUIT_BREAKER_OPEN|CIRCUIT_BREAKER_HALF_OPEN|HEALTH_CHECK_FAILED|ADAPTER_NOT_LOADED|ADAPTER_NOT_LOADABLE|CACHE_BUDGET_EXCEEDED|CPU_THROTTLED|OUT_OF_MEMORY|FD_EXHAUSTED|THREAD_POOL_SATURATED|GPU_UNAVAILABLE|DISK_FULL|TEMP_DIR_UNAVAILABLE|CACHE_STALE|CACHE_EVICTION|STREAM_DISCONNECTED|EVENT_GAP_DETECTED|MODEL_NOT_READY|NO_COMPATIBLE_WORKER|WORKER_DEGRADED|WORKER_ID_UNAVAILABLE)
        status_family="5xx"
        owner_domain="runtime"
        ;;
      GATEWAY_TIMEOUT|REQUEST_TIMEOUT)
        status_family="5xx"
        owner_domain="timeout"
        ;;
      DEV_BYPASS_IN_RELEASE|JWT_MODE_NOT_CONFIGURED|API_KEY_MODE_NOT_CONFIGURED)
        status_family="5xx"
        owner_domain="bootstrap"
        ;;
      PAYLOAD_TOO_LARGE)
        status_family="4xx"
        owner_domain="validation"
        ;;
    esac

    if [[ "$code" == "REQUEST_TIMEOUT" ]]; then
      deprecated="true"
      alias_of="\"GATEWAY_TIMEOUT\""
    fi

    if [[ $first -eq 0 ]]; then
      printf ',\n'
    fi
    first=0
    printf '    "%s": {"status_family":"%s","owner_domain":"%s","deprecated":%s,"alias_of":%s}' \
      "$code" "$status_family" "$owner_domain" "$deprecated" "$alias_of"
  done < "$CANONICAL_FILE"
  printf '\n'
} > "$METADATA_ENTRIES_FILE"

cat > "$JSON_FILE" <<JSON
{
  "generated_at": "$(date -u +"%Y-%m-%dT%H:%M:%SZ")",
  "canonical_constants_count": $const_count,
  "literal_with_code_count": $literal_count,
  "non_canonical_literal_count": $non_canonical_count,
  "non_canonical_literal_ref_count": $non_canonical_ref_count,
  "dynamic_with_code_sites_count": $dynamic_count,
  "error_enum_count": $enum_count,
  "sources": {
    "canonical": "crates/adapteros-core/src/error_codes.rs",
    "with_code_literal_refs": "$WITH_CODE_REFS_FILE",
    "dynamic_with_code_refs": "$DYNAMIC_REFS_FILE",
    "enum_inventory": "var/tmp/aos_error_enums_with_locations.tsv"
  },
  "codes": {
$(cat "$METADATA_ENTRIES_FILE")
  }
}
JSON

{
  echo "# ERROR CODE INVENTORY"
  echo
  echo "Generated: $(date -u +"%Y-%m-%d %H:%M:%SZ")"
  echo
  echo "## Summary"
  echo
  echo "- Canonical constants: $const_count"
  echo "- Literal emitted via with_code(\"...\"): $literal_count"
  echo "- Non-canonical literals: $non_canonical_count"
  echo "- Dynamic emission sites: $dynamic_count"
  echo "- Error enums discovered: $enum_count"
  echo
  echo "## Canonical Constants"
  echo
  cat "$CANONICAL_FILE"
  echo
  echo "## Literal Emitted Codes"
  echo
  cat "$WITH_CODE_CODES_FILE"
  echo
  echo "## Non-Canonical Codes"
  echo
  cat "$NON_CANONICAL_CODES_FILE"
  echo
  echo "## Non-Canonical References (file:line)"
  echo
  cat "$NON_CANONICAL_REFS_FILE"
  echo
  echo "## Dynamic Emission Sites (file:line)"
  echo
  cat "$DYNAMIC_REFS_FILE"
  echo
  echo "## Error Enums + Locations"
  echo
  if [[ -f var/tmp/aos_error_enums_with_locations.tsv ]]; then
    cat var/tmp/aos_error_enums_with_locations.tsv
  else
    echo "(missing var/tmp/aos_error_enums_with_locations.tsv)"
  fi
} > "$MD_FILE"

# Build disposition CSV from non-canonical codes
{
  echo "code,first_seen_path,surface,status_intended,disposition,canonical_target,deprecation_epoch,owner_team"
  while IFS= read -r code; do
    [[ -z "$code" ]] && continue
    first_ref="$(rg -n --no-heading -g '*.rs' "with_code\\(\\s*\"${code}\"\\s*\\)" crates | head -n1 || true)"
    first_path="${first_ref%%:*}"

    surface="internal"
    owner_team="Integration Sheriff"
    if [[ "$first_path" == *"adapteros-ui"* ]]; then
      surface="ui"
      owner_team="UI/UX"
    elif [[ "$first_path" == *"server-api"* || "$first_path" == *"api"* ]]; then
      surface="api"
      owner_team="Backends"
    fi

    status_intended="500"
    canonical_target="INTERNAL_ERROR"
    disposition="PROMOTE_TO_CANONICAL"

    case "$code" in
      INTERNAL_SERVER_ERROR) canonical_target="INTERNAL_ERROR"; status_intended="500"; disposition="DEPRECATE_ALIAS" ;;
      DB_ERROR) canonical_target="DATABASE_ERROR"; status_intended="500"; disposition="DEPRECATE_ALIAS" ;;
      WORKER_UNAVAILABLE) canonical_target="SERVICE_UNAVAILABLE"; status_intended="503"; disposition="DEPRECATE_ALIAS" ;;
      REPOSITORY_NOT_FOUND) canonical_target="REPO_NOT_FOUND"; status_intended="404"; disposition="DEPRECATE_ALIAS" ;;
      TOKEN_ERROR|INVALID_TOKEN) canonical_target="TOKEN_INVALID"; status_intended="401"; disposition="DEPRECATE_ALIAS" ;;
      SESSION_INVALID) canonical_target="UNAUTHORIZED"; status_intended="401"; disposition="DEPRECATE_ALIAS" ;;
      TENANT_HEADER_MISSING|TENANT_ACCESS_DENIED|TENANT_MISMATCH) canonical_target="TENANT_ISOLATION_ERROR"; status_intended="403"; disposition="DEPRECATE_ALIAS" ;;
      RATE_LIMIT_EXCEEDED) canonical_target="TOO_MANY_REQUESTS"; status_intended="429"; disposition="DEPRECATE_ALIAS" ;;
      NOT_IMPLEMENTED) canonical_target="FEATURE_DISABLED"; status_intended="501"; disposition="DEPRECATE_ALIAS" ;;
      PATH_TRAVERSAL|INVALID_PATH) canonical_target="BAD_REQUEST"; status_intended="400"; disposition="DEPRECATE_ALIAS" ;;
      DATASET_NOT_FOUND|NODE_NOT_FOUND|MODEL_PATH_MISSING) canonical_target="NOT_FOUND"; status_intended="404"; disposition="DEPRECATE_ALIAS" ;;
      MODEL_PATH_FORBIDDEN) canonical_target="FORBIDDEN"; status_intended="403"; disposition="DEPRECATE_ALIAS" ;;
      VERIFICATION_FAILED|VERIFICATION_ERROR) canonical_target="INTEGRITY_VIOLATION"; status_intended="403"; disposition="DEPRECATE_ALIAS" ;;
      PROMOTION_FAILED) canonical_target="PROMOTION_ERROR"; status_intended="409"; disposition="DEPRECATE_ALIAS" ;;
      ROLLBACK_FAILED) canonical_target="CONFLICT"; status_intended="409"; disposition="DEPRECATE_ALIAS" ;;
      *TEST*|*MOCK*|*DUMMY*|DEV_*) disposition="INTERNAL_ONLY"; canonical_target=""; status_intended="" ;;
    esac

    printf '%s,%s,%s,%s,%s,%s,%s,%s\n' \
      "$code" "$first_path" "$surface" "$status_intended" "$disposition" "$canonical_target" "2026-Q2" "$owner_team"
  done < "$NON_CANONICAL_CODES_FILE"
} > "$CSV_FILE"

if [[ -s "$UNKNOWN_NON_CANONICAL_FILE" ]]; then
  echo "New non-canonical error codes detected (not in canonical registry or allowlist):"
  cat "$UNKNOWN_NON_CANONICAL_FILE"
  exit 1
fi

if [[ -s "$NEW_NO_CODE_FILE" ]]; then
  echo "New ErrorResponse::new(...) sites without nearby .with_code(...) detected:"
  cat "$NEW_NO_CODE_FILE"
  exit 1
fi

echo "Error code drift check passed."
echo "Inventory: $MD_FILE"
echo "JSON: $JSON_FILE"
echo "Disposition: $CSV_FILE"
