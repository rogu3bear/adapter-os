#!/usr/bin/env python3
"""
Generate synthetic system metrics dataset for AdapterOS testing.

Purpose: Validates metrics ingestion + DB schema, tests /v1/metrics/summary endpoint
Format: JSONL with timestamp, component, metric, value fields
Target: 300+ entries covering all metric types
"""

import json
import random
import time
from datetime import datetime, timedelta

# Metric configurations with realistic ranges and patterns
METRIC_CONFIGS = {
    "router": [
        {"name": "decision_rate", "min": 80.0, "max": 200.0, "unit": "decisions/sec", "pattern": "spiky"},
        {"name": "k_sparse_selections", "min": 1, "max": 8, "unit": "adapters", "pattern": "discrete"},
        {"name": "gate_computation_ms", "min": 0.5, "max": 5.0, "unit": "milliseconds", "pattern": "normal"},
        {"name": "tie_breaks", "min": 0, "max": 15, "unit": "count", "pattern": "poisson"},
    ],
    "lifecycle": [
        {"name": "promotions", "min": 0, "max": 10, "unit": "count", "pattern": "rare"},
        {"name": "demotions", "min": 0, "max": 8, "unit": "count", "pattern": "rare"},
        {"name": "evictions", "min": 0, "max": 5, "unit": "count", "pattern": "rare"},
        {"name": "activation_rate", "min": 0.0, "max": 100.0, "unit": "percent", "pattern": "normal"},
    ],
    "memory": [
        {"name": "vram_usage_mb", "min": 2048.0, "max": 8192.0, "unit": "megabytes", "pattern": "trending"},
        {"name": "headroom_percent", "min": 5.0, "max": 50.0, "unit": "percent", "pattern": "inverse_trending"},
        {"name": "adapter_count", "min": 5, "max": 25, "unit": "count", "pattern": "stepped"},
    ],
    "deterministic_exec": [
        {"name": "tick_rate", "min": 100.0, "max": 500.0, "unit": "ticks/sec", "pattern": "normal"},
        {"name": "task_queue_depth", "min": 0, "max": 50, "unit": "count", "pattern": "spiky"},
        {"name": "barrier_wait_ms", "min": 0.1, "max": 100.0, "unit": "milliseconds", "pattern": "exponential"},
    ],
    "training": [
        {"name": "job_count", "min": 0, "max": 8, "unit": "count", "pattern": "discrete"},
        {"name": "avg_loss", "min": 0.1, "max": 2.5, "unit": "loss", "pattern": "decreasing"},
        {"name": "tokens_per_sec", "min": 1000.0, "max": 5000.0, "unit": "tokens/sec", "pattern": "normal"},
    ],
    "policy": [
        {"name": "egress_blocks", "min": 0, "max": 5, "unit": "count", "pattern": "rare"},
        {"name": "determinism_checks", "min": 50, "max": 200, "unit": "count", "pattern": "normal"},
        {"name": "policy_violations", "min": 0, "max": 2, "unit": "count", "pattern": "very_rare"},
    ],
    "telemetry": [
        {"name": "events_emitted", "min": 100, "max": 1000, "unit": "count", "pattern": "normal"},
        {"name": "bundle_size_kb", "min": 10.0, "max": 500.0, "unit": "kilobytes", "pattern": "normal"},
    ],
}

def generate_value(config, index, total):
    """Generate value based on metric pattern."""
    pattern = config["pattern"]
    min_val = config["min"]
    max_val = config["max"]

    if pattern == "normal":
        # Normal distribution around midpoint
        mean = (min_val + max_val) / 2
        stddev = (max_val - min_val) / 6
        return max(min_val, min(max_val, random.gauss(mean, stddev)))

    elif pattern == "spiky":
        # Occasional spikes above normal
        base = random.uniform(min_val, (min_val + max_val) / 2)
        if random.random() < 0.15:  # 15% spike probability
            return random.uniform(max_val * 0.8, max_val)
        return base

    elif pattern == "trending":
        # Upward trend over time
        progress = index / total
        trend = min_val + (max_val - min_val) * progress
        noise = random.uniform(-0.1, 0.1) * (max_val - min_val)
        return max(min_val, min(max_val, trend + noise))

    elif pattern == "inverse_trending":
        # Downward trend over time
        progress = index / total
        trend = max_val - (max_val - min_val) * progress
        noise = random.uniform(-0.1, 0.1) * (max_val - min_val)
        return max(min_val, min(max_val, trend + noise))

    elif pattern == "discrete":
        # Integer values uniformly distributed
        return random.randint(int(min_val), int(max_val))

    elif pattern == "stepped":
        # Changes in steps rather than continuously
        step_size = (max_val - min_val) / 5
        step = int(index / (total / 5))
        base = min_val + step * step_size
        return int(base + random.uniform(-step_size * 0.2, step_size * 0.2))

    elif pattern == "poisson":
        # Poisson-like distribution (mostly low, occasional high)
        lambda_param = 2.0
        return min(int(max_val), int(random.expovariate(1/lambda_param)))

    elif pattern == "exponential":
        # Exponential distribution (most values low, rare high)
        return min(max_val, random.expovariate(2.0 / (max_val - min_val)) + min_val)

    elif pattern == "rare":
        # Mostly zero, occasional events
        if random.random() < 0.1:  # 10% event probability
            return random.randint(1, int(max_val))
        return 0

    elif pattern == "very_rare":
        # Very rare events (5% probability)
        if random.random() < 0.05:
            return random.randint(1, int(max_val))
        return 0

    elif pattern == "decreasing":
        # Decreasing trend (for training loss)
        progress = index / total
        base = max_val - (max_val - min_val) * progress
        noise = random.uniform(-0.05, 0.05) * (max_val - min_val)
        return max(min_val, base + noise)

    else:
        # Fallback: uniform distribution
        return random.uniform(min_val, max_val)

