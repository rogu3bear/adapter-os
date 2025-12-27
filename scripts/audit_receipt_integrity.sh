#!/usr/bin/env bash
# Verify router_decisions integrity by tampering a receipt and expecting TRACE_TAMPER.
set -euo pipefail

BUNDLE_PATH="${1:-var/receipts/latest}"
WORKDIR="$(mktemp -d -t aos-receipt-audit.XXXXXX)"

cleanup() {
  rm -rf "${WORKDIR}"
}
trap cleanup EXIT

echo "[audit] Using bundle: ${BUNDLE_PATH}"

if [ -d "${BUNDLE_PATH}" ]; then
  cp -R "${BUNDLE_PATH}" "${WORKDIR}/bundle"
  TARGET_DIR="${WORKDIR}/bundle"
elif [ -f "${BUNDLE_PATH}" ]; then
  TARGET_DIR="${WORKDIR}/bundle"
  mkdir -p "${TARGET_DIR}"
  cp "${BUNDLE_PATH}" "${TARGET_DIR}/receipt_bundle.json"
else
  echo "Bundle path not found: ${BUNDLE_PATH}" >&2
  exit 1
fi

TRACE_FILE=""
for candidate in inference_trace.json run_receipt.json receipt_bundle.json; do
  if [ -f "${TARGET_DIR}/${candidate}" ]; then
    TRACE_FILE="${TARGET_DIR}/${candidate}"
    break
  fi
done

if [ -z "${TRACE_FILE}" ]; then
  echo "Could not find inference_trace.json/run_receipt.json/receipt_bundle.json in ${TARGET_DIR}" >&2
  exit 1
fi

cp "${TRACE_FILE}" "${TRACE_FILE}.bak"
echo "[audit] Flipping a bit in router_decisions inside $(basename "${TRACE_FILE}")"
sed -i '' '0,/router_decisions/{s/0/1/}' "${TRACE_FILE}"

OUTPUT_JSON="${WORKDIR}/verify_output.json"
set +e
aosctl --json verify-receipt --bundle "${TARGET_DIR}" >"${OUTPUT_JSON}" 2>&1
STATUS=$?
set -e

if [ ${STATUS} -eq 0 ]; then
  echo "❌ Verification unexpectedly succeeded on tampered receipt" >&2
  exit 1
fi

if ! grep -q "TRACE_TAMPER" "${OUTPUT_JSON}"; then
  echo "❌ Verification failed but TRACE_TAMPER not reported" >&2
  cat "${OUTPUT_JSON}"
  exit 1
fi

echo "✅ Tampering detected (TRACE_TAMPER). Details logged to ${OUTPUT_JSON}"
