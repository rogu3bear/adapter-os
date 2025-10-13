//! Adapter lifecycle management commands

use anyhow::Result;
use clap::Subcommand;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Table};
use tracing::{info, error, warn};

/// Adapter state structure for UDS communication
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdapterState {
    pub id: String,
    pub vram_mb: u64,
    pub active: bool,
}

/// Adapter profile structure for UDS communication
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdapterProfile {
    pub state: String,
    pub activation_pct: f32,
    pub activations: u64,
    pub total_tokens: u64,
    pub avg_latency_us: f32,
    pub memory_kb: u64,
    pub quality_delta: f32,
    pub recent_activations: Vec<ActivationWindow>,
}

/// Activation window for profiling data
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActivationWindow {
    pub start_token: u64,
    pub end_token: u64,
    pub count: u64,
}

/// Connect to worker via UDS and fetch adapter states
///
/// # Errors
///
/// Returns error if:
/// - Socket connection fails
/// - Response parsing fails
/// - Timeout exceeded
async fn connect_and_fetch_adapter_states(
    socket_path: &std::path::Path,
) -> Result<Vec<AdapterState>> {
    use adapteros_client::UdsClient;
    use std::time::Duration;

    info!(socket_path = ?socket_path, "Fetching adapter states via UDS");

    let client = UdsClient::new(Duration::from_secs(5));
    let json_response = client
        .list_adapters(socket_path)
        .await
        .map_err(|e| {
            error!(error = %e, "Failed to list adapters via UDS");
            anyhow::anyhow!("Failed to list adapters: {}", e)
        })?;

    let adapters: Vec<AdapterState> = serde_json::from_str(&json_response)
        .map_err(|e| {
            error!(error = %e, "Failed to parse adapter response");
            anyhow::anyhow!("Failed to parse adapter response: {}", e)
        })?;

    info!(count = adapters.len(), "Retrieved adapter states");
    Ok(adapters)
}

/// Connect to worker via UDS and fetch adapter profile
///
/// # Errors
///
/// Returns error if:
/// - Socket connection fails
/// - Response parsing fails
/// - Adapter not found
async fn connect_and_fetch_adapter_profile(
    socket_path: &std::path::Path,
    adapter_id: &str,
) -> Result<AdapterProfile> {
    use adapteros_client::UdsClient;
    use std::time::Duration;

    info!(socket_path = ?socket_path, adapter_id = %adapter_id, "Fetching adapter profile via UDS");

    let client = UdsClient::new(Duration::from_secs(5));
    let json_response = client
        .get_adapter_profile(socket_path, adapter_id)
        .await
        .map_err(|e| {
            error!(error = %e, adapter_id = %adapter_id, "Failed to get adapter profile");
            anyhow::anyhow!("Failed to get adapter profile: {}", e)
        })?;

    let profile: AdapterProfile = serde_json::from_str(&json_response)
        .map_err(|e| {
            error!(error = %e, "Failed to parse adapter profile");
            anyhow::anyhow!("Failed to parse adapter profile: {}", e)
        })?;

    info!(adapter_id = %adapter_id, "Retrieved adapter profile");
    Ok(profile)
}

/// Send adapter command via UDS
///
/// # Errors
///
/// Returns error if:
/// - Socket connection fails
/// - Command execution fails
async fn send_adapter_command(
    socket_path: &std::path::Path,
    command: &str,
    adapter_id: &str,
) -> Result<()> {
    use adapteros_client::UdsClient;
    use std::time::Duration;

    info!(
        socket_path = ?socket_path,
        command = %command,
        adapter_id = %adapter_id,
        "Sending adapter command via UDS"
    );

    let client = UdsClient::new(Duration::from_secs(5));
    client
        .adapter_command(socket_path, adapter_id, command)
        .await
        .map_err(|e| {
            error!(error = %e, command = %command, adapter_id = %adapter_id, "Failed to send adapter command");
            anyhow::anyhow!("Failed to send adapter command: {}", e)
        })?;

    info!(command = %command, adapter_id = %adapter_id, "Adapter command sent successfully");
    Ok(())
}

#[derive(Debug, Subcommand)]
pub enum AdapterCommand {
    /// List all adapters with their states
    List,
    /// Show detailed metrics for an adapter
    Profile { adapter_id: String },
    /// Manually promote an adapter
    Promote { adapter_id: String },
    /// Manually demote an adapter
    Demote { adapter_id: String },
    /// Pin adapter to resident state
    Pin { adapter_id: String },
    /// Unpin adapter from resident state
    Unpin { adapter_id: String },
}

