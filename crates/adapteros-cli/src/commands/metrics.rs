//! System metrics CLI commands
//!
//! Provides CLI commands for managing system metrics, viewing health status,
//! and exporting metrics data.
//!
//! This module uses `adapteros_db::SystemMetricsDbOps` for database operations
//! and `sysinfo` for live metrics collection.

use adapteros_db::{Db, SystemMetricsDbOps};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use comfy_table::{Cell, Table};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use sysinfo::{Disks, Networks, System};

/// Thresholds configuration for policy checking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThresholdsConfig {
    pub cpu_warning: f64,
    pub cpu_critical: f64,
    pub memory_warning: f64,
    pub memory_critical: f64,
    pub disk_warning: f64,
    pub disk_critical: f64,
    pub gpu_warning: f64,
    pub gpu_critical: f64,
    pub min_memory_headroom: f64,
}

impl Default for ThresholdsConfig {
    fn default() -> Self {
        Self {
            cpu_warning: 70.0,
            cpu_critical: 90.0,
            memory_warning: 80.0,
            memory_critical: 95.0,
            disk_warning: 85.0,
            disk_critical: 95.0,
            gpu_warning: 80.0,
            gpu_critical: 95.0,
            min_memory_headroom: 15.0,
        }
    }
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    pub collection_interval_secs: u64,
    pub sampling_rate: f32,
    pub enable_gpu_metrics: bool,
    pub enable_disk_metrics: bool,
    pub enable_network_metrics: bool,
    pub retention_days: u32,
    pub thresholds: ThresholdsConfig,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            collection_interval_secs: 30,
            sampling_rate: 0.05,
            enable_gpu_metrics: true,
            enable_disk_metrics: true,
            enable_network_metrics: true,
            retention_days: 30,
            thresholds: ThresholdsConfig::default(),
        }
    }
}

/// Policy violation for real-time check
#[derive(Debug, Clone)]
pub struct PolicyViolation {
    pub metric: String,
    pub current_value: f64,
    pub threshold_value: f64,
    pub severity: ViolationSeverity,
    pub message: String,
}

/// Violation severity levels
#[derive(Debug, Clone, PartialEq)]
pub enum ViolationSeverity {
    Warning,
    Critical,
}

/// System health status
#[derive(Debug, Clone, PartialEq)]
pub enum SystemHealthStatus {
    Healthy,
    Warning,
    Critical,
}

impl SystemHealthStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            SystemHealthStatus::Healthy => "healthy",
            SystemHealthStatus::Warning => "warning",
            SystemHealthStatus::Critical => "critical",
        }
    }
}

/// Live system metrics for real-time display
#[derive(Debug, Clone)]
pub struct LiveMetrics {
    pub cpu_usage: f64,
    pub memory_usage: f64,
    pub disk_usage_percent: f32,
    pub network_bandwidth_mbps: f32,
    pub gpu_utilization: Option<f64>,
}

/// Simple live metrics collector using sysinfo
struct LiveMetricsCollector {
    sys: System,
}

impl LiveMetricsCollector {
    fn new() -> Self {
        let mut sys = System::new_all();
        sys.refresh_all();
        Self { sys }
    }

    fn collect(&mut self) -> LiveMetrics {
        self.sys.refresh_all();

        let cpu_usage = self.calculate_cpu_usage();
        let memory_usage = self.calculate_memory_usage();
        let disk_usage_percent = self.calculate_disk_usage();
        let network_bandwidth_mbps = self.calculate_network_bandwidth();

        LiveMetrics {
            cpu_usage,
            memory_usage,
            disk_usage_percent,
            network_bandwidth_mbps,
            gpu_utilization: None, // GPU metrics require Metal/CoreML integration
        }
    }

    fn calculate_cpu_usage(&mut self) -> f64 {
        self.sys.refresh_cpu_usage();
        let cpus = self.sys.cpus();
        if cpus.is_empty() {
            return 0.0;
        }
        let total_usage: f32 = cpus.iter().map(|cpu| cpu.cpu_usage()).sum();
        (total_usage / cpus.len() as f32) as f64
    }

