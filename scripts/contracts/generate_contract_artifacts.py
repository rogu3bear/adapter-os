#!/usr/bin/env python3
"""Generate canonical contract artifacts for API/UI/startup rectification.

Outputs:
- docs/generated/api-route-inventory.json
- docs/generated/ui-route-inventory.json
- docs/generated/middleware-chain.json

Use --check to verify committed artifacts are up to date.
Use --ui-only / --check-ui for frontend-only workflows.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Dict, List, Tuple

ROOT = Path(__file__).resolve().parents[2]
ROUTES_DIR = ROOT / "crates/adapteros-server-api/src/routes"
API_ROUTES_FILE = ROUTES_DIR / "mod.rs"
UI_LIB_FILE = ROOT / "crates/adapteros-ui/src/lib.rs"
OUT_API = ROOT / "docs/generated/api-route-inventory.json"
OUT_UI = ROOT / "docs/generated/ui-route-inventory.json"
OUT_MIDDLEWARE = ROOT / "docs/generated/middleware-chain.json"
MERGE_CALL_RE = re.compile(r"([A-Za-z_][A-Za-z0-9_:]*)::[A-Za-z_][A-Za-z0-9_]*\s*\(")


def _extract_string_literal(line: str) -> str | None:
    match = re.search(r'"([^"]+)"', line)
    if not match:
        return None
    return match.group(1)


def _collect_route_literals(text: str) -> List[str]:
    lines = text.splitlines()
    routes: set[str] = set()
    pending_kind: str | None = None

    for line in lines:
        stripped = line.strip()

        if pending_kind and ("\"" in stripped):
            maybe_path = _extract_string_literal(stripped)
            if maybe_path and maybe_path.startswith("/"):
                routes.add(maybe_path)
            pending_kind = None

        if ".route(" in stripped or ".nest(" in stripped:
            pending_kind = "route" if ".route(" in stripped else "nest"
            maybe_path = _extract_string_literal(stripped)
            if maybe_path and maybe_path.startswith("/"):
                routes.add(maybe_path)
                pending_kind = None

    return sorted(routes)


def _routes_for_module(module_name: str, cache: Dict[str, List[str]]) -> List[str]:
    if module_name in cache:
        return cache[module_name]

    module_files = sorted(path for path in ROUTES_DIR.rglob(f"{module_name}.rs") if path.name != "mod.rs")
    if not module_files:
        cache[module_name] = []
        return cache[module_name]

    merged_paths: set[str] = set()
    for module_file in module_files:
        merged_paths.update(_collect_route_literals(module_file.read_text(encoding="utf-8")))
    cache[module_name] = sorted(merged_paths)
    return cache[module_name]


def _resolve_route_module_name(module_path: str) -> str | None:
    for segment in reversed(module_path.split("::")):
        if segment in {"crate", "self", "super"}:
            continue
        if any(path for path in ROUTES_DIR.rglob(f"{segment}.rs") if path.name != "mod.rs"):
            return segment
    return None


def _extract_merged_route_modules(merge_expression: str) -> List[str]:
    modules: set[str] = set()
    for candidate in MERGE_CALL_RE.findall(merge_expression):
        module_name = _resolve_route_module_name(candidate)
        if module_name:
            modules.add(module_name)
    return sorted(modules)


def parse_api_routes() -> Dict[str, object]:
    text = API_ROUTES_FILE.read_text(encoding="utf-8")
    lines = text.splitlines()
    route_module_sources = sorted(
        str(path.relative_to(ROOT))
        for path in ROUTES_DIR.rglob("*.rs")
        if path.name != "mod.rs"
    )

    tier_aliases = {
        "health_routes": "health",
        "public_routes": "public",
        "optional_auth_routes": "optional_auth",
        "internal_routes": "internal",
        "protected_routes": "protected",
        "spoke_audit_routes": "spoke_audit",
    }
    routes: Dict[str, set[str]] = {tier: set() for tier in tier_aliases.values()}
    merged_modules_by_tier: Dict[str, set[str]] = {tier: set() for tier in tier_aliases.values()}
    module_route_cache: Dict[str, List[str]] = {}

    current_tier: str | None = None
    pending_tier: str | None = None
    pending_kind: str | None = None
    merge_buffer: List[str] = []
    merge_depth = 0
    merge_tier: str | None = None

    for line in lines:
        stripped = line.strip()

        for var_name, tier_name in tier_aliases.items():
            if re.search(rf"\b{re.escape(var_name)}\b", stripped) and "=" in stripped:
                current_tier = tier_name
                break

        if pending_tier and pending_kind and ("\"" in stripped):
            maybe_path = _extract_string_literal(stripped)
            if maybe_path and maybe_path.startswith("/"):
                routes[pending_tier].add(maybe_path)
            pending_tier = None
            pending_kind = None

        if ".route(" in stripped or ".nest(" in stripped:
            kind = "route" if ".route(" in stripped else "nest"
            tier = current_tier
            if not tier:
                continue

            maybe_path = _extract_string_literal(stripped)
            if maybe_path and maybe_path.startswith("/"):
                routes[tier].add(maybe_path)
            else:
                pending_tier = tier
                pending_kind = kind

        if merge_depth > 0:
            merge_buffer.append(stripped)
            merge_depth += stripped.count("(") - stripped.count(")")
            if merge_depth <= 0 and merge_tier:
                merged_modules = _extract_merged_route_modules(" ".join(merge_buffer))
                for module_name in merged_modules:
                    module_paths = _routes_for_module(module_name, module_route_cache)
                    if module_paths:
                        routes[merge_tier].update(module_paths)
                        merged_modules_by_tier[merge_tier].add(module_name)
                merge_buffer = []
                merge_depth = 0
                merge_tier = None
            continue

        if ".merge(" in stripped and current_tier:
            merge_buffer = [stripped[stripped.index(".merge("):]]
            merge_depth = merge_buffer[0].count("(") - merge_buffer[0].count(")")
            merge_tier = current_tier
            if merge_depth <= 0:
                merged_modules = _extract_merged_route_modules(" ".join(merge_buffer))
                for module_name in merged_modules:
                    module_paths = _routes_for_module(module_name, module_route_cache)
                    if module_paths:
                        routes[merge_tier].update(module_paths)
                        merged_modules_by_tier[merge_tier].add(module_name)
                merge_buffer = []
                merge_depth = 0
                merge_tier = None

    return {
        "source": str(API_ROUTES_FILE.relative_to(ROOT)),
        "route_module_sources": route_module_sources,
        "tiers": {tier: sorted(paths) for tier, paths in routes.items()},
        "counts": {tier: len(paths) for tier, paths in routes.items()},
        "merged_modules_by_tier": {
            tier: sorted(modules) for tier, modules in merged_modules_by_tier.items() if modules
        },
    }


def parse_ui_routes() -> Dict[str, object]:
    text = UI_LIB_FILE.read_text(encoding="utf-8")

    route_matches = re.findall(r'<Route\s+path=path!\("([^"]+)"\)', text)
    parent_route_matches = re.findall(r'<ParentRoute\s+path=path!\("([^"]*)"\)', text)

    all_routes = sorted(set(route_matches))
    public_routes = sorted({"/login", "/safe"} & set(all_routes))
    protected_routes = sorted(set(all_routes) - set(public_routes))

    return {
        "source": str(UI_LIB_FILE.relative_to(ROOT)),
        "parent_routes": sorted(set(parent_route_matches)),
        "public_routes": public_routes,
        "protected_routes": protected_routes,
        "all_routes": all_routes,
        "counts": {
            "public": len(public_routes),
            "protected": len(protected_routes),
            "all": len(all_routes),
        },
    }


def parse_middleware_chains() -> Dict[str, object]:
    text = API_ROUTES_FILE.read_text(encoding="utf-8")

    protected_expected = [
        "auth_middleware",
        "tenant_route_guard_middleware",
        "csrf_middleware",
        "context_middleware",
        "policy_enforcement_middleware",
        "audit_middleware",
    ]

    protected_present: List[str] = []
    protected_region_match = re.search(
        r"Middleware execution order \(outermost -> innermost\):(.*?)// Spoke audit routes",
        text,
        re.S,
    )
    if protected_region_match:
        region = protected_region_match.group(1)
        for name in protected_expected:
            if re.search(rf"\b{re.escape(name)}\b", region):
                protected_present.append(name)

    global_expected = [
        "TraceLayer::new_for_http",
        "ErrorCodeEnforcementLayer",
        "idempotency_middleware",
        "cors_layer",
        "rate_limiting_middleware",
        "request_size_limit_middleware",
        "security_headers_middleware",
        "caching::caching_middleware",
        "versioning::versioning_middleware",
        "trace_context_middleware",
        "request_id::request_id_middleware",
        "seed_isolation_middleware",
        "client_ip_middleware",
        "request_tracking_middleware",
        "lifecycle_gate",
        "drain_middleware",
        "observability_middleware",
        "CompressionLayer::new",
    ]

    global_present: List[str] = []
    global_region_match = re.search(r"let app = app(.*?)\.with_state\(state\.clone\(\)\);", text, re.S)
    if global_region_match:
        region = global_region_match.group(1)
        for name in global_expected:
            if re.search(rf"\b{re.escape(name)}\b", region):
                global_present.append(name)

    return {
        "source": str(API_ROUTES_FILE.relative_to(ROOT)),
        "protected_expected_order": protected_expected,
        "protected_present_order": protected_present,
        "global_expected_order": global_expected,
        "global_present_order": global_present,
    }


def write_json(path: Path, payload: Dict[str, object]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def check_json(path: Path, payload: Dict[str, object]) -> Tuple[bool, str]:
    expected = json.dumps(payload, indent=2, sort_keys=True) + "\n"
    if not path.exists():
        return False, f"missing artifact: {path.relative_to(ROOT)}"
    current = path.read_text(encoding="utf-8")
    if current != expected:
        return False, f"artifact drift: {path.relative_to(ROOT)}"
    return True, "ok"


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="verify artifacts are up to date")
    parser.add_argument(
        "--ui-only",
        action="store_true",
        help="generate only docs/generated/ui-route-inventory.json",
    )
    parser.add_argument(
        "--check-ui",
        action="store_true",
        help="verify only docs/generated/ui-route-inventory.json",
    )
    args = parser.parse_args()

    ui_payload = parse_ui_routes()

    if args.ui_only and args.check_ui:
        print("ERROR: --ui-only and --check-ui are mutually exclusive", file=sys.stderr)
        return 1

    if args.check and (args.ui_only or args.check_ui):
        print("ERROR: --check cannot be combined with --ui-only or --check-ui", file=sys.stderr)
        return 1

    if args.check_ui:
        ok, msg = check_json(OUT_UI, ui_payload)
        if not ok:
            print(f"ERROR: {msg}", file=sys.stderr)
            print(
                "Run scripts/contracts/generate_contract_artifacts.py --ui-only and commit generated files.",
                file=sys.stderr,
            )
            return 1
        print("UI route inventory is up to date.")
        return 0

    if args.ui_only:
        write_json(OUT_UI, ui_payload)
        print("Generated contract artifacts:")
        print(f"- {OUT_UI.relative_to(ROOT)}")
        return 0

    api_payload = parse_api_routes()
    middleware_payload = parse_middleware_chains()

    if args.check:
        results = [
            check_json(OUT_API, api_payload),
            check_json(OUT_UI, ui_payload),
            check_json(OUT_MIDDLEWARE, middleware_payload),
        ]
        failures = [msg for ok, msg in results if not ok]
        if failures:
            for failure in failures:
                print(f"ERROR: {failure}", file=sys.stderr)
            print(
                "Run scripts/contracts/generate_contract_artifacts.py and commit generated files.",
                file=sys.stderr,
            )
            return 1
        print("Contract artifacts are up to date.")
        return 0

    write_json(OUT_API, api_payload)
    write_json(OUT_UI, ui_payload)
    write_json(OUT_MIDDLEWARE, middleware_payload)
    print("Generated contract artifacts:")
    print(f"- {OUT_API.relative_to(ROOT)}")
    print(f"- {OUT_UI.relative_to(ROOT)}")
    print(f"- {OUT_MIDDLEWARE.relative_to(ROOT)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
