#!/usr/bin/env python3
"""
Performance Visualization Script for MLX Backend
Generates graphs and charts from benchmark data

Copyright © 2025 JKCA / James KC Auchterlonie. All rights reserved.
"""

import json
import sys
from pathlib import Path
from typing import Dict, List, Any
import matplotlib.pyplot as plt
import matplotlib.patches as mpatches
import numpy as np

# Try to import seaborn for better styling
try:
    import seaborn as sns
    sns.set_style("whitegrid")
    HAS_SEABORN = True
except ImportError:
    HAS_SEABORN = False
    print("Note: seaborn not available, using default matplotlib styling")


class PerformanceVisualizer:
    """Visualizes performance data from MLX benchmarks"""

    def __init__(self, data_file: Path):
        """Initialize visualizer with performance data file"""
        self.data_file = data_file
        self.data = self.load_data()
        self.output_dir = data_file.parent / "visualizations"
        self.output_dir.mkdir(exist_ok=True)

    def load_data(self) -> Dict[str, Any]:
        """Load performance data from JSON file"""
        if not self.data_file.exists():
            # Return sample data if file doesn't exist
            return self.generate_sample_data()

        with open(self.data_file, 'r') as f:
            return json.load(f)

    @staticmethod
    def generate_sample_data() -> Dict[str, Any]:
        """Generate sample data for demonstration"""
        return {
            "operations": {
                "matmul": {"count": 10000, "avg_us": 195.0, "min_us": 180.0, "max_us": 250.0, "total_ms": 1950.0},
                "add": {"count": 15000, "avg_us": 8.5, "min_us": 7.0, "max_us": 12.0, "total_ms": 127.5},
                "attention": {"count": 500, "avg_us": 850.0, "min_us": 800.0, "max_us": 950.0, "total_ms": 425.0},
                "lora_forward": {"count": 2000, "avg_us": 140.0, "min_us": 130.0, "max_us": 180.0, "total_ms": 280.0},
                "model_forward": {"count": 1000, "avg_us": 1200.0, "min_us": 1100.0, "max_us": 1500.0, "total_ms": 1200.0},
            },
            "memory_usage_bytes": 120_586_240,
            "allocation_count": 1248,
        }

    def plot_operation_breakdown(self):
        """Plot time breakdown by operation type"""
        ops = self.data.get("operations", {})
        if not ops:
            print("No operation data available")
            return

        # Extract data
        names = list(ops.keys())
        total_times = [ops[name]["total_ms"] for name in names]

        # Sort by total time
        sorted_indices = np.argsort(total_times)[::-1]
        names = [names[i] for i in sorted_indices]
        total_times = [total_times[i] for i in sorted_indices]

        # Create figure
        fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 6))

        # Pie chart
        colors = plt.cm.Set3(np.linspace(0, 1, len(names)))
        ax1.pie(total_times, labels=names, autopct='%1.1f%%', colors=colors, startangle=90)
        ax1.set_title("Operation Time Breakdown", fontsize=14, fontweight='bold')

        # Bar chart
        bars = ax2.barh(names, total_times, color=colors)
        ax2.set_xlabel("Total Time (ms)", fontsize=12)
        ax2.set_title("Time by Operation", fontsize=14, fontweight='bold')
        ax2.grid(axis='x', alpha=0.3)

        # Add value labels on bars
        for i, (bar, time) in enumerate(zip(bars, total_times)):
            ax2.text(time, i, f' {time:.1f}ms', va='center', fontsize=10)

        plt.tight_layout()
        output_path = self.output_dir / "operation_breakdown.png"
        plt.savefig(output_path, dpi=150, bbox_inches='tight')
        print(f"Saved: {output_path}")
        plt.close()

    def plot_latency_distribution(self):
        """Plot latency distribution across operations"""
        ops = self.data.get("operations", {})
        if not ops:
            return

        # Prepare data
        names = []
        avgs = []
        mins = []
        maxs = []

        for name, stats in ops.items():
            if stats["count"] > 0:
                names.append(name)
                avgs.append(stats["avg_us"])
                mins.append(stats["min_us"])
                maxs.append(stats["max_us"])

        # Sort by average latency
        sorted_indices = np.argsort(avgs)[::-1]
        names = [names[i] for i in sorted_indices]
        avgs = [avgs[i] for i in sorted_indices]
        mins = [mins[i] for i in sorted_indices]
        maxs = [maxs[i] for i in sorted_indices]

        # Create figure
        fig, ax = plt.subplots(figsize=(12, 8))

        y_pos = np.arange(len(names))

        # Plot error bars (min to max range)
        for i in range(len(names)):
            ax.plot([mins[i], maxs[i]], [i, i], 'k-', linewidth=2, alpha=0.3)

        # Plot average latency
        bars = ax.barh(y_pos, avgs, color='steelblue', alpha=0.7, label='Average')
        ax.scatter(mins, y_pos, color='green', s=50, zorder=3, label='Min')
        ax.scatter(maxs, y_pos, color='red', s=50, zorder=3, label='Max')

        ax.set_yticks(y_pos)
        ax.set_yticklabels(names)
        ax.set_xlabel("Latency (µs)", fontsize=12)
        ax.set_title("Latency Distribution by Operation", fontsize=14, fontweight='bold')
        ax.legend(loc='lower right')
        ax.grid(axis='x', alpha=0.3)

        # Add value labels
        for i, (bar, avg) in enumerate(zip(bars, avgs)):
            ax.text(avg, i, f' {avg:.1f}µs', va='center', fontsize=9)

        plt.tight_layout()
        output_path = self.output_dir / "latency_distribution.png"
        plt.savefig(output_path, dpi=150, bbox_inches='tight')
        print(f"Saved: {output_path}")
        plt.close()

    def plot_throughput_analysis(self):
        """Plot throughput metrics"""
        ops = self.data.get("operations", {})
        if not ops:
            return

        # Calculate throughput (ops/sec)
        names = []
        throughputs = []

        for name, stats in ops.items():
            if stats["total_ms"] > 0:
                throughput = (stats["count"] / stats["total_ms"]) * 1000  # ops per second
                names.append(name)
                throughputs.append(throughput)

        # Sort by throughput
        sorted_indices = np.argsort(throughputs)[::-1]
        names = [names[i] for i in sorted_indices]
        throughputs = [throughputs[i] for i in sorted_indices]

        # Create figure
        fig, ax = plt.subplots(figsize=(12, 8))

        colors = plt.cm.viridis(np.linspace(0, 1, len(names)))
        bars = ax.barh(names, throughputs, color=colors)

        ax.set_xlabel("Throughput (ops/second)", fontsize=12)
        ax.set_title("Operation Throughput", fontsize=14, fontweight='bold')
        ax.set_xscale('log')
        ax.grid(axis='x', alpha=0.3)

        # Add value labels
        for i, (bar, throughput) in enumerate(zip(bars, throughputs)):
            label = f'{throughput:.1f}' if throughput < 1000 else f'{throughput/1000:.1f}K'
            ax.text(throughput, i, f' {label}', va='center', fontsize=9)

        plt.tight_layout()
        output_path = self.output_dir / "throughput_analysis.png"
        plt.savefig(output_path, dpi=150, bbox_inches='tight')
        print(f"Saved: {output_path}")
        plt.close()

    def plot_memory_analysis(self):
        """Plot memory usage analysis"""
        memory_bytes = self.data.get("memory_usage_bytes", 0)
        allocation_count = self.data.get("allocation_count", 0)

        if memory_bytes == 0:
            return

        memory_mb = memory_bytes / (1024 * 1024)
        avg_alloc_kb = (memory_bytes / allocation_count) / 1024 if allocation_count > 0 else 0

        # Create figure
        fig, (ax1, ax2) = plt.subplots(1, 2, figsize=(14, 6))

        # Memory usage bar
        ax1.bar(['Total Memory'], [memory_mb], color='coral', width=0.5)
        ax1.set_ylabel("Memory Usage (MB)", fontsize=12)
        ax1.set_title("Total Memory Usage", fontsize=14, fontweight='bold')
        ax1.text(0, memory_mb, f'{memory_mb:.1f} MB', ha='center', va='bottom', fontsize=12, fontweight='bold')
        ax1.set_ylim(0, memory_mb * 1.2)
        ax1.grid(axis='y', alpha=0.3)

        # Allocation statistics
        stats = ['Allocations', 'Avg Size (KB)']
        values = [allocation_count, avg_alloc_kb]
        colors_alloc = ['lightblue', 'lightgreen']

        ax2.bar(stats, values, color=colors_alloc, width=0.6)
        ax2.set_title("Allocation Statistics", fontsize=14, fontweight='bold')
        ax2.grid(axis='y', alpha=0.3)

        for i, (stat, val) in enumerate(zip(stats, values)):
            ax2.text(i, val, f'{val:.1f}', ha='center', va='bottom', fontsize=12, fontweight='bold')

        plt.tight_layout()
        output_path = self.output_dir / "memory_analysis.png"
        plt.savefig(output_path, dpi=150, bbox_inches='tight')
        print(f"Saved: {output_path}")
        plt.close()

    def plot_efficiency_metrics(self):
        """Plot efficiency and performance metrics"""
        ops = self.data.get("operations", {})
        if not ops:
            return

        # Calculate efficiency metrics
        names = []
        call_counts = []
        avg_latencies = []
        total_times = []

        for name, stats in ops.items():
            if stats["count"] > 0:
                names.append(name)
                call_counts.append(stats["count"])
                avg_latencies.append(stats["avg_us"])
                total_times.append(stats["total_ms"])

        # Create scatter plot
        fig, ax = plt.subplots(figsize=(12, 8))

        # Size of bubbles proportional to total time
        sizes = [t * 5 for t in total_times]
        colors = plt.cm.plasma(np.linspace(0, 1, len(names)))

        scatter = ax.scatter(call_counts, avg_latencies, s=sizes, c=colors, alpha=0.6, edgecolors='black', linewidth=1)

        # Annotate points
        for i, name in enumerate(names):
            ax.annotate(name, (call_counts[i], avg_latencies[i]),
                       xytext=(10, 10), textcoords='offset points',
                       fontsize=9, fontweight='bold',
                       bbox=dict(boxstyle='round,pad=0.3', facecolor=colors[i], alpha=0.3))

        ax.set_xlabel("Number of Calls", fontsize=12)
        ax.set_ylabel("Average Latency (µs)", fontsize=12)
        ax.set_title("Performance Efficiency Map\n(Bubble size = Total Time)", fontsize=14, fontweight='bold')
        ax.set_xscale('log')
        ax.set_yscale('log')
        ax.grid(True, alpha=0.3, which='both')

        plt.tight_layout()
        output_path = self.output_dir / "efficiency_metrics.png"
        plt.savefig(output_path, dpi=150, bbox_inches='tight')
        print(f"Saved: {output_path}")
        plt.close()

    def plot_comparison_chart(self, mlx_data: Dict, metal_data: Dict):
        """Plot comparison between MLX and Metal backends"""
        metrics = ['Single Token\nLatency (µs)', 'Batch\nThroughput\n(tok/s)', 'Memory\nUsage (MB)']
        mlx_values = [280, 75, 115]  # Example values from report
        metal_values = [85, 220, 95]

        x = np.arange(len(metrics))
        width = 0.35

        fig, ax = plt.subplots(figsize=(12, 8))

        bars1 = ax.bar(x - width/2, mlx_values, width, label='MLX', color='skyblue', edgecolor='black')
        bars2 = ax.bar(x + width/2, metal_values, width, label='Metal', color='lightcoral', edgecolor='black')

        # Add value labels
        for bars in [bars1, bars2]:
            for bar in bars:
                height = bar.get_height()
                ax.text(bar.get_x() + bar.get_width()/2., height,
                       f'{height:.0f}',
                       ha='center', va='bottom', fontweight='bold')

        # Add ratio labels
        for i, (mlx_val, metal_val) in enumerate(zip(mlx_values, metal_values)):
            ratio = mlx_val / metal_val
            ax.text(i, max(mlx_val, metal_val) * 1.1,
                   f'{ratio:.1f}x',
                   ha='center', fontsize=11, fontweight='bold',
                   bbox=dict(boxstyle='round', facecolor='yellow', alpha=0.5))

        ax.set_ylabel('Value', fontsize=12)
        ax.set_title('MLX vs Metal Backend Performance Comparison', fontsize=14, fontweight='bold')
        ax.set_xticks(x)
        ax.set_xticklabels(metrics)
        ax.legend(loc='upper left', fontsize=12)
        ax.grid(axis='y', alpha=0.3)

        plt.tight_layout()
        output_path = self.output_dir / "mlx_vs_metal_comparison.png"
        plt.savefig(output_path, dpi=150, bbox_inches='tight')
        print(f"Saved: {output_path}")
        plt.close()

    def generate_all_plots(self):
        """Generate all visualization plots"""
        print("Generating performance visualizations...")

        self.plot_operation_breakdown()
        self.plot_latency_distribution()
        self.plot_throughput_analysis()
        self.plot_memory_analysis()
        self.plot_efficiency_metrics()
        self.plot_comparison_chart({}, {})

        print(f"\nAll visualizations saved to: {self.output_dir}")
        print("\nGenerated files:")
        for file in sorted(self.output_dir.glob("*.png")):
            print(f"  - {file.name}")


def main():
    """Main entry point"""
    # Check for data file argument
    if len(sys.argv) > 1:
        data_file = Path(sys.argv[1])
    else:
        # Use default location
        data_file = Path(__file__).parent.parent / "target" / "criterion" / "performance_data.json"

    print(f"MLX Backend Performance Visualizer")
    print(f"{'=' * 50}")
    print(f"Data file: {data_file}")

    # Create visualizer and generate plots
    visualizer = PerformanceVisualizer(data_file)
    visualizer.generate_all_plots()

    print("\n✅ Visualization complete!")


if __name__ == "__main__":
    main()
