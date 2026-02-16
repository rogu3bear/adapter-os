#!/usr/bin/env python3
"""Detect highly similar Leptos #[component] function bodies."""

from __future__ import annotations

import argparse
import difflib
import json
import re
import sys
from dataclasses import dataclass
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
UI_SRC = ROOT / "crates" / "adapteros-ui" / "src"
ATTR_RE = re.compile(r"(?m)^\s*#\s*\[\s*component\b[^\]]*\]")
FN_RE = re.compile(r"\b(?:pub\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\b")


@dataclass(frozen=True)
class Component:
    name: str
    file: str
    line: int
    normalized: str

    @property
    def key(self) -> str:
        return f"{self.file}:{self.line}:{self.name}"


@dataclass(frozen=True)
class Pair:
    left: int
    right: int
    ratio: float


def is_ident_start(ch: str) -> bool:
    return ch.isalpha() or ch == "_"


def starts_char_literal(text: str, i: int) -> bool:
    n = len(text)
    if i + 2 < n and text[i + 2] == "'":
        return True
    if i + 3 < n and text[i + 1] == "\\" and text[i + 3] == "'":
        return True
    return False


def remove_comments(text: str) -> str:
    out: list[str] = []
    i = 0
    n = len(text)
    block_depth = 0
    in_line = False
    in_str = False
    in_char = False
    in_raw = False
    raw_hashes = 0

    while i < n:
        c = text[i]
        nxt = text[i + 1] if i + 1 < n else ""

        if in_line:
            if c == "\n":
                in_line = False
                out.append(c)
            i += 1
            continue

        if block_depth > 0:
            if c == "/" and nxt == "*":
                block_depth += 1
                i += 2
                continue
            if c == "*" and nxt == "/":
                block_depth -= 1
                i += 2
                continue
            i += 1
            continue

        if in_str:
            out.append(c)
            if c == "\\" and i + 1 < n:
                out.append(text[i + 1])
                i += 2
                continue
            if c == '"':
                in_str = False
            i += 1
            continue

        if in_char:
            out.append(c)
            if c == "\\" and i + 1 < n:
                out.append(text[i + 1])
                i += 2
                continue
            if c == "'":
                in_char = False
            i += 1
            continue

        if in_raw:
            out.append(c)
            if c == '"':
                j = i + 1
                cnt = 0
                while j < n and text[j] == "#":
                    cnt += 1
                    j += 1
                if cnt == raw_hashes:
                    out.extend("#" * cnt)
                    i = j
                    in_raw = False
                    raw_hashes = 0
                    continue
            i += 1
            continue

        if c == "/" and nxt == "/":
            in_line = True
            i += 2
            continue
        if c == "/" and nxt == "*":
            block_depth = 1
            i += 2
            continue

        if c == "r":
            j = i + 1
            hashes = 0
            while j < n and text[j] == "#":
                hashes += 1
                j += 1
            if j < n and text[j] == '"':
                out.append("r")
                if hashes:
                    out.extend("#" * hashes)
                out.append('"')
                i = j + 1
                in_raw = True
                raw_hashes = hashes
                continue

        if c == '"':
            in_str = True
            out.append(c)
            i += 1
            continue

        if c == "'":
            if starts_char_literal(text, i):
                in_char = True
                out.append(c)
                i += 1
                continue
            if i + 1 < n and is_ident_start(text[i + 1]):
                out.append(c)
                i += 1
                continue
            in_char = True
            out.append(c)
            i += 1
            continue

        out.append(c)
        i += 1

    return "".join(out)


def find_matching_brace(text: str, open_idx: int) -> int:
    i = open_idx
    n = len(text)
    depth = 0
    in_line = False
    block_depth = 0
    in_str = False
    in_char = False
    in_raw = False
    raw_hashes = 0

    while i < n:
        c = text[i]
        nxt = text[i + 1] if i + 1 < n else ""

        if in_line:
            if c == "\n":
                in_line = False
            i += 1
            continue

        if block_depth > 0:
            if c == "/" and nxt == "*":
                block_depth += 1
                i += 2
                continue
            if c == "*" and nxt == "/":
                block_depth -= 1
                i += 2
                continue
            i += 1
            continue

        if in_str:
            if c == "\\":
                i += 2
                continue
            if c == '"':
                in_str = False
            i += 1
            continue

        if in_char:
            if c == "\\":
                i += 2
                continue
            if c == "'":
                in_char = False
            i += 1
            continue

        if in_raw:
            if c == '"':
                j = i + 1
                cnt = 0
                while j < n and text[j] == "#":
                    cnt += 1
                    j += 1
                if cnt == raw_hashes:
                    in_raw = False
                    raw_hashes = 0
                    i = j
                    continue
            i += 1
            continue

        if c == "/" and nxt == "/":
            in_line = True
            i += 2
            continue

        if c == "/" and nxt == "*":
            block_depth = 1
            i += 2
            continue

        if c == "r":
            j = i + 1
            hashes = 0
            while j < n and text[j] == "#":
                hashes += 1
                j += 1
            if j < n and text[j] == '"':
                in_raw = True
                raw_hashes = hashes
                i = j + 1
                continue

        if c == '"':
            in_str = True
            i += 1
            continue

        if c == "'":
            if starts_char_literal(text, i):
                in_char = True
                i += 1
                continue
            if i + 1 < n and is_ident_start(text[i + 1]):
                i += 1
                continue
            in_char = True
            i += 1
            continue

        if c == "{":
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                return i

        i += 1

    return -1