    fn calculate_memory_usage(&self) -> f64 {
        let total_memory = self.sys.total_memory();
        if total_memory == 0 {
            return 0.0;
        }
        let used_memory = self.sys.used_memory();
        (used_memory as f64 / total_memory as f64) * 100.0
    }

    fn calculate_disk_usage(&self) -> f32 {
        let disks = Disks::new_with_refreshed_list();
        let mut total_space = 0u64;
        let mut available_space = 0u64;

        for disk in &disks {
            total_space += disk.total_space();
            available_space += disk.available_space();
        }

        if total_space > 0 {
            ((total_space - available_space) as f32 / total_space as f32) * 100.0
        } else {
            0.0
        }
    }

    fn calculate_network_bandwidth(&self) -> f32 {
        let networks = Networks::new_with_refreshed_list();
        let mut total_rx_bytes = 0u64;
        let mut total_tx_bytes = 0u64;

        for (_, network) in &networks {
            total_rx_bytes += network.received();
            total_tx_bytes += network.transmitted();
        }

        // Simplified bandwidth calculation (cumulative, not rate)
        let total_bytes = total_rx_bytes + total_tx_bytes;
        (total_bytes as f32 * 8.0) / 1_000_000.0 // Convert to Mbps
    }

    fn uptime_seconds(&self) -> u64 {
        System::uptime()
    }

    fn process_count(&self) -> usize {
        self.sys.processes().len()
    }

    fn load_average(&self) -> (f64, f64, f64) {
        let load = System::load_average();
        (load.one, load.five, load.fifteen)
    }
}

/// Check policy violations against live metrics
fn check_policy_violations(
    metrics: &LiveMetrics,
    thresholds: &ThresholdsConfig,
) -> Vec<PolicyViolation> {
    let mut violations = Vec::new();

    // CPU usage violations
    if metrics.cpu_usage > thresholds.cpu_critical {
        violations.push(PolicyViolation {
            metric: "cpu_usage".to_string(),
            current_value: metrics.cpu_usage,
            threshold_value: thresholds.cpu_critical,
            severity: ViolationSeverity::Critical,
            message: format!(
                "CPU usage {:.1}% exceeds critical threshold {:.1}%",
                metrics.cpu_usage, thresholds.cpu_critical
            ),
        });
    } else if metrics.cpu_usage > thresholds.cpu_warning {
        violations.push(PolicyViolation {
            metric: "cpu_usage".to_string(),
            current_value: metrics.cpu_usage,
            threshold_value: thresholds.cpu_warning,
            severity: ViolationSeverity::Warning,
            message: format!(
                "CPU usage {:.1}% exceeds warning threshold {:.1}%",
                metrics.cpu_usage, thresholds.cpu_warning
            ),
        });
    }

    // Memory usage violations
    if metrics.memory_usage > thresholds.memory_critical {
        violations.push(PolicyViolation {
            metric: "memory_usage".to_string(),
            current_value: metrics.memory_usage,
            threshold_value: thresholds.memory_critical,
            severity: ViolationSeverity::Critical,
            message: format!(
                "Memory usage {:.1}% exceeds critical threshold {:.1}%",
                metrics.memory_usage, thresholds.memory_critical
            ),
        });
    } else if metrics.memory_usage > thresholds.memory_warning {
        violations.push(PolicyViolation {
            metric: "memory_usage".to_string(),
            current_value: metrics.memory_usage,
            threshold_value: thresholds.memory_warning,
            severity: ViolationSeverity::Warning,
            message: format!(
                "Memory usage {:.1}% exceeds warning threshold {:.1}%",
                metrics.memory_usage, thresholds.memory_warning
            ),
        });
    }

    // Disk usage violations
    if (metrics.disk_usage_percent as f64) > thresholds.disk_critical {
        violations.push(PolicyViolation {
            metric: "disk_usage".to_string(),
            current_value: metrics.disk_usage_percent as f64,
            threshold_value: thresholds.disk_critical,
            severity: ViolationSeverity::Critical,
            message: format!(
                "Disk usage {:.1}% exceeds critical threshold {:.1}%",
                metrics.disk_usage_percent, thresholds.disk_critical
            ),
        });
    } else if (metrics.disk_usage_percent as f64) > thresholds.disk_warning {
        violations.push(PolicyViolation {
            metric: "disk_usage".to_string(),
            current_value: metrics.disk_usage_percent as f64,
            threshold_value: thresholds.disk_warning,
            severity: ViolationSeverity::Warning,
            message: format!(
                "Disk usage {:.1}% exceeds warning threshold {:.1}%",
                metrics.disk_usage_percent, thresholds.disk_warning
            ),
        });
    }

    // GPU utilization violations
    if let Some(gpu_util) = metrics.gpu_utilization {
        if gpu_util > thresholds.gpu_critical {
            violations.push(PolicyViolation {
                metric: "gpu_utilization".to_string(),
                current_value: gpu_util,
                threshold_value: thresholds.gpu_critical,
                severity: ViolationSeverity::Critical,
                message: format!(
                    "GPU utilization {:.1}% exceeds critical threshold {:.1}%",
                    gpu_util, thresholds.gpu_critical
                ),
            });
        } else if gpu_util > thresholds.gpu_warning {
            violations.push(PolicyViolation {
                metric: "gpu_utilization".to_string(),
                current_value: gpu_util,
                threshold_value: thresholds.gpu_warning,
                severity: ViolationSeverity::Warning,
                message: format!(
                    "GPU utilization {:.1}% exceeds warning threshold {:.1}%",
                    gpu_util, thresholds.gpu_warning
                ),
            });
        }
    }

    violations
}

