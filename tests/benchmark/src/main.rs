//! adapterOS Benchmark Suite Main Entry Point
//!
//! This binary provides a command-line interface for running comprehensive
//! performance benchmarks for adapterOS components.

// Stub main when benchmarks are not enabled
#[cfg(not(all(test, feature = "extended-tests")))]
fn main() {
    eprintln!("adapterOS benchmarks require feature 'extended-tests' to be enabled.");
    eprintln!("Run with: cargo test -p adapteros-benchmarks --features extended-tests");
    std::process::exit(1);
}

// Full benchmark implementation when enabled
#[cfg(all(test, feature = "extended-tests"))]
mod benchmark_impl {
    use adapteros_benchmarks::runner::{BenchmarkRunner, RunnerConfig};
    use clap::{Parser, Subcommand};
    use std::path::PathBuf;

    #[derive(Parser)]
    #[command(name = "adapteros-benchmarks")]
    #[command(about = "Comprehensive performance benchmarking suite for adapterOS")]
    #[command(version = env!("CARGO_PKG_VERSION"))]
    pub struct Cli {
        #[command(subcommand)]
        pub command: Commands,
    }

    #[derive(Subcommand)]
    pub enum Commands {
        /// Run all benchmarks
        Run {
            /// Output directory for results
            #[arg(short, long, default_value = "benchmark_results")]
            output_dir: PathBuf,

            /// Baseline file for comparison
            #[arg(short, long)]
            baseline: Option<PathBuf>,

            /// Performance regression threshold (as decimal, e.g., 0.05 for 5%)
            #[arg(long, default_value = "0.05")]
            threshold: f64,

            /// Don't fail on performance regressions
            #[arg(long)]
            no_fail_on_regression: bool,

            /// Skip HTML report generation
            #[arg(long)]
            no_html: bool,

            /// Only run kernel benchmarks
            #[arg(long)]
            kernel_only: bool,

            /// Only run memory benchmarks
            #[arg(long)]
            memory_only: bool,

            /// Only run throughput benchmarks
            #[arg(long)]
            throughput_only: bool,

            /// Only run system benchmarks
            #[arg(long)]
            system_only: bool,

            /// Only run isolation benchmarks
            #[arg(long)]
            isolation_only: bool,

            /// Only run evidence benchmarks
            #[arg(long)]
            evidence_only: bool,
        },

        /// Compare benchmark results with baseline
        Compare {
            /// Current results file
            current: PathBuf,

            /// Baseline results file
            baseline: PathBuf,

            /// Output format (text, json, html)
            #[arg(short, long, default_value = "text")]
            format: String,
        },

        /// List available benchmarks
        List,

        /// Show benchmark configuration
        Config,
    }

    pub fn run_main() -> Result<(), Box<dyn std::error::Error>> {
        let cli = Cli::parse();

        match cli.command {
            Commands::Run {
                output_dir,
                baseline,
                threshold,
                no_fail_on_regression,
                no_html,
                kernel_only,
                memory_only,
                throughput_only,
                system_only,
                isolation_only,
                evidence_only,
            } => {
                let config = RunnerConfig {
                    output_dir: output_dir.to_string_lossy().to_string(),
                    baseline_file: baseline.map(|p| p.to_string_lossy().to_string()),
                    comparison_threshold: threshold,
                    fail_on_regression: !no_fail_on_regression,
                    generate_html_report: !no_html,
                    run_kernel_benchmarks: !memory_only
                        && !throughput_only
                        && !system_only
                        && !isolation_only
                        && !evidence_only
                        || kernel_only,
                    run_memory_benchmarks: !kernel_only
                        && !throughput_only
                        && !system_only
                        && !isolation_only
                        && !evidence_only
                        || memory_only,
                    run_throughput_benchmarks: !kernel_only
                        && !memory_only
                        && !system_only
                        && !isolation_only
                        && !evidence_only
                        || throughput_only,
                    run_system_benchmarks: !kernel_only
                        && !memory_only
                        && !throughput_only
                        && !isolation_only
                        && !evidence_only
                        || system_only,
                    run_isolation_benchmarks: !kernel_only
                        && !memory_only
                        && !throughput_only
                        && !system_only
                        && !evidence_only
                        || isolation_only,
                    run_evidence_benchmarks: !kernel_only
                        && !memory_only
                        && !throughput_only
                        && !system_only
                        && !isolation_only
                        || evidence_only,
                };

                let mut runner = BenchmarkRunner::new(config);
                runner.run_all_benchmarks()?;
            }

            Commands::Compare {
                current,
                baseline,
                format,
            } => {
                compare_results(&current, &baseline, &format)?;
            }

            Commands::List => {
                list_benchmarks();
            }

            Commands::Config => {
                show_config();
            }
        }

        Ok(())
    }

