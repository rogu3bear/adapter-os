#!/usr/bin/env python3
"""
Benchmark comparison script for AdapterOS.

Compares Criterion benchmark results between baseline and current runs.
Fails if any benchmark regressed more than the threshold (default 10%).
"""

import json
import sys
import argparse
import os
from pathlib import Path
from typing import Dict, List, Tuple, Optional


def parse_criterion_json(json_file: Path) -> Dict:
    """Parse Criterion JSON output file."""
    try:
        with open(json_file, 'r') as f:
            return json.load(f)
    except FileNotFoundError:
        print(f"Error: Benchmark file not found: {json_file}", file=sys.stderr)
        sys.exit(1)
    except json.JSONDecodeError as e:
        print(f"Error: Invalid JSON in benchmark file: {e}", file=sys.stderr)
        sys.exit(1)


def extract_benchmark_times(data: Dict) -> Dict[str, float]:
    """
    Extract benchmark names and mean times from Criterion JSON.
    
    Criterion JSON structure:
    {
      "mean": {
        "point_estimate": <nanoseconds>,
        ...
      },
      ...
    }
    """
    benchmarks = {}
    
    # Handle different Criterion JSON formats
    if "mean" in data:
        # Single benchmark result
        mean_estimate = data.get("mean", {}).get("point_estimate")
        if mean_estimate is not None:
            # Use file name as benchmark name if available
            benchmarks["benchmark"] = float(mean_estimate)
    elif isinstance(data, dict):
        # Multiple benchmarks or nested structure
        for key, value in data.items():
            if isinstance(value, dict) and "mean" in value:
                mean_estimate = value.get("mean", {}).get("point_estimate")
                if mean_estimate is not None:
                    benchmarks[key] = float(mean_estimate)
            elif isinstance(value, (int, float)):
                benchmarks[key] = float(value)
    
    return benchmarks


def compare_benchmarks(
    baseline: Dict[str, float],
    current: Dict[str, float],
    threshold: float = 0.10
) -> Tuple[List[Dict], List[Dict]]:
    """
    Compare benchmarks and detect regressions.
    
    Args:
        baseline: Baseline benchmark results {name: time_ns}
        current: Current benchmark results {name: time_ns}
        threshold: Maximum allowed regression (0.10 = 10%)
    
    Returns:
        Tuple of (passing_benchmarks, failing_benchmarks)
    """
    passing = []
    failing = []
    
    # Find common benchmarks
    common_names = set(baseline.keys()) & set(current.keys())
    
    if not common_names:
        print("Warning: No common benchmarks found between baseline and current", file=sys.stderr)
        return passing, failing
    
    for name in sorted(common_names):
        baseline_time = baseline[name]
        current_time = current[name]
        
        # Calculate percentage change (positive = regression, negative = improvement)
        if baseline_time == 0:
            pct_change = float('inf') if current_time > 0 else 0.0
        else:
            pct_change = (current_time - baseline_time) / baseline_time
        
        result = {
            "name": name,
            "baseline_time_ns": baseline_time,
            "current_time_ns": current_time,
            "pct_change": pct_change * 100,  # Convert to percentage
            "status": "pass" if pct_change <= threshold else "fail"
        }
        
        if pct_change <= threshold:
            passing.append(result)
        else:
            failing.append(result)
    
    return passing, failing


def generate_report(passing: List[Dict], failing: List[Dict], threshold: float) -> str:
    """Generate markdown benchmark comparison report."""
    lines = []
    lines.append("# Benchmark Comparison Report\n")
    
    total_benchmarks = len(passing) + len(failing)
    lines.append(f"**Total Benchmarks:** {total_benchmarks}\n")
    lines.append(f"**Passing:** {len(passing)} | **Failing:** {len(failing)}\n")
    lines.append(f"**Regression Threshold:** {threshold * 100:.0f}%\n")
    
    if failing:
        lines.append("\n## ❌ Regressed Benchmarks\n")
        lines.append("| Benchmark | Baseline (ns) | Current (ns) | Change |")
        lines.append("|-----------|----------------|--------------|--------|")
        for bench in sorted(failing, key=lambda x: x["pct_change"], reverse=True):
            change_str = f"+{bench['pct_change']:.2f}%"
            lines.append(
                f"| `{bench['name']}` | {bench['baseline_time_ns']:.2e} | "
                f"{bench['current_time_ns']:.2e} | {change_str} |"
            )
    
    if passing:
        lines.append("\n## ✅ Passing Benchmarks\n")
        lines.append("| Benchmark | Baseline (ns) | Current (ns) | Change |")
        lines.append("|-----------|----------------|--------------|--------|")
        for bench in sorted(passing, key=lambda x: x["pct_change"], reverse=True):
            change_str = f"{bench['pct_change']:+.2f}%"
            lines.append(
                f"| `{bench['name']}` | {bench['baseline_time_ns']:.2e} | "
                f"{bench['current_time_ns']:.2e} | {change_str} |"
            )
    
    return "\n".join(lines)


