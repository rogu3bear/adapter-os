//! System status commands for aosctl.
//!
//! Provides:
//! - `aosctl status adapters` – adapter activity and memory usage
//! - `aosctl status cluster`  – node list and health
//! - `aosctl status tick`     – global tick ledger summary
//! - `aosctl status memory`   – host memory usage and headroom

use crate::output::OutputWriter;
use adapteros_db::{sqlx, Db};
use anyhow::Result;
use clap::{Args, Subcommand};
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use serde::Serialize;
use sysinfo::System;

/// Top-level `status` command.
#[derive(Debug, Args, Clone)]
pub struct StatusCommand {
    #[command(subcommand)]
    pub subcommand: StatusSubcommand,
}

/// Subcommands under `aosctl status`.
#[derive(Debug, Subcommand, Clone)]
pub enum StatusSubcommand {
    /// Show adapter activity, pinning, and memory usage
    Adapters,

    /// Show cluster nodes and basic health
    Cluster,

    /// Show latest tick and last divergence from tick ledger
    Tick,

    /// Show host memory usage and headroom
    Memory,
}

/// Dispatch the selected status subcommand.
pub async fn run(cmd: StatusCommand, output: &OutputWriter) -> Result<()> {
    match cmd.subcommand {
        StatusSubcommand::Adapters => adapters_status(output).await,
        StatusSubcommand::Cluster => cluster_status(output).await,
        StatusSubcommand::Tick => tick_status(output).await,
        StatusSubcommand::Memory => memory_status(output),
    }
}

// ---------------------------------------------------------------------------
// status adapters
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct AdapterStatus {
    name: String,
    tenant_id: String,
    active: bool,
    pinned: bool,
    expires_at: Option<String>,
    memory_bytes: i64,
}