def should_exclude(path: Path, excluded_suffixes: list[str]) -> bool:
    p = path.as_posix()
    return any(p.endswith(suf) for suf in excluded_suffixes)


def extract_components(source_path: Path, root: Path) -> list[Component]:
    text = source_path.read_text(encoding="utf-8")
    rel_file = source_path.relative_to(root).as_posix()
    components: list[Component] = []

    for am in ATTR_RE.finditer(text):
        fm = FN_RE.search(text, am.end())
        if fm is None:
            continue

        next_attr = ATTR_RE.search(text, am.end())
        if next_attr is not None and fm.start() > next_attr.start():
            continue

        open_idx = text.find("{", fm.end())
        if open_idx == -1:
            continue

        close_idx = find_matching_brace(text, open_idx)
        if close_idx == -1:
            continue

        body = text[open_idx + 1 : close_idx]
        normalized = " ".join(remove_comments(body).split())
        line = text.count("\n", 0, fm.start()) + 1

        components.append(
            Component(
                name=fm.group(1),
                file=rel_file,
                line=line,
                normalized=normalized,
            )
        )

    return components


def scan_components(root: Path, excluded_suffixes: list[str]) -> list[Component]:
    components: list[Component] = []
    for path in sorted(root.rglob("*.rs")):
        if should_exclude(path, excluded_suffixes):
            continue
        components.extend(extract_components(path, root))
    return components


def max_possible_ratio(left_len: int, right_len: int) -> float:
    if left_len == 0 and right_len == 0:
        return 1.0
    if left_len == 0 or right_len == 0:
        return 0.0
    shorter = left_len if left_len < right_len else right_len
    longer = right_len if left_len < right_len else left_len
    return (2.0 * shorter) / (shorter + longer)


def compute_pairs(
    components: list[Component], threshold: float
) -> tuple[list[Pair], int, int, int]:
    pairs: list[Pair] = []
    total_pairs = 0
    skipped_by_length = 0
    skipped_by_quick = 0

    norms = [c.normalized for c in components]
    lengths = [len(n) for n in norms]

    for i in range(len(components)):
        ai = norms[i]
        li = lengths[i]
        for j in range(i + 1, len(components)):
            total_pairs += 1
            bj = norms[j]
            lj = lengths[j]

            if max_possible_ratio(li, lj) < threshold:
                skipped_by_length += 1
                continue

            sm = difflib.SequenceMatcher(None, ai, bj)
            if sm.real_quick_ratio() < threshold or sm.quick_ratio() < threshold:
                skipped_by_quick += 1
                continue

            pairs.append(Pair(left=i, right=j, ratio=sm.ratio()))

    return pairs, total_pairs, skipped_by_length, skipped_by_quick


