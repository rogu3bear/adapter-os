#!/usr/bin/env python3
"""
PRD-UI-010: Utility Usage Audit Script

Scans the UI codebase and produces:
- var/reports/ui_util/ui_util_usage.md       - Full usage report
- var/reports/ui_util/ui_util_unused.txt     - Allowlisted but never referenced
- var/reports/ui_util/ui_util_defined_unused.txt - Defined in CSS but never referenced
- var/reports/ui_util/ui_util_reject_remove.txt  - Reject utilities status
"""

import os
import re
import sys
from collections import defaultdict
from pathlib import Path

# Configuration
UI_ROOT = Path(__file__).parent.parent / "crates" / "adapteros-ui"
REPORTS_DIR = Path(__file__).parent.parent / "var" / "reports" / "ui_util"

RS_FILES = list(UI_ROOT.glob("src/**/*.rs"))
CSS_FILES = list(UI_ROOT.glob("dist/*.css"))
INDEX_HTML = UI_ROOT / "index.html"
ALLOWLIST_MD = UI_ROOT / "STYLE_ALLOWLIST.md"

# Known CSS class prefixes that help validate tokens
CSS_PREFIXES = {
    'flex', 'grid', 'items', 'justify', 'gap', 'space', 'p', 'px', 'py', 'pt', 'pb', 'pl', 'pr',
    'm', 'mx', 'my', 'mt', 'mb', 'ml', 'mr', 'w', 'h', 'min', 'max', 'text', 'font', 'leading',
    'tracking', 'whitespace', 'break', 'bg', 'border', 'rounded', 'shadow', 'opacity', 'z',
    'absolute', 'relative', 'fixed', 'sticky', 'inset', 'top', 'right', 'bottom', 'left',
    'overflow', 'cursor', 'pointer', 'select', 'transition', 'duration', 'hover', 'focus',
    'disabled', 'sr', 'hidden', 'block', 'inline', 'truncate', 'animate', 'ring', 'col', 'row',
    'sm', 'md', 'lg', 'xl', '2xl', 'dark', 'shrink', 'grow', 'order', 'self', 'place',
    'caption', 'align', 'table', 'backdrop', 'file'
}

# Component class patterns (semantic classes)
COMPONENT_PATTERNS = {
    'btn', 'card', 'input', 'dialog', 'toggle', 'spinner', 'badge', 'status', 'table',
    'label', 'toast', 'banner', 'shell', 'workspace', 'page', 'chat', 'search', 'nav',
    'header', 'footer', 'sidebar', 'panel', 'menu', 'form', 'glass', 'chart', 'heatmap',
    'sparkline', 'mini', 'mobile'
}


def parse_allowlist(path: Path) -> dict:
    """Parse STYLE_ALLOWLIST.md and extract classes with their status."""
    content = path.read_text()
    classes = {}

    # Match table rows with class names: | `.class-name` | Status | Notes |
    pattern = r'\|\s*`\.([^`]+)`\s*\|\s*(Core|Transitional|Reject)\s*\|'

    for match in re.finditer(pattern, content):
        class_name = match.group(1)
        status = match.group(2)
        classes[class_name] = status

    # Handle range patterns like `.px-0` to `.px-8`
    range_pattern = r'\|\s*`\.([\w-]+)`\s*to\s*`\.([\w-]+)`\s*\|\s*(Core|Transitional|Reject)\s*\|'
    for match in re.finditer(range_pattern, content):
        start = match.group(1)
        end = match.group(2)
        status = match.group(3)

        # Extract prefix and range
        start_match = re.match(r'^([\w-]+-?)(\d+)$', start)
        end_match = re.match(r'^([\w-]+-?)(\d+)$', end)

        if start_match and end_match and start_match.group(1) == end_match.group(1):
            prefix = start_match.group(1)
            start_num = int(start_match.group(2))
            end_num = int(end_match.group(2))
            for i in range(start_num, end_num + 1):
                classes[f"{prefix}{i}"] = status

    return classes


