#!/usr/bin/env bash
# CI gate: validates boot + health without full training
# Wraps smoke_happy_path.sh for PLAN_3 compliance
#
# This is the lightweight CI regression gate. For full training validation,
# use scripts/golden_path_adapter_chat.sh (not suitable for every commit).
set -euo pipefail
exec "$(dirname "$0")/../test/smoke_happy_path.sh" "$@"
