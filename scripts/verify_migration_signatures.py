#!/usr/bin/env python3
"""
Verify migration signatures against on-disk SQL files.

Checks:
- signatures.json exists and is well-formed
- signatures cover all migrations and no extras
- stored hashes match computed hashes
"""

from __future__ import annotations

import hashlib
import json
import re
import subprocess
import sys
from pathlib import Path

try:
    import blake3  # type: ignore
except ImportError:
    blake3 = None


ROOT_DIR = Path(__file__).resolve().parents[1]
MIGRATIONS_DIR = ROOT_DIR / "migrations"
SIGNATURES_FILE = MIGRATIONS_DIR / "signatures.json"
MIGRATION_RE = re.compile(r"^\d{4}.*\.sql$")


def die(msg: str, code: int = 1) -> "None":
    print(msg, file=sys.stderr)
    sys.exit(code)


def list_migrations() -> list[Path]:
    if not MIGRATIONS_DIR.exists():
        die(f"Missing migrations directory: {MIGRATIONS_DIR}")
    return sorted(
        [p for p in MIGRATIONS_DIR.iterdir() if p.is_file() and MIGRATION_RE.match(p.name)],
        key=lambda p: p.name,
    )


def blake3_hex(path: Path) -> str:
    if blake3 is not None:
        hasher = blake3.blake3()
        with path.open("rb") as handle:
            for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                hasher.update(chunk)
        return hasher.hexdigest()

    # Fall back to b3sum when the optional python blake3 module is unavailable.
    try:
        output = subprocess.check_output(["b3sum", str(path)], text=True)
    except FileNotFoundError as exc:
        raise RuntimeError(
            "python module 'blake3' not found and b3sum is unavailable; "
            "install either `python -m pip install blake3` or `b3sum`."
        ) from exc
    return output.split()[0].strip()


def sha256_hex(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return hasher.hexdigest()


def compute_hash(path: Path, algo: str) -> str:
    if algo == "blake3":
        return blake3_hex(path)
    if algo == "sha256":
        return sha256_hex(path)
    raise ValueError(f"Unsupported hash_algorithm={algo}")


def main() -> None:
    if not SIGNATURES_FILE.exists():
        die(f"Missing signatures file: {SIGNATURES_FILE}\nRun: ./scripts/sign_migrations.sh")

    try:
        schema = json.loads(SIGNATURES_FILE.read_text())
    except Exception as exc:
        die(f"Failed to parse {SIGNATURES_FILE}: {exc}")

    signatures = schema.get("signatures")
    if not isinstance(signatures, dict):
        die(f"Invalid signatures.json: expected top-level 'signatures' object")

    migrations = list_migrations()
    expected = {p.name for p in migrations}
    present = set(signatures.keys())

    missing = sorted(expected - present)
    extra = sorted(present - expected)

    mismatched: list[str] = []
    for path in migrations:
        entry = signatures.get(path.name)
        if not isinstance(entry, dict):
            mismatched.append(f"{path.name}: invalid signature entry")
            continue
        algo = entry.get("hash_algorithm")
        stored_hash = entry.get("hash")
        if not isinstance(algo, str) or not isinstance(stored_hash, str):
            mismatched.append(f"{path.name}: missing hash/hash_algorithm")
            continue
        try:
            computed = compute_hash(path, algo)
        except Exception as exc:
            mismatched.append(f"{path.name}: failed to compute hash ({exc})")
            continue
        if computed != stored_hash:
            mismatched.append(f"{path.name}: hash mismatch (expected {stored_hash}, got {computed})")

        signature = entry.get("signature")
        if not isinstance(signature, str) or not signature:
            mismatched.append(f"{path.name}: missing signature")

    if missing or extra or mismatched:
        lines = ["Migration signatures out of date:"]
        if missing:
            lines.append(f"  Missing signatures ({len(missing)}): " + ", ".join(missing))
        if extra:
            lines.append(f"  Extra signatures ({len(extra)}): " + ", ".join(extra))
        if mismatched:
            lines.append("  Issues:")
            lines.extend([f"    - {item}" for item in mismatched[:50]])
            if len(mismatched) > 50:
                lines.append(f"    ... ({len(mismatched) - 50} more)")
        lines.append("Run: ./scripts/sign_migrations.sh")
        die("\n".join(lines))

    print(f"Migration signatures verified ({len(migrations)} files)")


if __name__ == "__main__":
    main()
