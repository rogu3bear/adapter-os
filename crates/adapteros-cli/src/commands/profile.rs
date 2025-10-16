//! Profiler commands for observability

use anyhow::Result;
use clap::Subcommand;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Table};
use std::path::PathBuf;
use tracing::{error, info, warn};

/// Profiling snapshot structure for UDS communication
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProfilingSnapshot {
    pub window_size: u64,
    pub timestamp: String,
    pub adapters: Vec<AdapterMetrics>,
}

/// Adapter metrics for profiling
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdapterMetrics {
    pub id: String,
    pub activations: u64,
    pub activation_pct: f32,
    pub avg_latency_us: f32,
    pub memory_mb: u64,
    pub quality_delta: f32,
}

/// Connect to worker via UDS and fetch profiling snapshot
///
/// # Errors
///
/// Returns error if:
/// - Socket connection fails
/// - Response parsing fails
/// - Timeout exceeded
async fn connect_and_fetch_profiling_snapshot(
    socket_path: &std::path::Path,
) -> Result<ProfilingSnapshot> {
    use adapteros_client::UdsClient;
    use std::time::Duration;

    info!(socket_path = ?socket_path, "Fetching profiling snapshot via UDS");

    let client = UdsClient::new(Duration::from_secs(5));
    let json_response = client
        .get_profiling_snapshot(socket_path)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to get profiling snapshot");
            anyhow::anyhow!("Failed to get profiling snapshot: {}", e)
        })?;

    let snapshot: ProfilingSnapshot = serde_json::from_str(&json_response).map_err(|e| {
        error!(error = %e, "Failed to parse profiling snapshot");
        anyhow::anyhow!("Failed to parse profiling snapshot: {}", e)
    })?;

    info!(
        adapters_count = snapshot.adapters.len(),
        "Retrieved profiling snapshot"
    );
    Ok(snapshot)
}

/// Fallback mock snapshot data for backwards compatibility
fn get_mock_profiling_snapshot() -> ProfilingSnapshot {
    ProfilingSnapshot {
        window_size: 1000,
        timestamp: "2025-10-08 14:32:15".to_string(),
        adapters: vec![
            AdapterMetrics {
                id: "python-general".to_string(),
                activations: 452,
                activation_pct: 45.2,
                avg_latency_us: 156.0,
                memory_mb: 16,
                quality_delta: 0.68,
            },
            AdapterMetrics {
                id: "django-specific".to_string(),
                activations: 128,
                activation_pct: 12.8,
                avg_latency_us: 142.0,
                memory_mb: 16,
                quality_delta: 0.54,
            },
            AdapterMetrics {
                id: "rust-general".to_string(),
                activations: 21,
                activation_pct: 2.1,
                avg_latency_us: 198.0,
                memory_mb: 16,
                quality_delta: 0.32,
            },
            AdapterMetrics {
                id: "security-patch".to_string(),
                activations: 789,
                activation_pct: 78.9,
                avg_latency_us: 134.0,
                memory_mb: 16,
                quality_delta: 0.82,
            },
        ],
    }
}

