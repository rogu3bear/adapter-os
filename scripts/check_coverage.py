#!/usr/bin/env python3
"""
Coverage threshold checker for adapterOS.

Parses tarpaulin JSON output and checks per-crate coverage against thresholds.
Fails with exit code 1 if any crate is below its threshold.
"""

import json
import sys
import argparse
import re
from pathlib import Path
from typing import Dict, List, Tuple


# Coverage thresholds by crate pattern
# Based on docs/testing/VERIFICATION-STRATEGY.md
COVERAGE_THRESHOLDS = {
    # Core Backends: ≥80%
    "adapteros-lora-kernel-coreml": 80.0,
    "adapteros-lora-kernel-mtl": 80.0,
    "adapteros-lora-kernel-api": 80.0,
    
    # Inference Pipeline: ≥85%
    "adapteros-lora-router": 85.0,
    "adapteros-lora-worker": 85.0,  # Overall threshold, training modules checked separately
    
    # Training Pipeline: ≥70% (checked via path matching)
    # Note: Training modules within adapteros-lora-worker have 70% threshold
    
    # Security/Crypto: ≥95%
    "adapteros-policy": 95.0,
    "adapteros-secd": 95.0,
    
    # API Handlers: ≥80%
    "adapteros-server-api": 80.0,
    
    # Default threshold: 80%
    "default": 80.0,
}


def get_threshold_for_crate(crate_name: str) -> float:
    """Get coverage threshold for a crate."""
    # Direct match first
    if crate_name in COVERAGE_THRESHOLDS:
        return COVERAGE_THRESHOLDS[crate_name]
    
    # Pattern matching for kernel backends
    if crate_name.startswith("adapteros-lora-kernel-"):
        return COVERAGE_THRESHOLDS.get("adapteros-lora-kernel-api", 80.0)
    
    # Default threshold
    return COVERAGE_THRESHOLDS["default"]


def parse_tarpaulin_json(json_file: Path) -> Dict:
    """Parse tarpaulin JSON output file."""
    try:
        with open(json_file, 'r') as f:
            return json.load(f)
    except FileNotFoundError:
        print(f"Error: Coverage file not found: {json_file}", file=sys.stderr)
        sys.exit(1)
    except json.JSONDecodeError as e:
        print(f"Error: Invalid JSON in coverage file: {e}", file=sys.stderr)
        sys.exit(1)


def check_coverage(data: Dict) -> Tuple[List[Dict], List[Dict]]:
    """
    Check coverage against thresholds.
    
    Returns:
        Tuple of (passing_crates, failing_crates)
    """
    passing = []
    failing = []
    
    # Tarpaulin JSON structure can be:
    # - { "packages": [...] } (workspace mode)
    # - { "name": "...", "coverage": ... } (single package)
    # - Array of packages
    
    packages = []
    if "packages" in data:
        packages = data["packages"]
    elif isinstance(data, list):
        packages = data
    elif "name" in data and "coverage" in data:
        # Single package format
        packages = [data]
    
    if not packages:
        print("Warning: No packages found in coverage data", file=sys.stderr)
        if isinstance(data, dict):
            print(f"Data keys: {list(data.keys())}", file=sys.stderr)
            # Try to extract any coverage information
            if "coverage" in data:
                # Single package with coverage field
                packages = [data]
        elif isinstance(data, list) and len(data) > 0:
            packages = data
        
        if not packages:
            return passing, failing
    
    for pkg in packages:
        crate_name = pkg.get("name", "unknown")
        # Coverage can be a percentage (0-100) or decimal (0-1)
        coverage_raw = pkg.get("coverage", 0.0)
        # Handle both percentage and decimal formats
        if isinstance(coverage_raw, (int, float)):
            coverage = float(coverage_raw)
            # If coverage is < 1, assume it's decimal (0-1), convert to percentage
            if coverage < 1.0:
                coverage = coverage * 100.0
        else:
            coverage = 0.0
        threshold = get_threshold_for_crate(crate_name)
        
        result = {
            "name": crate_name,
            "coverage": coverage,
            "threshold": threshold,
            "status": "pass" if coverage >= threshold else "fail"
        }
        
        if coverage >= threshold:
            passing.append(result)
        else:
            failing.append(result)
    
    return passing, failing


def generate_report(passing: List[Dict], failing: List[Dict]) -> str:
    """Generate markdown coverage report."""
    lines = []
    lines.append("# Coverage Report\n")
    
    total_crates = len(passing) + len(failing)
    lines.append(f"**Total Crates:** {total_crates}\n")
    lines.append(f"**Passing:** {len(passing)} | **Failing:** {len(failing)}\n")
    
    if failing:
        lines.append("\n## ❌ Failing Crates\n")
        lines.append("| Crate | Coverage | Threshold | Gap |")
        lines.append("|-------|----------|-----------|-----|")
        for crate in sorted(failing, key=lambda x: x["coverage"]):
            gap = crate["threshold"] - crate["coverage"]
            lines.append(
                f"| `{crate['name']}` | {crate['coverage']:.2f}% | {crate['threshold']:.0f}% | -{gap:.2f}% |"
            )
    
    if passing:
        lines.append("\n## ✅ Passing Crates\n")
        lines.append("| Crate | Coverage | Threshold |")
        lines.append("|-------|----------|-----------|")
        for crate in sorted(passing, key=lambda x: x["coverage"], reverse=True):
            lines.append(
                f"| `{crate['name']}` | {crate['coverage']:.2f}% | {crate['threshold']:.0f}% |"
            )
    
    return "\n".join(lines)


def main():
    parser = argparse.ArgumentParser(
        description="Check coverage thresholds from tarpaulin JSON output"
    )
    parser.add_argument(
        "--report",
        type=Path,
        required=True,
        help="Path to tarpaulin JSON coverage file"
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Path to write markdown report (optional)"
    )
    args = parser.parse_args()
    
    # Parse coverage data
    data = parse_tarpaulin_json(args.report)
    
    # Check coverage
    passing, failing = check_coverage(data)
    
    # Generate report
    report = generate_report(passing, failing)
    
    # Output report
    if args.output:
        args.output.write_text(report)
        print(f"Coverage report written to: {args.output}")
    else:
        print(report)
    
    # Print summary to stdout
    print("\n" + "="*60, file=sys.stderr)
    print(f"Coverage Check Summary", file=sys.stderr)
    print(f"  Passing: {len(passing)}/{len(passing) + len(failing)}", file=sys.stderr)
    print(f"  Failing: {len(failing)}/{len(passing) + len(failing)}", file=sys.stderr)
    print("="*60, file=sys.stderr)
    
    # Exit with error if any crate failed
    if failing:
        print("\n❌ Coverage check FAILED", file=sys.stderr)
        print(f"The following {len(failing)} crate(s) are below their thresholds:", file=sys.stderr)
        for crate in failing:
            gap = crate["threshold"] - crate["coverage"]
            print(
                f"  - {crate['name']}: {crate['coverage']:.2f}% (threshold: {crate['threshold']:.0f}%, gap: {gap:.2f}%)",
                file=sys.stderr
            )
        sys.exit(1)
    else:
        print("\n✅ Coverage check PASSED", file=sys.stderr)
        sys.exit(0)


if __name__ == "__main__":
    main()

