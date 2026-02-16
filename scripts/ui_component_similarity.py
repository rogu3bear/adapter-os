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
COMPONENT_ATTR_RE = re.compile(r"#\s*\[\s*component(?:\s*\([^]]*\))?\s*\]", re.MULTILINE)
FN_RE = re.compile(
    r"\b(?:pub(?:\([^)]*\))?\s+)?(?:async\s+)?fn\s+([A-Za-z_][A-Za-z0-9_]*)\b",
    re.MULTILINE,
)


@dataclass(frozen=True)
class Component:
    name: str
    file: str
    line: int
    body: str
    normalized: str
    tokens: tuple[str, ...]

    @property
    def key(self) -> str:
        return f"{self.file}:{self.line}:{self.name}"


@dataclass(frozen=True)
class Pair:
    left: int
    right: int
    ratio: float


def _maybe_raw_string_start(text: str, i: int) -> tuple[int, int] | None:
    """Return (index_after_open_quote, hash_count) if a raw string starts at i."""
    if i >= len(text):
        return None

    if text[i] == "r":
        prefix_start = i
        j = i + 1
    elif text[i] == "b" and i + 1 < len(text) and text[i + 1] == "r":
        prefix_start = i
        j = i + 2
    else:
        return None

    if prefix_start > 0 and (text[prefix_start - 1].isalnum() or text[prefix_start - 1] == "_"):
        return None

    hashes = 0
    while j < len(text) and text[j] == "#":
        hashes += 1
        j += 1
    if j < len(text) and text[j] == '"':
        return (j + 1, hashes)
    return None


def strip_comments(text: str) -> str:
    """Strip // and /* */ comments while preserving strings/chars."""
    out: list[str] = []
    i = 0
    state = "normal"
    block_depth = 0
    raw_hashes = 0
    escaped = False

    while i < len(text):
        ch = text[i]
        nxt = text[i + 1] if i + 1 < len(text) else ""

        if state == "normal":
            raw_start = _maybe_raw_string_start(text, i)
            if raw_start is not None:
                end_of_open, raw_hashes = raw_start
                out.append(text[i:end_of_open])
                i = end_of_open
                state = "raw_string"
                continue
            if ch == "/" and nxt == "/":
                state = "line_comment"
                i += 2
                continue
            if ch == "/" and nxt == "*":
                state = "block_comment"
                block_depth = 1
                i += 2
                continue
            if ch == '"':
                state = "string"
                escaped = False
                out.append(ch)
                i += 1
                continue
            if ch == "'":
                state = "char"
                escaped = False
                out.append(ch)
                i += 1
                continue
            out.append(ch)
            i += 1
            continue

        if state == "line_comment":
            if ch == "\n":
                out.append("\n")
                state = "normal"
            i += 1
            continue

        if state == "block_comment":
            if ch == "/" and nxt == "*":
                block_depth += 1
                i += 2
                continue
            if ch == "*" and nxt == "/":
                block_depth -= 1
                i += 2
                if block_depth == 0:
                    state = "normal"
                continue
            if ch == "\n":
                out.append("\n")
            i += 1
            continue

        if state == "string":
            out.append(ch)
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == '"':
                state = "normal"
            i += 1
            continue

        if state == "char":
            out.append(ch)
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == "'":
                state = "normal"
            i += 1
            continue

        if state == "raw_string":
            out.append(ch)
            if ch == '"':
                closing = True
                for offset in range(raw_hashes):
                    if i + 1 + offset >= len(text) or text[i + 1 + offset] != "#":
                        closing = False
                        break
                if closing:
                    if raw_hashes:
                        out.append("#" * raw_hashes)
                    i += 1 + raw_hashes
                    state = "normal"
                    continue
            i += 1
            continue

    return "".join(out)


def collapse_whitespace(text: str) -> str:
    return " ".join(text.split())


def normalize_body(body: str) -> str:
    return collapse_whitespace(strip_comments(body))


def tokenize(normalized: str) -> tuple[str, ...]:
    if not normalized:
        return ()
    return tuple(normalized.split(" "))


def _find_next_open_brace(text: str, start: int) -> int | None:
    i = start
    state = "normal"
    block_depth = 0
    raw_hashes = 0
    escaped = False

    while i < len(text):
        ch = text[i]
        nxt = text[i + 1] if i + 1 < len(text) else ""

        if state == "normal":
            raw_start = _maybe_raw_string_start(text, i)
            if raw_start is not None:
                i = raw_start[0]
                raw_hashes = raw_start[1]
                state = "raw_string"
                continue
            if ch == "/" and nxt == "/":
                state = "line_comment"
                i += 2
                continue
            if ch == "/" and nxt == "*":
                state = "block_comment"
                block_depth = 1
                i += 2
                continue
            if ch == '"':
                state = "string"
                escaped = False
                i += 1
                continue
            if ch == "'":
                state = "char"
                escaped = False
                i += 1
                continue
            if ch == "{":
                return i
            i += 1
            continue

        if state == "line_comment":
            if ch == "\n":
                state = "normal"
            i += 1
            continue

        if state == "block_comment":
            if ch == "/" and nxt == "*":
                block_depth += 1
                i += 2
                continue
            if ch == "*" and nxt == "/":
                block_depth -= 1
                i += 2
                if block_depth == 0:
                    state = "normal"
                continue
            i += 1
            continue

        if state == "string":
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == '"':
                state = "normal"
            i += 1
            continue

        if state == "char":
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == "'":
                state = "normal"
            i += 1
            continue

        if state == "raw_string":
            if ch == '"':
                closing = True
                for offset in range(raw_hashes):
                    if i + 1 + offset >= len(text) or text[i + 1 + offset] != "#":
                        closing = False
                        break
                if closing:
                    i += 1 + raw_hashes
                    state = "normal"
                    continue
            i += 1
            continue

    return None