/// Get health status from violations
fn get_health_status(violations: &[PolicyViolation]) -> SystemHealthStatus {
    if violations.is_empty() {
        SystemHealthStatus::Healthy
    } else if violations
        .iter()
        .any(|v| v.severity == ViolationSeverity::Critical)
    {
        SystemHealthStatus::Critical
    } else {
        SystemHealthStatus::Warning
    }
}

#[derive(Parser)]
pub struct MetricsCommand {
    #[command(subcommand)]
    pub subcommand: MetricsSubcommand,
}

#[derive(Subcommand)]
pub enum MetricsSubcommand {
    /// Show current system metrics
    Show,
    /// Show metrics history from database
    History {
        /// Number of hours to show (default: 24)
        #[arg(short, long, default_value = "24")]
        hours: u32,
        /// Maximum number of records to show
        #[arg(short, long, default_value = "100")]
        limit: usize,
    },
    /// Show system health status
    Health,
    /// Export metrics to file from database
    Export {
        /// Output file path
        #[arg(short, long)]
        output: std::path::PathBuf,
        /// Export format (json, csv)
        #[arg(short, long, default_value = "json")]
        format: String,
        /// Number of hours to export (default: 24)
        #[arg(short, long, default_value = "24")]
        hours: u32,
    },
    /// Check policy thresholds against current metrics
    Check,
    /// Show threshold violations from database
    Violations {
        /// Show only unresolved violations
        #[arg(short, long)]
        unresolved: bool,
    },
    /// Configure metrics collection
    Config {
        /// Configuration key
        #[arg(short, long)]
        key: Option<String>,
        /// Configuration value
        #[arg(short, long)]
        value: Option<String>,
        /// List all configuration
        #[arg(short, long)]
        list: bool,
    },
}

