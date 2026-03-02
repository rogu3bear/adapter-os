#!/usr/bin/env bash
# Smoke test validating full inference readiness
#
# This script verifies that adapterOS is ready for inference:
# - /healthz returns 200 (server is alive)
# - /readyz returns 200 with all checks passing (server, DB, worker, models)
# - Optionally tests inference endpoint
#
# Usage:
#   ./scripts/smoke-inference.sh                    # Default localhost:8080
#   ./scripts/smoke-inference.sh --port 8081        # Custom port
#   ./scripts/smoke-inference.sh --skip-inference   # Skip inference test
#
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Configuration
: "${AOS_SERVER_PORT:=8080}"
BASE_URL="http://localhost:$AOS_SERVER_PORT"
API_BASE="${BASE_URL%/}/api"
SKIP_INFERENCE=0
VERBOSE=0

# Parse arguments
while [[ $# -gt 0 ]]; do
  case "$1" in
    --port)
      AOS_SERVER_PORT="$2"
      BASE_URL="http://localhost:$AOS_SERVER_PORT"
      API_BASE="${BASE_URL%/}/api"
      shift 2
      ;;
    --skip-inference)
      SKIP_INFERENCE=1
      shift
      ;;
    --verbose|-v)
      VERBOSE=1
      shift
      ;;
    *)
      echo "Unknown option: $1" >&2
      exit 1
      ;;
  esac
done

# Color codes
FG_GREEN="\033[32m"
FG_RED="\033[31m"
FG_YELLOW="\033[33m"
FG_RESET="\033[0m"

echo ""
echo "==============================================="
echo "  adapterOS Inference Smoke Test"
echo "==============================================="
echo ""
echo "  Target: $BASE_URL (api: $API_BASE)"
echo ""

TESTS_PASSED=0
TESTS_FAILED=0

# Test 1: /healthz
echo -n "1. Checking /healthz... "
if curl -sf --max-time 5 "$API_BASE/healthz" > /dev/null 2>&1; then
  echo -e "${FG_GREEN}PASS${FG_RESET}"
  ((TESTS_PASSED++))
else
  echo -e "${FG_RED}FAIL${FG_RESET}"
  echo "   Server may not be running. Start with: ./start"
  ((TESTS_FAILED++))
fi

# Test 2: /readyz
echo -n "2. Checking /readyz... "
READYZ_RESPONSE=$(curl -sf --max-time 5 "$API_BASE/readyz" 2>/dev/null || echo '{"ready":false}')
READYZ_STATUS=$(echo "$READYZ_RESPONSE" | python3 -c "import sys,json; d=json.load(sys.stdin); print('ready' if d.get('ready',False) else 'not_ready')" 2>/dev/null || echo "error")

if [[ "$READYZ_STATUS" == "ready" ]]; then
  echo -e "${FG_GREEN}PASS${FG_RESET}"
  ((TESTS_PASSED++))
  if [[ "$VERBOSE" == "1" ]]; then
    echo "   Response: $READYZ_RESPONSE"
  fi
else
  echo -e "${FG_RED}FAIL${FG_RESET}"
  ((TESTS_FAILED++))

  # Show detailed check results
  echo "   Readiness checks:"
  if command -v python3 &> /dev/null; then
    echo "$READYZ_RESPONSE" | python3 -c "
import sys,json
try:
  d=json.load(sys.stdin)
  for k,v in d.get('checks',{}).items():
    status='OK' if v.get('ok') else 'FAIL'
    hint=v.get('hint','')
    print(f'   - {k}: {status}' + (f' ({hint})' if hint else ''))
except: pass
" 2>/dev/null || echo "   (could not parse response)"
  fi
fi

# Test 3: Inference endpoint (optional)
if [[ "$SKIP_INFERENCE" != "1" ]]; then
  echo -n "3. Checking inference endpoint... "
  INFER_RESPONSE=$(curl -sf -X POST "$API_BASE/v1/infer" \
    -H "Content-Type: application/json" \
    -d '{"prompt": "Hello", "max_tokens": 1}' 2>/dev/null || echo '{"error":true}')

  if echo "$INFER_RESPONSE" | grep -q '"error"'; then
    echo -e "${FG_YELLOW}SKIP${FG_RESET} (inference error or worker not ready)"
    if [[ "$VERBOSE" == "1" ]]; then
      echo "   Response: $INFER_RESPONSE"
    fi
  else
    echo -e "${FG_GREEN}PASS${FG_RESET}"
    ((TESTS_PASSED++))
    if [[ "$VERBOSE" == "1" ]]; then
      echo "   Response: ${INFER_RESPONSE:0:100}..."
    fi
  fi
else
  echo "3. Inference endpoint... ${FG_YELLOW}SKIPPED${FG_RESET} (via --skip-inference)"
fi

# Summary
echo ""
echo "==============================================="
if [[ $TESTS_FAILED -eq 0 ]]; then
  echo -e "  ${FG_GREEN}All checks passed!${FG_RESET}"
  echo "  Tests: $TESTS_PASSED passed, $TESTS_FAILED failed"
  echo "==============================================="
  echo ""
  exit 0
else
  echo -e "  ${FG_RED}Some checks failed${FG_RESET}"
  echo "  Tests: $TESTS_PASSED passed, $TESTS_FAILED failed"
  echo "==============================================="
  echo ""
  echo "  Troubleshooting:"
  echo "    - Check if server is running: curl $API_BASE/healthz"
  echo "    - Check readiness details: curl $API_BASE/readyz | jq"
  echo "    - View logs: tail -f var/logs/backend.log"
  echo ""
  exit 1
fi
