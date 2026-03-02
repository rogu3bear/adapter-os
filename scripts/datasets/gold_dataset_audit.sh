#!/bin/sh
set -eu

usage() {
  cat <<'USAGE'
Usage:
  scripts/datasets/gold_dataset_audit.sh [options] <path> [<path> ...]

Options:
  --slice-quota SPEC      Comma-separated quotas: slice:min[:max],slice:min[:max]
  --slice-quota-file FILE JSON file mapping slice -> {"min":N,"max":N}
  -h, --help              Show help

Notes:
  - Accepts JSONL files and/or directories (directories are scanned recursively for *.jsonl).
  - Prints machine-readable summary JSON to stdout.
  - Exit code is 0 when no violations are found; otherwise 1.
USAGE
}

SLICE_QUOTA_SPEC=""
SLICE_QUOTA_FILE=""

PATH_ARGS=""

while [ "$#" -gt 0 ]; do
  case "$1" in
    --slice-quota)
      [ "$#" -ge 2 ] || { echo "missing value for --slice-quota" >&2; exit 2; }
      SLICE_QUOTA_SPEC="$2"
      shift 2
      ;;
    --slice-quota-file)
      [ "$#" -ge 2 ] || { echo "missing value for --slice-quota-file" >&2; exit 2; }
      SLICE_QUOTA_FILE="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    --)
      shift
      break
      ;;
    -*)
      echo "unknown option: $1" >&2
      usage >&2
      exit 2
      ;;
    *)
      if [ -z "$PATH_ARGS" ]; then
        PATH_ARGS="$1"
      else
        PATH_ARGS="$PATH_ARGS\n$1"
      fi
      shift
      ;;
  esac
done

while [ "$#" -gt 0 ]; do
  if [ -z "$PATH_ARGS" ]; then
    PATH_ARGS="$1"
  else
    PATH_ARGS="$PATH_ARGS\n$1"
  fi
  shift
done

[ -n "$PATH_ARGS" ] || { usage >&2; exit 2; }

command -v python3 >/dev/null 2>&1 || { echo "python3 is required" >&2; exit 1; }

TMP_INPUTS="$(mktemp)"
trap 'rm -f "$TMP_INPUTS"' EXIT HUP INT TERM
printf '%b\n' "$PATH_ARGS" | sed '/^$/d' > "$TMP_INPUTS"

SLICE_QUOTA_SPEC="$SLICE_QUOTA_SPEC" SLICE_QUOTA_FILE="$SLICE_QUOTA_FILE" python3 - "$TMP_INPUTS" <<'PY'
import hashlib
import json
import os
import re
import sys
from pathlib import Path
from typing import Dict, List, Tuple

inputs_file = Path(sys.argv[1])


def parse_quota_spec(spec: str) -> Dict[str, Dict[str, int]]:
    out: Dict[str, Dict[str, int]] = {}
    if not spec.strip():
      return out
    for part in spec.split(','):
        part = part.strip()
        if not part:
            continue
        bits = part.split(':')
        if len(bits) < 2 or len(bits) > 3:
            raise ValueError(f"invalid --slice-quota entry: {part}")
        name = bits[0].strip()
        if not name:
            raise ValueError(f"empty slice name in --slice-quota entry: {part}")
        min_val = int(bits[1])
        if min_val < 0:
            raise ValueError(f"min must be >= 0 in --slice-quota entry: {part}")
        quota: Dict[str, int] = {"min": min_val}
        if len(bits) == 3 and bits[2].strip() != "":
            max_val = int(bits[2])
            if max_val < min_val:
                raise ValueError(f"max must be >= min in --slice-quota entry: {part}")
            quota["max"] = max_val
        out[name] = quota
    return out


def parse_quota_file(path: str) -> Dict[str, Dict[str, int]]:
    if not path:
        return {}
    p = Path(path)
    raw = json.loads(p.read_text(encoding="utf-8"))
    if not isinstance(raw, dict):
        raise ValueError("slice quota file must be a JSON object")
    out: Dict[str, Dict[str, int]] = {}
    for key, val in raw.items():
        if not isinstance(key, str) or not key:
            raise ValueError("slice quota file has invalid slice key")
        if not isinstance(val, dict):
            raise ValueError(f"slice quota value for '{key}' must be an object")
        q: Dict[str, int] = {}
        if "min" in val:
            min_v = int(val["min"])
            if min_v < 0:
                raise ValueError(f"slice '{key}' min must be >= 0")
            q["min"] = min_v
        if "max" in val and val["max"] is not None:
            max_v = int(val["max"])
            if max_v < 0:
                raise ValueError(f"slice '{key}' max must be >= 0")
            q["max"] = max_v
        if "min" in q and "max" in q and q["max"] < q["min"]:
            raise ValueError(f"slice '{key}' max must be >= min")
        out[key] = q
    return out


