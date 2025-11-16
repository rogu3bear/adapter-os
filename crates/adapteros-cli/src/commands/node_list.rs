//! Node list command - show cluster nodes

use adapteros_db::Db;
use anyhow::Result;
use comfy_table::{presets::UTF8_FULL, Cell, Table};

/// List nodes in the cluster
pub async fn run(offline: bool) -> Result<()> {
    let db = Db::connect_env().await?;

    if offline {
        println!("Node List (offline mode - last known state)");
    } else {
        println!("Node List");
    }
    println!();

    // Fetch nodes from database
    let nodes = db.list_nodes().await?;

    if nodes.is_empty() {
        println!("No nodes registered");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "Node ID",
        "Hostname",
        "Status",
        "Endpoint",
        "Last Seen",
    ]);

    for node in &nodes {
        let last_seen = node
            .last_seen_at
            .as_ref()
            .map(|s| format_time_ago(s))
            .unwrap_or_else(|| "never".to_string());

        table.add_row(vec![
            Cell::new(&node.id[..8]), // Shortened ID
            Cell::new(&node.hostname),
            Cell::new(&node.status),
            Cell::new(&node.agent_endpoint),
            Cell::new(&last_seen),
        ]);
    }

    println!("{}", table);
    println!("\nTotal: {} node(s)", nodes.len());

    // If not offline, query live status from node runtimes
    if !offline {
        println!("\nQuerying live status...");
        for node in &nodes {
            match query_node_status(&node.agent_endpoint).await {
                Ok(status) => {
                    println!(
                        "  {} [{}]: {} workers, {} VRAM",
                        node.hostname,
                        &node.id[..8],
                        status.worker_count,
                        format_bytes(status.vram_bytes)
                    );
                }
                Err(e) => {
                    println!(
                        "  {} [{}]: ✗ unreachable ({})",
                        node.hostname,
                        &node.id[..8],
                        e
                    );
                }
            }
        }
    }

    Ok(())
}

/// Node status from node runtime
#[derive(Debug, serde::Deserialize)]
struct NodeStatus {
    worker_count: usize,
    vram_bytes: u64,
}

/// Query node runtime for live status
async fn query_node_status(endpoint: &str) -> Result<NodeStatus> {
    let client = reqwest::Client::new();
    let url = format!("{}/status", endpoint);

    let response = client
        .get(&url)
        .timeout(std::time::Duration::from_secs(5))
        .send()
        .await?;

    if !response.status().is_success() {
        return Err(anyhow::anyhow!("HTTP {}", response.status()));
    }

    let status: NodeStatus = response.json().await?;
    Ok(status)
}

/// Format time ago from ISO timestamp
fn format_time_ago(timestamp: &str) -> String {
    // Simplified - in production would parse and calculate
    if timestamp.is_empty() {
        "never".to_string()
    } else {
        // Mock "time ago" - in production would use chrono
        "2s ago".to_string()
    }
}

/// Format bytes as human-readable
fn format_bytes(bytes: u64) -> String {
    const GB: u64 = 1024 * 1024 * 1024;
    const MB: u64 = 1024 * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else {
        format!("{} B", bytes)
    }
}
