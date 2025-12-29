#!/usr/bin/env python3
"""
Log triage tool for AdapterOS.

Consumes a log digest JSON and applies regex rules to produce
actionable triage output (JSON + text).
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import re
from pathlib import Path
from typing import Any, Dict, List, Optional


def load_json(path: Path) -> Dict[str, Any]:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except FileNotFoundError as exc:
        raise SystemExit(f"missing file: {path}") from exc
    except json.JSONDecodeError as exc:
        raise SystemExit(f"invalid json: {path}: {exc}") from exc


def normalize_rules(data: Dict[str, Any]) -> List[Dict[str, Any]]:
    rules = data.get("rules")
    if rules is None and isinstance(data, list):
        rules = data
    if not isinstance(rules, list):
        raise SystemExit("rules file must contain a list or {\"rules\": [...]}")

    normalized: List[Dict[str, Any]] = []
    for item in rules:
        if not isinstance(item, dict):
            continue
        rule_id = item.get("id")
        pattern = item.get("pattern")
        if not rule_id or not pattern:
            continue
        priority = int(item.get("priority", 100))
        severity = str(item.get("severity", "unknown")).lower()
        levels = item.get("levels")
        if levels is not None and isinstance(levels, list):
            levels = [str(level).upper() for level in levels]
        else:
            levels = None
        normalized.append(
            {
                "id": str(rule_id),
                "pattern": str(pattern),
                "regex": re.compile(str(pattern), re.IGNORECASE),
                "priority": priority,
                "severity": severity,
                "title": item.get("title"),
                "levels": levels,
                "remediation": item.get("remediation", {}),
            }
        )

    normalized.sort(key=lambda rule: rule["priority"])
    return normalized


def match_rule(message: str, levels: Dict[str, int], rules: List[Dict[str, Any]]) -> Optional[Dict[str, Any]]:
    for rule in rules:
        if rule["levels"]:
            if not any(level in levels for level in rule["levels"]):
                continue
        if rule["regex"].search(message):
            return rule
    return None


def render_text_summary(summary: Dict[str, Any]) -> str:
    lines = []
    lines.append(f"Log triage generated_at={summary['generated_at']}")
    lines.append(f"digest_path={summary['digest_path']}")
    lines.append(f"rules_path={summary['rules_path']}")
    lines.append(f"total_groups={summary['summary']['total_groups']}")
    lines.append(f"matched_groups={summary['summary']['matched_groups']}")
    lines.append(f"unmatched_groups={summary['summary']['unmatched_groups']}")
    lines.append("severity_counts=" + json.dumps(summary["summary"]["by_severity"]))
    lines.append("")

    if summary["findings"]:
        lines.append("Findings:")
        for finding in summary["findings"]:
            levels = finding.get("levels", {})
            lines.append(
                f"- [{finding['severity']}] {finding.get('title') or finding['rule_id']} "
                f"(count={finding['count']} errors={levels.get('ERROR', 0)} warns={levels.get('WARN', 0)})"
            )
            lines.append(f"  message={finding['message']}")
            if finding.get("first_seen") or finding.get("last_seen"):
                lines.append(
                    f"  first={finding.get('first_seen')} last={finding.get('last_seen')}"
                )
            sample = finding.get("sample")
            if sample:
                lines.append(f"  sample={sample.get('file')}:{sample.get('line')}")
            remediation = finding.get("remediation") or {}
            if remediation.get("summary"):
                lines.append(f"  remediation={remediation['summary']}")
            for step in remediation.get("steps", []):
                lines.append(f"  step: {step}")
            for cmd in remediation.get("commands", []):
                lines.append(f"  cmd: {cmd}")
    else:
        lines.append("Findings: none")

    if summary["unmatched"]:
        lines.append("")
        lines.append("Unmatched:")
        for item in summary["unmatched"]:
            lines.append(
                f"- count={item['count']} errors={item['levels'].get('ERROR', 0)} "
                f"warns={item['levels'].get('WARN', 0)} message={item['message']}"
            )
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description="Triage log digest with regex rules.")
    default_var_dir = Path(os.environ.get("AOS_VAR_DIR", "var"))
    default_rules = os.environ.get("AOS_LOG_TRIAGE_RULES", "configs/log_triage_rules.json")
    parser.add_argument(
        "--digest",
        default=str(default_var_dir / "analysis" / "digest.json"),
        help="Digest JSON path (default: $AOS_VAR_DIR/analysis/digest.json).",
    )
    parser.add_argument(
        "--rules",
        default=default_rules,
        help="Rules file (default: configs/log_triage_rules.json).",
    )
    parser.add_argument(
        "--out-dir",
        default=str(default_var_dir / "analysis"),
        help="Output directory (default: $AOS_VAR_DIR/analysis).",
    )
    parser.add_argument(
        "--max-findings",
        type=int,
        default=50,
        help="Maximum findings to include (default: 50).",
    )
    parser.add_argument(
        "--max-unmatched",
        type=int,
        default=20,
        help="Maximum unmatched groups to include (default: 20).",
    )
    args = parser.parse_args()

    digest_path = Path(args.digest)
    rules_path = Path(args.rules)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    digest = load_json(digest_path)
    rules = normalize_rules(load_json(rules_path))

    top_messages = digest.get("summary", {}).get("top_messages", [])
    if not isinstance(top_messages, list):
        top_messages = []

    findings = []
    unmatched = []
    by_severity: Dict[str, int] = {}

    for item in top_messages:
        if not isinstance(item, dict):
            continue
        message = str(item.get("message", "")).strip()
        if not message:
            continue
        levels = item.get("levels") or {}
        if not isinstance(levels, dict):
            levels = {}

        rule = match_rule(message, levels, rules)
        if rule:
            severity = rule["severity"]
            by_severity[severity] = by_severity.get(severity, 0) + 1
            findings.append(
                {
                    "rule_id": rule["id"],
                    "title": rule.get("title"),
                    "severity": severity,
                    "message": message,
                    "count": item.get("count", 0),
                    "levels": levels,
                    "first_seen": item.get("first_seen"),
                    "last_seen": item.get("last_seen"),
                    "sample": item.get("sample"),
                    "remediation": rule.get("remediation", {}),
                }
            )
        else:
            unmatched.append(
                {
                    "message": message,
                    "count": item.get("count", 0),
                    "levels": levels,
                }
            )

    findings = findings[: args.max_findings]
    unmatched = unmatched[: args.max_unmatched]

    summary = {
        "generated_at": dt.datetime.now(dt.timezone.utc).isoformat(),
        "digest_path": str(digest_path),
        "rules_path": str(rules_path),
        "summary": {
            "total_groups": len(top_messages),
            "matched_groups": len(findings),
            "unmatched_groups": len(unmatched),
            "by_severity": by_severity,
        },
        "findings": findings,
        "unmatched": unmatched,
    }

    json_path = out_dir / "triage.json"
    txt_path = out_dir / "triage.txt"
    json_path.write_text(json.dumps(summary, indent=2, sort_keys=True), encoding="utf-8")
    txt_path.write_text(render_text_summary(summary), encoding="utf-8")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