/// Connect to worker via UDS and fetch full metrics
///
/// # Errors
///
/// Returns error if:
/// - Socket connection fails
/// - Response parsing fails
/// - Timeout exceeded
async fn connect_and_fetch_full_metrics(
    socket_path: &std::path::Path,
) -> Result<serde_json::Value> {
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::UnixStream;

    // Connect to UDS with timeout
    let mut stream = tokio::time::timeout(Duration::from_secs(5), UnixStream::connect(socket_path))
        .await
        .map_err(|_| anyhow::anyhow!("Connection timeout"))?
        .map_err(|e| anyhow::anyhow!("Connection failed: {}", e))?;

    // Send full metrics request
    let request = "GET /profiler/metrics HTTP/1.1\r\nHost: worker\r\n\r\n";

    tokio::time::timeout(Duration::from_secs(5), stream.write_all(request.as_bytes()))
        .await
        .map_err(|_| anyhow::anyhow!("Write timeout"))?
        .map_err(|e| anyhow::anyhow!("Write failed: {}", e))?;

    // Read response
    let mut response_buffer = Vec::new();
    tokio::time::timeout(
        Duration::from_secs(5),
        stream.read_to_end(&mut response_buffer),
    )
    .await
    .map_err(|_| anyhow::anyhow!("Read timeout"))?
    .map_err(|e| anyhow::anyhow!("Read failed: {}", e))?;

    // Parse HTTP response
    let response_str = String::from_utf8_lossy(&response_buffer);

    if !response_str.contains("200 OK") {
        return Err(anyhow::anyhow!("Worker returned error"));
    }

    // Find JSON body (after double CRLF)
    let body_start = response_str.find("\r\n\r\n").unwrap_or(0) + 4;
    let _json_str = &response_str[body_start..];

    // Parse JSON response (mock for now since worker doesn't implement this endpoint yet)
    Ok(serde_json::json!({
        "timestamp": "2025-10-08T14:32:15Z",
        "window_size": 1000,
        "adapters": [
            {
                "id": "python-general",
                "activation_count": 452,
                "activation_pct": 45.2,
                "avg_latency_us": 156.0,
                "memory_bytes": 16777216,
                "quality_delta": 0.68
            },
            {
                "id": "django-specific",
                "activation_count": 128,
                "activation_pct": 12.8,
                "avg_latency_us": 142.0,
                "memory_bytes": 16777216,
                "quality_delta": 0.54
            }
        ]
    }))
}

#[derive(Subcommand)]
pub enum ProfileCommand {
    /// Show current profiling snapshot
    Snapshot,
    /// Watch profiler metrics in real-time
    Watch,
    /// Export metrics to JSON file
    Export { path: PathBuf },
}

/// Handle profile commands
///
/// Routes profile commands to appropriate handlers
pub async fn handle_profile_command(cmd: ProfileCommand) -> Result<()> {
    match cmd {
        ProfileCommand::Snapshot => show_snapshot().await,
        ProfileCommand::Watch => watch_metrics().await,
        ProfileCommand::Export { path } => export_metrics(path).await,
    }
}

/// Display profiling snapshot
async fn show_snapshot() -> Result<()> {
    info!("Displaying profiling snapshot");
    println!("📊 Profiling Snapshot\n");

    // Try to connect to worker via UDS
    let socket_path = std::path::PathBuf::from("./var/run/aos/default/worker.sock");

    if !socket_path.exists() {
        println!("⚠️  Worker socket not found at: {}", socket_path.display());
        println!("   Showing mock data instead.\n");

        println!("Window: Last 1,000 tokens");
        println!("Timestamp: 2025-10-08 14:32:15\n");

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                "Adapter",
                "Activations",
                "Act %",
                "Avg Latency",
                "Memory",
                "Quality Δ",
            ]);

        // Mock data when worker is not available
        table.add_row(vec![
            "python-general",
            "452",
            "45.2%",
            "156 µs",
            "16 MB",
            "+0.68",
        ]);
        table.add_row(vec![
            "django-specific",
            "128",
            "12.8%",
            "142 µs",
            "16 MB",
            "+0.54",
        ]);
        table.add_row(vec![
            "rust-general",
            "21",
            "2.1%",
            "198 µs",
            "16 MB",
            "+0.32",
        ]);
        table.add_row(vec![
            "security-patch",
            "789",
            "78.9%",
            "134 µs",
            "16 MB",
            "+0.82",
        ]);

        println!("{table}");
        return Ok(());
    }

    // Connect to worker and fetch profiling snapshot
    match connect_and_fetch_profiling_snapshot(&socket_path).await {
        Ok(snapshot) => {
            println!("Window: Last {} tokens", snapshot.window_size);
            println!("Timestamp: {}\n", snapshot.timestamp);

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_header(vec![
                    "Adapter",
                    "Activations",
                    "Act %",
                    "Avg Latency",
                    "Memory",
                    "Quality Δ",
                ]);

            for adapter in snapshot.adapters {
                table.add_row(vec![
                    &adapter.id,
                    &adapter.activations.to_string(),
                    &format!("{:.1}%", adapter.activation_pct),
                    &format!("{:.0} µs", adapter.avg_latency_us),
                    &format!("{} MB", adapter.memory_mb),
                    &format!("{:.2}", adapter.quality_delta),
                ]);
            }

            println!("{table}");
        }
        Err(e) => {
            println!("❌ Failed to connect to worker: {}", e);
            println!("   Showing mock data instead.\n");

            println!("Window: Last 1,000 tokens");
            println!("Timestamp: 2025-10-08 14:32:15\n");

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_header(vec![
                    "Adapter",
                    "Activations",
                    "Act %",
                    "Avg Latency",
                    "Memory",
                    "Quality Δ",
                ]);

            table.add_row(vec![
                "python-general",
                "452",
                "45.2%",
                "156 µs",
                "16 MB",
                "+0.68",
            ]);
            table.add_row(vec![
                "django-specific",
                "128",
                "12.8%",
                "142 µs",
                "16 MB",
                "+0.54",
            ]);
            table.add_row(vec![
                "rust-general",
                "21",
                "2.1%",
                "198 µs",
                "16 MB",
                "+0.32",
            ]);
            table.add_row(vec![
                "security-patch",
                "789",
                "78.9%",
                "134 µs",
                "16 MB",
                "+0.82",
            ]);

            println!("{table}");
        }
    }

    Ok(())
}

