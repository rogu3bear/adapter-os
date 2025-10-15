//! Benchmark runner and reporting utilities

use crate::reporting::{BenchmarkResult, BenchmarkReport, SystemInfo, BenchmarkSummary};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Benchmark runner configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerConfig {
    pub output_dir: String,
    pub baseline_file: Option<String>,
    pub comparison_threshold: f64,
    pub fail_on_regression: bool,
    pub generate_html_report: bool,
    pub run_kernel_benchmarks: bool,
    pub run_memory_benchmarks: bool,
    pub run_throughput_benchmarks: bool,
    pub run_system_benchmarks: bool,
    pub run_isolation_benchmarks: bool,
    pub run_evidence_benchmarks: bool,
}

impl Default for RunnerConfig {
    fn default() -> Self {
        Self {
            output_dir: "benchmark_results".to_string(),
            baseline_file: None,
            comparison_threshold: 0.05, // 5% regression threshold
            fail_on_regression: true,
            generate_html_report: true,
            run_kernel_benchmarks: true,
            run_memory_benchmarks: true,
            run_throughput_benchmarks: true,
            run_system_benchmarks: true,
            run_isolation_benchmarks: true,
            run_evidence_benchmarks: true,
        }
    }
}

/// Benchmark runner
pub struct BenchmarkRunner {
    config: RunnerConfig,
    results: Vec<BenchmarkResult>,
}

impl BenchmarkRunner {
    pub fn new(config: RunnerConfig) -> Self {
        Self {
            config,
            results: Vec::new(),
        }
    }

    /// Run all configured benchmarks
    pub fn run_all_benchmarks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Starting AdapterOS performance benchmarks...");

        // Create output directory
        fs::create_dir_all(&self.config.output_dir)?;

        // Run kernel benchmarks
        if self.config.run_kernel_benchmarks {
            self.run_kernel_benchmarks()?;
        }

        // Run memory benchmarks
        if self.config.run_memory_benchmarks {
            self.run_memory_benchmarks()?;
        }

        // Run throughput benchmarks
        if self.config.run_throughput_benchmarks {
            self.run_throughput_benchmarks()?;
        }

        // Run system benchmarks
        if self.config.run_system_benchmarks {
            self.run_system_benchmarks()?;
        }

        // Run isolation benchmarks
        if self.config.run_isolation_benchmarks {
            self.run_isolation_benchmarks()?;
        }

        // Run evidence benchmarks
        if self.config.run_evidence_benchmarks {
            self.run_evidence_benchmarks()?;
        }

        // Generate report
        self.generate_report()?;

