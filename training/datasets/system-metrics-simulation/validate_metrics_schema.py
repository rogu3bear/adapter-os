#!/usr/bin/env python3
"""
Schema validation script for system-metrics simulation dataset.

Purpose: Ensures JSONL entries conform to expected schema before DB ingestion
Checks: Required fields, type validation, value ranges, component/metric naming
"""

import json
import sys
from datetime import datetime
from typing import Dict, List, Set

# Expected component names (from CLAUDE.md)
VALID_COMPONENTS = {
    "router",
    "lifecycle",
    "memory",
    "deterministic_exec",
    "training",
    "policy",
    "telemetry",
}

# Expected metric configurations per component
EXPECTED_METRICS = {
    "router": {"decision_rate", "k_sparse_selections", "gate_computation_ms", "tie_breaks"},
    "lifecycle": {"promotions", "demotions", "evictions", "activation_rate"},
    "memory": {"vram_usage_mb", "headroom_percent", "adapter_count"},
    "deterministic_exec": {"tick_rate", "task_queue_depth", "barrier_wait_ms"},
    "training": {"job_count", "avg_loss", "tokens_per_sec"},
    "policy": {"egress_blocks", "determinism_checks", "policy_violations"},
    "telemetry": {"events_emitted", "bundle_size_kb"},
}

# Type expectations (int or float)
INTEGER_METRICS = {
    "k_sparse_selections", "tie_breaks", "promotions", "demotions", "evictions",
    "adapter_count", "task_queue_depth", "job_count", "egress_blocks",
    "determinism_checks", "policy_violations", "events_emitted",
}

# Required fields
REQUIRED_FIELDS = {"timestamp", "component", "metric", "value", "unit"}

class ValidationError(Exception):
    """Custom exception for validation failures."""
    pass

def validate_entry(entry: Dict, line_num: int) -> None:
    """Validate a single metric entry."""
    # Check required fields
    missing_fields = REQUIRED_FIELDS - set(entry.keys())
    if missing_fields:
        raise ValidationError(
            f"Line {line_num}: Missing required fields: {missing_fields}"
        )

    # Validate timestamp
    timestamp = entry["timestamp"]
    if not isinstance(timestamp, int):
        raise ValidationError(
            f"Line {line_num}: timestamp must be integer, got {type(timestamp).__name__}"
        )

    if timestamp < 0:
        raise ValidationError(
            f"Line {line_num}: timestamp must be non-negative, got {timestamp}"
        )

    # Validate timestamp is reasonable (not too far in past/future)
    try:
        dt = datetime.fromtimestamp(timestamp)
        current_year = datetime.now().year
        if not (2020 <= dt.year <= current_year + 10):
            raise ValidationError(
                f"Line {line_num}: timestamp {timestamp} ({dt.isoformat()}) is outside reasonable range"
            )
    except (OSError, ValueError) as e:
        raise ValidationError(
            f"Line {line_num}: Invalid timestamp {timestamp}: {e}"
        )

    # Validate component
    component = entry["component"]
    if not isinstance(component, str):
        raise ValidationError(
            f"Line {line_num}: component must be string, got {type(component).__name__}"
        )

    if component not in VALID_COMPONENTS:
        raise ValidationError(
            f"Line {line_num}: Unknown component '{component}'. Valid: {VALID_COMPONENTS}"
        )

    # Validate metric
    metric = entry["metric"]
    if not isinstance(metric, str):
        raise ValidationError(
            f"Line {line_num}: metric must be string, got {type(metric).__name__}"
        )

    expected_metrics = EXPECTED_METRICS.get(component, set())
    if metric not in expected_metrics:
        raise ValidationError(
            f"Line {line_num}: Unknown metric '{metric}' for component '{component}'. "
            f"Expected one of: {expected_metrics}"
        )

    # Validate value
    value = entry["value"]
    if not isinstance(value, (int, float)):
        raise ValidationError(
            f"Line {line_num}: value must be number, got {type(value).__name__}"
        )

    # Check type consistency (int vs float)
    if metric in INTEGER_METRICS and not isinstance(value, int):
        raise ValidationError(
            f"Line {line_num}: metric '{metric}' expects integer, got {type(value).__name__}"
        )

    # Validate unit
    unit = entry["unit"]
    if not isinstance(unit, str):
        raise ValidationError(
            f"Line {line_num}: unit must be string, got {type(unit).__name__}"
        )

    if not unit.strip():
        raise ValidationError(
            f"Line {line_num}: unit cannot be empty"
        )

