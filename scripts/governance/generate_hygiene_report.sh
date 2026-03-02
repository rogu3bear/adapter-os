#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT_DIR"

OUT_DIR="var/reports/governance"
mkdir -p "$OUT_DIR"

STAMP="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
JSON_OUT="$OUT_DIR/hygiene-report.json"
MD_OUT="$OUT_DIR/hygiene-report.md"

TMP_GEN="$(mktemp)"
TMP_TOOL="$(mktemp)"
TMP_SIZE="$(mktemp)"

scripts/ci/check_tracked_generated_policy.sh --strict --format json > "$TMP_GEN" || true
scripts/ci/check_tooling_state_policy.sh --strict --format json > "$TMP_TOOL" || true
scripts/ci/check_repo_size_budget.sh --strict --format json > "$TMP_SIZE" || true

python3 - <<'PY' "$STAMP" "$JSON_OUT" "$MD_OUT" "$TMP_GEN" "$TMP_TOOL" "$TMP_SIZE"
import json,sys,subprocess
stamp,json_out,md_out,gen_p,tool_p,size_p=sys.argv[1:]

def load(path):
    with open(path,encoding='utf-8') as f:
        return json.load(f)

def cmd(c):
    return subprocess.check_output(c,shell=True,text=True).strip()

gen=load(gen_p)
tool=load(tool_p)
size=load(size_p)
report={
  "timestamp_utc": stamp,
  "repo_root": cmd("pwd"),
  "git_branch": cmd("git branch --show-current"),
  "git_head": cmd("git rev-parse HEAD"),
  "checks": {
    "tracked_generated_policy": gen,
    "tooling_state_policy": tool,
    "repo_size_budget": size,
  },
  "summary": {
    "status": "pass" if all(c.get("status")=="pass" for c in [gen,tool,size]) else "fail"
  }
}
with open(json_out,"w",encoding='utf-8') as f:
    json.dump(report,f,indent=2,sort_keys=True)

lines=[
  "# Governance Hygiene Report",
  "",
  f"- Timestamp (UTC): `{stamp}`",
  f"- Branch: `{report['git_branch']}`",
  f"- HEAD: `{report['git_head']}`",
  f"- Overall status: **{report['summary']['status']}**",
  "",
  "## Check Status",
  "",
  f"- tracked_generated_policy: `{gen.get('status')}`",
  f"- tooling_state_policy: `{tool.get('status')}`",
  f"- repo_size_budget: `{size.get('status')}`",
  "",
  "## Violations",
  "",
]
for label,data,key in [
  ("Tracked generated policy",gen,"violations"),
  ("Tooling state policy",tool,"violations"),
  ("Repo size budget oversized",size,"oversized_files"),
  ("Repo size budget disallowed binary",size,"disallowed_binary_files"),
]:
    vals=data.get(key,[])
    lines.append(f"### {label}")
    if vals:
        for v in vals:
            lines.append(f"- `{v}`")
    else:
        lines.append("- none")
    lines.append("")

with open(md_out,"w",encoding='utf-8') as f:
    f.write("\n".join(lines)+"\n")
PY

rm -f "$TMP_GEN" "$TMP_TOOL" "$TMP_SIZE"

echo "wrote: $JSON_OUT"
echo "wrote: $MD_OUT"
