#!/usr/bin/env python3
"""Mechanically insert #[instrument(skip_all)] on async functions.

Walks target crate src/ directories, finds async functions that are not
already instrumented, and inserts the appropriate #[instrument(...)]
annotation.  Handles multi-line signatures and extracts *_id fields for
the `fields(...)` clause.

Usage:
    python3 scripts/dev/add_instrument.py [--dry-run]
"""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

# ── configuration ──────────────────────────────────────────────────────
TARGET_CRATES = [
    "crates/adapteros-server-api/src",
    "crates/adapteros-db/src",
    "crates/adapteros-orchestrator/src",
    "crates/adapteros-lora-worker/src",
    "crates/adapteros-server/src",
]

# Directories to skip (test files, build artifacts)
SKIP_DIRS = {"tests", "test", "target", "benches"}

# Pattern to match async function definitions
# Captures: leading whitespace, visibility, fn name, full param list
ASYNC_FN_START = re.compile(
    r'^(\s*)(pub(?:\((?:crate|super|in [^)]+)\))?\s+)?async\s+fn\s+'
)

# Pattern to match *_id parameters
ID_PARAM = re.compile(r'\b(\w+_id)\b')

# Pattern to detect destructured Path extractors: Path(name): Path<...>
PATH_EXTRACTOR = re.compile(r'Path\((\w+)\)\s*:\s*Path<')

# Pattern to detect existing #[instrument
INSTRUMENT_ATTR = re.compile(r'^\s*#\[instrument')

# Pattern to detect tracing import lines
TRACING_IMPORT = re.compile(r'^(\s*use\s+tracing::)\{([^}]+)\}\s*;')
TRACING_IMPORT_SINGLE = re.compile(r'^(\s*use\s+tracing::)(\w+)\s*;')

# Known id field names for reference
KNOWN_IDS = {
    "tenant_id", "adapter_id", "job_id", "worker_id", "request_id",
    "session_id", "document_id", "model_id", "node_id", "stack_id",
    "collection_id", "review_id", "dataset_id", "repo_id", "version_id",
    "policy_id", "trace_id", "bundle_id", "workspace_id", "key_id",
    "run_id",
}


def is_test_file(path: Path) -> bool:
    """Check if file is in a test directory or is a test file."""
    parts = path.parts
    for skip in SKIP_DIRS:
        if skip in parts:
            return True
    # Files named *_test.rs, test_*.rs, or tests.rs
    name = path.name
    if name.endswith("_test.rs") or name.startswith("test_") or name == "tests.rs":
        return True
    return False


def extract_full_signature(lines: list[str], start_idx: int) -> tuple[str, int]:
    """Extract the full function signature, handling multi-line sigs.

    Returns (full_sig_text, end_line_idx) where end_line_idx is the line
    containing the closing ')' or '{'.
    """
    sig = ""
    paren_depth = 0
    idx = start_idx
    while idx < len(lines):
        line = lines[idx]
        sig += line + "\n"
        paren_depth += line.count('(') - line.count(')')
        # We've closed all parens
        if paren_depth <= 0 and '(' in sig:
            return sig, idx
        # Safety: if we hit '{' at depth 0, signature is done
        if '{' in line and paren_depth <= 0:
            return sig, idx
        idx += 1
    return sig, idx


def extract_id_fields(signature: str) -> list[tuple[str, bool]]:
    """Extract *_id field names from a function signature.

    Returns list of (field_name, needs_format) tuples.
    needs_format is True for Path-destructured params (use = %name).
    """
    fields = []
    seen = set()

    # Find Path(xxx) destructured params
    for m in PATH_EXTRACTOR.finditer(signature):
        name = m.group(1)
        if name.endswith("_id") and name not in seen:
            fields.append((name, True))  # needs %
            seen.add(name)

    # Find bare *_id params (not inside Path())
    # Remove Path(...) sections first to avoid double-counting
    cleaned = PATH_EXTRACTOR.sub('', signature)
    for m in ID_PARAM.finditer(cleaned):
        name = m.group(1)
        if name not in seen:
            fields.append((name, False))
            seen.add(name)

    return fields


def build_instrument_attr(fields: list[tuple[str, bool]], indent: str) -> str:
    """Build the #[instrument(...)] attribute string."""
    if not fields:
        return f"{indent}#[instrument(skip_all)]"

    field_parts = []
    for name, needs_format in fields:
        if needs_format:
            field_parts.append(f"{name} = %{name}")
        else:
            field_parts.append(name)

    fields_str = ", ".join(field_parts)
    attr = f"{indent}#[instrument(skip_all, fields({fields_str}))]"

    # If too long, keep it single line anyway (cargo fmt will handle it)
    return attr


def has_preceding_instrument(lines: list[str], fn_line_idx: int) -> bool:
    """Check if the function already has an #[instrument] attribute."""
    # Walk backwards through preceding lines, skipping blank lines,
    # comments, and other attributes
    idx = fn_line_idx - 1
    while idx >= 0:
        line = lines[idx].strip()
        if not line or line.startswith("//"):
            idx -= 1
            continue
        if line.startswith("#["):
            if INSTRUMENT_ATTR.match(lines[idx]):
                return True
            # Other attribute, keep looking
            idx -= 1
            continue
        # Hit actual code - no instrument found
        break
    return False


