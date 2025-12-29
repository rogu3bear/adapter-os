#!/usr/bin/env python3
"""
Log prompt builder for AdapterOS.

Converts triage JSON into a reviewable LLM prompt file.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
from pathlib import Path
from typing import Any, Dict, List


def load_json(path: Path) -> Dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise SystemExit(f"missing file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid json: {path}: {exc}") from exc


def format_findings(findings: List[Dict[str, Any]], max_items: int) -> List[str]:
    lines: List[str] = []
    for item in findings[:max_items]:
        title = item.get("title") or item.get("rule_id", "unknown")
        severity = str(item.get("severity", "unknown")).lower()
        levels = item.get("levels") or {}
        count = item.get("count", 0)
        errors = levels.get("ERROR", 0)
        warns = levels.get("WARN", 0)

        lines.append(f"- [{severity}] {title} (count={count} errors={errors} warns={warns})")
        message = str(item.get("message", "")).strip()
        if message:
            lines.append(f"  message: {message}")
        if item.get("first_seen") or item.get("last_seen"):
            lines.append(
                f"  first_seen: {item.get('first_seen')} last_seen: {item.get('last_seen')}"
            )
        sample = item.get("sample") or {}
        if sample.get("file"):
            lines.append(f"  sample: {sample.get('file')}:{sample.get('line')}")

        remediation = item.get("remediation") or {}
        if remediation.get("summary"):
            lines.append(f"  remediation_summary: {remediation['summary']}")
        for step in remediation.get("steps", []):
            lines.append(f"  remediation_step: {step}")
        for cmd in remediation.get("commands", []):
            lines.append(f"  suggested_command: {cmd}")
    if not lines:
        lines.append("- none")
    return lines


def format_unmatched(unmatched: List[Dict[str, Any]], max_items: int) -> List[str]:
    lines: List[str] = []
    for item in unmatched[:max_items]:
        message = str(item.get("message", "")).strip()
        levels = item.get("levels") or {}
        count = item.get("count", 0)
        errors = levels.get("ERROR", 0)
        warns = levels.get("WARN", 0)
        lines.append(f"- count={count} errors={errors} warns={warns} message={message}")
    if not lines:
        lines.append("- none")
    return lines


def build_prompt(triage: Dict[str, Any], max_findings: int, max_unmatched: int) -> str:
    summary = triage.get("summary", {})
    by_severity = summary.get("by_severity", {})
    var_dir = Path(os.environ.get("AOS_VAR_DIR", "var"))

    lines: List[str] = []
    lines.append("# AdapterOS Log Analysis Prompt")
    lines.append("")
    lines.append(f"generated_at: {triage.get('generated_at')}")
    lines.append(f"digest_path: {triage.get('digest_path')}")
    lines.append(f"rules_path: {triage.get('rules_path')}")
    lines.append(f"triage_path: (this file)")
    lines.append(f"var_dir: {var_dir}")
    lines.append("")
    lines.append("## Objective")
    lines.append("Analyze the findings below and propose minimal, safe fixes.")
    lines.append(
        "You must return a concise plan and any suggested code or config changes."
    )
    lines.append("")
    lines.append("## Constraints")
    lines.append("- Do not apply changes automatically.")
    lines.append("- Prefer minimal edits; avoid unrelated refactors.")
    lines.append("- Keep all runtime outputs under var/ (AOS_VAR_DIR).")
    lines.append("- If commands are suggested, mark them as SAFE or RISKY.")
    lines.append("")
    lines.append("## Summary")
    lines.append(
        f"total_groups={summary.get('total_groups')} "
        f"matched_groups={summary.get('matched_groups')} "
        f"unmatched_groups={summary.get('unmatched_groups')}"
    )
    lines.append(f"by_severity={json.dumps(by_severity)}")
    lines.append("")
    lines.append("## Findings")
    lines.extend(format_findings(triage.get("findings", []), max_findings))
    lines.append("")
    lines.append("## Unmatched")
    lines.extend(format_unmatched(triage.get("unmatched", []), max_unmatched))
    lines.append("")
    lines.append("## Required Output Format")
    lines.append("1) Root causes (bullet list, ordered by severity)")
    lines.append("2) Proposed fixes (bullet list with SAFE/RISKY tags)")
    lines.append("3) Minimal patch plan (file paths + changes)")
    lines.append("4) Commands to run (if any) with brief justification")
    lines.append("")
    lines.append("## Approval")
    lines.append("Wait for explicit approval before executing any command or patch.")
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description="Build an LLM prompt from triage JSON.")
    default_var_dir = Path(os.environ.get("AOS_VAR_DIR", "var"))
    parser.add_argument(
        "--triage",
        default=str(default_var_dir / "analysis" / "triage.json"),
        help="Triage JSON path (default: $AOS_VAR_DIR/analysis/triage.json).",
    )
    parser.add_argument(
        "--out-dir",
        default=str(default_var_dir / "analysis" / "proposals"),
        help="Output directory (default: $AOS_VAR_DIR/analysis/proposals).",
    )
    parser.add_argument(
        "--max-findings",
        type=int,
        default=20,
        help="Maximum findings to include (default: 20).",
    )
    parser.add_argument(
        "--max-unmatched",
        type=int,
        default=10,
        help="Maximum unmatched groups to include (default: 10).",
    )
    args = parser.parse_args()

    triage_path = Path(args.triage)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    triage = load_json(triage_path)
    prompt = build_prompt(triage, args.max_findings, args.max_unmatched)

    ts = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%d-%H%M%S")
    prompt_path = out_dir / f"prompt-{ts}.md"
    latest_path = out_dir / "prompt-latest.md"

    prompt_path.write_text(prompt, encoding="utf-8")
    latest_path.write_text(prompt, encoding="utf-8")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