    fn compare_results(
        current_path: &PathBuf,
        baseline_path: &PathBuf,
        format: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        use adapteros_benchmarks::reporting::BenchmarkReport;
        use std::collections::HashMap;

        // Load results
        let current_content = std::fs::read_to_string(current_path)?;
        let current_report: BenchmarkReport = serde_json::from_str(&current_content)?;

        let baseline_content = std::fs::read_to_string(baseline_path)?;
        let baseline_report: BenchmarkReport = serde_json::from_str(&baseline_content)?;

        // Create lookup map for baseline results
        let baseline_map: HashMap<_, _> = baseline_report
            .results
            .iter()
            .map(|r| ((r.category.clone(), r.name.clone()), r.mean_time_ns))
            .collect();

        // Compare results
        let mut regressions = Vec::new();
        let mut improvements = Vec::new();
        let mut unchanged = Vec::new();

        for result in &current_report.results {
            let key = (result.category.clone(), result.name.clone());

            if let Some(baseline_time) = baseline_map.get(&key) {
                let change_ratio = result.mean_time_ns / baseline_time;

                if change_ratio > 1.05 {
                    // 5% regression
                    regressions.push((
                        result.name.clone(),
                        change_ratio,
                        result.mean_time_ns,
                        *baseline_time,
                    ));
                } else if change_ratio < 0.95 {
                    // 5% improvement
                    improvements.push((
                        result.name.clone(),
                        change_ratio,
                        result.mean_time_ns,
                        *baseline_time,
                    ));
                } else {
                    unchanged.push((
                        result.name.clone(),
                        change_ratio,
                        result.mean_time_ns,
                        *baseline_time,
                    ));
                }
            }
        }

        // Output results based on format
        match format {
            "json" => {
                let comparison = serde_json::json!({
                    "regressions": regressions,
                    "improvements": improvements,
                    "unchanged": unchanged,
                    "current_timestamp": current_report.timestamp,
                    "baseline_timestamp": baseline_report.timestamp
                });
                println!("{}", serde_json::to_string_pretty(&comparison)?);
            }

            "html" => {
                generate_comparison_html(
                    &regressions,
                    &improvements,
                    &unchanged,
                    &current_report,
                    &baseline_report,
                )?;
            }

            _ => {
                // text format (default)
                println!("adapterOS Benchmark Comparison");
                println!("================================");
                println!(
                    "Current:  {}",
                    current_report.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
                );
                println!(
                    "Baseline: {}",
                    baseline_report.timestamp.format("%Y-%m-%d %H:%M:%S UTC")
                );
                println!();

                if !regressions.is_empty() {
                    println!("🚨 Performance Regressions:");
                    for (name, ratio, current, baseline) in &regressions {
                        println!(
                            "  • {}: {:.2}% slower ({:.2}ns → {:.2}ns)",
                            name,
                            (ratio - 1.0) * 100.0,
                            baseline,
                            current
                        );
                    }
                    println!();
                }

                if !improvements.is_empty() {
                    println!("✅ Performance Improvements:");
                    for (name, ratio, current, baseline) in &improvements {
                        println!(
                            "  • {}: {:.2}% faster ({:.2}ns → {:.2}ns)",
                            name,
                            (1.0 - ratio) * 100.0,
                            baseline,
                            current
                        );
                    }
                    println!();
                }

                println!("Summary:");
                println!("  Regressions: {}", regressions.len());
                println!("  Improvements: {}", improvements.len());
                println!("  Unchanged: {}", unchanged.len());
            }
        }

        Ok(())
    }