pub async fn run_metrics(cmd: MetricsCommand) -> Result<()> {
    let mode = crate::output::OutputMode::from_env();

    match cmd.subcommand {
        MetricsSubcommand::Show => {
            crate::output::command_header(&mode, "System Metrics");
            show_current_metrics(&mode).await?;
        }
        MetricsSubcommand::History { hours, limit } => {
            crate::output::command_header(
                &mode,
                &format!("System Metrics History ({} hours)", hours),
            );
            show_metrics_history(&mode, hours, limit).await?;
        }
        MetricsSubcommand::Health => {
            crate::output::command_header(&mode, "System Health Status");
            show_health_status(&mode).await?;
        }
        MetricsSubcommand::Export {
            output,
            format,
            hours,
        } => {
            crate::output::command_header(
                &mode,
                &format!("Exporting metrics to {}", output.display()),
            );
            export_metrics(&mode, &output, &format, hours).await?;
        }
        MetricsSubcommand::Check => {
            crate::output::command_header(&mode, "Policy Threshold Check");
            check_policy_thresholds(&mode).await?;
        }
        MetricsSubcommand::Violations { unresolved } => {
            crate::output::command_header(&mode, "Threshold Violations");
            show_violations(&mode, unresolved).await?;
        }
        MetricsSubcommand::Config { key, value, list } => {
            if list {
                crate::output::command_header(&mode, "Metrics Configuration");
                list_config(&mode).await?;
            } else if let (Some(key), Some(value)) = (key, value) {
                crate::output::command_header(
                    &mode,
                    &format!("Setting config: {} = {}", key, value),
                );
                set_config(&mode, &key, &value).await?;
            } else {
                anyhow::bail!("Either --list or both --key and --value must be specified");
            }
        }
    }

    Ok(())
}

