//! Adapter lifecycle management commands

use anyhow::Result;
use clap::Subcommand;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Table};

// UDS communication helper functions

/// Adapter state structure for UDS communication
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AdapterState {
    id: String,
    vram_mb: u64,
    active: bool,
}

/// Adapter profile structure for UDS communication
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AdapterProfile {
    state: String,
    activation_pct: f32,
    activations: u64,
    total_tokens: u64,
    avg_latency_us: f32,
    memory_kb: u64,
    quality_delta: f32,
    recent_activations: Vec<ActivationWindow>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ActivationWindow {
    start_token: u64,
    end_token: u64,
    count: u64,
}

/// Connect to worker via UDS and fetch adapter states
async fn connect_and_fetch_adapter_states(
    socket_path: &std::path::Path,
) -> Result<Vec<AdapterState>> {
    use adapteros_client::UdsClient;
    use std::time::Duration;

    let client = UdsClient::new(Duration::from_secs(5));
    let json_response = client
        .list_adapters(socket_path)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to list adapters: {}", e))?;

    let adapters: Vec<AdapterState> =
        serde_json::from_str(&json_response).unwrap_or_else(|_| vec![]);

    Ok(adapters)
}

/// Connect to worker via UDS and fetch adapter profile
async fn connect_and_fetch_adapter_profile(
    socket_path: &std::path::Path,
    adapter_id: &str,
) -> Result<AdapterProfile> {
    use adapteros_client::UdsClient;
    use std::time::Duration;

    let client = UdsClient::new(Duration::from_secs(5));
    let json_response = client
        .get_adapter_profile(socket_path, adapter_id)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to get adapter profile: {}", e))?;

    let profile: AdapterProfile = serde_json::from_str(&json_response)?;

    Ok(profile)
}

/// Send adapter command via UDS
async fn send_adapter_command(
    socket_path: &std::path::Path,
    command: &str,
    adapter_id: &str,
) -> Result<()> {
    use adapteros_client::UdsClient;
    use std::time::Duration;

    let client = UdsClient::new(Duration::from_secs(5));
    client
        .adapter_command(socket_path, adapter_id, command)
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send adapter command: {}", e))?;

    Ok(())
}

#[derive(Subcommand)]
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

pub async fn handle_adapter_command(cmd: AdapterCommand) -> Result<()> {
    match cmd {
        AdapterCommand::List => list_adapters().await,
        AdapterCommand::Profile { adapter_id } => profile_adapter(&adapter_id).await,
        AdapterCommand::Promote { adapter_id } => promote_adapter(&adapter_id).await,
        AdapterCommand::Demote { adapter_id } => demote_adapter(&adapter_id).await,
        AdapterCommand::Pin { adapter_id } => pin_adapter(&adapter_id).await,
        AdapterCommand::Unpin { adapter_id } => unpin_adapter(&adapter_id).await,
    }
}

async fn list_adapters() -> Result<()> {
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

async fn profile_adapter(adapter_id: &str) -> Result<()> {
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

async fn promote_adapter(adapter_id: &str) -> Result<()> {
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

async fn demote_adapter(adapter_id: &str) -> Result<()> {
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

async fn pin_adapter(adapter_id: &str) -> Result<()> {
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

async fn unpin_adapter(adapter_id: &str) -> Result<()> {
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
