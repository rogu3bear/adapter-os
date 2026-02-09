#!/usr/bin/env bash
# generate_route_map.sh - Generate ROUTE_MAP.md from canonical route inventory.
#
# Usage: ./scripts/dev/generate_route_map.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
OUTPUT_FILE="$REPO_ROOT/docs/api/ROUTE_MAP.md"

cd "$REPO_ROOT"

scripts/contracts/generate_contract_artifacts.py >/dev/null

python3 - <<'PY' > "$OUTPUT_FILE"
import json
from pathlib import Path

root = Path.cwd()
inv = json.loads((root / "docs/generated/api-route-inventory.json").read_text(encoding="utf-8"))

source = inv["source"]
counts = inv["counts"]
tiers = inv["tiers"]

def tier_label(name: str) -> str:
    return {
        "health": "Health",
        "public": "Public",
        "optional_auth": "Optional Auth",
        "internal": "Internal",
        "protected": "Protected",
        "spoke_audit": "Spoke Audit",
    }.get(name, name)

print("# adapterOS API Route Map")
print()
print("> **Auto-generated:** Do not edit manually.")
print("> Run `./scripts/dev/generate_route_map.sh` to regenerate.")
print()
print("## Overview")
print()
print("| Metric | Count |")
print("|--------|-------|")
print(f"| **Total Route Registrations (tiered)** | {sum(counts.values())} |")
for tier in ["health", "public", "optional_auth", "internal", "protected", "spoke_audit"]:
    print(f"| **{tier_label(tier)} routes** | {counts.get(tier, 0)} |")
print()
print("## Canonical Source")
print()
print(f"- `{source}`")
print()
print("## Route Table")
print()
print("| Tier | Path |")
print("|------|------|")
for tier in ["health", "public", "optional_auth", "internal", "protected", "spoke_audit"]:
    for path in tiers.get(tier, []):
        print(f"| `{tier}` | `{path}` |")
print()
print("## Notes")
print()
print("- This map is route-path inventory by security tier, not a complete method/handler signature map.")
print("- For method-level OpenAPI details, see `docs/api/openapi.json`.")
print("- Tier and middleware contracts are validated by scripts under `scripts/contracts/`.")
PY

echo "Route map written to $OUTPUT_FILE"
