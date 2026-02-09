#!/usr/bin/env python3
"""Validate UI route contract against generated inventory."""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
INVENTORY = ROOT / "docs/generated/ui-route-inventory.json"


def fail(msg: str) -> int:
    print(f"FAIL: {msg}", file=sys.stderr)
    return 1


def main() -> int:
    if not INVENTORY.exists():
        return fail("Missing docs/generated/ui-route-inventory.json")

    data = json.loads(INVENTORY.read_text(encoding="utf-8"))
    public_routes = set(data.get("public_routes", []))
    protected_routes = set(data.get("protected_routes", []))

    expected_public = {"/login", "/safe", "/style-audit"}
    if public_routes != expected_public:
        return fail(
            "Public UI routes drifted. "
            f"expected={sorted(expected_public)} actual={sorted(public_routes)}"
        )

    if public_routes & protected_routes:
        return fail("A route is both public and protected")

    required_protected = {"/", "/chat", "/runs", "/workers", "/adapters"}
    missing = sorted(required_protected - protected_routes)
    if missing:
        return fail(f"Missing protected UI routes: {missing}")

    print("=== UI Route Contract Check: PASSED ===")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