def validate_dataset(file_path: str) -> Dict:
    """Validate entire JSONL dataset."""
    stats = {
        "total_entries": 0,
        "components": {},
        "metrics": {},
        "value_ranges": {},
        "errors": [],
    }

    with open(file_path, 'r') as f:
        for line_num, line in enumerate(f, start=1):
            line = line.strip()
            if not line:
                continue

            try:
                # Parse JSON
                try:
                    entry = json.loads(line)
                except json.JSONDecodeError as e:
                    raise ValidationError(f"Line {line_num}: Invalid JSON: {e}")

                # Validate entry
                validate_entry(entry, line_num)

                # Collect statistics
                stats["total_entries"] += 1

                component = entry["component"]
                metric = entry["metric"]
                value = entry["value"]

                # Component stats
                if component not in stats["components"]:
                    stats["components"][component] = {"count": 0, "metrics": set()}
                stats["components"][component]["count"] += 1
                stats["components"][component]["metrics"].add(metric)

                # Metric stats
                if metric not in stats["metrics"]:
                    stats["metrics"][metric] = {"count": 0}
                stats["metrics"][metric]["count"] += 1

                # Value ranges
                key = f"{component}.{metric}"
                if key not in stats["value_ranges"]:
                    stats["value_ranges"][key] = {"min": value, "max": value}
                else:
                    stats["value_ranges"][key]["min"] = min(
                        stats["value_ranges"][key]["min"], value
                    )
                    stats["value_ranges"][key]["max"] = max(
                        stats["value_ranges"][key]["max"], value
                    )

            except ValidationError as e:
                stats["errors"].append(str(e))

    # Convert sets to lists for JSON serialization
    for component_stats in stats["components"].values():
        component_stats["metrics"] = sorted(list(component_stats["metrics"]))

    return stats

def print_validation_report(stats: Dict) -> bool:
    """Print validation report and return success status."""
    print(f"\n{'=' * 60}")
    print("SYSTEM-METRICS DATASET VALIDATION REPORT")
    print(f"{'=' * 60}\n")

    # Summary
    print(f"Total Entries: {stats['total_entries']}")
    print(f"Total Errors: {len(stats['errors'])}\n")

    # Component coverage
    print("Component Coverage:")
    for component, data in sorted(stats["components"].items()):
        metrics_str = ", ".join(data["metrics"])
        print(f"  • {component}: {data['count']} entries")
        print(f"    Metrics: {metrics_str}")

    # Value ranges
    print("\nValue Ranges:")
    for key, ranges in sorted(stats["value_ranges"].items()):
        print(f"  • {key}: [{ranges['min']}, {ranges['max']}]")

    # Errors
    if stats["errors"]:
        print("\n" + "=" * 60)
        print("VALIDATION ERRORS:")
        print("=" * 60)
        for error in stats["errors"]:
            print(f"  ✗ {error}")
        print()
        return False
    else:
        print("\n" + "=" * 60)
        print("✓ VALIDATION PASSED")
        print("=" * 60)
        print(f"All {stats['total_entries']} entries conform to expected schema.")
        print()
        return True

def main():
    """Main entry point."""
    if len(sys.argv) != 2:
        print("Usage: python3 validate_metrics_schema.py <metrics_dataset.jsonl>")
        sys.exit(1)

    file_path = sys.argv[1]

    try:
        stats = validate_dataset(file_path)
        success = print_validation_report(stats)

        # Exit with appropriate code
        sys.exit(0 if success else 1)

    except FileNotFoundError:
        print(f"Error: File not found: {file_path}")
        sys.exit(1)
    except Exception as e:
        print(f"Unexpected error: {e}")
        sys.exit(1)

if __name__ == "__main__":
    main()