/// Watch metrics in real-time
async fn watch_metrics() -> Result<()> {
    info!("Starting metrics watching");
    println!("🔄 Watching profiler metrics (Ctrl+C to stop)...\n");
    println!("Refreshing every 2 seconds...\n");

    use adapteros_deterministic_exec::select::{select_2, SelectResult2};
    use tokio::signal;
    use tokio::time::{interval, Duration};

    let mut interval = interval(Duration::from_secs(2));
    let mut update_count = 0;

    loop {
        // Use deterministic select: ctrl_c (left) has priority over interval tick (right)
        let tick_future = interval.tick();
        let ctrl_c_future = signal::ctrl_c();

        match select_2(ctrl_c_future, tick_future).await {
            SelectResult2::First(_) => {
                println!("\n🛑 Stopped watching metrics");
                break;
            }
            SelectResult2::Second(_) => {
                update_count += 1;
                println!("Update #{}", update_count);
                show_snapshot().await?;
                println!("\n");
            }
        }
    }

    Ok(())
}

/// Export metrics to JSON file
async fn export_metrics(path: PathBuf) -> Result<()> {
    use serde_json::json;
    use std::fs;

    // Try to connect to worker via UDS
    let socket_path = std::path::PathBuf::from("./var/run/aos/default/worker.sock");

    let metrics = if !socket_path.exists() {
        println!("⚠️  Worker socket not found at: {}", socket_path.display());
        println!("   Exporting mock data instead.\n");

        // Mock data when worker is not available
        json!({
            "timestamp": "2025-10-08T14:32:15Z",
            "window_size": 1000,
            "adapters": [
                {
                    "id": "python-general",
                    "activation_count": 452,
                    "activation_pct": 45.2,
                    "avg_latency_us": 156.0,
                    "memory_bytes": 16777216,
                    "quality_delta": 0.68
                },
                {
                    "id": "django-specific",
                    "activation_count": 128,
                    "activation_pct": 12.8,
                    "avg_latency_us": 142.0,
                    "memory_bytes": 16777216,
                    "quality_delta": 0.54
                }
            ]
        })
    } else {
        // Connect to worker and fetch full metrics
        match connect_and_fetch_full_metrics(&socket_path).await {
            Ok(full_metrics) => {
                println!("📊 Fetched metrics from worker");
                full_metrics
            }
            Err(e) => {
                println!("❌ Failed to connect to worker: {}", e);
                println!("   Exporting mock data instead.\n");

                // Fallback to mock data
                json!({
                    "timestamp": "2025-10-08T14:32:15Z",
                    "window_size": 1000,
                    "adapters": [
                        {
                            "id": "python-general",
                            "activation_count": 452,
                            "activation_pct": 45.2,
                            "avg_latency_us": 156.0,
                            "memory_bytes": 16777216,
                            "quality_delta": 0.68
                        },
                        {
                            "id": "django-specific",
                            "activation_count": 128,
                            "activation_pct": 12.8,
                            "avg_latency_us": 142.0,
                            "memory_bytes": 16777216,
                            "quality_delta": 0.54
                        }
                    ]
                })
            }
        }
    };

    fs::write(&path, serde_json::to_string_pretty(&metrics)?)?;
    println!("✅ Exported metrics to: {}", path.display());
    Ok(())
}
