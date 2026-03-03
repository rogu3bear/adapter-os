#!/usr/bin/env python3
"""API surface governance checks and matrix generation.

Default mode:
  - Fails only when endpoints marked as kept/kept_no_ui in the matrix
    disappear from runtime inventory or drift tier.

Write mode (--write-matrix):
  - Regenerates docs/api-surface-matrix.md and
    docs/generated/api-surface-matrix.json from:
      - docs/generated/api-route-inventory.json (canonical route inventory)
      - request_log telemetry over the last 24 hours
"""

from __future__ import annotations

import argparse
import json
import re
import sqlite3
import sys
from dataclasses import dataclass
from datetime import UTC, datetime, timedelta
from pathlib import Path
from typing import Dict, Iterable, List, Tuple

ROOT = Path(__file__).resolve().parents[2]
INVENTORY_PATH = ROOT / "docs/generated/api-route-inventory.json"
MATRIX_JSON_PATH = ROOT / "docs/generated/api-surface-matrix.json"
MATRIX_MD_PATH = ROOT / "docs/api-surface-matrix.md"
DEFAULT_DB_PATH = ROOT / "var/aos-cp.sqlite3"

STATUS_KEPT = "kept"
STATUS_KEPT_NO_UI = "kept_no_ui"
STATUS_UNUSED = "unused_documented"
VALID_STATUSES = {STATUS_KEPT, STATUS_KEPT_NO_UI, STATUS_UNUSED}

STRATEGIC_KEEP_TIERS = {"health", "public", "optional_auth"}
STRATEGIC_KEEP_NO_UI_TIERS = {"internal"}

UI_BACKED_API_PREFIXES = (
    "/v1/adapters",
    "/v1/admin",
    "/v1/audit",
    "/v1/chat",
    "/v1/dashboard/config",
    "/v1/diag/runs",
    "/v1/documents",
    "/v1/models",
    "/v1/policies",
    "/v1/system",
    "/v1/training",
    "/v1/ui/config",
    "/v1/workers",
)


@dataclass(frozen=True)
class UsageSnapshot:
    hits_24h: int
    last_seen_24h: str | None


def fail(message: str) -> int:
    print(f"FAIL: {message}", file=sys.stderr)
    return 1


def load_inventory() -> Dict[str, List[str]]:
    if not INVENTORY_PATH.exists():
        raise FileNotFoundError(f"missing inventory: {INVENTORY_PATH}")
    payload = json.loads(INVENTORY_PATH.read_text(encoding="utf-8"))
    tiers = payload.get("tiers")
    if not isinstance(tiers, dict):
        raise ValueError("invalid api-route-inventory.json: missing tiers object")
    normalized: Dict[str, List[str]] = {}
    for tier, paths in tiers.items():
        if not isinstance(paths, list):
            continue
        normalized[tier] = sorted(str(p) for p in paths)
    return normalized


def build_inventory_map(tiers: Dict[str, List[str]]) -> Dict[str, str]:
    inventory_map: Dict[str, str] = {}
    for tier, paths in tiers.items():
        for path in paths:
            # Keep first tier in case of accidental overlap; overlap should not happen.
            inventory_map.setdefault(path, tier)
    return inventory_map


def path_to_pattern(path: str) -> str:
    out = []
    i = 0
    while i < len(path):
        ch = path[i]
        if ch == "{":
            end = path.find("}", i + 1)
            if end == -1:
                out.append("\\{")
                i += 1
                continue
            out.append("[^/]+")
            i = end + 1
            continue
        if ch in ".^$*+?[]()|\\":
            out.append("\\" + ch)
        else:
            out.append(ch)
        i += 1
    return "^" + "".join(out) + "$"


def normalize_telemetry_route(
    route_or_path: str, templates: Dict[str, str], cache: Dict[str, str | None]
) -> str | None:
    if route_or_path in cache:
        return cache[route_or_path]
    if route_or_path in templates:
        cache[route_or_path] = route_or_path
        return route_or_path
    for template, pattern in templates.items():
        if re.match(pattern, route_or_path):
            cache[route_or_path] = template
            return template
    cache[route_or_path] = None
    return None


