#!/usr/bin/env bash
# Deterministic SBOM + provenance generator for release bundles.
# - Stages release binaries into target/release-bundle/artifacts/
# - Emits sbom.json (with build + artifact hashes) and build_provenance.json
# - Optionally signs both with an Ed25519 key (RELEASE_SIGNING_KEY_PEM).

set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

if ! command -v b3sum >/dev/null 2>&1; then
  echo "b3sum is required (install with: cargo install b3sum)" >&2
  exit 1
fi

OUT_DIR="${OUT_DIR:-$ROOT/target/release-bundle}"
STAGE_DIR="$OUT_DIR/artifacts"
mkdir -p "$STAGE_DIR"
: "${SBOM_REQUIRE_SIGNING:=0}"

SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-$(git -C "$ROOT" log -1 --format=%ct || date +%s)}"
BUILD_TIMESTAMP="$(date -u -r "$SOURCE_DATE_EPOCH" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u +%Y-%m-%dT%H:%M:%SZ)"
GIT_SHA="$(git -C "$ROOT" rev-parse HEAD)"
GIT_SHA_SHORT="$(git -C "$ROOT" rev-parse --short=12 HEAD)"
BUILD_ID="${BUILD_ID:-$GIT_SHA_SHORT}"

DEFAULT_ARTIFACT_GROUPS=(
  "target/release/aos-server target/release/adapteros-server"
  "target/release/aos-worker target/release/aos_worker"
  "target/release/aosctl"
)

stage_artifact() {
  local artifact="$1"
  local abs="$ROOT/$artifact"
  local name
  name="$(basename "$artifact")"
  local staged="$STAGE_DIR/$name"

  cp "$abs" "$staged"
  touch -h -t "$TOUCH_TIMESTAMP" "$staged" 2>/dev/null || touch -t "$TOUCH_TIMESTAMP" "$staged" 2>/dev/null || true

  local hash
  hash="$(b3sum "$staged" | awk '{print $1}')"
  ARTIFACTS_JSON+=("{\"path\":\"artifacts/$name\",\"hash\":\"$hash\",\"hash_algo\":\"blake3\",\"kind\":\"binary\"}")
}

stage_from_list() {
  local list=("${@}")
  for artifact in "${list[@]}"; do
    local abs="$ROOT/$artifact"
    if [[ ! -f "$abs" ]]; then
      echo "Skipping missing artifact: $artifact" >&2
      continue
    fi

    stage_artifact "$artifact"
  done
}

if [[ -n "${ARTIFACTS:-}" ]]; then
  # Space-separated list of artifact paths
  IFS=' ' read -r -a ARTIFACTS_LIST <<<"${ARTIFACTS}"
  ARTIFACTS_JSON=()
  TOUCH_TIMESTAMP="$(date -u -r "$SOURCE_DATE_EPOCH" +%Y%m%d%H%M.%S 2>/dev/null || date -u +%Y%m%d%H%M.%S)"
  stage_from_list "${ARTIFACTS_LIST[@]}"
else
  ARTIFACTS_JSON=()
  TOUCH_TIMESTAMP="$(date -u -r "$SOURCE_DATE_EPOCH" +%Y%m%d%H%M.%S 2>/dev/null || date -u +%Y%m%d%H%M.%S)"
  for group in "${DEFAULT_ARTIFACT_GROUPS[@]}"; do
    read -r -a candidates <<<"$group"
    staged=false
    for candidate in "${candidates[@]}"; do
      if [[ -f "$ROOT/$candidate" ]]; then
        stage_artifact "$candidate"
        staged=true
        break
      fi
    done

    if [[ "$staged" = false ]]; then
      echo "Skipping missing artifact variants: ${group}" >&2
    fi
  done
fi