def discover_files(items: List[str]) -> List[Path]:
    paths: List[Path] = []
    for item in items:
        p = Path(item).resolve()
        if p.is_file() and p.suffix == ".jsonl":
            paths.append(p)
        elif p.is_dir():
            for found in sorted(p.rglob("*.jsonl")):
                if found.is_file():
                    paths.append(found.resolve())
    # unique, stable order
    dedup = []
    seen = set()
    for p in sorted(paths):
        key = str(p)
        if key not in seen:
            seen.add(key)
            dedup.append(p)
    return dedup


def check_role_order(messages: List[dict]) -> Tuple[bool, str]:
    if not messages:
        return False, "messages array must not be empty"
    roles = [m.get("role") for m in messages]
    if any(r not in {"system", "user", "assistant"} for r in roles):
        return False, "messages contains invalid role"

    i = 0
    if roles and roles[0] == "system":
        i = 1
        if roles.count("system") > 1:
            return False, "at most one system message is allowed"
        if len(roles) < 3:
            return False, "system-prefixed records require user and assistant turns"
    else:
        if roles.count("system") > 0:
            return False, "system message must be first when present"

    if i >= len(roles):
        return False, "missing user/assistant turns"
    if roles[i] != "user":
        return False, "first non-system message must be user"
    if roles[-1] != "assistant":
        return False, "last message must be assistant"

    expected = "user"
    for idx in range(i, len(roles)):
        role = roles[idx]
        if role != expected:
            return False, f"role order violation at message index {idx}: expected {expected}, got {role}"
        expected = "assistant" if expected == "user" else "user"
    return True, ""


def contract_ok(contract: str, assistant_text: str) -> bool:
    text = assistant_text.strip()
    if contract == "numbered_steps":
        return bool(re.search(r"(^|\n)\s*1[\.)]\s+", text))
    if contract == "short_status":
        lines = [ln for ln in text.splitlines() if ln.strip()]
        return len(lines) <= 3 and len(text) <= 280
    if contract == "json_object":
        try:
            parsed = json.loads(text)
            return isinstance(parsed, dict)
        except Exception:
            return False
    if contract == "single_sentence":
        return text.count(".") + text.count("!") + text.count("?") <= 1
    if contract == "verbatim":
        return len(text) > 0
    return True


summary = {
    "ok": True,
    "files_scanned": 0,
    "records_scanned": 0,
    "checks": {
        "schema": {"violations": 0},
        "message_role_order": {"violations": 0},
        "output_contract": {"violations": 0},
        "duplicates": {"violations": 0, "duplicate_lines": 0},
        "slice_quota": {"violations": 0},
    },
    "distribution": {
        "slice_counts": {},
        "slice_percentages": {},
    },
    "errors": [],
}

try:
    quota = parse_quota_file(os.environ.get("SLICE_QUOTA_FILE", ""))
    quota.update(parse_quota_spec(os.environ.get("SLICE_QUOTA_SPEC", "")))
except Exception as e:
    print(json.dumps({"ok": False, "error": f"invalid quota config: {e}"}, ensure_ascii=True))
    sys.exit(2)

inputs = [line.strip() for line in inputs_file.read_text(encoding="utf-8").splitlines() if line.strip()]
files = discover_files(inputs)
summary["files_scanned"] = len(files)

seen_line_hash: Dict[str, List[Tuple[str, int]]] = {}
slice_counts: Dict[str, int] = {}

