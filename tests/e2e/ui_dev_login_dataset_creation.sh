#!/usr/bin/env bash
set -euo pipefail

BASE_URL="http://localhost:18080"

if [[ "${AOS_DEV_NO_AUTH:-}" != "1" ]]; then
  echo "AOS_DEV_NO_AUTH=1 is required for this test" >&2
  exit 1
fi

if ! curl -sf "${BASE_URL}/healthz" >/dev/null 2>&1; then
  echo "Backend not healthy at ${BASE_URL}/healthz" >&2
  exit 1
fi

export CODEX_HOME="${CODEX_HOME:-$HOME/.codex}"
export PWCLI="$CODEX_HOME/skills/playwright/scripts/playwright_cli.sh"
export PLAYWRIGHT_CLI_SESSION="dev_login_dataset_creation"

if [[ ! -x "$PWCLI" ]]; then
  echo "Playwright CLI wrapper not found at $PWCLI" >&2
  exit 1
fi

if ! command -v npx >/dev/null 2>&1; then
  echo "npx is required for Playwright CLI. Install Node.js/npm first." >&2
  exit 1
fi

MANIFEST_PATH="training/datasets/docs/adapteros_qa/manifest.json"
JSONL_PATH="training/datasets/docs/adapteros_qa/adapteros-qa.jsonl"

if [[ ! -f "$MANIFEST_PATH" ]] || [[ ! -f "$JSONL_PATH" ]]; then
  echo "Missing dataset fixtures under training/datasets/docs/adapteros_qa" >&2
  exit 1
fi

mkdir -p var/playwright

MANIFEST_ABS="$(pwd)/${MANIFEST_PATH}"
JSONL_ABS="$(pwd)/${JSONL_PATH}"

"$PWCLI" open "${BASE_URL}/datasets"
"$PWCLI" snapshot

# Click "Upload Dataset"
UPLOAD_BUTTON_REF=$("$PWCLI" snapshot | rg -n "button \"Upload Dataset\"" | head -n 1 | sed -E 's/.*\[(e[0-9]+)\].*/\1/')
if [[ -z "$UPLOAD_BUTTON_REF" ]]; then
  echo "Could not find Upload Dataset button" >&2
  exit 1
fi
"$PWCLI" click "$UPLOAD_BUTTON_REF"
"$PWCLI" snapshot

# Fill dataset name
DATASET_NAME_REF=$("$PWCLI" snapshot | rg -n "Dataset Name" -A 3 | rg -o "\[(e[0-9]+)\]" | head -n 1 | tr -d '[]')
if [[ -z "$DATASET_NAME_REF" ]]; then
  echo "Could not find Dataset Name input" >&2
  exit 1
fi
"$PWCLI" fill "$DATASET_NAME_REF" "dev-login-e2e"

# Upload manifest and jsonl files via selectors
"$PWCLI" run-code "await page.setInputFiles('input[type=\"file\"][accept=\".json\"]', '${MANIFEST_ABS}');"
"$PWCLI" run-code "await page.setInputFiles('input[type=\"file\"][accept=\".jsonl\"]', '${JSONL_ABS}');"

# Click "Upload dataset"
UPLOAD_REF=$("$PWCLI" snapshot | rg -n "button \"Upload dataset\"" | head -n 1 | sed -E 's/.*\[(e[0-9]+)\].*/\1/')
if [[ -z "$UPLOAD_REF" ]]; then
  echo "Could not find Upload dataset button" >&2
  exit 1
fi
"$PWCLI" click "$UPLOAD_REF"

# Wait for navigation
"$PWCLI" run-code "await page.waitForURL(/\\/datasets\\/.+/, { timeout: 15000 });"

DATASET_PATH=$("$PWCLI" eval "window.location.pathname" | tr -d '\r')
DATASET_PATH="${DATASET_PATH%\"}"
DATASET_PATH="${DATASET_PATH#\"}"

if [[ ! "$DATASET_PATH" =~ ^/datasets/ ]]; then
  echo "Expected navigation to /datasets/{id}, got: $DATASET_PATH" >&2
  exit 1
fi

DATASET_ID="${DATASET_PATH#/datasets/}"

# Cleanup dataset
curl -s -X DELETE "${BASE_URL}/v1/datasets/${DATASET_ID}" >/dev/null 2>&1 || true

"$PWCLI" screenshot var/playwright/dev_login_dataset_creation.png

echo "UI dev login dataset creation completed: ${DATASET_ID}"
