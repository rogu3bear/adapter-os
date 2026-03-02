#!/usr/bin/env python3
"""Generate prod-cut route closure artifacts from runtime inventory and OpenAPI."""

from __future__ import annotations

import argparse
import csv
import datetime as dt
import json
import re
import sys
from dataclasses import dataclass
from datetime import date
from pathlib import Path
from typing import Dict, Iterable, List, Set, Tuple

ROOT = Path(__file__).resolve().parents[2]
INVENTORY_JSON = ROOT / "docs/generated/api-route-inventory.json"
OPENAPI_JSON = ROOT / "docs/api/openapi.json"
RUNTIME_EXCLUSIONS = ROOT / "docs/api/openapi_route_coverage_exclusions.txt"
OPENAPI_ONLY_ALLOWLIST = ROOT / "docs/api/openapi_only_route_allowlist.csv"
PARAM_MISMATCH_ALLOWLIST = ROOT / "docs/api/openapi_param_mismatch_allowlist.csv"
DEFAULT_OUT_DIR = ROOT / ".planning/prod-cut/artifacts"

SHAPE_RE = re.compile(r"\{[^/{}]+\}")


@dataclass(frozen=True)
class AllowlistEntry:
    key: str
    owner: str
    expires_on: date
    reason: str
    source: str


def normalize_shape(path: str) -> str:
    return SHAPE_RE.sub("{}", path)


def load_runtime_paths() -> Tuple[List[str], Dict[str, List[str]], str]:
    data = json.loads(INVENTORY_JSON.read_text(encoding="utf-8"))
    tiers = data.get("tiers", {})
    paths = sorted(
        {
            path
            for tier_paths in tiers.values()
            for path in tier_paths
            if isinstance(path, str) and path.startswith("/")
        }
    )
    source = str(data.get("source", "docs/generated/api-route-inventory.json"))
    return paths, tiers, source


def load_openapi_paths() -> Tuple[Dict[str, List[str]], str]:
    data = json.loads(OPENAPI_JSON.read_text(encoding="utf-8"))
    methods_by_path: Dict[str, List[str]] = {}
    for path, operations in data.get("paths", {}).items():
        if not isinstance(path, str) or not path.startswith("/"):
            continue
        if isinstance(operations, dict):
            methods = sorted(
                method.upper()
                for method, value in operations.items()
                if isinstance(value, dict) and method.lower() != "parameters"
            )
        else:
            methods = []
        methods_by_path[path] = methods or ["*"]
    source = "docs/api/openapi.json"
    return methods_by_path, source


def load_runtime_exclusion_shapes() -> Set[str]:
    if not RUNTIME_EXCLUSIONS.exists():
        return set()

    shapes: Set[str] = set()
    for raw_line in RUNTIME_EXCLUSIONS.read_text(encoding="utf-8").splitlines():
        line = raw_line.split("#", 1)[0].strip()
        if line.startswith("/"):
            shapes.add(normalize_shape(line))
    return shapes


def load_allowlist(path: Path, key_field: str) -> Dict[str, AllowlistEntry]:
    if not path.exists():
        return {}

    with path.open("r", encoding="utf-8", newline="") as handle:
        reader = csv.DictReader(handle)
        required_fields = {key_field, "owner", "expires_on"}
        fieldnames = set(reader.fieldnames or [])
        missing = required_fields - fieldnames
        if missing:
            raise ValueError(
                f"{path.relative_to(ROOT)} missing required columns: {sorted(missing)}"
            )
        if "reason" not in fieldnames and "rationale" not in fieldnames:
            raise ValueError(
                f"{path.relative_to(ROOT)} missing required column: reason (or rationale)"
            )

        loaded: Dict[str, AllowlistEntry] = {}
        for idx, row in enumerate(reader, start=2):
            key = (row.get(key_field) or "").strip()
            if not key:
                continue

            owner = (row.get("owner") or "").strip()
            expires_on_raw = (row.get("expires_on") or "").strip()
            reason = (row.get("reason") or row.get("rationale") or "").strip()

            if not owner:
                raise ValueError(f"{path.relative_to(ROOT)}:{idx} missing owner")
            if not reason:
                raise ValueError(f"{path.relative_to(ROOT)}:{idx} missing reason")
            try:
                expires_on = date.fromisoformat(expires_on_raw)
            except ValueError as exc:
                raise ValueError(
                    f"{path.relative_to(ROOT)}:{idx} invalid expires_on '{expires_on_raw}'"
                ) from exc

            loaded[key] = AllowlistEntry(
                key=key,
                owner=owner,
                expires_on=expires_on,
                reason=reason,
                source=f"{path.relative_to(ROOT)}:{idx}",
            )
    return loaded