        Ok(())
    }

    /// Run kernel performance benchmarks
    fn run_kernel_benchmarks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Running kernel performance benchmarks...");

        let output = Command::new("cargo")
            .args(&["bench", "--bench", "kernel_performance"])
            .output()?;

        if !output.status.success() {
            eprintln!("Kernel benchmarks failed: {}", String::from_utf8_lossy(&output.stderr));
            return Err("Kernel benchmarks failed".into());
        }

        // Parse results from criterion output
        self.parse_criterion_results("kernel_performance", &output.stdout)?;

        Ok(())
    }

    /// Run memory benchmarks
    fn run_memory_benchmarks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Running memory benchmarks...");

        let output = Command::new("cargo")
            .args(&["bench", "--bench", "memory_benchmarks"])
            .output()?;

        if !output.status.success() {
            eprintln!("Memory benchmarks failed: {}", String::from_utf8_lossy(&output.stderr));
            return Err("Memory benchmarks failed".into());
        }

        self.parse_criterion_results("memory_benchmarks", &output.stdout)?;

        Ok(())
    }

    /// Run throughput benchmarks
    fn run_throughput_benchmarks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Running throughput benchmarks...");

        let output = Command::new("cargo")
            .args(&["bench", "--bench", "throughput_benchmarks"])
            .output()?;

        if !output.status.success() {
            eprintln!("Throughput benchmarks failed: {}", String::from_utf8_lossy(&output.stderr));
            return Err("Throughput benchmarks failed".into());
        }

        self.parse_criterion_results("throughput_benchmarks", &output.stdout)?;

        Ok(())
    }

    /// Run system benchmarks
    fn run_system_benchmarks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Running system benchmarks...");

        let output = Command::new("cargo")
            .args(&["bench", "--bench", "system_metrics"])
            .output()?;

        if !output.status.success() {
            eprintln!("System benchmarks failed: {}", String::from_utf8_lossy(&output.stderr));
            return Err("System benchmarks failed".into());
        }

        self.parse_criterion_results("system_metrics", &output.stdout)?;

        Ok(())
    }

    /// Run isolation benchmarks
    fn run_isolation_benchmarks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Running isolation benchmarks...");

        let output = Command::new("cargo")
            .args(&["bench", "--bench", "isolation_benchmarks"])
            .output()?;

        if !output.status.success() {
            eprintln!("Isolation benchmarks failed: {}", String::from_utf8_lossy(&output.stderr));
            return Err("Isolation benchmarks failed".into());
        }

        self.parse_criterion_results("isolation_benchmarks", &output.stdout)?;

        Ok(())
    }

    /// Run evidence benchmarks
    fn run_evidence_benchmarks(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        println!("Running evidence benchmarks...");

        let output = Command::new("cargo")
            .args(&["bench", "--bench", "evidence_benchmarks"])
            .output()?;

        if !output.status.success() {
            eprintln!("Evidence benchmarks failed: {}", String::from_utf8_lossy(&output.stderr));
            return Err("Evidence benchmarks failed".into());
        }

        self.parse_criterion_results("evidence_benchmarks", &output.stdout)?;

        Ok(())
    }

    /// Parse Criterion.rs benchmark results
    fn parse_criterion_results(&mut self, category: &str, output: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
        let output_str = String::from_utf8_lossy(output);

        // Simple parsing of criterion output - in practice, you'd want more robust parsing
        // This is a simplified implementation
        for line in output_str.lines() {
            if line.contains("time:") && line.contains("ns/iter") {
                // Extract benchmark name and timing
                if let Some(name_start) = line.find('"') {
                    if let Some(name_end) = line[name_start + 1..].find('"') {
                        let name = &line[name_start + 1..name_start + 1 + name_end];

                        // Extract timing (simplified)
                        if let Some(time_start) = line.find("time: [") {
                            if let Some(time_end) = line[time_start..].find(']') {
                                let time_str = &line[time_start + 7..time_start + time_end];
                                if let Some(mean_str) = time_str.split(',').next() {
                                    if let Ok(mean_ns) = mean_str.trim().parse::<f64>() {
                                        let result = BenchmarkResult {
                                            name: name.to_string(),
                                            category: category.to_string(),
                                            mean_time_ns: mean_ns,
                                            std_dev_ns: 0.0, // Would need more parsing for this
                                            throughput: None,
                                            memory_usage_mb: None,
                                            metadata: HashMap::new(),
                                        };
                                        self.results.push(result);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Generate comprehensive benchmark report
    fn generate_report(&self) -> Result<(), Box<dyn std::error::Error>> {
        let timestamp = Utc::now();

        // Collect system information
        let system_info = self.collect_system_info();

        // Analyze results for regressions/improvements
        let (regressions, improvements) = if let Some(baseline_path) = &self.config.baseline_file {
            self.compare_with_baseline(baseline_path)?
        } else {
            (Vec::new(), Vec::new())
        };

        // Calculate summary statistics
        let total_benchmarks = self.results.len();
        let total_time_seconds = self.results.iter()
            .map(|r| r.mean_time_ns / 1_000_000_000.0)
            .sum::<f64>();

        let summary = BenchmarkSummary {
            total_benchmarks,
            total_time_seconds,
            regressions,
            improvements,
        };

        let report = BenchmarkReport {
            timestamp,
            system_info,
            results: self.results.clone(),
            summary,
        };

        // Save JSON report
        let json_path = Path::new(&self.config.output_dir).join("benchmark_report.json");
        let json_content = serde_json::to_string_pretty(&report)?;
        fs::write(json_path, json_content)?;

        // Generate HTML report if requested
        if self.config.generate_html_report {
            self.generate_html_report(&report)?;
        }

        // Print summary to console
        self.print_summary(&report);

        // Check for regressions and fail if configured
        if self.config.fail_on_regression && !report.summary.regressions.is_empty() {
            eprintln!("Performance regressions detected:");
            for regression in &report.summary.regressions {
                eprintln!("  - {}", regression);
            }
            return Err("Performance regressions detected".into());
        }

        Ok(())
    }

    /// Collect system information
    fn collect_system_info(&self) -> SystemInfo {
        let system = sysinfo::System::new_all();

        SystemInfo {
            os: format!("{} {}", system.name().unwrap_or_default(), system.os_version().unwrap_or_default()),
            cpu: system.global_cpu_info().brand().to_string(),
            memory_gb: system.total_memory() as f64 / 1024.0 / 1024.0 / 1024.0,
            gpu: Some("Metal GPU".to_string()), // Would need more sophisticated detection
        }
    }

    /// Compare results with baseline
    fn compare_with_baseline(&self, baseline_path: &str) -> Result<(Vec<String>, Vec<String>), Box<dyn std::error::Error>> {
        let baseline_content = fs::read_to_string(baseline_path)?;
        let baseline_report: BenchmarkReport = serde_json::from_str(&baseline_content)?;

        let mut regressions = Vec::new();
        let mut improvements = Vec::new();

        // Create lookup map for baseline results
        let baseline_map: HashMap<_, _> = baseline_report.results.iter()
            .map(|r| ((r.category.clone(), r.name.clone()), r.mean_time_ns))
            .collect();

        for result in &self.results {
            let key = (result.category.clone(), result.name.clone());

            if let Some(baseline_time) = baseline_map.get(&key) {
                let change_ratio = result.mean_time_ns / baseline_time;

                if change_ratio > (1.0 + self.config.comparison_threshold) {
                    regressions.push(format!(
                        "{}: {:.2}% slower ({:.2}ns -> {:.2}ns)",
                        result.name,
                        (change_ratio - 1.0) * 100.0,
                        baseline_time,
                        result.mean_time_ns
                    ));
                } else if change_ratio < (1.0 - self.config.comparison_threshold) {
                    improvements.push(format!(
                        "{}: {:.2}% faster ({:.2}ns -> {:.2}ns)",
                        result.name,
                        (1.0 - change_ratio) * 100.0,
                        baseline_time,
                        result.mean_time_ns
                    ));
                }
            }
        }

        Ok((regressions, improvements))
    }

    /// Generate HTML report
    fn generate_html_report(&self, report: &BenchmarkReport) -> Result<(), Box<dyn std::error::Error>> {
        let html_path = Path::new(&self.config.output_dir).join("benchmark_report.html");

        let html_content = format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>AdapterOS Benchmark Report</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 40px; }}
        .header {{ background: #f0f0f0; padding: 20px; border-radius: 5px; }}
        .summary {{ background: #e8f4f8; padding: 15px; margin: 20px 0; border-radius: 5px; }}
        .results {{ margin: 20px 0; }}
        table {{ border-collapse: collapse; width: 100%; }}
        th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
        th {{ background-color: #f2f2f2; }}
        .regression {{ color: red; }}
        .improvement {{ color: green; }}
    </style>
</head>
<body>
    <div class="header">
        <h1>AdapterOS Performance Benchmark Report</h1>
        <p>Generated: {}</p>
        <p>System: {} | CPU: {} | Memory: {:.1} GB | GPU: {}</p>
    </div>

    <div class="summary">
        <h2>Summary</h2>
        <p>Total Benchmarks: {}</p>
        <p>Total Time: {:.2} seconds</p>
        <p>Regressions: {} | Improvements: {}</p>
    </div>

    <div class="results">
        <h2>Detailed Results</h2>
        <table>
            <tr>
                <th>Category</th>
                <th>Benchmark</th>
                <th>Mean Time (ns)</th>
                <th>Std Dev (ns)</th>
                <th>Throughput</th>
                <th>Memory (MB)</th>
            </tr>
            {}
        </table>
    </div>
</body>
</html>"#,
            report.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            report.system_info.os,
            report.system_info.cpu,
            report.system_info.memory_gb,
            report.system_info.gpu.as_deref().unwrap_or("Unknown"),
            report.summary.total_benchmarks,
            report.summary.total_time_seconds,
            report.summary.regressions.len(),
            report.summary.improvements.len(),
            self.generate_html_table_rows(report)
        );

        fs::write(html_path, html_content)?;
        Ok(())
    }

    /// Generate HTML table rows for results
    fn generate_html_table_rows(&self, report: &BenchmarkReport) -> String {
        report.results.iter().map(|result| {
            format!(
                "<tr>
                    <td>{}</td>
                    <td>{}</td>
                    <td>{:.2}</td>
                    <td>{:.2}</td>
                    <td>{}</td>
                    <td>{}</td>
                </tr>",
                result.category,
                result.name,
                result.mean_time_ns,
                result.std_dev_ns,
                result.throughput.map_or("N/A".to_string(), |t| format!("{:.2}", t)),
                result.memory_usage_mb.map_or("N/A".to_string(), |m| format!("{:.2}", m))
            )
        }).collect::<Vec<_>>().join("\n")
    }

    /// Print summary to console
    fn print_summary(&self, report: &BenchmarkReport) {
        println!("\n{}", "=".repeat(60));
        println!("AdapterOS Benchmark Report Summary");
        println!("{}", "=".repeat(60));
        println!("Timestamp: {}", report.timestamp.format("%Y-%m-%d %H:%M:%S UTC"));
        println!("System: {}", report.system_info.os);
        println!("CPU: {}", report.system_info.cpu);
        println!("Memory: {:.1} GB", report.system_info.memory_gb);
        if let Some(gpu) = &report.system_info.gpu {
            println!("GPU: {}", gpu);
        }
        println!();
        println!("Total Benchmarks: {}", report.summary.total_benchmarks);
        println!("Total Time: {:.2} seconds", report.summary.total_time_seconds);
        println!();

        if !report.summary.regressions.is_empty() {
            println!("🚨 Performance Regressions:");
            for regression in &report.summary.regressions {
                println!("  • {}", regression);
            }
            println!();
        }

        if !report.summary.improvements.is_empty() {
            println!("✅ Performance Improvements:");
            for improvement in &report.summary.improvements {
                println!("  • {}", improvement);
            }
            println!();
        }

        println!("Results saved to: {}/", self.config.output_dir);
        println!("{}", "=".repeat(60));
    }
}

/// Command-line interface for benchmark runner
pub fn run_benchmarks_from_args() -> Result<(), Box<dyn std::error::Error>> {
    let config = RunnerConfig::default();
    let mut runner = BenchmarkRunner::new(config);
    runner.run_all_benchmarks()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_runner_config_default() {
        let config = RunnerConfig::default();
        assert_eq!(config.output_dir, "benchmark_results");
        assert_eq!(config.comparison_threshold, 0.05);
        assert!(config.fail_on_regression);
    }

    #[test]
    fn test_system_info_collection() {
        let runner = BenchmarkRunner::new(RunnerConfig::default());
        let system_info = runner.collect_system_info();
        assert!(!system_info.os.is_empty());
        assert!(!system_info.cpu.is_empty());
        assert!(system_info.memory_gb > 0.0);
    }
}