/// Handle adapter lifecycle commands
///
/// Routes adapter commands to appropriate handlers
pub async fn handle_adapter_command(cmd: AdapterCommand) -> Result<()> {
    info!(command = ?cmd, "Handling adapter command");
    
    match cmd {
        AdapterCommand::List => list_adapters().await,
        AdapterCommand::Profile { adapter_id } => profile_adapter(&adapter_id).await,
        AdapterCommand::Promote { adapter_id } => promote_adapter(&adapter_id).await,
        AdapterCommand::Demote { adapter_id } => demote_adapter(&adapter_id).await,
        AdapterCommand::Pin { adapter_id } => pin_adapter(&adapter_id).await,
        AdapterCommand::Unpin { adapter_id } => unpin_adapter(&adapter_id).await,
    }
}

/// List all adapters with their current states
async fn list_adapters() -> Result<()> {
    info!("Listing adapter lifecycle status");
    println!("📊 Adapter Lifecycle Status\n");

    // Try to connect to worker via UDS
    let socket_path = std::path::PathBuf::from("./var/run/aos/default/worker.sock");

    if !socket_path.exists() {
        println!("⚠️  Worker socket not found at: {}", socket_path.display());
        println!("   Showing mock data instead.\n");

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                "ID",
                "Name",
                "State",
                "Activation %",
                "Memory",
                "Pinned",
            ]);

        // Mock data when worker is not available
        table.add_row(vec!["0", "python-general", "hot", "45.2%", "16 MB", "no"]);
        table.add_row(vec!["1", "django-specific", "warm", "12.8%", "16 MB", "no"]);
        table.add_row(vec!["2", "rust-general", "cold", "2.1%", "16 MB", "no"]);
        table.add_row(vec![
            "3",
            "security-patch",
            "resident",
            "78.9%",
            "16 MB",
            "yes",
        ]);

        println!("{table}");
        return Ok(());
    }

    // Connect to worker and fetch adapter states
    match connect_and_fetch_adapter_states(&socket_path).await {
        Ok(adapters) => {
            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_header(vec![
                    "ID",
                    "Name",
                    "State",
                    "Activation %",
                    "Memory",
                    "Pinned",
                ]);

            for adapter in adapters {
                let state = if adapter.active { "active" } else { "staged" };
                let pinned = if adapter.vram_mb > 20 { "yes" } else { "no" }; // Mock pinned logic
                table.add_row(vec![
                    &adapter.id,
                    &adapter.id, // Use ID as name for now
                    state,
                    "N/A", // Activation % would come from profiler
                    &format!("{} MB", adapter.vram_mb),
                    pinned,
                ]);
            }

            println!("{table}");
        }
        Err(e) => {
            println!("❌ Failed to connect to worker: {}", e);
            println!("   Showing mock data instead.\n");

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_header(vec![
                    "ID",
                    "Name",
                    "State",
                    "Activation %",
                    "Memory",
                    "Pinned",
                ]);

            table.add_row(vec!["0", "python-general", "hot", "45.2%", "16 MB", "no"]);
            table.add_row(vec!["1", "django-specific", "warm", "12.8%", "16 MB", "no"]);
            table.add_row(vec!["2", "rust-general", "cold", "2.1%", "16 MB", "no"]);
            table.add_row(vec![
                "3",
                "security-patch",
                "resident",
                "78.9%",
                "16 MB",
                "yes",
            ]);

            println!("{table}");
        }
    }

    Ok(())
}