def is_in_test_module(lines: list[str], fn_line_idx: int) -> bool:
    """Check if the function is inside a #[cfg(test)] module."""
    # Walk backwards looking for #[cfg(test)] or mod tests
    brace_depth = 0
    for idx in range(fn_line_idx - 1, -1, -1):
        line = lines[idx]
        brace_depth += line.count('}') - line.count('{')
        if '#[cfg(test)]' in line:
            return True
        if re.match(r'\s*mod\s+tests\s*\{', line):
            return True
        # If we've exited an enclosing scope entirely, stop
        if brace_depth < 0:
            break
    return False


def is_in_doc_comment(lines: list[str], fn_line_idx: int) -> bool:
    """Check if the async fn is inside a doc comment (/// example)."""
    idx = fn_line_idx
    line = lines[idx].lstrip()
    if line.startswith("///") or line.startswith("//!"):
        return True
    # Also check if the function keyword itself is in a comment
    stripped = lines[fn_line_idx].lstrip()
    if stripped.startswith("//"):
        return True
    return False


def add_instrument_import(lines: list[str]) -> list[str]:
    """Ensure `instrument` is imported from tracing."""
    # Check if instrument is already imported
    for line in lines:
        if 'instrument' in line and 'tracing' in line:
            # Already imported
            return lines

    # Try to add to existing `use tracing::{...};`
    for i, line in enumerate(lines):
        m = TRACING_IMPORT.match(line)
        if m:
            prefix = m.group(1)
            imports = m.group(2)
            # Add instrument to the import list
            items = [s.strip() for s in imports.split(',')]
            if 'instrument' not in items:
                items.append('instrument')
                items.sort()
                new_line = f"{prefix}{{{', '.join(items)}}};"
                lines[i] = new_line
            return lines

        m = TRACING_IMPORT_SINGLE.match(line)
        if m:
            prefix = m.group(1)
            existing = m.group(2)
            if existing != 'instrument':
                items = sorted([existing, 'instrument'])
                new_line = f"{prefix}{{{', '.join(items)}}};"
                lines[i] = new_line
            return lines

    # No existing tracing import - add one after the last `use` statement/block
    # Must account for multi-line use blocks (use foo::{...};)
    last_use_end_idx = -1
    i = 0
    while i < len(lines):
        stripped = lines[i].strip()
        if stripped.startswith("use "):
            if '};' in stripped or ('{' not in stripped and ';' in stripped):
                # Single-line use statement
                last_use_end_idx = i
            elif '{' in stripped:
                # Multi-line use block - find closing };
                j = i + 1
                while j < len(lines):
                    if '};' in lines[j] or lines[j].strip() == '};':
                        last_use_end_idx = j
                        break
                    j += 1
                i = j
        i += 1

    if last_use_end_idx >= 0:
        lines.insert(last_use_end_idx + 1, "use tracing::instrument;")
    else:
        # No use statements at all - add at top after any #![...] or comments
        insert_idx = 0
        for i, line in enumerate(lines):
            stripped = line.strip()
            if stripped.startswith("#!") or stripped.startswith("//") or not stripped:
                insert_idx = i + 1
            else:
                break
        lines.insert(insert_idx, "use tracing::instrument;")

    return lines


def process_file(filepath: Path, dry_run: bool = False) -> int:
    """Process a single Rust file. Returns number of functions instrumented."""
    lines = filepath.read_text().splitlines()
    instrumented = 0
    insertions: list[tuple[int, str]] = []  # (line_idx, attr_text)

    i = 0
    while i < len(lines):
        line = lines[i]
        m = ASYNC_FN_START.match(line)
        if m:
            indent = m.group(1)

            # Skip if in doc comment
            if is_in_doc_comment(lines, i):
                i += 1
                continue

            # Skip if in test module
            if is_in_test_module(lines, i):
                i += 1
                continue

            # Skip if already instrumented
            if has_preceding_instrument(lines, i):
                i += 1
                continue

            # Extract full signature for field analysis
            sig, sig_end = extract_full_signature(lines, i)

            # Extract id fields
            id_fields = extract_id_fields(sig)

            # Build the attribute
            attr = build_instrument_attr(id_fields, indent)
            insertions.append((i, attr))
            instrumented += 1

            i = sig_end + 1
        else:
            i += 1

    if instrumented > 0 and not dry_run:
        # Apply insertions in reverse order to preserve line indices
        for line_idx, attr_text in reversed(insertions):
            lines.insert(line_idx, attr_text)

        # Add instrument import
        lines = add_instrument_import(lines)

        filepath.write_text('\n'.join(lines) + '\n')

    return instrumented


def main():
    dry_run = "--dry-run" in sys.argv
    root = Path(__file__).resolve().parents[2]  # repo root

    total_files = 0
    total_fns = 0

    for crate_src in TARGET_CRATES:
        src_path = root / crate_src
        if not src_path.exists():
            print(f"  SKIP {crate_src} (not found)")
            continue

        crate_files = 0
        crate_fns = 0

        for rs_file in sorted(src_path.rglob("*.rs")):
            if is_test_file(rs_file):
                continue

            count = process_file(rs_file, dry_run=dry_run)
            if count > 0:
                rel = rs_file.relative_to(root)
                print(f"  {rel}: {count} functions")
                crate_files += 1
                crate_fns += count

        crate_name = crate_src.split("/")[1]
        print(f"\n{'[DRY RUN] ' if dry_run else ''}{crate_name}: "
              f"{crate_fns} functions in {crate_files} files")
        print()

        total_files += crate_files
        total_fns += crate_fns

    print(f"\n{'[DRY RUN] ' if dry_run else ''}TOTAL: "
          f"{total_fns} functions instrumented across {total_files} files")


if __name__ == "__main__":
    main()
