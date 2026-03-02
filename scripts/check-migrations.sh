#!/usr/bin/env bash
#
# Fails fast on:
# - Duplicate migration numbers in `migrations/*.sql` (and `migrations/postgres/*.sql`)
# - `migrations/signatures.json` missing/out-of-date (hash mismatches, missing/extra entries)
#
# This is a lightweight CI/preflight guard. To regenerate signatures:
#   ./scripts/sign_migrations.sh
#
# Canonical role in migration checks:
# - scripts/check-migrations.sh: signatures + duplicate-number gate
# - scripts/check_migrations.sh: numbering, gaps, collisions
# - scripts/db/check_migrations.sh: CI/test wrapper that orchestrates both

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
MIGRATIONS_DIR="$PROJECT_ROOT/migrations"
POSTGRES_MIGRATIONS_DIR="$MIGRATIONS_DIR/postgres"
SIGNATURES_FILE="$MIGRATIONS_DIR/signatures.json"

PYTHON_BIN="${PYTHON_BIN:-}"
if [ -z "$PYTHON_BIN" ]; then
  if command -v python3 >/dev/null 2>&1; then
    PYTHON_BIN="python3"
  elif command -v python >/dev/null 2>&1; then
    PYTHON_BIN="python"
  else
    echo "Error: python3/python is required" >&2
    exit 1
  fi
fi

PROJECT_ROOT="$PROJECT_ROOT" \
MIGRATIONS_DIR="$MIGRATIONS_DIR" \
POSTGRES_MIGRATIONS_DIR="$POSTGRES_MIGRATIONS_DIR" \
SIGNATURES_FILE="$SIGNATURES_FILE" \
"$PYTHON_BIN" - <<'PY'
from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

project_root = Path(os.environ["PROJECT_ROOT"])
migrations_dir = Path(os.environ["MIGRATIONS_DIR"])
postgres_dir = Path(os.environ["POSTGRES_MIGRATIONS_DIR"])
signatures_file = Path(os.environ["SIGNATURES_FILE"])


def die(msg: str, code: int = 1) -> "None":
    print(msg, file=sys.stderr)
    sys.exit(code)


def list_migration_files(dir_path: Path) -> list[Path]:
    if not dir_path.exists():
        return []
    files: list[Path] = []
    for p in dir_path.iterdir():
        if p.is_file() and re.match(r"^\d{4}.*\.sql$", p.name):
            files.append(p)
    return sorted(files, key=lambda p: p.name)


def find_duplicate_numbers(files: list[Path]) -> dict[str, list[str]]:
    by_num: dict[str, list[str]] = {}
    for p in files:
        # Migrations can be 4-digit incremental (e.g. 0295_*) or timestamp-based
        # (e.g. 20260211120000_*). Treat the full leading digit run as the "number".
        m = re.match(r"^(\d+)", p.name)
        if not m:
            continue
        by_num.setdefault(m.group(1), []).append(p.name)
    return {n: names for n, names in by_num.items() if len(names) > 1}


def blake3_hex_via_b3sum(path: Path) -> str:
    try:
        out = subprocess.check_output(["b3sum", str(path)], text=True)
    except FileNotFoundError as e:
        raise RuntimeError("b3sum not found (needed for blake3 signature checks)") from e
    return out.split()[0].strip()


def sha256_hex(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def check_duplicates() -> None:
    root_files = list_migration_files(migrations_dir)
    dups = find_duplicate_numbers(root_files)
    if dups:
        lines = ["Duplicate migration numbers in migrations/:"]
        for num in sorted(dups):
            lines.append(f"  {num}: {', '.join(sorted(dups[num]))}")
        die("\n".join(lines))

    pg_files = list_migration_files(postgres_dir)
    pg_dups = find_duplicate_numbers(pg_files)
    if pg_dups:
        lines = ["Duplicate migration numbers in migrations/postgres/:"]
        for num in sorted(pg_dups):
            lines.append(f"  {num}: {', '.join(sorted(pg_dups[num]))}")
        die("\n".join(lines))


def check_signatures() -> None:
    if not signatures_file.exists():
        die(f"Missing signatures file: {signatures_file}\nRun: ./scripts/sign_migrations.sh")

    try:
        schema = json.loads(signatures_file.read_text())
    except Exception as e:
        die(f"Failed to parse {signatures_file}: {e}")

    sigs = schema.get("signatures")
    if not isinstance(sigs, dict):
        die(f"Invalid {signatures_file}: expected top-level 'signatures' object")

    root_files = list_migration_files(migrations_dir)
    expected = {p.name for p in root_files}
    present = set(sigs.keys())

    missing = sorted(expected - present)
    extra = sorted(present - expected)

    mismatched: list[str] = []
    for filename in sorted(expected):
        entry = sigs.get(filename)
        if not isinstance(entry, dict):
            continue
        algo = entry.get("hash_algorithm")
        stored_hash = entry.get("hash")
        if not isinstance(algo, str) or not isinstance(stored_hash, str):
            mismatched.append(f"{filename}: invalid signature entry (missing hash/hash_algorithm)")
            continue

        path = migrations_dir / filename
        try:
            if algo == "blake3":
                computed = blake3_hex_via_b3sum(path)
            elif algo == "sha256":
                computed = sha256_hex(path)
            else:
                mismatched.append(f"{filename}: unsupported hash_algorithm={algo}")
                continue
        except Exception as e:
            mismatched.append(f"{filename}: failed to compute hash ({e})")
            continue

        if computed != stored_hash:
            mismatched.append(f"{filename}: hash mismatch (expected {stored_hash}, got {computed})")

    if missing or extra or mismatched:
        lines = ["Migration signatures out of date:"]
        if missing:
            lines.append(f"  Missing signatures ({len(missing)}): " + ", ".join(missing))
        if extra:
            lines.append(f"  Extra signatures ({len(extra)}): " + ", ".join(extra))
        if mismatched:
            lines.append("  Hash mismatches:")
            lines.extend([f"    - {m}" for m in mismatched[:50]])
            if len(mismatched) > 50:
                lines.append(f"    ... ({len(mismatched) - 50} more)")
        lines.append("Run: ./scripts/sign_migrations.sh")
        die("\n".join(lines))


def main() -> None:
    check_duplicates()
    check_signatures()
    print("✓ Migrations OK (no duplicates; signatures up to date)")


if __name__ == "__main__":
    main()
PY
