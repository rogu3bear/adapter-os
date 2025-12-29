#!/usr/bin/env python3
"""
Log digest tool for AdapterOS.

Scans a unified log directory, extracts WARN/ERROR entries within a time window,
and writes a JSON + text summary for review before any automated action.
"""

from __future__ import annotations

import argparse
import datetime as dt
import json
import os
import re
from collections import defaultdict
from pathlib import Path
from typing import Any, Dict, Iterable, Optional, Tuple


ISO_TS_RE = re.compile(r"(?P<ts>\d{4}-\d{2}-\d{2}T[^\s]+)")
LEVEL_RE = re.compile(r"\b(ERROR|WARN|WARNING)\b", re.IGNORECASE)


def parse_iso_ts(raw: Optional[str]) -> Optional[dt.datetime]:
    if not raw:
        return None
    try:
        if raw.endswith("Z"):
            return dt.datetime.fromisoformat(raw.replace("Z", "+00:00"))
        return dt.datetime.fromisoformat(raw)
    except ValueError:
        return None


def normalize_level(raw: Optional[str]) -> Optional[str]:
    if not raw:
        return None
    upper = raw.upper()
    if upper == "WARNING":
        return "WARN"
    if upper in ("WARN", "ERROR"):
        return upper
    return None


def parse_plain_line(line: str) -> Tuple[Optional[str], Optional[str], str]:
    ts_match = ISO_TS_RE.search(line)
    ts = ts_match.group("ts") if ts_match else None
    level_match = LEVEL_RE.search(line)
    level = normalize_level(level_match.group(1) if level_match else None)
    message = line.strip()
    if ts_match and level_match:
        # Strip leading timestamp and level for a cleaner message
        message = line[ts_match.end() :].strip()
        if message.startswith(level_match.group(1)):
            message = message[len(level_match.group(1)) :].strip()
    return ts, level, message


def iter_log_files(log_dir: Path) -> Iterable[Path]:
    if not log_dir.exists():
        return []
    files = []
    for root, _, filenames in os.walk(log_dir):
        for name in filenames:
            path = Path(root) / name
            if path.is_file():
                files.append(path)
    return sorted(files)


def within_window(
    ts: Optional[dt.datetime],
    file_mtime: dt.datetime,
    cutoff: dt.datetime,
) -> bool:
    if ts is not None:
        return ts >= cutoff
    return file_mtime >= cutoff


def render_text_summary(summary: Dict[str, Any]) -> str:
    lines = []
    lines.append(f"Log digest generated_at={summary['generated_at']}")
    lines.append(f"log_dir={summary['log_dir']}")
    lines.append(f"window_minutes={summary['window_minutes']}")
    lines.append(f"files_scanned={summary['files_scanned']}")
    lines.append(f"entries_total={summary['summary']['total_entries']}")
    lines.append(
        f"errors={summary['summary']['by_level'].get('ERROR', 0)} "
        f"warns={summary['summary']['by_level'].get('WARN', 0)}"
    )
    lines.append("")
    lines.append("Top messages:")
    for item in summary["summary"]["top_messages"]:
        lines.append(
            f"- count={item['count']} "
            f"errors={item['levels'].get('ERROR', 0)} "
            f"warns={item['levels'].get('WARN', 0)} "
            f"first={item['first_seen']} last={item['last_seen']}\n"
            f"  message={item['message']}"
        )
    return "\n".join(lines)


