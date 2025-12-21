#!/usr/bin/env python3
"""Verify migration signatures are up to date."""

from __future__ import annotations

import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
MIGRATIONS_DIR = ROOT / "migrations"
SIGNATURES_PATH = MIGRATIONS_DIR / "signatures.json"


def load_blake3():
    try:
        import blake3  # type: ignore
    except ModuleNotFoundError as exc:
        raise SystemExit(
            "blake3 module not available; install with `python -m pip install blake3`"
        ) from exc
    return blake3


def compute_hash(path: Path, algorithm: str, blake3_module) -> str:
    data = path.read_bytes()
    algo = algorithm.lower()
    if algo == "blake3":
        return blake3_module.blake3(data).hexdigest()
    if algo == "sha256":
        return hashlib.sha256(data).hexdigest()
    raise SystemExit(f"Unsupported hash_algorithm '{algorithm}' for {path.name}")


def main() -> int:
    if not SIGNATURES_PATH.exists():
        print(f"signatures.json missing at {SIGNATURES_PATH}", file=sys.stderr)
        return 1

    if not MIGRATIONS_DIR.exists():
        print(f"migrations directory missing at {MIGRATIONS_DIR}", file=sys.stderr)
        return 1

    data = json.loads(SIGNATURES_PATH.read_text())
    signatures = data.get("signatures", {})

    blake3_module = load_blake3()

    migration_files = sorted(MIGRATIONS_DIR.glob("[0-9][0-9][0-9][0-9]_*.sql"))
    errors: list[str] = []

    for path in migration_files:
        entry = signatures.get(path.name)
        if entry is None:
            errors.append(f"missing signature entry for {path.name}")
            continue

        expected_hash = entry.get("hash")
        algorithm = entry.get("hash_algorithm", "blake3")
        actual_hash = compute_hash(path, algorithm, blake3_module)

        if expected_hash != actual_hash:
            errors.append(f"hash mismatch for {path.name}")

    extra_entries = sorted(set(signatures.keys()) - {p.name for p in migration_files})
    if extra_entries:
        errors.append(f"signatures exist for missing files: {', '.join(extra_entries)}")

    if errors:
        for err in errors:
            print(f"❌ {err}", file=sys.stderr)
        return 1

    print(f"✅ signatures.json covers all {len(migration_files)} migrations")
    return 0


if __name__ == "__main__":
    sys.exit(main())