async fn show_current_metrics(mode: &crate::output::OutputMode) -> Result<()> {
    let mut collector = LiveMetricsCollector::new();
    let metrics = collector.collect();
    let load_avg = collector.load_average();

    if mode.is_json() {
        let response = serde_json::json!({
            "cpu_usage": metrics.cpu_usage,
            "memory_usage": metrics.memory_usage,
            "disk_usage": metrics.disk_usage_percent,
            "network_bandwidth": metrics.network_bandwidth_mbps,
            "gpu_utilization": metrics.gpu_utilization,
            "uptime_seconds": collector.uptime_seconds(),
            "process_count": collector.process_count(),
            "load_average": {
                "load_1min": load_avg.0,
                "load_5min": load_avg.1,
                "load_15min": load_avg.2
            },
            "timestamp": SystemTime::now().duration_since(UNIX_EPOCH).expect("System time before UNIX epoch").as_secs()
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        let mut table = Table::new();
        table.set_header(vec!["Metric", "Value", "Unit"]);

        table.add_row(vec![
            Cell::new("CPU Usage"),
            Cell::new(format!("{:.1}", metrics.cpu_usage)),
            Cell::new("%"),
        ]);

        table.add_row(vec![
            Cell::new("Memory Usage"),
            Cell::new(format!("{:.1}", metrics.memory_usage)),
            Cell::new("%"),
        ]);

        table.add_row(vec![
            Cell::new("Disk Usage"),
            Cell::new(format!("{:.1}", metrics.disk_usage_percent)),
            Cell::new("%"),
        ]);

        table.add_row(vec![
            Cell::new("Network Bandwidth"),
            Cell::new(format!("{:.2}", metrics.network_bandwidth_mbps)),
            Cell::new("Mbps"),
        ]);

        if let Some(gpu_util) = metrics.gpu_utilization {
            table.add_row(vec![
                Cell::new("GPU Utilization"),
                Cell::new(format!("{:.1}", gpu_util)),
                Cell::new("%"),
            ]);
        }

        table.add_row(vec![
            Cell::new("Uptime"),
            Cell::new(format!("{}", collector.uptime_seconds())),
            Cell::new("seconds"),
        ]);

        table.add_row(vec![
            Cell::new("Process Count"),
            Cell::new(format!("{}", collector.process_count())),
            Cell::new("processes"),
        ]);

        table.add_row(vec![
            Cell::new("Load Average (1m)"),
            Cell::new(format!("{:.2}", load_avg.0)),
            Cell::new(""),
        ]);

        table.add_row(vec![
            Cell::new("Load Average (5m)"),
            Cell::new(format!("{:.2}", load_avg.1)),
            Cell::new(""),
        ]);

        table.add_row(vec![
            Cell::new("Load Average (15m)"),
            Cell::new(format!("{:.2}", load_avg.2)),
            Cell::new(""),
        ]);

        println!("{}", table);
    }

    Ok(())
}

/// Connect to the database using adapteros_db::Db
async fn connect_db() -> Result<Db> {
    let db_path = get_metrics_db_path();
    Db::connect(&db_path)
        .await
        .context("Failed to connect to metrics database")
}

async fn show_metrics_history(
    mode: &crate::output::OutputMode,
    hours: u32,
    limit: usize,
) -> Result<()> {
    // Connect to the database using adapteros_db::Db
    let db = connect_db().await?;

    // Fetch metrics history using SystemMetricsDbOps trait
    let records = db
        .get_metrics_history(hours, limit)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch metrics history: {}", e))?;

    if mode.is_json() {
        let response = serde_json::json!({
            "hours": hours,
            "limit": limit,
            "record_count": records.len(),
            "records": records.iter().map(|r| serde_json::json!({
                "timestamp": r.timestamp,
                "cpu_usage": r.cpu_usage,
                "memory_usage": r.memory_usage,
                "disk_usage_percent": r.disk_usage_percent,
                "network_bandwidth_mbps": r.network_bandwidth_mbps,
                "gpu_utilization": r.gpu_utilization,
                "uptime_seconds": r.uptime_seconds,
                "process_count": r.process_count,
                "load_1min": r.load_1min,
                "load_5min": r.load_5min,
                "load_15min": r.load_15min,
            })).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        if records.is_empty() {
            println!("No metrics history found for the past {} hours", hours);
            return Ok(());
        }

        let mut table = Table::new();
        table.set_header(vec![
            "Timestamp",
            "CPU %",
            "Mem %",
            "Disk %",
            "Net Mbps",
            "GPU %",
            "Processes",
        ]);

        for record in records.iter().take(limit) {
            let timestamp = chrono::DateTime::from_timestamp(record.timestamp, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| record.timestamp.to_string());

            table.add_row(vec![
                Cell::new(timestamp),
                Cell::new(format!("{:.1}", record.cpu_usage)),
                Cell::new(format!("{:.1}", record.memory_usage)),
                Cell::new(format!("{:.1}", record.disk_usage_percent.unwrap_or(0.0))),
                Cell::new(format!(
                    "{:.2}",
                    record.network_bandwidth_mbps.unwrap_or(0.0)
                )),
                Cell::new(
                    record
                        .gpu_utilization
                        .map(|v| format!("{:.1}", v))
                        .unwrap_or_else(|| "N/A".to_string()),
                ),
                Cell::new(format!("{}", record.process_count)),
            ]);
        }

        println!("{}", table);
        println!(
            "\nShowing {} of {} records from the past {} hours",
            records.len().min(limit),
            records.len(),
            hours
        );
    }

    Ok(())
}

/// Get the metrics database path from environment or default
fn get_metrics_db_path() -> String {
    std::env::var("AOS_METRICS_DB_PATH")
        .or_else(|_| std::env::var("AOS_DB_PATH"))
        .unwrap_or_else(|_| "var/aos-cp.sqlite3".to_string())
}

async fn show_health_status(mode: &crate::output::OutputMode) -> Result<()> {
    let mut collector = LiveMetricsCollector::new();
    let metrics = collector.collect();
    let thresholds = ThresholdsConfig::default();

    let violations = check_policy_violations(&metrics, &thresholds);
    let health_status = get_health_status(&violations);

    if mode.is_json() {
        let response = serde_json::json!({
            "status": health_status.as_str(),
            "violations": violations.iter().map(|v| serde_json::json!({
                "metric": v.metric,
                "current_value": v.current_value,
                "threshold_value": v.threshold_value,
                "severity": format!("{:?}", v.severity),
                "message": v.message
            })).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        println!("System Health Status: {}", health_status.as_str());

        if violations.is_empty() {
            println!("All checks passed");
        } else {
            println!("{} violations detected:", violations.len());
            for violation in violations {
                println!(
                    "  - {}: {} (threshold: {})",
                    violation.metric, violation.current_value, violation.threshold_value
                );
            }
        }
    }

    Ok(())
}

async fn export_metrics(
    mode: &crate::output::OutputMode,
    output: &Path,
    format: &str,
    hours: u32,
) -> Result<()> {
    // Connect to the database using adapteros_db::Db
    let db = connect_db().await?;

    // Fetch all metrics for the time period (use large limit for export)
    let records = db
        .get_metrics_history(hours, 100_000)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch metrics for export: {}", e))?;

    if records.is_empty() {
        if mode.is_json() {
            let response = serde_json::json!({
                "status": "no_data",
                "message": format!("No metrics data found for the past {} hours", hours),
                "output": output.to_string_lossy(),
                "format": format,
                "hours": hours
            });
            println!("{}", serde_json::to_string_pretty(&response)?);
        } else {
            println!("No metrics data found for the past {} hours", hours);
        }
        return Ok(());
    }

    // Export based on format
    let (data, file_size) = match format.to_lowercase().as_str() {
        "json" => {
            let json_data = serde_json::to_string_pretty(&records)?;
            let size = json_data.len();
            (json_data, size)
        }
        "csv" => {
            let mut csv_output = String::new();
            csv_output.push_str("timestamp,cpu_usage,memory_usage,disk_usage_percent,network_bandwidth_mbps,gpu_utilization,uptime_seconds,process_count,load_1min,load_5min,load_15min\n");
            for record in &records {
                csv_output.push_str(&format!(
                    "{},{:.2},{:.2},{:.2},{:.2},{},{},{},{:.2},{:.2},{:.2}\n",
                    record.timestamp,
                    record.cpu_usage,
                    record.memory_usage,
                    record.disk_usage_percent.unwrap_or(0.0),
                    record.network_bandwidth_mbps.unwrap_or(0.0),
                    record
                        .gpu_utilization
                        .map(|v| format!("{:.2}", v))
                        .unwrap_or_else(|| "".to_string()),
                    record.uptime_seconds,
                    record.process_count,
                    record.load_1min,
                    record.load_5min,
                    record.load_15min,
                ));
            }
            let size = csv_output.len();
            (csv_output, size)
        }
        _ => {
            anyhow::bail!(
                "Unsupported export format: {}. Use 'json' or 'csv'.",
                format
            );
        }
    };

    // Write to file
    let mut file = std::fs::File::create(output).context(format!(
        "Failed to create output file: {}",
        output.display()
    ))?;
    file.write_all(data.as_bytes())
        .context("Failed to write metrics data")?;

    if mode.is_json() {
        let response = serde_json::json!({
            "status": "success",
            "output": output.to_string_lossy(),
            "format": format,
            "hours": hours,
            "record_count": records.len(),
            "file_size_bytes": file_size
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        println!(
            "Exported {} metrics records to {}",
            records.len(),
            output.display()
        );
        println!("Format: {}", format);
        println!("Time range: past {} hours", hours);
        println!("File size: {} bytes", file_size);
    }

    Ok(())
}

async fn check_policy_thresholds(mode: &crate::output::OutputMode) -> Result<()> {
    let mut collector = LiveMetricsCollector::new();
    let metrics = collector.collect();
    let thresholds = ThresholdsConfig::default();

    let violations = check_policy_violations(&metrics, &thresholds);

    if mode.is_json() {
        let response = serde_json::json!({
            "thresholds": {
                "cpu_warning": thresholds.cpu_warning,
                "cpu_critical": thresholds.cpu_critical,
                "memory_warning": thresholds.memory_warning,
                "memory_critical": thresholds.memory_critical,
                "disk_warning": thresholds.disk_warning,
                "disk_critical": thresholds.disk_critical,
                "gpu_warning": thresholds.gpu_warning,
                "gpu_critical": thresholds.gpu_critical,
                "min_memory_headroom": thresholds.min_memory_headroom
            },
            "violations": violations.iter().map(|v| serde_json::json!({
                "metric": v.metric,
                "current_value": v.current_value,
                "threshold_value": v.threshold_value,
                "severity": format!("{:?}", v.severity),
                "message": v.message
            })).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        println!("Policy Threshold Check");
        println!("=====================");

        if violations.is_empty() {
            println!("All thresholds within limits");
        } else {
            println!("Threshold violations detected:");
            for violation in violations {
                println!(
                    "  - {}: {:.1}% (threshold: {:.1}%) [{}]",
                    violation.metric,
                    violation.current_value,
                    violation.threshold_value,
                    format!("{:?}", violation.severity).to_lowercase()
                );
            }
        }
    }

    Ok(())
}

async fn show_violations(mode: &crate::output::OutputMode, unresolved: bool) -> Result<()> {
    // Connect to the database using adapteros_db::Db
    let db = connect_db().await?;

    // Fetch violations using SystemMetricsDbOps trait
    let violations = db
        .get_violations(unresolved)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to fetch violations: {}", e))?;

    if mode.is_json() {
        let response = serde_json::json!({
            "unresolved_only": unresolved,
            "violation_count": violations.len(),
            "violations": violations.iter().map(|v| serde_json::json!({
                "id": v.id,
                "timestamp": v.timestamp,
                "metric_name": v.metric_name,
                "current_value": v.current_value,
                "threshold_value": v.threshold_value,
                "severity": v.severity,
                "resolved_at": v.resolved_at,
                "created_at": v.created_at,
            })).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        if violations.is_empty() {
            println!(
                "No {} violations found",
                if unresolved { "unresolved" } else { "" }
            );
            return Ok(());
        }

        println!(
            "Threshold Violations{}:",
            if unresolved { " (Unresolved)" } else { "" }
        );
        println!();

        let mut table = Table::new();
        table.set_header(vec![
            "ID",
            "Metric",
            "Value",
            "Threshold",
            "Severity",
            "Time",
        ]);

        for violation in &violations {
            let timestamp = chrono::DateTime::from_timestamp(violation.timestamp, 0)
                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                .unwrap_or_else(|| violation.timestamp.to_string());

            table.add_row(vec![
                Cell::new(violation.id.to_string()),
                Cell::new(&violation.metric_name),
                Cell::new(format!("{:.2}", violation.current_value)),
                Cell::new(format!("{:.2}", violation.threshold_value)),
                Cell::new(&violation.severity),
                Cell::new(timestamp),
            ]);
        }

        println!("{}", table);
        println!("\nTotal: {} violations", violations.len());
    }

    Ok(())
}

async fn list_config(mode: &crate::output::OutputMode) -> Result<()> {
    let mut config = MetricsConfig::default();

    if let Ok(db) = connect_db().await {
        for key in metrics_config_keys() {
            if let Some(value) = db.get_config(key).await? {
                apply_config_value(&mut config, key, &value)?;
            }
        }
    }

    if mode.is_json() {
        let response = serde_json::json!({
            "collection_interval_secs": config.collection_interval_secs,
            "sampling_rate": config.sampling_rate,
            "enable_gpu_metrics": config.enable_gpu_metrics,
            "enable_disk_metrics": config.enable_disk_metrics,
            "enable_network_metrics": config.enable_network_metrics,
            "retention_days": config.retention_days,
            "thresholds": {
                "cpu_warning": config.thresholds.cpu_warning,
                "cpu_critical": config.thresholds.cpu_critical,
                "memory_warning": config.thresholds.memory_warning,
                "memory_critical": config.thresholds.memory_critical,
                "disk_warning": config.thresholds.disk_warning,
                "disk_critical": config.thresholds.disk_critical,
                "gpu_warning": config.thresholds.gpu_warning,
                "gpu_critical": config.thresholds.gpu_critical,
                "min_memory_headroom": config.thresholds.min_memory_headroom
            }
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        let mut table = Table::new();
        table.set_header(vec!["Key", "Value"]);

        table.add_row(vec![
            Cell::new("collection_interval_secs"),
            Cell::new(format!("{}", config.collection_interval_secs)),
        ]);

        table.add_row(vec![
            Cell::new("sampling_rate"),
            Cell::new(format!("{:.3}", config.sampling_rate)),
        ]);

        table.add_row(vec![
            Cell::new("enable_gpu_metrics"),
            Cell::new(format!("{}", config.enable_gpu_metrics)),
        ]);

        table.add_row(vec![
            Cell::new("enable_disk_metrics"),
            Cell::new(format!("{}", config.enable_disk_metrics)),
        ]);

        table.add_row(vec![
            Cell::new("enable_network_metrics"),
            Cell::new(format!("{}", config.enable_network_metrics)),
        ]);

        table.add_row(vec![
            Cell::new("retention_days"),
            Cell::new(format!("{}", config.retention_days)),
        ]);

        println!("{}", table);
    }

    Ok(())
}

async fn set_config(mode: &crate::output::OutputMode, key: &str, value: &str) -> Result<()> {
    let db = connect_db().await?;
    let mut config = MetricsConfig::default();
    apply_config_value(&mut config, key, value)?;
    db.set_config(key, value).await?;

    if mode.is_json() {
        let response = serde_json::json!({
            "status": "ok",
            "key": key,
            "value": value
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        println!("Config updated");
        println!("Key: {}", key);
        println!("Value: {}", value);
    }

    Ok(())
}

fn metrics_config_keys() -> Vec<&'static str> {
    vec![
        "collection_interval_secs",
        "sampling_rate",
        "enable_gpu_metrics",
        "enable_disk_metrics",
        "enable_network_metrics",
        "retention_days",
        "thresholds.cpu_warning",
        "thresholds.cpu_critical",
        "thresholds.memory_warning",
        "thresholds.memory_critical",
        "thresholds.disk_warning",
        "thresholds.disk_critical",
        "thresholds.gpu_warning",
        "thresholds.gpu_critical",
        "thresholds.min_memory_headroom",
    ]
}

fn apply_config_value(config: &mut MetricsConfig, key: &str, value: &str) -> Result<()> {
    match key {
        "collection_interval_secs" => {
            config.collection_interval_secs = value.parse()?;
        }
        "sampling_rate" => {
            config.sampling_rate = value.parse()?;
        }
        "enable_gpu_metrics" => {
            config.enable_gpu_metrics = value.parse()?;
        }
        "enable_disk_metrics" => {
            config.enable_disk_metrics = value.parse()?;
        }
        "enable_network_metrics" => {
            config.enable_network_metrics = value.parse()?;
        }
        "retention_days" => {
            config.retention_days = value.parse()?;
        }
        "thresholds.cpu_warning" => {
            config.thresholds.cpu_warning = value.parse()?;
        }
        "thresholds.cpu_critical" => {
            config.thresholds.cpu_critical = value.parse()?;
        }
        "thresholds.memory_warning" => {
            config.thresholds.memory_warning = value.parse()?;
        }
        "thresholds.memory_critical" => {
            config.thresholds.memory_critical = value.parse()?;
        }
        "thresholds.disk_warning" => {
            config.thresholds.disk_warning = value.parse()?;
        }
        "thresholds.disk_critical" => {
            config.thresholds.disk_critical = value.parse()?;
        }
        "thresholds.gpu_warning" => {
            config.thresholds.gpu_warning = value.parse()?;
        }
        "thresholds.gpu_critical" => {
            config.thresholds.gpu_critical = value.parse()?;
        }
        "thresholds.min_memory_headroom" => {
            config.thresholds.min_memory_headroom = value.parse()?;
        }
        _ => {
            return Err(anyhow::anyhow!(
                "Unknown metrics config key '{}'",
                key
            ));
        }
    }
    Ok(())
}