/// Display detailed profile for an adapter
async fn profile_adapter(adapter_id: &str) -> Result<()> {
    info!(adapter_id = %adapter_id, "Profiling adapter");
    println!("📈 Adapter Profile: {}\n", adapter_id);

    // Try to connect to worker via UDS
    let socket_path = std::path::PathBuf::from("./var/run/aos/default/worker.sock");

    if !socket_path.exists() {
        println!("⚠️  Worker socket not found at: {}", socket_path.display());
        println!("   Showing mock data instead.\n");

        println!("State:           hot");
        println!("Activation:      45.2% (1,234 / 2,730 tokens)");
        println!("Avg Latency:     156.2 µs");
        println!("Memory Usage:    16,384 KB");
        println!("Quality Delta:   +0.68");
        println!("\nLast 10 activations:");
        println!("  Token 100-150:  3 activations");
        println!("  Token 150-200:  5 activations");
        println!("  Token 200-250:  2 activations");
        return Ok(());
    }

    // Connect to worker and fetch adapter profile
    match connect_and_fetch_adapter_profile(&socket_path, adapter_id).await {
        Ok(profile) => {
            println!("State:           {}", profile.state);
            println!(
                "Activation:      {:.1}% ({} / {} tokens)",
                profile.activation_pct, profile.activations, profile.total_tokens
            );
            println!("Avg Latency:     {:.1} µs", profile.avg_latency_us);
            println!("Memory Usage:    {} KB", profile.memory_kb);
            println!("Quality Delta:   {:.2}", profile.quality_delta);
            println!("\nLast 10 activations:");
            for activation in &profile.recent_activations {
                println!(
                    "  Token {}-{}:  {} activations",
                    activation.start_token, activation.end_token, activation.count
                );
            }
        }
        Err(e) => {
            println!("❌ Failed to connect to worker: {}", e);
            println!("   Showing mock data instead.\n");

            println!("State:           hot");
            println!("Activation:      45.2% (1,234 / 2,730 tokens)");
            println!("Avg Latency:     156.2 µs");
            println!("Memory Usage:    16,384 KB");
            println!("Quality Delta:   +0.68");
            println!("\nLast 10 activations:");
            println!("  Token 100-150:  3 activations");
            println!("  Token 150-200:  5 activations");
            println!("  Token 200-250:  2 activations");
        }
    }

    Ok(())
}

/// Promote adapter to higher priority state
async fn promote_adapter(adapter_id: &str) -> Result<()> {
    info!(adapter_id = %adapter_id, "Promoting adapter");
    let socket_path = std::path::PathBuf::from("./var/run/aos/default/worker.sock");

    if !socket_path.exists() {
        println!("⚠️  Worker socket not found at: {}", socket_path.display());
        println!("✅ Promoted adapter: {} (mock)", adapter_id);
        println!("   State: warm → hot");
        return Ok(());
    }

    match send_adapter_command(&socket_path, "promote", adapter_id).await {
        Ok(_) => {
            println!("✅ Promoted adapter: {}", adapter_id);
            println!("   State: warm → hot");
        }
        Err(e) => {
            println!("❌ Failed to promote adapter: {}", e);
        }
    }

    Ok(())
}

/// Demote adapter to lower priority state
async fn demote_adapter(adapter_id: &str) -> Result<()> {
    info!(adapter_id = %adapter_id, "Demoting adapter");
    let socket_path = std::path::PathBuf::from("./var/run/aos/default/worker.sock");

    if !socket_path.exists() {
        println!("⚠️  Worker socket not found at: {}", socket_path.display());
        println!("⬇️  Demoted adapter: {} (mock)", adapter_id);
        println!("   State: hot → warm");
        return Ok(());
    }

    match send_adapter_command(&socket_path, "demote", adapter_id).await {
        Ok(_) => {
            println!("⬇️  Demoted adapter: {}", adapter_id);
            println!("   State: hot → warm");
        }
        Err(e) => {
            println!("❌ Failed to demote adapter: {}", e);
        }
    }

    Ok(())
}

/// Pin adapter to resident state (prevent eviction)
async fn pin_adapter(adapter_id: &str) -> Result<()> {
    info!(adapter_id = %adapter_id, "Pinning adapter");
    let socket_path = std::path::PathBuf::from("./var/run/aos/default/worker.sock");

    if !socket_path.exists() {
        println!("⚠️  Worker socket not found at: {}", socket_path.display());
        println!("📌 Pinned adapter: {} (mock)", adapter_id);
        println!("   State: → resident (pinned)");
        return Ok(());
    }

    match send_adapter_command(&socket_path, "pin", adapter_id).await {
        Ok(_) => {
            println!("📌 Pinned adapter: {}", adapter_id);
            println!("   State: → resident (pinned)");
        }
        Err(e) => {
            println!("❌ Failed to pin adapter: {}", e);
        }
    }

    Ok(())
}

/// Unpin adapter (allow eviction)
async fn unpin_adapter(adapter_id: &str) -> Result<()> {
    info!(adapter_id = %adapter_id, "Unpinning adapter");
    let socket_path = std::path::PathBuf::from("./var/run/aos/default/worker.sock");

    if !socket_path.exists() {
        println!("⚠️  Worker socket not found at: {}", socket_path.display());
        println!("📍 Unpinned adapter: {} (mock)", adapter_id);
        println!("   Adapter can now be demoted");
        return Ok(());
    }

    match send_adapter_command(&socket_path, "unpin", adapter_id).await {
        Ok(_) => {
            println!("📍 Unpinned adapter: {}", adapter_id);
            println!("   Adapter can now be demoted");
        }
        Err(e) => {
            println!("❌ Failed to unpin adapter: {}", e);
        }
    }

    Ok(())
}