def find_criterion_results(directory: Path) -> Dict[str, float]:
    """
    Find all Criterion benchmark results in a directory.
    
    Criterion stores results in: target/criterion/<bench>/<baseline>/estimates.json
    Structure: target/criterion/<benchmark_name>/<baseline_name>/estimates.json
    """
    benchmarks = {}
    
    criterion_dir = directory / "target" / "criterion"
    if not criterion_dir.exists():
        return benchmarks
    
    # Walk through criterion directory structure
    for bench_dir in criterion_dir.iterdir():
        if not bench_dir.is_dir():
            continue
        
        bench_name = bench_dir.name
        
        # Look for estimates.json in baseline or new directories
        # Priority: "new" (current run) > "baseline" (saved baseline)
        for result_type in ["new", "baseline"]:
            estimates_file = bench_dir / result_type / "estimates.json"
            if estimates_file.exists():
                try:
                    data = parse_criterion_json(estimates_file)
                    bench_times = extract_benchmark_times(data)
                    
                    if bench_times:
                        # Criterion JSON structure: { "mean": { "point_estimate": <ns> } }
                        if "benchmark" in bench_times:
                            benchmarks[bench_name] = bench_times["benchmark"]
                        elif bench_times:
                            # Take the mean estimate value
                            benchmarks[bench_name] = list(bench_times.values())[0]
                        break
                except Exception as e:
                    print(f"Warning: Failed to parse {estimates_file}: {e}", file=sys.stderr)
                    continue
    
    return benchmarks


def main():
    parser = argparse.ArgumentParser(
        description="Compare Criterion benchmark results"
    )
    parser.add_argument(
        "--baseline",
        type=Path,
        help="Path to baseline benchmark JSON file or directory"
    )
    parser.add_argument(
        "--current",
        type=Path,
        help="Path to current benchmark JSON file or directory"
    )
    parser.add_argument(
        "--threshold",
        type=float,
        default=0.10,
        help="Maximum allowed regression as decimal (default: 0.10 = 10%%)"
    )
    parser.add_argument(
        "--output",
        type=Path,
        help="Path to write markdown report (optional)"
    )
    args = parser.parse_args()
    
    # Load benchmark data
    if args.baseline.is_file():
        baseline_data = parse_criterion_json(args.baseline)
        baseline = extract_benchmark_times(baseline_data)
    elif args.baseline.is_dir():
        baseline = find_criterion_results(args.baseline)
    else:
        print(f"Error: Baseline path not found: {args.baseline}", file=sys.stderr)
        sys.exit(1)
    
    if args.current.is_file():
        current_data = parse_criterion_json(args.current)
        current = extract_benchmark_times(current_data)
    elif args.current.is_dir():
        current = find_criterion_results(args.current)
    else:
        print(f"Error: Current path not found: {args.current}", file=sys.stderr)
        sys.exit(1)
    
    if not baseline:
        print("Error: No baseline benchmarks found", file=sys.stderr)
        sys.exit(1)
    
    if not current:
        print("Error: No current benchmarks found", file=sys.stderr)
        sys.exit(1)
    
    # Compare benchmarks
    passing, failing = compare_benchmarks(baseline, current, args.threshold)
    
    # Generate report
    report = generate_report(passing, failing, args.threshold)
    
    # Output report
    if args.output:
        args.output.write_text(report)
        print(f"Benchmark comparison report written to: {args.output}")
    else:
        print(report)
    
    # Print summary to stderr
    print("\n" + "="*60, file=sys.stderr)
    print(f"Benchmark Comparison Summary", file=sys.stderr)
    print(f"  Passing: {len(passing)}/{len(passing) + len(failing)}", file=sys.stderr)
    print(f"  Failing: {len(failing)}/{len(passing) + len(failing)}", file=sys.stderr)
    print(f"  Threshold: {args.threshold * 100:.0f}%", file=sys.stderr)
    print("="*60, file=sys.stderr)
    
    # Exit with error if any benchmark regressed
    if failing:
        print("\n❌ Benchmark comparison FAILED", file=sys.stderr)
        print(f"The following {len(failing)} benchmark(s) regressed >{args.threshold * 100:.0f}%:", file=sys.stderr)
        for bench in sorted(failing, key=lambda x: x["pct_change"], reverse=True):
            print(
                f"  - {bench['name']}: {bench['pct_change']:+.2f}% "
                f"({bench['baseline_time_ns']:.2e}ns → {bench['current_time_ns']:.2e}ns)",
                file=sys.stderr
            )
        sys.exit(1)
    else:
        print("\n✅ Benchmark comparison PASSED", file=sys.stderr)
        sys.exit(0)


if __name__ == "__main__":
    main()