for file_path in files:
    try:
        with file_path.open("r", encoding="utf-8", errors="replace") as fh:
            for line_no, line in enumerate(fh, start=1):
                raw = line.rstrip("\n")
                if not raw.strip():
                    continue

                summary["records_scanned"] += 1
                line_hash = hashlib.sha256(raw.encode("utf-8", errors="replace")).hexdigest()
                seen_line_hash.setdefault(line_hash, []).append((str(file_path), line_no))

                try:
                    obj = json.loads(raw)
                except Exception as e:
                    summary["checks"]["schema"]["violations"] += 1
                    summary["errors"].append({
                        "type": "schema",
                        "file": str(file_path),
                        "line": line_no,
                        "message": f"invalid JSON: {e}",
                    })
                    continue

                if not isinstance(obj, dict):
                    summary["checks"]["schema"]["violations"] += 1
                    summary["errors"].append({
                        "type": "schema",
                        "file": str(file_path),
                        "line": line_no,
                        "message": "record must be a JSON object",
                    })
                    continue

                messages = obj.get("messages")
                metadata = obj.get("metadata")

                if not isinstance(messages, list) or not isinstance(metadata, dict):
                    summary["checks"]["schema"]["violations"] += 1
                    summary["errors"].append({
                        "type": "schema",
                        "file": str(file_path),
                        "line": line_no,
                        "message": "record requires messages[] and metadata{}",
                    })
                    continue

                msg_schema_ok = True
                for mi, m in enumerate(messages):
                    if not isinstance(m, dict):
                        msg_schema_ok = False
                        err = f"message[{mi}] must be object"
                        break
                    role = m.get("role")
                    content = m.get("content")
                    if role not in {"system", "user", "assistant"}:
                        msg_schema_ok = False
                        err = f"message[{mi}].role is invalid"
                        break
                    if not isinstance(content, str) or content.strip() == "":
                        msg_schema_ok = False
                        err = f"message[{mi}].content must be non-empty string"
                        break
                if not msg_schema_ok:
                    summary["checks"]["schema"]["violations"] += 1
                    summary["errors"].append({
                        "type": "schema",
                        "file": str(file_path),
                        "line": line_no,
                        "message": err,
                    })
                    continue

                ok_order, reason = check_role_order(messages)
                if not ok_order:
                    summary["checks"]["message_role_order"]["violations"] += 1
                    summary["errors"].append({
                        "type": "message_role_order",
                        "file": str(file_path),
                        "line": line_no,
                        "message": reason,
                    })

                assistant_text = ""
                for m in reversed(messages):
                    if m.get("role") == "assistant":
                        assistant_text = m.get("content", "")
                        break

                contract = metadata.get("expected_output_contract")
                if not isinstance(contract, str) or not contract.strip():
                    summary["checks"]["output_contract"]["violations"] += 1
                    summary["errors"].append({
                        "type": "output_contract",
                        "file": str(file_path),
                        "line": line_no,
                        "message": "metadata.expected_output_contract is required",
                    })
                elif not contract_ok(contract.strip(), assistant_text):
                    summary["checks"]["output_contract"]["violations"] += 1
                    summary["errors"].append({
                        "type": "output_contract",
                        "file": str(file_path),
                        "line": line_no,
                        "message": f"assistant output violates expected_output_contract={contract}",
                    })

                slice_name = metadata.get("slice")
                if isinstance(slice_name, str) and slice_name.strip():
                    key = slice_name.strip()
                    slice_counts[key] = slice_counts.get(key, 0) + 1
                else:
                    slice_counts["__missing__"] = slice_counts.get("__missing__", 0) + 1

    except Exception as e:
        summary["checks"]["schema"]["violations"] += 1
        summary["errors"].append({
            "type": "schema",
            "file": str(file_path),
            "line": 0,
            "message": f"failed to read file: {e}",
        })

for refs in seen_line_hash.values():
    if len(refs) > 1:
        summary["checks"]["duplicates"]["duplicate_lines"] += 1
        summary["checks"]["duplicates"]["violations"] += 1
        summary["errors"].append({
            "type": "duplicates",
            "file": refs[0][0],
            "line": refs[0][1],
            "message": f"duplicate JSONL line appears {len(refs)} times",
            "occurrences": [{"file": f, "line": l} for f, l in refs],
        })

summary["distribution"]["slice_counts"] = dict(sorted(slice_counts.items()))

total = sum(slice_counts.values())
if total > 0:
    summary["distribution"]["slice_percentages"] = {
        k: round((v / total) * 100.0, 4) for k, v in sorted(slice_counts.items())
    }

if quota:
    for name, rule in sorted(quota.items()):
        actual = slice_counts.get(name, 0)
        min_v = rule.get("min")
        max_v = rule.get("max")
        if min_v is not None and actual < min_v:
            summary["checks"]["slice_quota"]["violations"] += 1
            summary["errors"].append({
                "type": "slice_quota",
                "file": "",
                "line": 0,
                "message": f"slice '{name}' below min quota: {actual} < {min_v}",
            })
        if max_v is not None and actual > max_v:
            summary["checks"]["slice_quota"]["violations"] += 1
            summary["errors"].append({
                "type": "slice_quota",
                "file": "",
                "line": 0,
                "message": f"slice '{name}' above max quota: {actual} > {max_v}",
            })

for check in summary["checks"].values():
    if check.get("violations", 0) > 0:
        summary["ok"] = False
        break

print(json.dumps(summary, ensure_ascii=True, sort_keys=True))
if summary["ok"]:
    sys.exit(0)
sys.exit(1)
PY