def format_value(value, unit):
    """Format value based on unit type."""
    if "count" in unit or "adapters" in unit:
        return int(value)
    elif "percent" in unit or "milliseconds" in unit or "sec" in unit:
        return round(value, 2)
    elif "megabytes" in unit or "kilobytes" in unit:
        return round(value, 1)
    else:
        return round(value, 3)

def generate_dataset(num_entries=300, time_span_hours=24):
    """Generate synthetic metrics dataset."""
    entries = []

    # Calculate time distribution
    start_time = int(time.time()) - (time_span_hours * 3600)
    time_interval = (time_span_hours * 3600) / num_entries

    # Distribute entries across components
    total_metrics = sum(len(configs) for configs in METRIC_CONFIGS.values())
    entries_per_metric = num_entries // total_metrics

    entry_index = 0

    for component, metric_configs in METRIC_CONFIGS.items():
        for metric_config in metric_configs:
            # Generate entries for this metric
            for i in range(entries_per_metric):
                timestamp = int(start_time + (entry_index * time_interval))
                value = generate_value(metric_config, i, entries_per_metric)
                formatted_value = format_value(value, metric_config["unit"])

                entry = {
                    "timestamp": timestamp,
                    "component": component,
                    "metric": metric_config["name"],
                    "value": formatted_value,
                    "unit": metric_config["unit"]
                }

                entries.append(entry)
                entry_index += 1

    # Add some extra random entries to reach target count
    while len(entries) < num_entries:
        component = random.choice(list(METRIC_CONFIGS.keys()))
        metric_config = random.choice(METRIC_CONFIGS[component])
        timestamp = int(start_time + random.uniform(0, time_span_hours * 3600))
        value = generate_value(metric_config, random.randint(0, 100), 100)
        formatted_value = format_value(value, metric_config["unit"])

        entry = {
            "timestamp": timestamp,
            "component": component,
            "metric": metric_config["name"],
            "value": formatted_value,
            "unit": metric_config["unit"]
        }
        entries.append(entry)

    # Sort by timestamp
    entries.sort(key=lambda x: x["timestamp"])

    return entries

def write_jsonl(entries, output_path):
    """Write entries to JSONL file."""
    with open(output_path, 'w') as f:
        for entry in entries:
            f.write(json.dumps(entry) + '\n')

def generate_summary_stats(entries):
    """Generate summary statistics for the dataset."""
    stats = {
        "total_entries": len(entries),
        "components": {},
        "time_range": {
            "start": entries[0]["timestamp"],
            "end": entries[-1]["timestamp"],
            "span_hours": (entries[-1]["timestamp"] - entries[0]["timestamp"]) / 3600
        }
    }

    for entry in entries:
        component = entry["component"]
        if component not in stats["components"]:
            stats["components"][component] = {
                "count": 0,
                "metrics": {}
            }

        stats["components"][component]["count"] += 1

        metric = entry["metric"]
        if metric not in stats["components"][component]["metrics"]:
            stats["components"][component]["metrics"][metric] = {
                "count": 0,
                "min": float('inf'),
                "max": float('-inf'),
                "sum": 0
            }

        metric_stats = stats["components"][component]["metrics"][metric]
        metric_stats["count"] += 1
        metric_stats["min"] = min(metric_stats["min"], entry["value"])
        metric_stats["max"] = max(metric_stats["max"], entry["value"])
        metric_stats["sum"] += entry["value"]

    # Calculate averages
    for component in stats["components"].values():
        for metric_stats in component["metrics"].values():
            metric_stats["avg"] = metric_stats["sum"] / metric_stats["count"]
            del metric_stats["sum"]

    return stats

if __name__ == "__main__":
    print("Generating system metrics dataset...")

    # Generate 300+ entries
    entries = generate_dataset(num_entries=320, time_span_hours=24)

    # Write JSONL file
    output_path = "metrics_dataset.jsonl"
    write_jsonl(entries, output_path)
    print(f"✓ Generated {len(entries)} metric entries -> {output_path}")

    # Generate and save summary stats
    stats = generate_summary_stats(entries)
    stats_path = "metrics_stats.json"
    with open(stats_path, 'w') as f:
        json.dump(stats, f, indent=2)

    print(f"✓ Summary statistics -> {stats_path}")
    print(f"\nDataset coverage:")
    for component, data in stats["components"].items():
        print(f"  • {component}: {data['count']} entries, {len(data['metrics'])} metrics")

    print(f"\nTime range: {stats['time_range']['span_hours']:.1f} hours")
    print(f"Start: {datetime.fromtimestamp(stats['time_range']['start']).isoformat()}")
    print(f"End: {datetime.fromtimestamp(stats['time_range']['end']).isoformat()}")
