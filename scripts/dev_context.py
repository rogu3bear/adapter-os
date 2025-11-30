#!/usr/bin/env python3
"""
scripts/dev_context.py

Multi-Agent Development Coordinator
Helps parallel agents isolate their changes and coordinate commits by "locking" file paths.

Usage:
  python3 scripts/dev_context.py claim --intent "Fix Login" --paths "crates/auth" "ui/src/login"
  python3 scripts/dev_context.py status
  python3 scripts/dev_context.py diff --id <ctx_id>
  python3 scripts/dev_context.py release --id <ctx_id>
"""

import os
import sys
import json
import time
import random
import argparse
import subprocess
from pathlib import Path
from datetime import datetime

# Configuration
STATE_DIR = Path("var/dev_context")
COLORS = {
    "BLUE": "\033[0;34m",
    "GREEN": "\033[0;32m",
    "YELLOW": "\033[0;33m",
    "RED": "\033[0;31m",
    "NC": "\033[0m",
}

def init():
    STATE_DIR.mkdir(parents=True, exist_ok=True)

def color(text, color_name):
    if sys.stdout.isatty():
        return f"{COLORS.get(color_name, '')}{text}{COLORS['NC']}"
    return text

def run_git_cmd(args):
    try:
        result = subprocess.run(
            ["git"] + args,
            capture_output=True,
            text=True,
            check=True
        )
        return result.stdout.strip()
    except subprocess.CalledProcessError:
        return ""

def get_active_contexts():
    contexts = []
    if not STATE_DIR.exists():
        return contexts
    
    for f in STATE_DIR.glob("*.json"):
        try:
            with open(f, "r") as fd:
                data = json.load(fd)
                contexts.append(data)
        except Exception:
            continue
    return contexts

def check_overlaps(new_paths, current_contexts):
    conflicts = []
    for ctx in current_contexts:
        ctx_paths = ctx.get("paths", [])
        for np in new_paths:
            for cp in ctx_paths:
                # Check if one path contains the other (folder overlap)
                # Normalize to ensure we match correctly
                np_s = str(np).strip("/")
                cp_s = str(cp).strip("/")
                if np_s.startswith(cp_s) or cp_s.startswith(np_s):
                    conflicts.append((ctx["id"], ctx["intent"], cp))
    return conflicts

def cmd_claim(args):
    init()
    contexts = get_active_contexts()
    
    # Check conflicts
    conflicts = check_overlaps(args.paths, contexts)
    if conflicts and not args.force:
        print(color("ERROR: Path conflict detected!", "RED"))
        for cid, intent, path in conflicts:
            print(f"  - Agent {cid} is working on '{path}' (Intent: {intent})")
        print(color("Use --force to override.", "YELLOW"))
        sys.exit(1)

    # Generate ID
    ctx_id = f"ctx-{random.randint(10000, 99999)}"
    
    state = {
        "id": ctx_id,
        "timestamp": datetime.now().isoformat(),
        "intent": args.intent,
        "paths": args.paths,
        "pid": os.getpid()
    }
    
    with open(STATE_DIR / f"{ctx_id}.json", "w") as f:
        json.dump(state, f, indent=2)
    
    print(color(f"Context claimed: {ctx_id}", "GREEN"))
    print(f"Intent: {args.intent}")
    print(f"Scope: {', '.join(args.paths)}")

def cmd_release(args):
    target = STATE_DIR / f"{args.id}.json"
    if target.exists():
        # Load context info before deletion for logging
        with open(target, "r") as f:
            data = json.load(f)
            
        target.unlink()
        print(color(f"Released context {args.id}", "GREEN"))
        print(f"Completed task: {data.get('intent', 'Unknown')}")
    else:
        print(color(f"Context {args.id} not found", "RED"))
        sys.exit(1)

def cmd_status(args):
    contexts = get_active_contexts()
    
    # Get all changed files from git
    git_status = run_git_cmd(["diff", "--name-only"])
    all_changed_files = set(git_status.splitlines()) if git_status else set()
    
    print(color("=== Active Development Contexts ===", "BLUE"))
    if not contexts:
        print("No active agents.")
    
    claimed_files = set()
    
    for ctx in contexts:
        print(f"\n{color('Agent ' + ctx['id'], 'GREEN')} ({ctx['intent']})")
        print(f"  Scope: {', '.join(ctx['paths'])}")
        
        # Find which changed files belong to this agent
        agent_files = []
        for f in all_changed_files:
            for p in ctx['paths']:
                if f.startswith(p):
                    agent_files.append(f)
                    claimed_files.add(f)
                    break
        
        if agent_files:
            print(color("  Pending Changes:", "YELLOW"))
            for f in agent_files[:10]:
                print(f"    - {f}")
            if len(agent_files) > 10:
                print(f"    ... and {len(agent_files)-10} more")
        else:
            print("  (No pending changes detected in scope)")

    # Show unclaimed changes
    unclaimed = all_changed_files - claimed_files
    if unclaimed:
        print(f"\n{color('=== Unclaimed Changes (Untracked) ===', 'RED')}")
        for f in list(unclaimed)[:10]:
            print(f"  ? {f}")
        if len(unclaimed) > 10:
            print(f"  ... and {len(unclaimed)-10} more")
    print("")

def cmd_diff(args):
    target = STATE_DIR / f"{args.id}.json"
    if not target.exists():
        print(color(f"Context {args.id} not found", "RED"))
        sys.exit(1)
        
    with open(target, "r") as f:
        data = json.load(f)
        
    paths = data.get("paths", [])
    if not paths:
        print("No paths in context")
        return

    print(color(f"=== Diff for {args.id} ({data['intent']}) ===", "BLUE"))
    # Run git diff on specific paths
    subprocess.run(["git", "diff", "--"] + paths)

def main():
    parser = argparse.ArgumentParser(description="Dev Context Manager")
    subparsers = parser.add_subparsers(dest="command", help="Command")
    
    # Claim
    p_claim = subparsers.add_parser("claim", help="Claim a development scope")
    p_claim.add_argument("--intent", required=True, help="Description of work")
    p_claim.add_argument("--paths", nargs="+", required=True, help="Paths to lock")
    p_claim.add_argument("--force", action="store_true", help="Ignore conflicts")
    
    # Release
    p_release = subparsers.add_parser("release", help="Release a context")
    p_release.add_argument("--id", required=True, help="Context ID")
    
    # Status
    p_status = subparsers.add_parser("status", help="Show status")
    
    # Diff
    p_diff = subparsers.add_parser("diff", help="Show scoped diff")
    p_diff.add_argument("--id", required=True, help="Context ID")
    
    # Clear
    p_clear = subparsers.add_parser("clear", help="Clear all contexts")

    args = parser.parse_args()
    
    if args.command == "claim":
        cmd_claim(args)
    elif args.command == "release":
        cmd_release(args)
    elif args.command == "status":
        cmd_status(args)
    elif args.command == "diff":
        cmd_diff(args)
    elif args.command == "clear":
        if STATE_DIR.exists():
            for f in STATE_DIR.glob("*.json"):
                f.unlink()
        print("All contexts cleared.")
    else:
        parser.print_help()

if __name__ == "__main__":
    main()
