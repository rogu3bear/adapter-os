//! System metrics CLI commands
//!
//! Provides CLI commands for managing system metrics, viewing health status,
//! and exporting metrics data.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use comfy_table::{Cell, Table};
use adapteros_system_metrics::{MetricsConfig, SystemMetricsCollector, SystemMetricsDb};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Parser)]
pub struct MetricsCommand {
    #[command(subcommand)]
    pub subcommand: MetricsSubcommand,
}

#[derive(Subcommand)]
pub enum MetricsSubcommand {
    /// Show current system metrics
    Show,
    /// Show metrics history
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
    /// Export metrics to file
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
    /// Check policy thresholds
    Check,
    /// Show threshold violations
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
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let load_avg = collector.load_average();

    if mode.is_json() {
        let response = serde_json::json!({
            "cpu_usage": metrics.cpu_usage,
            "memory_usage": metrics.memory_usage,
            "disk_usage": metrics.disk_io.usage_percent,
            "network_bandwidth": metrics.network_io.bandwidth_mbps,
            "gpu_utilization": metrics.gpu_metrics.utilization,
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
            Cell::new(format!("{:.1}", metrics.disk_io.usage_percent)),
            Cell::new("%"),
        ]);

        table.add_row(vec![
            Cell::new("Network Bandwidth"),
            Cell::new(format!("{:.2}", metrics.network_io.bandwidth_mbps)),
            Cell::new("Mbps"),
        ]);

        if let Some(gpu_util) = metrics.gpu_metrics.utilization {
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

async fn show_metrics_history(
    mode: &crate::output::OutputMode,
    hours: u32,
    limit: usize,
) -> Result<()> {
    // This would require database connection
    // For now, show a placeholder message
    if mode.is_json() {
        let response = serde_json::json!({
            "message": "Metrics history requires database connection",
            "hours": hours,
            "limit": limit
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        println!("Metrics history requires database connection");
        println!("Hours: {}, Limit: {}", hours, limit);
    }

    Ok(())
}

async fn show_health_status(mode: &crate::output::OutputMode) -> Result<()> {
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let policy = adapteros_system_metrics::SystemMetricsPolicy::new(
        adapteros_system_metrics::ThresholdsConfig::default(),
    );

    let health_status = policy.get_health_status(&metrics);
    let violations = policy.get_violations(&metrics);

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
            println!("✅ All checks passed");
        } else {
            println!("⚠️  {} violations detected:", violations.len());
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
    // This would require database connection and actual export logic
    // For now, show a placeholder message
    if mode.is_json() {
        let response = serde_json::json!({
            "message": "Metrics export requires database connection",
            "output": output.to_string_lossy(),
            "format": format,
            "hours": hours
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        println!("Metrics export requires database connection");
        println!("Output: {}", output.display());
        println!("Format: {}", format);
        println!("Hours: {}", hours);
    }

    Ok(())
}

async fn check_policy_thresholds(mode: &crate::output::OutputMode) -> Result<()> {
    let mut collector = SystemMetricsCollector::new();
    let metrics = collector.collect_metrics();
    let policy = adapteros_system_metrics::SystemMetricsPolicy::new(
        adapteros_system_metrics::ThresholdsConfig::default(),
    );

    let violations = policy.get_violations(&metrics);

    if mode.is_json() {
        let response = serde_json::json!({
            "thresholds": {
                "cpu_warning": 70.0,
                "cpu_critical": 90.0,
                "memory_warning": 80.0,
                "memory_critical": 95.0,
                "disk_warning": 85.0,
                "disk_critical": 95.0,
                "gpu_warning": 80.0,
                "gpu_critical": 95.0,
                "min_memory_headroom": 15.0
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
            println!("✅ All thresholds within limits");
        } else {
            println!("⚠️  Threshold violations detected:");
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
    // This would require database connection
    // For now, show a placeholder message
    if mode.is_json() {
        let response = serde_json::json!({
            "message": "Violations view requires database connection",
            "unresolved_only": unresolved
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        println!("Violations view requires database connection");
        println!("Unresolved only: {}", unresolved);
    }

    Ok(())
}

async fn list_config(mode: &crate::output::OutputMode) -> Result<()> {
    // This would require database connection
    // For now, show default configuration
    let default_config = MetricsConfig::default();

    if mode.is_json() {
        let response = serde_json::json!({
            "collection_interval_secs": default_config.collection_interval_secs,
            "sampling_rate": default_config.sampling_rate,
            "enable_gpu_metrics": default_config.enable_gpu_metrics,
            "enable_disk_metrics": default_config.enable_disk_metrics,
            "enable_network_metrics": default_config.enable_network_metrics,
            "retention_days": default_config.retention_days,
            "thresholds": {
                "cpu_warning": default_config.thresholds.cpu_warning,
                "cpu_critical": default_config.thresholds.cpu_critical,
                "memory_warning": default_config.thresholds.memory_warning,
                "memory_critical": default_config.thresholds.memory_critical,
                "disk_warning": default_config.thresholds.disk_warning,
                "disk_critical": default_config.thresholds.disk_critical,
                "gpu_warning": default_config.thresholds.gpu_warning,
                "gpu_critical": default_config.thresholds.gpu_critical,
                "min_memory_headroom": default_config.thresholds.min_memory_headroom
            }
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        let mut table = Table::new();
        table.set_header(vec!["Key", "Value"]);

        table.add_row(vec![
            Cell::new("collection_interval_secs"),
            Cell::new(format!("{}", default_config.collection_interval_secs)),
        ]);

        table.add_row(vec![
            Cell::new("sampling_rate"),
            Cell::new(format!("{:.3}", default_config.sampling_rate)),
        ]);

        table.add_row(vec![
            Cell::new("enable_gpu_metrics"),
            Cell::new(format!("{}", default_config.enable_gpu_metrics)),
        ]);

        table.add_row(vec![
            Cell::new("enable_disk_metrics"),
            Cell::new(format!("{}", default_config.enable_disk_metrics)),
        ]);

        table.add_row(vec![
            Cell::new("enable_network_metrics"),
            Cell::new(format!("{}", default_config.enable_network_metrics)),
        ]);

        table.add_row(vec![
            Cell::new("retention_days"),
            Cell::new(format!("{}", default_config.retention_days)),
        ]);

        println!("{}", table);
    }

    Ok(())
}

async fn set_config(mode: &crate::output::OutputMode, key: &str, value: &str) -> Result<()> {
    // This would require database connection
    // For now, show a placeholder message
    if mode.is_json() {
        let response = serde_json::json!({
            "message": "Config setting requires database connection",
            "key": key,
            "value": value
        });
        println!("{}", serde_json::to_string_pretty(&response)?);
    } else {
        println!("Config setting requires database connection");
        println!("Key: {}", key);
        println!("Value: {}", value);
    }

    Ok(())
}
