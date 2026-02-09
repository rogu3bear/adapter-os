#!/usr/bin/env python3
"""Validate API route tier invariants against generated inventory."""

from __future__ import annotations

import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
INVENTORY = ROOT / "docs/generated/api-route-inventory.json"


def fail(msg: str) -> int:
    print(f"FAIL: {msg}", file=sys.stderr)
    return 1


def main() -> int:
    if not INVENTORY.exists():
        return fail("Missing docs/generated/api-route-inventory.json")

    data = json.loads(INVENTORY.read_text(encoding="utf-8"))
    tiers = data.get("tiers", {})

    public = set(tiers.get("public", []))
    internal = set(tiers.get("internal", []))
    protected = set(tiers.get("protected", []))

    if "/v1/workers/register" in public:
        return fail("Worker registration must never be public")
    if "/v1/workers/register" not in internal:
        return fail("Worker registration must be in internal tier")

    if "/v1/auth/refresh" not in public:
        return fail("/v1/auth/refresh expected in public tier")

    expected_protected = {
        "/v1/infer",
        "/v1/infer/stream",
        "/v1/workers/spawn",
        "/v1/workers/{worker_id}/stop",
        "/v1/workers/{worker_id}/drain",
    }
    missing = sorted(expected_protected - protected)
    if missing:
        return fail(f"Missing protected routes: {missing}")

    tier_overlap = (public & protected) | (public & internal) | (internal & protected)
    if tier_overlap:
        return fail(f"Routes appear in multiple tiers: {sorted(tier_overlap)}")

    print("=== API Route Tier Check: PASSED ===")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