async fn adapters_status(output: &OutputWriter) -> Result<()> {
    let db = Db::connect_env().await?;
    let adapters = db.list_adapters().await?;

    if adapters.is_empty() {
        output.warning("No adapters found in database");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec![
        "Name",
        "Tenant",
        "Active",
        "Pinned",
        "Expires At",
        "Memory",
    ]);

    let mut rows = Vec::new();

    for adapter in adapters {
        let name = adapter
            .adapter_id
            .clone()
            .unwrap_or_else(|| adapter.name.clone());

        let active = adapter.active != 0;
        let pinned = adapter.pinned != 0;
        let mem_bytes = adapter.memory_bytes;

        let expires_at = adapter.expires_at.clone();

        rows.push(AdapterStatus {
            name: name.clone(),
            tenant_id: adapter.tenant_id.clone(),
            active,
            pinned,
            expires_at: expires_at.clone(),
            memory_bytes: mem_bytes,
        });

        table.add_row(vec![
            Cell::new(name),
            Cell::new(adapter.tenant_id),
            Cell::new(if active { "yes" } else { "no" }),
            Cell::new(if pinned { "yes" } else { "no" }),
            Cell::new(expires_at.unwrap_or_else(|| "-".to_string())),
            Cell::new(format_bytes(mem_bytes)),
        ]);
    }

    output.section("Adapter status");
    output.table(&table as &dyn std::fmt::Display, Some(&rows))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// status cluster
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ClusterNodeStatus {
    id: String,
    hostname: String,
    status: String,
    last_seen_at: Option<String>,
}

async fn cluster_status(output: &OutputWriter) -> Result<()> {
    let db = Db::connect_env().await?;
    let nodes = db.list_nodes().await?;

    if nodes.is_empty() {
        output.warning("No cluster nodes registered");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Node ID", "Hostname", "Status", "Last Heartbeat"]);

    let mut rows = Vec::new();

    for node in nodes {
        let last_seen = node.last_seen_at.clone();

        rows.push(ClusterNodeStatus {
            id: node.id.clone(),
            hostname: node.hostname.clone(),
            status: node.status.clone(),
            last_seen_at: last_seen.clone(),
        });

        table.add_row(vec![
            Cell::new(&node.id[..8.min(node.id.len())]),
            Cell::new(node.hostname),
            Cell::new(node.status),
            Cell::new(last_seen.unwrap_or_else(|| "never".to_string())),
        ]);
    }

    output.section("Cluster nodes");
    output.table(&table as &dyn std::fmt::Display, Some(&rows))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// status tick
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct TickStatus {
    latest_tick: Option<i64>,
    latest_tenant_id: Option<String>,
    latest_host_id: Option<String>,
    latest_event_type: Option<String>,
    last_divergence: Option<TickDivergence>,
}

#[derive(Debug, Serialize)]
struct TickDivergence {
    tenant_id: String,
    host_a: String,
    host_b: String,
    tick_range_start: i64,
    tick_range_end: i64,
    divergence_count: i64,
    created_at: String,
}

async fn tick_status(output: &OutputWriter) -> Result<()> {
    let db = Db::connect_env().await?;
    let pool = db.pool();

    #[derive(sqlx::FromRow)]
    struct LatestEntry {
        tick: i64,
        tenant_id: String,
        host_id: String,
        event_type: String,
    }

    let latest: Option<LatestEntry> = sqlx::query_as(
        r#"
        SELECT tick, tenant_id, host_id, event_type
        FROM tick_ledger_entries
        ORDER BY tick DESC, timestamp_us DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    #[derive(sqlx::FromRow)]
    struct DivergenceRow {
        tenant_id: String,
        host_a: String,
        host_b: String,
        tick_range_start: i64,
        tick_range_end: i64,
        divergence_count: i64,
        created_at: String,
    }

    let last_divergence_row: Option<DivergenceRow> = sqlx::query_as(
        r#"
        SELECT tenant_id, host_a, host_b, tick_range_start, tick_range_end,
               divergence_count, created_at
        FROM tick_ledger_consistency_reports
        WHERE divergence_count > 0 OR consistent = 0
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await?;

    let status = TickStatus {
        latest_tick: latest.as_ref().map(|e| e.tick),
        latest_tenant_id: latest.as_ref().map(|e| e.tenant_id.clone()),
        latest_host_id: latest.as_ref().map(|e| e.host_id.clone()),
        latest_event_type: latest.as_ref().map(|e| e.event_type.clone()),
        last_divergence: last_divergence_row.map(|d| TickDivergence {
            tenant_id: d.tenant_id,
            host_a: d.host_a,
            host_b: d.host_b,
            tick_range_start: d.tick_range_start,
            tick_range_end: d.tick_range_end,
            divergence_count: d.divergence_count,
            created_at: d.created_at,
        }),
    };

    if output.is_json() {
        output.json(&status)?;
    } else {
        output.section("Tick ledger");
        match &status.latest_tick {
            Some(tick) => {
                output.kv("Latest tick", &tick.to_string());
                if let Some(tenant) = &status.latest_tenant_id {
                    output.kv("Latest tenant", tenant);
                }
                if let Some(host) = &status.latest_host_id {
                    output.kv("Latest host", host);
                }
                if let Some(event_type) = &status.latest_event_type {
                    output.kv("Latest event", event_type);
                }
            }
            None => {
                output.info("No tick ledger entries found");
            }
        }

        if let Some(div) = &status.last_divergence {
            output.section("Last divergence");
            output.kv("Tenant", &div.tenant_id);
            output.kv("Hosts", &format!("{} vs {}", div.host_a, div.host_b));
            output.kv(
                "Tick range",
                &format!("{}-{}", div.tick_range_start, div.tick_range_end),
            );
            output.kv("Divergences", &div.divergence_count.to_string());
            output.kv("Created at", &div.created_at);
        } else {
            output.info("No divergences recorded in tick ledger");
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// status memory
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct MemoryStatus {
    total_bytes: u64,
    used_bytes: u64,
    used_percent: f64,
    headroom_percent: f64,
}

fn memory_status(output: &OutputWriter) -> Result<()> {
    let mut sys = System::new();
    sys.refresh_memory();

    // In sysinfo 0.30, memory is already in bytes
    let total_bytes = sys.total_memory();
    let used_bytes = sys.used_memory();

    let used_percent = if total_bytes > 0 {
        (used_bytes as f64) * 100.0 / (total_bytes as f64)
    } else {
        0.0
    };
    let headroom_percent = 100.0 - used_percent;

    let status = MemoryStatus {
        total_bytes,
        used_bytes,
        used_percent,
        headroom_percent,
    };

    if output.is_json() {
        output.json(&status)?;
    } else {
        output.section("Host memory");
        output.kv("Total", &format_bytes(total_bytes as i64));
        output.kv("Used", &format_bytes(used_bytes as i64));
        output.kv("Used %", &format!("{:.1}%", used_percent));
        output.kv("Headroom %", &format!("{:.1}%", headroom_percent));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn format_bytes(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    let b = bytes.max(0) as f64;
    if b >= GB {
        format!("{:.2} GiB", b / GB)
    } else if b >= MB {
        format!("{:.2} MiB", b / MB)
    } else if b >= KB {
        format!("{:.2} KiB", b / KB)
    } else {
        format!("{} B", bytes.max(0))
    }
}