def main() -> int:
    parser = argparse.ArgumentParser(description="Summarize WARN/ERROR logs.")
    default_var_dir = Path(os.environ.get("AOS_VAR_DIR", "var"))
    parser.add_argument(
        "--log-dir",
        default=str(default_var_dir / "logs"),
        help="Log directory to scan (default: $AOS_VAR_DIR/logs or ./var/logs).",
    )
    parser.add_argument(
        "--out-dir",
        default=str(default_var_dir / "analysis"),
        help="Output directory (default: $AOS_VAR_DIR/analysis or ./var/analysis).",
    )
    parser.add_argument(
        "--minutes",
        type=int,
        default=60,
        help="Lookback window in minutes (default: 60).",
    )
    parser.add_argument(
        "--max-entries",
        type=int,
        default=500,
        help="Maximum entries to include (default: 500).",
    )
    parser.add_argument(
        "--top",
        type=int,
        default=20,
        help="Top message groups to include (default: 20).",
    )
    args = parser.parse_args()

    log_dir = Path(args.log_dir)
    out_dir = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    now = dt.datetime.now(dt.timezone.utc)
    cutoff = now - dt.timedelta(minutes=args.minutes)

    grouped: Dict[str, Dict[str, Any]] = {}
    entries = []
    by_level = defaultdict(int)
    files_scanned = 0

    for path in iter_log_files(log_dir):
        files_scanned += 1
        file_mtime = dt.datetime.fromtimestamp(path.stat().st_mtime, dt.timezone.utc)
        try:
            with path.open("r", encoding="utf-8", errors="replace") as handle:
                for idx, raw_line in enumerate(handle, start=1):
                    line = raw_line.strip()
                    if not line:
                        continue
                    level = None
                    ts = None
                    message = None
                    data: Dict[str, Any] = {}

                    if line.startswith("{") and line.endswith("}"):
                        try:
                            data = json.loads(line)
                        except json.JSONDecodeError:
                            data = {}

                    if data:
                        level = normalize_level(str(data.get("level", "")).upper())
                        if level is None:
                            continue
                        ts = parse_iso_ts(
                            data.get("ts")
                            or data.get("timestamp")
                            or data.get("time")
                            or data.get("datetime")
                        )
                        if not within_window(ts, file_mtime, cutoff):
                            continue
                        message = (
                            data.get("message")
                            or data.get("msg")
                            or data.get("event")
                            or data.get("error")
                            or ""
                        )
                        message = str(message).strip()
                    else:
                        ts_raw, level, message = parse_plain_line(line)
                        if level is None:
                            continue
                        ts = parse_iso_ts(ts_raw)
                        if not within_window(ts, file_mtime, cutoff):
                            continue

                    if not message:
                        message = line.strip()

                    by_level[level] += 1
                    key = message
                    group = grouped.get(key)
                    ts_iso = ts.isoformat() if ts else None
                    if group is None:
                        grouped[key] = {
                            "message": message,
                            "count": 1,
                            "levels": {level: 1},
                            "first_seen": ts_iso,
                            "last_seen": ts_iso,
                            "sample": {"file": str(path), "line": idx},
                        }
                    else:
                        group["count"] += 1
                        group["levels"][level] = group["levels"].get(level, 0) + 1
                        if ts_iso and (group["first_seen"] is None or ts_iso < group["first_seen"]):
                            group["first_seen"] = ts_iso
                        if ts_iso and (group["last_seen"] is None or ts_iso > group["last_seen"]):
                            group["last_seen"] = ts_iso

                    if args.max_entries > 0 and len(entries) < args.max_entries:
                        entry = {
                            "ts": ts_iso,
                            "level": level,
                            "message": message,
                            "file": str(path),
                            "line": idx,
                        }
                        if data:
                            entry["error_code"] = data.get("error_code") or data.get("code")
                            entry["component"] = data.get("component")
                            entry["phase"] = data.get("phase")
                        entries.append(entry)
        except OSError:
            continue

    top_messages = sorted(grouped.values(), key=lambda item: item["count"], reverse=True)[
        : args.top
    ]

    summary = {
        "generated_at": now.isoformat(),
        "log_dir": str(log_dir),
        "window_minutes": args.minutes,
        "files_scanned": files_scanned,
        "summary": {
            "total_entries": sum(by_level.values()),
            "by_level": dict(by_level),
            "top_messages": top_messages,
        },
        "entries": entries,
    }

    json_path = out_dir / "digest.json"
    txt_path = out_dir / "digest.txt"

    json_path.write_text(json.dumps(summary, indent=2, sort_keys=True), encoding="utf-8")
    txt_path.write_text(render_text_summary(summary), encoding="utf-8")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