def is_active(entry: AllowlistEntry) -> bool:
    return entry.expires_on >= date.today()


def domain_for(path: str) -> str:
    if path.startswith("/v1/adapters") or path.startswith("/v1/adapter-") or path.startswith("/v1/repos"):
        return "slice_a_adapters_repos"
    if (
        path.startswith("/v1/stacks")
        or path.startswith("/v1/adapter-stacks")
        or path.startswith("/v1/provenance")
        or path.startswith("/v1/security/key-rotations")
    ):
        return "slice_b_stacks_provenance_security"
    if path.startswith("/v1/training") or path.startswith("/v1/jobs"):
        return "slice_c_training_jobs_checkpoints"
    if path.startswith("/v1/auth") or path.startswith("/v1/api-keys") or path.startswith("/v1/tenants"):
        return "slice_d_auth_tenant"
    if path.startswith("/v1/tutorials"):
        return "slice_e_tutorials"
    return "slice_e_residual"


def target_week_for_domain(domain: str) -> str:
    if domain.startswith("slice_a"):
        return "week_1"
    if domain.startswith("slice_b"):
        return "week_1"
    if domain.startswith("slice_c"):
        return "week_2"
    if domain.startswith("slice_d"):
        return "week_2"
    return "week_2"


def _shape_index(paths: Iterable[str]) -> Dict[str, List[str]]:
    index: Dict[str, List[str]] = {}
    for path in paths:
        index.setdefault(normalize_shape(path), []).append(path)
    for key in index:
        index[key] = sorted(index[key])
    return index


def write_json(path: Path, payload: object) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_csv(path: Path, rows: List[Dict[str, str]]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="") as handle:
        fieldnames = [
            "method",
            "route",
            "domain",
            "runtime_present",
            "openapi_present",
            "decision",
            "owner",
            "target_week",
            "evidence_ref",
        ]
        writer = csv.DictWriter(handle, fieldnames=fieldnames)
        writer.writeheader()
        writer.writerows(rows)


def _shape_detail_lines(
    shapes: List[str],
    runtime_shape_index: Dict[str, List[str]],
    openapi_shape_index: Dict[str, List[str]],
) -> List[str]:
    lines: List[str] = []
    for shape in shapes:
        runtime_variants = ", ".join(runtime_shape_index.get(shape, [])) or "(none)"
        openapi_variants = ", ".join(openapi_shape_index.get(shape, [])) or "(none)"
        lines.append(f"- `{shape}`")
        lines.append(f"  runtime: {runtime_variants}")
        lines.append(f"  openapi: {openapi_variants}")
    return lines


def write_summary(
    path: Path,
    *,
    runtime_count: int,
    openapi_count: int,
    runtime_missing_shape_after_exclusions: List[str],
    openapi_only_shape_after_allowlist: List[str],
    unresolved_param_mismatch_shapes: List[str],
    runtime_shape_index: Dict[str, List[str]],
    openapi_shape_index: Dict[str, List[str]],
) -> None:
    lines = [
        "# Route Closure Summary",
        "",
        f"Generated: {dt.datetime.now(dt.timezone.utc).isoformat()}",
        "",
        "## Counts",
        f"- Runtime routes: {runtime_count}",
        f"- OpenAPI routes: {openapi_count}",
        (
            "- Runtime missing from OpenAPI (shape-based, after exclusions): "
            f"{len(runtime_missing_shape_after_exclusions)}"
        ),
        (
            "- OpenAPI-only (shape-based, after allowlist): "
            f"{len(openapi_only_shape_after_allowlist)}"
        ),
        (
            "- Unresolved parameter-name mismatch shapes: "
            f"{len(unresolved_param_mismatch_shapes)}"
        ),
        "",
        "## Runtime Missing from OpenAPI (shape-based, after exclusions)",
    ]

    if runtime_missing_shape_after_exclusions:
        lines.extend(
            _shape_detail_lines(
                runtime_missing_shape_after_exclusions,
                runtime_shape_index,
                openapi_shape_index,
            )
        )
    else:
        lines.append("- None")

    lines.extend(["", "## OpenAPI-only (shape-based, after allowlist)"])
    if openapi_only_shape_after_allowlist:
        lines.extend(
            _shape_detail_lines(
                openapi_only_shape_after_allowlist,
                runtime_shape_index,
                openapi_shape_index,
            )
        )
    else:
        lines.append("- None")

    lines.extend(["", "## Unresolved Param-name Mismatch Shapes"])
    if unresolved_param_mismatch_shapes:
        lines.extend(
            _shape_detail_lines(
                unresolved_param_mismatch_shapes,
                runtime_shape_index,
                openapi_shape_index,
            )
        )
    else:
        lines.append("- None")

    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def lookup_openapi_only_allowlist_entry(
    route: str, shape: str, openapi_allowlist: Dict[str, AllowlistEntry]
) -> AllowlistEntry | None:
    direct = openapi_allowlist.get(route)
    if direct is not None:
        return direct
    for key, entry in openapi_allowlist.items():
        if normalize_shape(key) == shape:
            return entry
    return None