def parse_css_classes(css_files: list) -> set:
    """Extract all defined CSS class names from CSS files."""
    classes = set()

    for css_file in css_files:
        content = css_file.read_text()

        # Match class selectors, handling escapes and pseudo-classes
        # Pattern: .class-name followed by { or : or ,
        pattern = r'\.([a-zA-Z_-][a-zA-Z0-9_\-\\/\.\:\[\]%]*)\s*(?:[,{:]|::)'

        for match in re.finditer(pattern, content):
            class_name = match.group(1)
            # Unescape backslashes
            class_name = class_name.replace('\\', '')
            # Remove trailing pseudo-class selectors
            class_name = re.sub(r':(hover|focus|active|disabled|focus-visible|first-child|last-child|nth-child\([^)]+\)).*$', '', class_name)
            if class_name and len(class_name) > 1:
                classes.add(class_name)

    return classes


def is_likely_css_class(token: str, allowlisted: set = None) -> bool:
    """Check if a token looks like a CSS utility class."""
    # If it's in the allowlist, always accept it
    if allowlisted and token in allowlisted:
        return True

    # Reject if starts with uppercase (likely a word/identifier)
    if token and token[0].isupper():
        return False

    # Reject if empty or too short
    if not token or len(token) < 2:
        return False

    # Accept special CSS patterns (with slashes, brackets, etc.)
    # These are Tailwind-style arbitrary values
    if re.match(r'^-?[a-z][a-z0-9-]*(\[.+\]|/\d+(/\d+)?|/[a-z]+)?$', token):
        pass  # Continue checking

    # Reject if it's a Rust/code pattern
    code_patterns = [
        r'^Option::',
        r'^Result::',
        r'^Some\(',
        r'^None$',
        r'^Ok\(',
        r'^Err\(',
        r'^[a-z]+_[a-z]+$',  # snake_case identifiers (but allow kebab-case)
        r'^[A-Z]',  # PascalCase
        r'^[a-z]+\.[a-z]+',  # method calls
        r'^\d',  # starts with number
        r'^[a-z]{1,2}$',  # too short (a, ab, etc.)
        r'^wasm',  # wasm-related
        r'^target',
        r'^cfg\(',
        r'::',  # Rust paths
        r'^http',
        r'^mailto',
    ]
    for pat in code_patterns:
        if re.match(pat, token):
            return False

    # Accept if matches known CSS patterns
    # Check prefix - handle negative utilities like -translate-x-1/2
    base_token = token.lstrip('-')
    base = base_token.split('-')[0].split(':')[-1]  # Handle hover:bg-xxx
    if base in CSS_PREFIXES or base in COMPONENT_PATTERNS:
        return True

    # Accept known responsive/state prefixes
    if any(token.startswith(p + ':') for p in ['hover', 'focus', 'focus-visible', 'disabled', 'sm', 'md', 'lg', 'xl', '2xl', 'dark', 'file', 'placeholder']):
        return True

    # Accept if it looks like a utility (lowercase kebab-case with valid pattern)
    # Also allow slashes for fractional values and brackets for arbitrary values
    if re.match(r'^-?[a-z][a-z0-9]*(-[a-z0-9/\[\]%\.]+)*$', token):
        # Additional filter: reject common English words
        common_words = {
            'and', 'the', 'for', 'with', 'from', 'this', 'that', 'your', 'you',
            'are', 'was', 'were', 'been', 'being', 'have', 'has', 'had', 'does',
            'will', 'would', 'could', 'should', 'may', 'might', 'must', 'can',
            'failed', 'error', 'success', 'loading', 'pending', 'active',
            'data', 'user', 'name', 'type', 'mode', 'not', 'but', 'all', 'get',
            'set', 'new', 'old', 'add', 'use', 'run', 'one', 'two', 'via',
            'message', 'request', 'response', 'token', 'value', 'status',
        }
        if token in common_words:
            return False
        return True

    # Accept component classes
    for comp in COMPONENT_PATTERNS:
        if token == comp or token.startswith(f'{comp}-') or token.startswith(f'{comp}_'):
            return True

    return False