def build_report(
    components: list[Component],
    pairs: list[Pair],
    threshold: float,
    max_qualifying: int | None,
    total_pairs: int,
    skipped_by_length: int,
    skipped_by_quick: int,
) -> dict[str, Any]:
    best_peer: dict[int, tuple[int | None, float]] = {
        idx: (None, 0.0) for idx in range(len(components))
    }
    qualifying_keys: set[str] = set()

    for pair in pairs:
        if pair.ratio >= threshold:
            qualifying_keys.add(components[pair.left].key)
            qualifying_keys.add(components[pair.right].key)

        lp, lr = best_peer[pair.left]
        if pair.ratio > lr:
            best_peer[pair.left] = (pair.right, pair.ratio)

        rp, rr = best_peer[pair.right]
        if pair.ratio > rr:
            best_peer[pair.right] = (pair.left, pair.ratio)

    top_pairs = sorted(pairs, key=lambda p: p.ratio, reverse=True)[:10]

    qualifying_components: list[dict[str, Any]] = []
    for idx, comp in enumerate(components):
        if comp.key not in qualifying_keys:
            continue
        peer_idx, ratio = best_peer[idx]
        peer = components[peer_idx] if peer_idx is not None else None
        qualifying_components.append(
            {
                "name": comp.name,
                "file": comp.file,
                "line": comp.line,
                "best_peer_ratio": ratio,
                "best_peer": (
                    {
                        "name": peer.name,
                        "file": peer.file,
                        "line": peer.line,
                    }
                    if peer is not None
                    else None
                ),
            }
        )

    return {
        "threshold": threshold,
        "total_components": len(components),
        "total_pairs": total_pairs,
        "evaluated_pairs": len(pairs),
        "skipped_pairs_by_length": skipped_by_length,
        "skipped_pairs_by_quick": skipped_by_quick,
        "qualifying_count": len(qualifying_keys),
        "max_qualifying": max_qualifying,
        "violates_max_qualifying": (
            max_qualifying is not None and len(qualifying_keys) > max_qualifying
        ),
        "qualifying_components": sorted(
            qualifying_components, key=lambda item: item["best_peer_ratio"], reverse=True
        ),
        "top_pairs": [
            {
                "ratio": pair.ratio,
                "left": {
                    "name": components[pair.left].name,
                    "file": components[pair.left].file,
                    "line": components[pair.left].line,
                },
                "right": {
                    "name": components[pair.right].name,
                    "file": components[pair.right].file,
                    "line": components[pair.right].line,
                },
            }
            for pair in top_pairs
        ],
    }


def print_human_summary(report: dict[str, Any], excluded_suffixes: list[str]) -> None:
    threshold = report["threshold"]
    qualifying_count = report["qualifying_count"]
    max_qualifying = report["max_qualifying"]
    violates = report["violates_max_qualifying"]

    print("=== UI Component Similarity Report ===")
    print(f"Source root: {UI_SRC.as_posix()}")
    print(f"Threshold: {threshold:.2f}")
    if excluded_suffixes:
        print(f"Excluded suffixes: {', '.join(excluded_suffixes)}")
    print(f"Total components: {report['total_components']}")
    print(f"Total pairs: {report['total_pairs']}")
    print(f"Evaluated pairs: {report['evaluated_pairs']}")
    print(f"Skipped by length bound: {report['skipped_pairs_by_length']}")
    print(f"Skipped by quick ratio bound: {report['skipped_pairs_by_quick']}")
    print(f"Qualifying components: {qualifying_count}")
    if max_qualifying is not None:
        status = "FAIL" if violates else "PASS"
        print(f"Max qualifying: {max_qualifying} ({status})")

    print("")
    print("Top similarity pairs:")
    top_pairs = report["top_pairs"]
    if not top_pairs:
        print("  (none)")
        return

    for idx, pair in enumerate(top_pairs, start=1):
        left = pair["left"]
        right = pair["right"]
        marker = " *" if pair["ratio"] >= threshold else ""
        print(
            f"  {idx:>2}. {pair['ratio']:.3f} "
            f"{left['name']} ({left['file']}:{left['line']}) <-> "
            f"{right['name']} ({right['file']}:{right['line']}){marker}"
        )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Find similar #[component] function bodies in adapteros-ui."
    )
    parser.add_argument(
        "--threshold",
        type=float,
        default=0.80,
        help="Similarity threshold for qualifying components (default: 0.80).",
    )
    parser.add_argument(
        "--exclude-file-suffix",
        action="append",
        default=[],
        help="File path suffix to exclude (repeatable).",
    )
    parser.add_argument(
        "--max-qualifying",
        type=int,
        default=None,
        help="Fail if qualifying component count exceeds this value.",
    )
    parser.add_argument(
        "--json",
        action="store_true",
        help="Emit JSON report instead of human-readable output.",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()

    if not (0.0 <= args.threshold <= 1.0):
        print("error: --threshold must be in [0.0, 1.0]", file=sys.stderr)
        return 2
    if args.max_qualifying is not None and args.max_qualifying < 0:
        print("error: --max-qualifying must be >= 0", file=sys.stderr)
        return 2
    if not UI_SRC.exists():
        print(f"error: UI source path not found: {UI_SRC}", file=sys.stderr)
        return 2

    components = scan_components(UI_SRC, args.exclude_file_suffix)
    pairs, total_pairs, skipped_by_length, skipped_by_quick = compute_pairs(
        components, args.threshold
    )
    report = build_report(
        components,
        pairs,
        args.threshold,
        args.max_qualifying,
        total_pairs,
        skipped_by_length,
        skipped_by_quick,
    )

    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print_human_summary(report, args.exclude_file_suffix)

    if report["violates_max_qualifying"]:
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
