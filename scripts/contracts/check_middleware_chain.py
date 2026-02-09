#!/usr/bin/env python3
"""Validate middleware chain ordering contract."""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
INVENTORY = ROOT / "docs/generated/middleware-chain.json"


def fail(msg: str) -> int:
    print(f"FAIL: {msg}", file=sys.stderr)
    return 1


def main() -> int:
    if not INVENTORY.exists():
        return fail("Missing docs/generated/middleware-chain.json")

    data = json.loads(INVENTORY.read_text(encoding="utf-8"))

    protected_expected = data.get("protected_expected_order", [])
    protected_present = data.get("protected_present_order", [])
    global_expected = data.get("global_expected_order", [])
    global_present = data.get("global_present_order", [])

    if protected_expected != protected_present:
        return fail(
            "Protected middleware order drifted: "
            f"expected={protected_expected} actual={protected_present}"
        )

    if global_expected != global_present:
        return fail(
            "Global middleware order drifted: "
            f"expected={global_expected} actual={global_present}"
        )

    print("=== Middleware Chain Check: PASSED ===")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