def display_path(path: Path) -> str:
    try:
        return str(path.resolve().relative_to(ROOT))
    except ValueError:
        return str(path)


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--out-dir", default=str(DEFAULT_OUT_DIR))
    parser.add_argument(
        "--strict-openapi-only",
        action="store_true",
        help="Fail when OpenAPI-only route shapes remain after allowlist filtering.",
    )
    parser.add_argument(
        "--strict-param-mismatch",
        action="store_true",
        help="Fail when param-name mismatch shapes remain unresolved after allowlist filtering.",
    )
    args = parser.parse_args()

    try:
        openapi_allowlist = load_allowlist(OPENAPI_ONLY_ALLOWLIST, "route")
        param_shape_allowlist = load_allowlist(PARAM_MISMATCH_ALLOWLIST, "shape")
    except ValueError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1

    runtime_paths, runtime_tiers, runtime_source = load_runtime_paths()
    openapi_methods, openapi_source = load_openapi_paths()
    openapi_paths = sorted(openapi_methods.keys())

    runtime_set = set(runtime_paths)
    openapi_set = set(openapi_paths)

    runtime_shape_index = _shape_index(runtime_paths)
    openapi_shape_index = _shape_index(openapi_paths)
    runtime_shape_set = set(runtime_shape_index.keys())
    openapi_shape_set = set(openapi_shape_index.keys())

    runtime_exclusion_shapes = load_runtime_exclusion_shapes()
    runtime_missing_shape_raw = sorted(runtime_shape_set - openapi_shape_set)
    runtime_missing_shape_after_exclusions = sorted(
        shape for shape in runtime_missing_shape_raw if shape not in runtime_exclusion_shapes
    )

    active_openapi_allowlist_shapes = {
        normalize_shape(route)
        for route, entry in openapi_allowlist.items()
        if is_active(entry)
    }
    openapi_only_shape_raw = sorted(openapi_shape_set - runtime_shape_set)
    openapi_only_shape_after_allowlist = sorted(
        shape
        for shape in openapi_only_shape_raw
        if shape not in active_openapi_allowlist_shapes
    )

    mismatch_shapes = sorted(
        shape
        for shape in (runtime_shape_set & openapi_shape_set)
        if runtime_shape_index.get(shape, []) != openapi_shape_index.get(shape, [])
    )
    active_param_allowlist_shapes = {
        shape for shape, entry in param_shape_allowlist.items() if is_active(entry)
    }
    unresolved_param_mismatch_shapes = sorted(
        shape for shape in mismatch_shapes if shape not in active_param_allowlist_shapes
    )

    out_dir = Path(args.out_dir).resolve()
    runtime_json = out_dir / "runtime_routes.json"
    openapi_json = out_dir / "openapi_routes.json"
    matrix_csv = out_dir / "route_closure_matrix.csv"
    summary_md = out_dir / "route_closure_summary.md"

    write_json(
        runtime_json,
        {
            "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
            "source": runtime_source,
            "tiers": runtime_tiers,
            "routes": [
                {
                    "route": path,
                    "shape": normalize_shape(path),
                    "domain": domain_for(path),
                    "excluded_from_openapi_coverage": normalize_shape(path)
                    in runtime_exclusion_shapes,
                }
                for path in runtime_paths
            ],
        },
    )

    write_json(
        openapi_json,
        {
            "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
            "source": openapi_source,
            "routes": [
                {
                    "route": path,
                    "shape": normalize_shape(path),
                    "methods": openapi_methods[path],
                    "domain": domain_for(path),
                    "approved_openapi_only_exclusion": normalize_shape(path)
                    in active_openapi_allowlist_shapes,
                }
                for path in openapi_paths
            ],
        },
    )

    rows: List[Dict[str, str]] = []
    for route in sorted(runtime_set | openapi_set):
        runtime_present = route in runtime_set
        openapi_present = route in openapi_set
        shape = normalize_shape(route)
        domain = domain_for(route)
        mismatch_shape = shape in mismatch_shapes

        if runtime_present and openapi_present:
            decision = "implement"
            owner = "api-contract-team"
            evidence_ref = "runtime+openapi"
        elif runtime_present:
            if shape in runtime_exclusion_shapes:
                decision = "exclude"
                owner = "api-contract-team"
                evidence_ref = "docs/api/openapi_route_coverage_exclusions.txt"
            else:
                decision = "implement"
                owner = "api-contract-team"
                evidence_ref = "runtime-only"
        else:
            allowlist_entry = lookup_openapi_only_allowlist_entry(
                route, shape, openapi_allowlist
            )
            if allowlist_entry is not None and is_active(allowlist_entry):
                decision = "exclude"
                owner = allowlist_entry.owner or "api-contract-team"
                evidence_ref = f"docs/api/openapi_only_route_allowlist.csv:{allowlist_entry.source}"
            else:
                decision = "remove"
                owner = "api-contract-team"
                evidence_ref = "openapi-only"

        if mismatch_shape:
            evidence_ref = f"{evidence_ref};shape-param-mismatch"

        rows.append(
            {
                "method": "|".join(openapi_methods.get(route, ["*"])),
                "route": route,
                "domain": domain,
                "runtime_present": "true" if runtime_present else "false",
                "openapi_present": "true" if openapi_present else "false",
                "decision": decision,
                "owner": owner,
                "target_week": target_week_for_domain(domain),
                "evidence_ref": evidence_ref,
            }
        )

    write_csv(matrix_csv, rows)

    write_summary(
        summary_md,
        runtime_count=len(runtime_paths),
        openapi_count=len(openapi_paths),
        runtime_missing_shape_after_exclusions=runtime_missing_shape_after_exclusions,
        openapi_only_shape_after_allowlist=openapi_only_shape_after_allowlist,
        unresolved_param_mismatch_shapes=unresolved_param_mismatch_shapes,
        runtime_shape_index=runtime_shape_index,
        openapi_shape_index=openapi_shape_index,
    )

    print("=== Route Inventory vs OpenAPI Coverage ===")
    print(f"Runtime paths: {len(runtime_paths)}")
    print(f"OpenAPI paths: {len(openapi_paths)}")
    print(
        "Runtime path-shapes missing from OpenAPI (after exclusions): "
        f"{len(runtime_missing_shape_after_exclusions)}"
    )
    print(
        "OpenAPI-only path-shapes (after allowlist): "
        f"{len(openapi_only_shape_after_allowlist)}"
    )
    print(
        "Parameter-name mismatch path-shapes (after allowlist): "
        f"{len(unresolved_param_mismatch_shapes)}"
    )
    print(f"generated: {display_path(runtime_json)}")
    print(f"generated: {display_path(openapi_json)}")
    print(f"generated: {display_path(matrix_csv)}")
    print(f"generated: {display_path(summary_md)}")

    if runtime_missing_shape_after_exclusions:
        print(
            "ERROR: runtime route shapes missing from OpenAPI after exclusions:",
            file=sys.stderr,
        )
        for shape in runtime_missing_shape_after_exclusions:
            print(
                f"{shape} :: runtime={', '.join(runtime_shape_index.get(shape, []))}",
                file=sys.stderr,
            )
        return 1

    if args.strict_openapi_only and openapi_only_shape_after_allowlist:
        print(
            "ERROR: OpenAPI-only route shapes remain after allowlist filtering.",
            file=sys.stderr,
        )
        for shape in openapi_only_shape_after_allowlist:
            print(
                f"{shape} :: openapi={', '.join(openapi_shape_index.get(shape, []))}",
                file=sys.stderr,
            )
        return 1

    if args.strict_param_mismatch and unresolved_param_mismatch_shapes:
        print(
            "ERROR: Parameter-name mismatch shapes remain unresolved.",
            file=sys.stderr,
        )
        for shape in unresolved_param_mismatch_shapes:
            print(
                f"{shape} :: runtime={', '.join(runtime_shape_index.get(shape, []))} :: openapi={', '.join(openapi_shape_index.get(shape, []))}",
                file=sys.stderr,
            )
        return 1

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