def _find_matching_brace(text: str, open_idx: int) -> int | None:
    i = open_idx
    depth = 0
    state = "normal"
    block_depth = 0
    raw_hashes = 0
    escaped = False

    while i < len(text):
        ch = text[i]
        nxt = text[i + 1] if i + 1 < len(text) else ""

        if state == "normal":
            raw_start = _maybe_raw_string_start(text, i)
            if raw_start is not None:
                i = raw_start[0]
                raw_hashes = raw_start[1]
                state = "raw_string"
                continue
            if ch == "/" and nxt == "/":
                state = "line_comment"
                i += 2
                continue
            if ch == "/" and nxt == "*":
                state = "block_comment"
                block_depth = 1
                i += 2
                continue
            if ch == '"':
                state = "string"
                escaped = False
                i += 1
                continue
            if ch == "'":
                state = "char"
                escaped = False
                i += 1
                continue
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    return i
            i += 1
            continue

        if state == "line_comment":
            if ch == "\n":
                state = "normal"
            i += 1
            continue

        if state == "block_comment":
            if ch == "/" and nxt == "*":
                block_depth += 1
                i += 2
                continue
            if ch == "*" and nxt == "/":
                block_depth -= 1
                i += 2
                if block_depth == 0:
                    state = "normal"
                continue
            i += 1
            continue

        if state == "string":
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == '"':
                state = "normal"
            i += 1
            continue

        if state == "char":
            if escaped:
                escaped = False
            elif ch == "\\":
                escaped = True
            elif ch == "'":
                state = "normal"
            i += 1
            continue

        if state == "raw_string":
            if ch == '"':
                closing = True
                for offset in range(raw_hashes):
                    if i + 1 + offset >= len(text) or text[i + 1 + offset] != "#":
                        closing = False
                        break
                if closing:
                    i += 1 + raw_hashes
                    state = "normal"
                    continue
            i += 1
            continue

    return None


def should_exclude(path: Path, excluded_suffixes: list[str]) -> bool:
    path_text = path.as_posix()
    return any(path_text.endswith(suffix) for suffix in excluded_suffixes)


def extract_components(source_path: Path, root: Path) -> list[Component]:
    text = source_path.read_text(encoding="utf-8")
    components: list[Component] = []

    for attr_match in COMPONENT_ATTR_RE.finditer(text):
        fn_match = FN_RE.search(text, attr_match.end())
        if fn_match is None:
            continue

        next_attr = COMPONENT_ATTR_RE.search(text, attr_match.end())
        if next_attr is not None and fn_match.start() > next_attr.start():
            continue

        open_brace = _find_next_open_brace(text, fn_match.end())
        if open_brace is None:
            continue
        close_brace = _find_matching_brace(text, open_brace)
        if close_brace is None:
            continue

        body = text[open_brace + 1 : close_brace]
        normalized = normalize_body(body)
        line = text.count("\n", 0, fn_match.start()) + 1
        rel_file = source_path.relative_to(root).as_posix()
        components.append(
            Component(
                name=fn_match.group(1),
                file=rel_file,
                line=line,
                body=body,
                normalized=normalized,
                tokens=tokenize(normalized),
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


def _max_possible_ratio(len_left: int, len_right: int) -> float:
    if len_left == 0 and len_right == 0:
        return 1.0
    if len_left == 0 or len_right == 0:
        return 0.0
    return (2.0 * min(len_left, len_right)) / float(len_left + len_right)


def compute_pairs(
    components: list[Component], threshold: float
) -> tuple[list[Pair], int, int, int]:
    pairs: list[Pair] = []
    total_pairs = 0
    skipped_by_length = 0
    skipped_by_quick = 0
    lengths = [len(component.tokens) for component in components]

    for left in range(len(components)):
        for right in range(left + 1, len(components)):
            total_pairs += 1
            if _max_possible_ratio(lengths[left], lengths[right]) < threshold:
                skipped_by_length += 1
                continue
            matcher = difflib.SequenceMatcher(
                None,
                components[left].tokens,
                components[right].tokens,
            )
            if matcher.real_quick_ratio() < threshold or matcher.quick_ratio() < threshold:
                skipped_by_quick += 1
                continue
            ratio = matcher.ratio()
            pairs.append(Pair(left=left, right=right, ratio=ratio))
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

        left_peer, left_ratio = best_peer[pair.left]
        if pair.ratio > left_ratio:
            best_peer[pair.left] = (pair.right, pair.ratio)
        right_peer, right_ratio = best_peer[pair.right]
        if pair.ratio > right_ratio:
            best_peer[pair.right] = (pair.left, pair.ratio)

    top_pairs = sorted(pairs, key=lambda p: p.ratio, reverse=True)[:10]
    qualifying_components: list[dict[str, Any]] = []
    for idx, component in enumerate(components):
        peer_idx, ratio = best_peer[idx]
        if component.key not in qualifying_keys:
            continue
        peer = components[peer_idx] if peer_idx is not None else None
        qualifying_components.append(
            {
                "name": component.name,
                "file": component.file,
                "line": component.line,
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

    report = {
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
    return report


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