def load_usage_by_route(
    db_path: Path, inventory_paths: Iterable[str]
) -> Tuple[Dict[str, UsageSnapshot], Dict[str, object]]:
    metadata: Dict[str, object] = {
        "db_path": str(db_path),
        "window": "last_24_hours",
        "query": (
            "SELECT COALESCE(NULLIF(route,''), path) AS route_or_path, "
            "COUNT(*) AS hits, MAX(timestamp) AS last_seen "
            "FROM request_log "
            "WHERE datetime(timestamp) >= datetime('now','-24 hours') "
            "GROUP BY route_or_path"
        ),
        "available": False,
        "rows_24h": 0,
    }

    usage: Dict[str, UsageSnapshot] = {}
    if not db_path.exists():
        metadata["note"] = "database file not found; usage defaults to zero"
        return usage, metadata

    conn = sqlite3.connect(str(db_path))
    conn.row_factory = sqlite3.Row
    try:
        tables = {
            row["name"]
            for row in conn.execute(
                "SELECT name FROM sqlite_master WHERE type='table'"
            ).fetchall()
        }
        if "request_log" not in tables:
            metadata["note"] = "request_log table not present; usage defaults to zero"
            return usage, metadata

        metadata["available"] = True
        templates = {path: path_to_pattern(path) for path in inventory_paths}
        cache: Dict[str, str | None] = {}
        row_count = 0
        for row in conn.execute(
            "SELECT COALESCE(NULLIF(route,''), path) AS route_or_path, "
            "COUNT(*) AS hits, MAX(timestamp) AS last_seen "
            "FROM request_log "
            "WHERE datetime(timestamp) >= datetime('now','-24 hours') "
            "GROUP BY route_or_path"
        ):
            route_or_path = row["route_or_path"]
            if not route_or_path:
                continue
            matched = normalize_telemetry_route(route_or_path, templates, cache)
            if not matched:
                continue
            hits = int(row["hits"] or 0)
            last_seen = row["last_seen"]
            prev = usage.get(matched)
            if prev:
                hits += prev.hits_24h
                if prev.last_seen_24h and (not last_seen or prev.last_seen_24h > last_seen):
                    last_seen = prev.last_seen_24h
            usage[matched] = UsageSnapshot(hits_24h=hits, last_seen_24h=last_seen)
            row_count += 1
        metadata["rows_24h"] = row_count
        return usage, metadata
    finally:
        conn.close()


def is_ui_backed_api(path: str) -> bool:
    return any(path.startswith(prefix) for prefix in UI_BACKED_API_PREFIXES)


def classify_status(tier: str, path: str, hits_24h: int) -> Tuple[str, str]:
    if tier in STRATEGIC_KEEP_TIERS:
        return STATUS_KEPT, "strategic_tier_keep"
    if tier in STRATEGIC_KEEP_NO_UI_TIERS:
        return STATUS_KEPT_NO_UI, "strategic_tier_keep_no_ui"
    if hits_24h > 0:
        if is_ui_backed_api(path):
            return STATUS_KEPT, "runtime_usage_24h_ui_backed"
        return STATUS_KEPT_NO_UI, "runtime_usage_24h_no_ui"
    return STATUS_UNUSED, "no_runtime_usage_24h_document_only"


def build_rows(
    tiers: Dict[str, List[str]], usage_map: Dict[str, UsageSnapshot], now: datetime
) -> List[Dict[str, object]]:
    next_review = (now + timedelta(days=7)).date().isoformat()
    rows: List[Dict[str, object]] = []
    for tier in sorted(tiers.keys()):
        for path in tiers[tier]:
            usage = usage_map.get(path, UsageSnapshot(hits_24h=0, last_seen_24h=None))
            status, reason = classify_status(tier=tier, path=path, hits_24h=usage.hits_24h)
            rows.append(
                {
                    "path": path,
                    "tier": tier,
                    "status": status,
                    "hits_24h": usage.hits_24h,
                    "last_seen_24h": usage.last_seen_24h,
                    "owner": "adapteros-runtime",
                    "next_review": next_review,
                    "evidence": f"request_log_24h hits={usage.hits_24h}",
                    "reason": reason,
                }
            )
    return rows


def summarize_rows(rows: List[Dict[str, object]]) -> Dict[str, object]:
    by_status: Dict[str, int] = {key: 0 for key in sorted(VALID_STATUSES)}
    by_tier: Dict[str, int] = {}
    for row in rows:
        by_status[row["status"]] = by_status.get(row["status"], 0) + 1
        by_tier[row["tier"]] = by_tier.get(row["tier"], 0) + 1
    return {
        "total_endpoints": len(rows),
        "by_status": by_status,
        "by_tier": dict(sorted(by_tier.items())),
    }


def write_matrix_json(payload: Dict[str, object]) -> None:
    MATRIX_JSON_PATH.parent.mkdir(parents=True, exist_ok=True)
    MATRIX_JSON_PATH.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )


def write_matrix_markdown(payload: Dict[str, object]) -> None:
    summary = payload["summary"]
    telemetry = payload["telemetry"]
    rows = payload["rows"]

    lines: List[str] = []
    lines.append("# API Surface Matrix")
    lines.append("")
    lines.append("This matrix records API retention decisions after UI surface reduction.")
    lines.append("")
    lines.append("## Snapshot")
    lines.append("")
    lines.append(f"- Generated (UTC): `{payload['generated_at_utc']}`")
    lines.append(f"- Inventory source: `{payload['inventory_source']}`")
    lines.append(f"- Telemetry source (DB): `{telemetry['db_path']}`")
    lines.append("- Telemetry window: `last_24_hours`")
    lines.append(f"- Telemetry query: `{telemetry['query']}`")
    lines.append(f"- Telemetry rows mapped (24h): `{telemetry['rows_24h']}`")
    if telemetry.get("note"):
        lines.append(f"- Telemetry note: `{telemetry['note']}`")
    lines.append("")
    lines.append("## Status Definitions")
    lines.append("")
    lines.append(f"- `{STATUS_KEPT}`: retained and contract-guarded.")
    lines.append(f"- `{STATUS_KEPT_NO_UI}`: retained without active UI route surface.")
    lines.append(f"- `{STATUS_UNUSED}`: no 24h usage; documented only this cycle.")
    lines.append("")
    lines.append("## Summary")
    lines.append("")
    lines.append(f"- Total endpoints: `{summary['total_endpoints']}`")
    for status, count in summary["by_status"].items():
        lines.append(f"- `{status}`: `{count}`")
    lines.append("")
    lines.append("## Matrix")
    lines.append("")
    lines.append(
        "| path | tier | status | hits_24h | last_seen_24h | owner | next_review | evidence |"
    )
    lines.append("|---|---|---|---:|---|---|---|---|")
    for row in rows:
        lines.append(
            f"| `{row['path']}` | `{row['tier']}` | `{row['status']}` | "
            f"{row['hits_24h']} | {row['last_seen_24h'] or ''} | `{row['owner']}` | "
            f"`{row['next_review']}` | `{row['evidence']}` |"
        )
    lines.append("")

    MATRIX_MD_PATH.write_text("\n".join(lines), encoding="utf-8")


def generate_matrix(db_path: Path) -> Dict[str, object]:
    now = datetime.now(UTC)
    tiers = load_inventory()
    inventory_paths = [p for paths in tiers.values() for p in paths]
    usage_map, telemetry_meta = load_usage_by_route(db_path=db_path, inventory_paths=inventory_paths)
    rows = build_rows(tiers=tiers, usage_map=usage_map, now=now)
    payload = {
        "generated_at_utc": now.isoformat().replace("+00:00", "Z"),
        "inventory_source": str(INVENTORY_PATH.relative_to(ROOT)),
        "telemetry": telemetry_meta,
        "rows": rows,
        "summary": summarize_rows(rows),
    }
    return payload


def run_check() -> int:
    if not MATRIX_JSON_PATH.exists():
        return fail(
            "Missing docs/generated/api-surface-matrix.json. "
            "Run scripts/contracts/check_api_surface.py --write-matrix."
        )

    payload = json.loads(MATRIX_JSON_PATH.read_text(encoding="utf-8"))
    rows = payload.get("rows", [])
    if not isinstance(rows, list):
        return fail("Invalid matrix JSON: rows must be a list")

    tiers = load_inventory()
    inventory_map = build_inventory_map(tiers)

    failures: List[str] = []
    for row in rows:
        if not isinstance(row, dict):
            continue
        status = row.get("status")
        if status not in VALID_STATUSES:
            continue
        if status not in {STATUS_KEPT, STATUS_KEPT_NO_UI}:
            continue
        path = row.get("path")
        tier = row.get("tier")
        if not isinstance(path, str) or not isinstance(tier, str):
            failures.append(f"Malformed kept row: {row}")
            continue
        current_tier = inventory_map.get(path)
        if current_tier is None:
            failures.append(f"Kept endpoint disappeared: {path} (matrix tier={tier})")
            continue
        if current_tier != tier:
            failures.append(
                f"Kept endpoint tier drift: {path} matrix={tier} runtime={current_tier}"
            )

    if failures:
        for msg in failures:
            print(f"FAIL: {msg}", file=sys.stderr)
        return 1

    print("=== API Surface Check: PASSED ===")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--write-matrix",
        action="store_true",
        help="generate docs/api-surface-matrix.md and docs/generated/api-surface-matrix.json",
    )
    parser.add_argument(
        "--db-path",
        default=str(DEFAULT_DB_PATH),
        help="sqlite path used for request_log telemetry (default: var/aos-cp.sqlite3)",
    )
    args = parser.parse_args()

    if args.write_matrix:
        payload = generate_matrix(Path(args.db_path))
        write_matrix_json(payload)
        write_matrix_markdown(payload)
        print("Generated API surface artifacts:")
        print(f"- {MATRIX_JSON_PATH.relative_to(ROOT)}")
        print(f"- {MATRIX_MD_PATH.relative_to(ROOT)}")
        return 0

    return run_check()


if __name__ == "__main__":
    raise SystemExit(main())