    fn generate_comparison_html(
        regressions: &[(String, f64, f64, f64)],
        improvements: &[(String, f64, f64, f64)],
        _unchanged: &[(String, f64, f64, f64)],
        current: &adapteros_benchmarks::reporting::BenchmarkReport,
        baseline: &adapteros_benchmarks::reporting::BenchmarkReport,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let html_content = format!(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>adapterOS Benchmark Comparison</title>
    <style>
        body {{ font-family: Arial, sans-serif; margin: 40px; }}
        .header {{ background: #f0f0f0; padding: 20px; border-radius: 5px; }}
        .regressions {{ background: #ffe6e6; padding: 15px; margin: 20px 0; border-radius: 5px; }}
        .improvements {{ background: #e6ffe6; padding: 15px; margin: 20px 0; border-radius: 5px; }}
        .summary {{ background: #e8f4f8; padding: 15px; margin: 20px 0; border-radius: 5px; }}
        table {{ border-collapse: collapse; width: 100%; margin: 10px 0; }}
        th, td {{ border: 1px solid #ddd; padding: 8px; text-align: left; }}
        th {{ background-color: #f2f2f2; }}
        .regression {{ color: red; }}
        .improvement {{ color: green; }}
    </style>
</head>
<body>
    <div class="header">
        <h1>adapterOS Benchmark Comparison</h1>
        <p>Current: {} | Baseline: {}</p>
    </div>

    <div class="summary">
        <h2>Summary</h2>
        <p>Regressions: {} | Improvements: {}</p>
    </div>

    <div class="regressions">
        <h2>🚨 Performance Regressions</h2>
        <table>
            <tr><th>Benchmark</th><th>Change</th><th>Current (ns)</th><th>Baseline (ns)</th></tr>
            {}
        </table>
    </div>

    <div class="improvements">
        <h2>✅ Performance Improvements</h2>
        <table>
            <tr><th>Benchmark</th><th>Change</th><th>Current (ns)</th><th>Baseline (ns)</th></tr>
            {}
        </table>
    </div>
</body>
</html>"#,
            current.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            baseline.timestamp.format("%Y-%m-%d %H:%M:%S UTC"),
            regressions.len(),
            improvements.len(),
            regressions.iter().map(|(name, ratio, current_time, baseline_time)| {
                format!("<tr><td>{}</td><td class='regression'>+{:.2}%</td><td>{:.2}</td><td>{:.2}</td></tr>",
                       name, (ratio - 1.0) * 100.0, current_time, baseline_time)
            }).collect::<Vec<_>>().join("\n"),
            improvements.iter().map(|(name, ratio, current_time, baseline_time)| {
                format!("<tr><td>{}</td><td class='improvement'>-{:.2}%</td><td>{:.2}</td><td>{:.2}</td></tr>",
                       name, (1.0 - ratio) * 100.0, current_time, baseline_time)
            }).collect::<Vec<_>>().join("\n")
        );

        std::fs::write("benchmark_comparison.html", html_content)?;
        println!("Comparison report saved to benchmark_comparison.html");

        Ok(())
    }

    fn list_benchmarks() {
        println!("Available adapterOS Benchmarks");
        println!("==============================");
        println!();
        println!("Kernel Performance Benchmarks:");
        println!("  • Metal kernel inference step");
        println!("  • Matrix multiplication (1024x1024)");
        println!("  • Attention mechanism (512 seq, 8 heads)");
        println!("  • LoRA adapter fusion (8 adapters)");
        println!();
        println!("Memory Benchmarks:");
        println!("  • Memory allocation (64B to 16MB)");
        println!("  • Memory pool allocation");
        println!("  • Sequential/random/strided access patterns");
        println!("  • Memory pressure and fragmentation");
        println!("  • Concurrent memory operations");
        println!("  • Memory tracking overhead");
        println!("  • Memory-mapped file operations");
        println!();
        println!("Throughput Benchmarks:");
        println!("  • Inference throughput (batch sizes 1-32)");
        println!("  • Concurrent request processing (1-32 workers)");
        println!("  • Request queue processing");
        println!("  • Adapter routing throughput");
        println!("  • Evidence processing throughput");
        println!("  • End-to-end response latency");
        println!();
        println!("System Benchmarks:");
        println!("  • System metrics collection (CPU, memory, disk, network)");
        println!("  • Telemetry collection and aggregation");
        println!("  • Policy evaluation performance");
        println!("  • Deterministic execution overhead");
        println!("  • Evidence processing and validation");
        println!("  • Alerting engine performance");
        println!();
        println!("Isolation Benchmarks:");
        println!("  • Multi-tenant context switching");
        println!("  • Resource quota enforcement");
        println!("  • Tenant data isolation");
        println!("  • Concurrent tenant operations");
        println!("  • Security boundary enforcement");
        println!("  • Performance isolation");
        println!("  • Tenant cleanup and reclamation");
        println!("  • Tenant migration scenarios");
        println!();
        println!("Evidence Benchmarks:");
        println!("  • Evidence collection and scoring");
        println!("  • Evidence ranking and filtering");
        println!("  • Response grounding with evidence");
        println!("  • Evidence caching and retrieval");
        println!("  • Evidence-based decision making");
        println!("  • Response latency with evidence processing");
    }

    fn show_config() {
        let config = RunnerConfig::default();

        println!("Default Benchmark Configuration");
        println!("===============================");
        println!("Output Directory: {}", config.output_dir);
        println!(
            "Comparison Threshold: {:.1}%",
            config.comparison_threshold * 100.0
        );
        println!("Fail on Regression: {}", config.fail_on_regression);
        println!("Generate HTML Report: {}", config.generate_html_report);
        println!();
        println!("Benchmark Categories:");
        println!("  Kernel Benchmarks: {}", config.run_kernel_benchmarks);
        println!("  Memory Benchmarks: {}", config.run_memory_benchmarks);
        println!(
            "  Throughput Benchmarks: {}",
            config.run_throughput_benchmarks
        );
        println!("  System Benchmarks: {}", config.run_system_benchmarks);
        println!(
            "  Isolation Benchmarks: {}",
            config.run_isolation_benchmarks
        );
        println!("  Evidence Benchmarks: {}", config.run_evidence_benchmarks);
    }
}

#[cfg(all(test, feature = "extended-tests"))]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    benchmark_impl::run_main()
}
