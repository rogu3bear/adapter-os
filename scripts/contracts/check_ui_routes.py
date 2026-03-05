#!/usr/bin/env python3
"""Validate UI route contract against the runtime route source."""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
UI_LIB = ROOT / "crates/adapteros-ui/src/lib.rs"
UI_SRC = ROOT / "crates/adapteros-ui/src"
EXPECTED_PUBLIC = {"/login", "/safe"}
EXPECTED_PROTECTED = {
    "/",
    "/adapters",
    "/adapters/:id",
    "/admin",
    "/audit",
    "/chat",
    "/chat/history",
    "/chat/s/:session_id",
    "/dashboard",
    "/datasets",
    "/datasets/:id",
    "/documents",
    "/documents/:id",
    "/flight-recorder",
    "/flight-recorder/:id",
    "/models",
    "/models/:id",
    "/policies",
    "/routing",
    "/runs",
    "/runs/:id",
    "/settings",
    "/system",
    "/training",
    "/training/:id",
    "/update-center",
    "/user",
    "/welcome",
    "/workers",
    "/workers/:id",
}
REQUIRED_COMPAT = {"/flight-recorder", "/flight-recorder/:id", "/user"}
REMOVED_UI_ROUTE_PREFIXES = {
    "/agents",
    "/collections",
    "/diff",
    "/errors",
    "/files",
    "/monitoring",
    "/repositories",
    "/review_detail",
    "/reviews",
    "/stacks",
    "/style-audit",
}


def parse_ui_routes() -> tuple[set[str], set[str], set[str]]:
    text = UI_LIB.read_text(encoding="utf-8")
    all_routes = set(re.findall(r'<Route\s+path=path!\("([^"]+)"\)', text))
    public_routes = EXPECTED_PUBLIC & all_routes
    protected_routes = all_routes - public_routes
    return all_routes, public_routes, protected_routes


def fail(msg: str) -> int:
    print(f"FAIL: {msg}", file=sys.stderr)
    return 1


def find_removed_route_references() -> list[tuple[str, int, str]]:
    # Match quoted strings like "/datasets", "/datasets/...", "/repositories?...".
    route_alt = "|".join(sorted(re.escape(p) for p in REMOVED_UI_ROUTE_PREFIXES))
    pattern = re.compile(rf"""["'](?P<route>{route_alt})(?P<suffix>(?:[/?][^"']*)?)["']""")

    findings: list[tuple[str, int, str]] = []
    for file_path in sorted(UI_SRC.rglob("*.rs")):
        rel = file_path.relative_to(ROOT)
        for lineno, line in enumerate(file_path.read_text(encoding="utf-8").splitlines(), start=1):
            for match in pattern.finditer(line):
                findings.append((str(rel), lineno, match.group("route") + match.group("suffix")))
    return findings


def main() -> int:
    if not UI_LIB.exists():
        return fail("Missing crates/adapteros-ui/src/lib.rs")

    all_routes, public_routes, protected_routes = parse_ui_routes()

    if public_routes != EXPECTED_PUBLIC:
        return fail(
            "Public UI routes drifted. "
            f"expected={sorted(EXPECTED_PUBLIC)} actual={sorted(public_routes)}"
        )

    if "/style-audit" in all_routes:
        return fail("Removed route /style-audit reappeared in active UI routes")

    if public_routes & protected_routes:
        return fail("A route is both public and protected")

    missing_protected = sorted(EXPECTED_PROTECTED - protected_routes)
    unexpected_protected = sorted(protected_routes - EXPECTED_PROTECTED)
    if missing_protected or unexpected_protected:
        return fail(
            "Protected UI routes drifted. "
            f"missing={missing_protected} unexpected={unexpected_protected}"
        )

    missing_compat = sorted(REQUIRED_COMPAT - all_routes)
    if missing_compat:
        return fail(f"Missing compatibility routes: {missing_compat}")

    removed_refs = find_removed_route_references()
    if removed_refs:
        sample = ", ".join(f"{path}:{line} ({route})" for path, line, route in removed_refs[:8])
        return fail(
            "Removed UI routes are still referenced in active UI source. "
            f"sample={sample}"
        )

    print("=== UI Route Contract Check: PASSED ===")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