def parse_referenced_classes(rs_files: list, html_file: Path, defined_classes: set, allowlisted: set) -> dict:
    """Extract referenced CSS classes from Rust files and HTML."""
    references = defaultdict(list)  # class -> [(file, line)]
    all_known = defined_classes | allowlisted

    for rs_file in rs_files:
        content = rs_file.read_text()
        lines = content.split('\n')

        for line_num, line in enumerate(lines, 1):
            # Skip comments and doc comments
            stripped = line.strip()
            if stripped.startswith('//') or stripped.startswith('///') or stripped.startswith('//!'):
                continue

            # Find quoted strings that likely contain CSS classes
            string_pattern = r'"([^"]+)"'

            for match in re.finditer(string_pattern, line):
                string_content = match.group(1)

                # Skip if it's clearly not a class context
                if len(string_content) > 500:  # Too long
                    continue
                if '<' in string_content and '>' in string_content:  # HTML-like
                    continue
                if string_content.startswith('http') or string_content.startswith('/'):
                    continue
                if '(' in string_content or ')' in string_content:
                    if 'class' not in line.lower():
                        continue

                # Extract class-like tokens
                tokens = string_content.split()
                for token in tokens:
                    # Clean up token
                    token = token.strip('{}(),;')
                    if not token or len(token) < 2:
                        continue

                    # Must be a plausible CSS class OR be in our defined/allowlisted set
                    if token in all_known or is_likely_css_class(token, allowlisted):
                        rel_path = rs_file.relative_to(UI_ROOT)
                        references[token].append((str(rel_path), line_num))

    # Parse HTML file
    if html_file.exists():
        content = html_file.read_text()
        lines = content.split('\n')

        for line_num, line in enumerate(lines, 1):
            # Match class="..." attributes
            class_pattern = r'class="([^"]+)"'
            for match in re.finditer(class_pattern, line):
                classes_str = match.group(1)
                for cls in classes_str.split():
                    if cls in all_known or is_likely_css_class(cls, allowlisted):
                        references[cls].append(("index.html", line_num))

    return dict(references)


