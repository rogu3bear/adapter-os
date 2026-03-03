#!/usr/bin/env python3
"""
PRD-UI-010: Trim STYLE_ALLOWLIST.md

Removes unused utilities from the allowlist based on var/reports/ui_util/ui_util_unused.txt
"""

import re
import sys
from pathlib import Path

UI_ROOT = Path(__file__).parent.parent / "crates" / "adapteros-ui"
REPORTS_DIR = Path(__file__).parent.parent / "var" / "reports" / "ui_util"
ALLOWLIST_MD = UI_ROOT / "STYLE_ALLOWLIST.md"
UNUSED_FILE = REPORTS_DIR / "ui_util_unused.txt"


def load_unused_classes(path: Path) -> set:
    """Load unused class names from the report."""
    classes = set()
    content = path.read_text()
    for line in content.split('\n'):
        line = line.strip()
        if not line or line.startswith('#'):
            continue
        # Format: "class-name (Status)"
        match = re.match(r'^([^\s]+)\s+\(', line)
        if match:
            classes.add(match.group(1))
    return classes


def trim_allowlist(allowlist_path: Path, unused: set) -> tuple:
    """Remove unused entries from the allowlist."""
    content = allowlist_path.read_text()
    lines = content.split('\n')

    new_lines = []
    removed = []
    in_table = False

    for line in lines:
        # Detect table rows
        if line.strip().startswith('|'):
            # Check if this is a table row with a class
            match = re.match(r'\|\s*`\.([^`]+)`\s*\|', line)
            if match:
                class_name = match.group(1)
                if class_name in unused:
                    removed.append(class_name)
                    continue  # Skip this line (remove it)

            # Also check for range patterns
            range_match = re.match(r'\|\s*`\.([\w-]+)`\s*to\s*`\.([\w-]+)`\s*\|', line)
            if range_match:
                start = range_match.group(1)
                end = range_match.group(2)
                # Extract prefix and range
                start_m = re.match(r'^([\w-]+-?)(\d+)$', start)
                end_m = re.match(r'^([\w-]+-?)(\d+)$', end)

                if start_m and end_m and start_m.group(1) == end_m.group(1):
                    prefix = start_m.group(1)
                    start_num = int(start_m.group(2))
                    end_num = int(end_m.group(2))

                    # Check if ALL classes in the range are unused
                    range_classes = {f"{prefix}{i}" for i in range(start_num, end_num + 1)}
                    if range_classes.issubset(unused):
                        removed.extend(sorted(range_classes))
                        continue  # Skip this line

        new_lines.append(line)

    return '\n'.join(new_lines), removed


def main():
    print("PRD-UI-010: Trim Allowlist")
    print("=" * 50)

    if not UNUSED_FILE.exists():
        print(f"ERROR: {UNUSED_FILE} not found. Run ui_util_audit.py first.")
        return 1

    print(f"\nLoading unused classes from {UNUSED_FILE.name}...")
    unused = load_unused_classes(UNUSED_FILE)
    print(f"  Found {len(unused)} unused classes to remove")

    print(f"\nTrimming {ALLOWLIST_MD.name}...")
    original_content = ALLOWLIST_MD.read_text()
    original_count = len(re.findall(r'\|\s*`\.([^`]+)`\s*\|', original_content))

    new_content, removed = trim_allowlist(ALLOWLIST_MD, unused)
    new_count = len(re.findall(r'\|\s*`\.([^`]+)`\s*\|', new_content))

    print(f"  Removed {len(removed)} entries")
    print(f"  Before: {original_count} entries")
    print(f"  After:  {new_count} entries")

    # Write the new content
    ALLOWLIST_MD.write_text(new_content)
    print(f"\n✅ {ALLOWLIST_MD.name} updated successfully")

    # Print summary of removed classes
    if removed:
        print("\nRemoved classes:")
        for cls in sorted(removed)[:20]:
            print(f"  - {cls}")
        if len(removed) > 20:
            print(f"  ... and {len(removed) - 20} more")

    return 0


if __name__ == "__main__":
    sys.exit(main())