if [[ ${#ARTIFACTS_JSON[@]} -eq 0 ]]; then
  echo "No artifacts were staged; run a release build first." >&2
  exit 1
fi

workspace_hash="$(b3sum Cargo.lock | awk '{print $1}')"
context_manifest_hash="$(b3sum crates/adapteros-core/src/context_manifest.rs | awk '{print $1}')"

cat > "$OUT_DIR/build_provenance.json" <<EOF
{
  "build_id": "$BUILD_ID",
  "git_sha": "$GIT_SHA",
  "build_timestamp": "$BUILD_TIMESTAMP",
  "source_date_epoch": "$SOURCE_DATE_EPOCH",
  "workspace_hash": "$workspace_hash",
  "context_manifest_hash": "$context_manifest_hash"
}
EOF

artifacts_block="$(printf "%s," "${ARTIFACTS_JSON[@]}")"
artifacts_block="${artifacts_block%,}"

cat > "$OUT_DIR/sbom.json" <<EOF
{
  "schema": "adapteros-bundle-sbom-v1",
  "build": {
    "build_id": "$BUILD_ID",
    "git_sha": "$GIT_SHA",
    "build_timestamp": "$BUILD_TIMESTAMP",
    "source_date_epoch": "$SOURCE_DATE_EPOCH",
    "workspace_hash": "$workspace_hash"
  },
  "artifacts": [
    $artifacts_block
  ]
}
EOF

sbom_hash="$(b3sum "$OUT_DIR/sbom.json" | awk '{print $1}')"

python - "$OUT_DIR/build_provenance.json" "$sbom_hash" <<'PY'
import json
import pathlib
import sys

prov_path = pathlib.Path(sys.argv[1])
sbom_hash = sys.argv[2]

data = json.loads(prov_path.read_text())
data["sbom_hash"] = sbom_hash
prov_path.write_text(json.dumps(data, indent=2) + "\n")
PY

maybe_sign() {
  local target_file="$1"
  local output_sig="$2"
  local key_file="$3"

  openssl pkeyutl -sign -inkey "$key_file" -rawin -in "$target_file" -out "$output_sig.bin"
  xxd -p -c 256 "$output_sig.bin" | tr -d '\n' > "$output_sig"
  rm -f "$output_sig.bin"
}

if [[ "$SBOM_REQUIRE_SIGNING" == "1" && -z "${RELEASE_SIGNING_KEY_PEM:-}" ]]; then
  echo "RELEASE_SIGNING_KEY_PEM is required when SBOM_REQUIRE_SIGNING=1" >&2
  exit 1
fi

if [[ -n "${RELEASE_SIGNING_KEY_PEM:-}" ]]; then
  if ! command -v openssl >/dev/null 2>&1; then
    echo "openssl is required for signing" >&2
    exit 1
  fi

  KEY_PATH="$OUT_DIR/release-signing-key.pem"
  if [[ -f "$RELEASE_SIGNING_KEY_PEM" ]]; then
    cp "$RELEASE_SIGNING_KEY_PEM" "$KEY_PATH"
  else
    printf "%s\n" "$RELEASE_SIGNING_KEY_PEM" > "$KEY_PATH"
  fi

  maybe_sign "$OUT_DIR/sbom.json" "$OUT_DIR/signature.sig" "$KEY_PATH"
  maybe_sign "$OUT_DIR/build_provenance.json" "$OUT_DIR/build_provenance.sig" "$KEY_PATH"

  if [[ -n "${RELEASE_SIGNING_PUBKEY_HEX:-}" ]]; then
    printf "%s\n" "$RELEASE_SIGNING_PUBKEY_HEX" > "$OUT_DIR/public_key.hex"
  fi

  rm -f "$KEY_PATH"
else
  echo "⚠️  RELEASE_SIGNING_KEY_PEM not set; sbom.json and build_provenance.json are unsigned." >&2
fi

required_files=(
  "$OUT_DIR/sbom.json"
  "$OUT_DIR/build_provenance.json"
)
for f in "${required_files[@]}"; do
  if [[ ! -f "$f" ]]; then
    echo "Missing required release artifact: $f" >&2
    exit 1
  fi
done

if [[ -n "${RELEASE_SIGNING_KEY_PEM:-}" || "$SBOM_REQUIRE_SIGNING" == "1" ]]; then
  for sig in "$OUT_DIR/signature.sig" "$OUT_DIR/build_provenance.sig"; do
    if [[ ! -f "$sig" ]]; then
      echo "Missing required signature artifact: $sig" >&2
      exit 1
    fi
  done
fi

VERIFICATION_LOG="$OUT_DIR/release_verification.log"
{
  echo "release_artifact_verification=ok"
  echo "generated_at=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "sbom_hash=$(b3sum "$OUT_DIR/sbom.json" | awk '{print $1}')"
  echo "provenance_hash=$(b3sum "$OUT_DIR/build_provenance.json" | awk '{print $1}')"
  if [[ -f "$OUT_DIR/signature.sig" ]]; then
    echo "signature_present=true"
  else
    echo "signature_present=false"
  fi
  if [[ -f "$OUT_DIR/build_provenance.sig" ]]; then
    echo "provenance_signature_present=true"
  else
    echo "provenance_signature_present=false"
  fi
} > "$VERIFICATION_LOG"

echo "SBOM + provenance staged in $OUT_DIR"