def generate_reports(allowlist: dict, defined: set, references: dict):
    """Generate all report files."""

    REPORTS_DIR.mkdir(parents=True, exist_ok=True)

    # Categorize
    allowlisted_classes = set(allowlist.keys())
    referenced_classes = set(references.keys())

    # Core, Transitional, Reject sets
    core_classes = {c for c, s in allowlist.items() if s == 'Core'}
    transitional_classes = {c for c, s in allowlist.items() if s == 'Transitional'}
    reject_classes = {c for c, s in allowlist.items() if s == 'Reject'}

    # Analysis
    allowlisted_but_unused = allowlisted_classes - referenced_classes
    defined_but_unused = defined - referenced_classes
    referenced_and_allowlisted = referenced_classes & allowlisted_classes
    unknown_referenced = referenced_classes - allowlisted_classes - defined

    # Filter unknown to only plausible CSS classes
    unknown_filtered = {c for c in unknown_referenced if is_likely_css_class(c, allowlisted_classes)}

    # Count references
    ref_counts = {cls: len(refs) for cls, refs in references.items()}
    top_30 = sorted(
        [(cls, cnt) for cls, cnt in ref_counts.items() if cls in allowlisted_classes or cls in defined],
        key=lambda x: -x[1]
    )[:30]

    # Reject utilities analysis
    reject_referenced = reject_classes & referenced_classes
    reject_unreferenced = reject_classes - referenced_classes

    # ===== ui_util_usage.md =====
    with open(REPORTS_DIR / "ui_util_usage.md", 'w') as f:
        f.write("# PRD-UI-010: UI Utility Usage Report\n\n")
        f.write(f"Generated: {os.popen('date').read().strip()}\n\n")

        f.write("## Summary\n\n")
        f.write("| Metric | Count |\n")
        f.write("|--------|-------|\n")
        f.write(f"| Defined in CSS | {len(defined)} |\n")
        f.write(f"| Allowlisted | {len(allowlisted_classes)} |\n")
        f.write(f"| Referenced in code | {len(referenced_classes)} |\n")
        f.write(f"| ├─ Referenced & allowlisted | {len(referenced_and_allowlisted)} |\n")
        f.write(f"| └─ Unknown (not allowlisted) | {len(unknown_filtered)} |\n")
        f.write(f"| Core classes | {len(core_classes)} |\n")
        f.write(f"| Transitional classes | {len(transitional_classes)} |\n")
        f.write(f"| Reject classes | {len(reject_classes)} |\n")
        f.write(f"| **Allowlisted but unused** | **{len(allowlisted_but_unused)}** |\n")
        f.write(f"| Defined but unused | {len(defined_but_unused)} |\n")
        f.write("\n")

        # Reduction calculation
        target = 150
        current = len(allowlisted_classes)
        can_remove = len(allowlisted_but_unused)
        after_removal = current - can_remove
        f.write("## Reduction Analysis\n\n")
        f.write(f"- Current allowlist size: **{current}**\n")
        f.write(f"- Target: **≤{target}**\n")
        f.write(f"- Unused (can remove): **{can_remove}**\n")
        f.write(f"- After removal: **{after_removal}**\n")
        f.write(f"- {'✅ Target met!' if after_removal <= target else f'❌ Still {after_removal - target} over target'}\n")
        f.write("\n")

        f.write("## Top 30 Referenced Utilities\n\n")
        f.write("| Class | References | Status |\n")
        f.write("|-------|------------|--------|\n")
        for cls, count in top_30:
            status = allowlist.get(cls, "defined")
            f.write(f"| `{cls}` | {count} | {status} |\n")
        f.write("\n")

        if unknown_filtered:
            f.write("## Unknown Classes (referenced but not defined/allowlisted)\n\n")
            f.write("These are valid-looking CSS classes not in the allowlist.\n\n")
            f.write("| Class | References |\n")
            f.write("|-------|------------|\n")
            for cls in sorted(unknown_filtered)[:50]:  # Limit to 50
                f.write(f"| `{cls}` | {ref_counts.get(cls, 0)} |\n")
            if len(unknown_filtered) > 50:
                f.write(f"\n_...and {len(unknown_filtered) - 50} more_\n")
        f.write("\n")

        f.write("## Reject Utilities Status\n\n")
        f.write(f"**Total Reject:** {len(reject_classes)}\n")
        f.write(f"**Referenced:** {len(reject_referenced)}\n")
        f.write(f"**Unreferenced (safe to remove):** {len(reject_unreferenced)}\n\n")

        if reject_referenced:
            f.write("### Reject utilities still in use (move to Transitional)\n\n")
            f.write("| Class | Locations |\n")
            f.write("|-------|----------|\n")
            for cls in sorted(reject_referenced):
                locs = references.get(cls, [])[:3]
                loc_str = ", ".join(f"`{fi}:{li}`" for fi, li in locs)
                if len(references.get(cls, [])) > 3:
                    loc_str += f" (+{len(references[cls]) - 3} more)"
                f.write(f"| `{cls}` | {loc_str} |\n")
        f.write("\n")

        # List of all unused allowlisted classes by category
        f.write("## Unused Allowlisted Classes by Status\n\n")

        unused_core = sorted(c for c in allowlisted_but_unused if allowlist.get(c) == 'Core')
        unused_trans = sorted(c for c in allowlisted_but_unused if allowlist.get(c) == 'Transitional')
        unused_reject = sorted(c for c in allowlisted_but_unused if allowlist.get(c) == 'Reject')

        f.write(f"### Core ({len(unused_core)})\n")
        for cls in unused_core:
            f.write(f"- `{cls}`\n")
        f.write("\n")

        f.write(f"### Transitional ({len(unused_trans)})\n")
        for cls in unused_trans:
            f.write(f"- `{cls}`\n")
        f.write("\n")

        if unused_reject:
            f.write(f"### Reject ({len(unused_reject)})\n")
            for cls in unused_reject:
                f.write(f"- `{cls}`\n")
            f.write("\n")

    # ===== ui_util_unused.txt =====
    with open(REPORTS_DIR / "ui_util_unused.txt", 'w') as f:
        f.write("# Allowlisted but never referenced\n")
        f.write("# These can be safely removed from STYLE_ALLOWLIST.md\n")
        f.write(f"# Total: {len(allowlisted_but_unused)}\n\n")
        for cls in sorted(allowlisted_but_unused):
            status = allowlist.get(cls, "?")
            f.write(f"{cls} ({status})\n")

    # ===== ui_util_defined_unused.txt =====
    with open(REPORTS_DIR / "ui_util_defined_unused.txt", 'w') as f:
        f.write("# Defined in CSS but never referenced\n")
        f.write("# Consider removing from CSS files\n")
        f.write(f"# Total: {len(defined_but_unused)}\n\n")
        for cls in sorted(defined_but_unused):
            f.write(f"{cls}\n")

    # ===== ui_util_reject_remove.txt =====
    with open(REPORTS_DIR / "ui_util_reject_remove.txt", 'w') as f:
        f.write("# Reject utilities analysis\n")
        f.write(f"# Total: {len(reject_classes)}\n")
        f.write(f"# Unreferenced (remove): {len(reject_unreferenced)}\n")
        f.write(f"# Referenced (move to Transitional): {len(reject_referenced)}\n\n")

        if reject_unreferenced:
            f.write("## REMOVE (unreferenced):\n")
            for cls in sorted(reject_unreferenced):
                f.write(f"{cls}\n")

        if reject_referenced:
            f.write("\n## MOVE TO TRANSITIONAL (still referenced):\n")
            for cls in sorted(reject_referenced):
                locs = references.get(cls, [])
                f.write(f"{cls}\n")
                for loc_file, loc_line in locs[:5]:
                    f.write(f"  - {loc_file}:{loc_line}\n")
                if len(locs) > 5:
                    f.write(f"  - ... and {len(locs) - 5} more\n")

        if not reject_classes:
            f.write("No utilities marked as Reject in STYLE_ALLOWLIST.md\n")
            f.write("\nNote: The allowlist defines 'Reject' as a status, but no utilities\n")
            f.write("currently have this status. The 'Rejected Patterns' section documents\n")
            f.write("patterns to avoid, not actual utilities to remove.\n")

    return {
        'allowlisted': len(allowlisted_classes),
        'defined': len(defined),
        'referenced': len(referenced_classes),
        'referenced_allowlisted': len(referenced_and_allowlisted),
        'allowlisted_unused': len(allowlisted_but_unused),
        'reject_total': len(reject_classes),
        'reject_unreferenced': len(reject_unreferenced),
        'reject_referenced': len(reject_referenced),
        'unknown': len(unknown_filtered),
        'after_removal': len(allowlisted_classes) - len(allowlisted_but_unused),
    }


def main():
    print("PRD-UI-010: UI Utility Audit")
    print("=" * 50)

    print(f"\nScanning:")
    print(f"  - {len(RS_FILES)} .rs files")
    print(f"  - {len(CSS_FILES)} .css files")
    print(f"  - index.html: {INDEX_HTML.exists()}")
    print(f"  - STYLE_ALLOWLIST.md: {ALLOWLIST_MD.exists()}")

    print("\nParsing allowlist...")
    allowlist = parse_allowlist(ALLOWLIST_MD)
    print(f"  Found {len(allowlist)} allowlisted classes")

    print("\nParsing CSS definitions...")
    defined = parse_css_classes(CSS_FILES)
    print(f"  Found {len(defined)} defined classes")

    print("\nScanning references in code...")
    allowlist_set = set(allowlist.keys())
    references = parse_referenced_classes(RS_FILES, INDEX_HTML, defined, allowlist_set)
    print(f"  Found {len(references)} unique classes referenced")

    print("\nGenerating reports...")
    stats = generate_reports(allowlist, defined, references)

    print("\n" + "=" * 50)
    print("SUMMARY")
    print("=" * 50)
    print(f"Allowlisted:              {stats['allowlisted']}")
    print(f"Defined in CSS:           {stats['defined']}")
    print(f"Referenced in code:       {stats['referenced']}")
    print(f"├─ Referenced & listed:   {stats['referenced_allowlisted']}")
    print(f"└─ Unknown:               {stats['unknown']}")
    print(f"Allowlisted but unused:   {stats['allowlisted_unused']}")
    print()
    print(f"After removing unused:    {stats['after_removal']}")
    print(f"Target:                   ≤150")
    over_target = stats['after_removal'] - 150
    if stats['after_removal'] <= 150:
        print("✅ Target achievable!")
    else:
        print(f"❌ Still {over_target} over target")

    print(f"\nReports written to {REPORTS_DIR}/")
    print("  - ui_util_usage.md")
    print("  - ui_util_unused.txt")
    print("  - ui_util_defined_unused.txt")
    print("  - ui_util_reject_remove.txt")

    return 0


if __name__ == "__main__":
    sys.exit(main())
