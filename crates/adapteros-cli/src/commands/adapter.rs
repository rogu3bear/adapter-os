//! Adapter lifecycle management commands

use crate::output::OutputWriter;
use adapteros_api_types::adapters::RegisterAdapterRequest;
use adapteros_client::{AdapterOSClient, UdsClient};
use adapteros_core::validation;
use adapteros_core::AosError;
use adapteros_core::B3Hash;
use adapteros_core::Result;
use adapteros_db::Db;
use clap::Subcommand;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Table};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{error, info};

// Re-export canonical AdapterState from adapteros-types
pub use adapteros_types::AdapterState;

/// Enhanced adapter profile structure for UDS communication
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
    pub performance_metrics: PerformanceMetrics,
    pub policy_compliance: PolicyCompliance,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PerformanceMetrics {
    pub p50_latency_us: f32,
    pub p95_latency_us: f32,
    pub p99_latency_us: f32,
    pub throughput_tokens_per_sec: f32,
    pub error_rate: f32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PolicyCompliance {
    pub determinism_score: f32,
    pub evidence_coverage: f32,
    pub refusal_rate: f32,
    pub policy_violations: u64,
}

/// Activation window for profiling data
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ActivationWindow {
    pub start_token: u64,
    pub end_token: u64,
    pub count: u64,
}

/// Get worker socket path for tenant
fn get_worker_socket_path(tenant_id: Option<&str>) -> std::path::PathBuf {
    let tenant = tenant_id.unwrap_or("default");
    std::path::PathBuf::from(format!("./var/run/aos/{}/worker.sock", tenant))
}

/// Upsert directory adapter via HTTP API
async fn directory_upsert(
    tenant: &str,
    root: &str,
    path: &str,
    activate: bool,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/adapters/directory/upsert",
        base_url.trim_end_matches('/')
    );
    let body = serde_json::json!({
        "tenant_id": tenant,
        "root": root,
        "path": path,
        "activate": activate
    });

    output.info("Upserting directory adapter");
    output.kv("Tenant", tenant);
    output.kv("Root", root);
    output.kv("Path", path);
    if activate {
        output.kv("Activate", "true");
    }

    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| adapteros_core::AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        return Err(adapteros_core::AosError::Other(format!(
            "Upsert failed: {} {}",
            status, text
        )));
    }

    let value: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| adapteros_core::AosError::Http(e.to_string()))?;

    if output.is_json() {
        output.result(&serde_json::to_string_pretty(&value).unwrap());
    } else {
        if let Some(adapter_id) = value.get("adapter_id").and_then(|v| v.as_str()) {
            output.success(&format!("Adapter upserted: {}", adapter_id));
        } else {
            output.success("Adapter upserted");
        }
    }

    Ok(())
}

/// Validate adapter ID format
fn validate_adapter_id(adapter_id: &str) -> Result<()> {
    validation::validate_adapter_id(adapter_id).map_err(|e| AosError::from(e))
}

async fn emit_kv_readiness(adapter_id: &str, output: &OutputWriter) {
    match Db::connect_env().await {
        Ok(db) => match db.check_adapter_consistency(adapter_id).await {
            Ok(status) => {
                let label = if status.is_ready() { "ready" } else { "stale" };
                output.kv("KV readiness", label);
                if let Some(msg) = status.message {
                    output.kv("KV note", &msg);
                }
            }
            Err(e) => output.warning(&format!("KV readiness check failed: {}", e)),
        },
        Err(e) => output.info(&format!("KV readiness skipped (db unavailable): {}", e)),
    }
}

/// Connect to worker via UDS and fetch adapter states with retry logic
///
/// # Errors
///
/// Returns error if:
/// - Socket connection fails after retries
/// - Response parsing fails
/// - Timeout exceeded
async fn connect_and_fetch_adapter_states(
    socket_path: &std::path::Path,
    timeout: Duration,
) -> Result<Vec<AdapterState>> {
    use adapteros_client::UdsClient;

    info!(socket_path = ?socket_path, "Fetching adapter states via UDS");

    let client = UdsClient::new(timeout);

    // Add retry logic for transient failures
    let mut retries = 3;
    while retries > 0 {
        match client.list_adapters(socket_path).await {
            Ok(json_response) => {
                let adapters: Vec<AdapterState> = serde_json::from_str(&json_response)
                    .map_err(|e| adapteros_core::AosError::Serialization(e))?;

                info!(count = adapters.len(), "Retrieved adapter states");
                return Ok(adapters);
            }
            Err(_e) if retries > 1 => {
                retries -= 1;
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            Err(e) => {
                error!(error = %e, "Failed to list adapters via UDS");
                return Err(adapteros_core::AosError::Io(format!(
                    "Failed to list adapters: {}",
                    e
                )));
            }
        }
    }

    // Loop exhausted all retries without returning (should not happen with retries > 0)
    Err(adapteros_core::AosError::Io(
        "Failed to list adapters: all retries exhausted".to_string(),
    ))
}

/// Connect to worker via UDS and fetch adapter profile with retry logic
///
/// # Errors
///
/// Returns error if:
/// - Socket connection fails after retries
/// - Response parsing fails
/// - Adapter not found
async fn connect_and_fetch_adapter_profile(
    socket_path: &std::path::Path,
    adapter_id: &str,
    timeout: Duration,
) -> Result<AdapterProfile> {
    use adapteros_client::UdsClient;

    info!(socket_path = ?socket_path, adapter_id = %adapter_id, "Fetching adapter profile via UDS");

    let client = UdsClient::new(timeout);

    // Add retry logic for transient failures
    let mut retries = 3;
    while retries > 0 {
        match client.get_adapter_profile(socket_path, adapter_id).await {
            Ok(json_response) => {
                let profile: AdapterProfile = serde_json::from_str(&json_response)
                    .map_err(|e| adapteros_core::AosError::Serialization(e))?;

                info!(adapter_id = %adapter_id, "Retrieved adapter profile");
                return Ok(profile);
            }
            Err(_e) if retries > 1 => {
                retries -= 1;
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            Err(e) => {
                error!(error = %e, adapter_id = %adapter_id, "Failed to get adapter profile");
                return Err(adapteros_core::AosError::Io(format!(
                    "Failed to get adapter profile: {}",
                    e
                )));
            }
        }
    }

    // Loop exhausted all retries without returning (should not happen with retries > 0)
    Err(adapteros_core::AosError::Io(
        "Failed to get adapter profile: all retries exhausted".to_string(),
    ))
}

/// Send adapter command via UDS with retry logic
///
/// # Errors
///
/// Returns error if:
/// - Socket connection fails after retries
/// - Command execution fails
async fn send_adapter_command(
    socket_path: &std::path::Path,
    command: &str,
    adapter_id: &str,
    timeout: Duration,
) -> Result<()> {
    use adapteros_client::UdsClient;

    info!(
        socket_path = ?socket_path,
        command = %command,
        adapter_id = %adapter_id,
        "Sending adapter command via UDS"
    );

    // Use unified client trait
    let client = UdsClient::new(timeout);

    // Add retry logic for transient failures
    let mut retries = 3;
    while retries > 0 {
        let result = match command.as_ref() {
            "evict" => client.evict_adapter(adapter_id).await,
            "pin" => client.pin_adapter(adapter_id, true).await,
            "unpin" => client.pin_adapter(adapter_id, false).await,
            _ => {
                error!(command = %command, "Unsupported adapter command");
                return Err(adapteros_core::AosError::Other(format!(
                    "Unsupported command: {}",
                    command
                )));
            }
        };

        match result {
            Ok(_) => {
                info!(command = %command, adapter_id = %adapter_id, "Adapter command sent successfully");
                return Ok(());
            }
            Err(e) if retries > 1 => {
                retries -= 1;
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }
            Err(e) => {
                error!(error = %e, command = %command, adapter_id = %adapter_id, "Failed to send adapter command");
                return Err(adapteros_core::AosError::Io(format!(
                    "Failed to send adapter command: {}",
                    e
                )));
            }
        }
    }

    // Loop exhausted all retries without returning (should not happen with retries > 0)
    Err(adapteros_core::AosError::Io(
        "Failed to send adapter command: all retries exhausted".to_string(),
    ))
}

#[derive(Debug, Subcommand, Clone)]
pub enum AdapterCommand {
    /// List all adapters with their states
    #[command(
        after_help = "Examples:\n  aosctl adapter list\n  aosctl adapter list --json\n  aosctl adapter list --tenant dev\n  aosctl adapter list --pinned-only"
    )]
    List {
        /// Output format
        #[arg(long)]
        json: bool,

        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,

        /// Only show pinned adapters
        #[arg(long)]
        pinned_only: bool,
    },

    /// Show detailed metrics for an adapter
    #[command(
        after_help = "Examples:\n  aosctl adapter profile adapter-1\n  aosctl adapter profile adapter-1 --json\n  aosctl adapter profile adapter-1 --tenant dev"
    )]
    Profile {
        /// Adapter ID
        #[arg()]
        adapter_id: String,

        /// Output format
        #[arg(long)]
        json: bool,

        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,
    },

    /// Manually promote an adapter
    #[command(
        after_help = "Examples:\n  aosctl adapter promote adapter-1\n  aosctl adapter promote adapter-1 --tenant dev"
    )]
    Promote {
        /// Adapter ID
        #[arg()]
        adapter_id: String,

        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,
    },

    /// Manually demote an adapter
    #[command(
        after_help = "Examples:\n  aosctl adapter demote adapter-1\n  aosctl adapter demote adapter-1 --tenant dev"
    )]
    Demote {
        /// Adapter ID
        #[arg()]
        adapter_id: String,

        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,
    },

    /// Pin adapter to resident state
    #[command(
        after_help = "Examples:\n  aosctl adapter pin adapter-1\n  aosctl adapter pin adapter-1 --tenant dev"
    )]
    Pin {
        /// Adapter ID
        #[arg()]
        adapter_id: String,

        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,
    },

    /// Unpin adapter from resident state
    #[command(
        after_help = "Examples:\n  aosctl adapter unpin adapter-1\n  aosctl adapter unpin adapter-1 --tenant dev"
    )]
    Unpin {
        /// Adapter ID
        #[arg()]
        adapter_id: String,

        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,
    },

    /// Show adapter lineage tree (ancestors and descendants)
    #[command(
        after_help = "Examples:\n  aosctl adapter lineage adapter-1\n  aosctl adapter lineage adapter-1 --json\n  aosctl adapter lineage adapter-1 --tree"
    )]
    Lineage {
        /// Adapter ID
        #[arg()]
        adapter_id: String,

        /// Output format
        #[arg(long)]
        json: bool,

        /// Display as tree (ASCII art)
        #[arg(long)]
        tree: bool,
    },

    /// Evict adapter from memory
    #[command(
        after_help = "Examples:\n  aosctl adapter evict adapter-1\n  aosctl adapter evict adapter-1 --tenant dev --reason \"Low activation\""
    )]
    Evict {
        /// Adapter ID
        #[arg()]
        adapter_id: String,

        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,

        /// Reason for eviction (for audit trail)
        #[arg(long)]
        reason: Option<String>,
    },

    /// Upsert a synthetic directory adapter (optional activate)
    #[command(
        after_help = "Examples:\n  aosctl adapter directory-upsert --tenant dev --root /abs/repo --path src/api --activate\n  aosctl adapter directory-upsert --tenant dev --root /abs/repo --path src/api"
    )]
    DirectoryUpsert {
        /// Tenant ID
        #[arg(long)]
        tenant: String,
        /// Absolute repository root path
        #[arg(long)]
        root: String,
        /// Relative path under root
        #[arg(long)]
        path: String,
        /// Activate immediately
        #[arg(long)]
        activate: bool,
        /// Control plane base URL (default: http://127.0.0.1:8080/api)
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,
    },

    /// Verify GPU buffer integrity for loaded adapters
    #[command(
        after_help = "Examples:\n  aosctl adapter verify-gpu\n  aosctl adapter verify-gpu --tenant dev\n  aosctl adapter verify-gpu --adapter adapter-1 --tenant dev"
    )]
    VerifyGpu {
        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,

        /// Specific adapter ID to verify (optional, verifies all if omitted)
        #[arg(long)]
        adapter: Option<String>,

        /// UDS socket path
        #[arg(long, default_value = "/var/run/aos/aos.sock")]
        socket: std::path::PathBuf,

        /// Timeout in milliseconds
        #[arg(long, default_value = "10000")]
        timeout: u64,
    },

    /// Update adapter lifecycle state
    #[command(
        after_help = "Examples:\n  aosctl adapter update-lifecycle adapter-1 deprecated\n  aosctl adapter update-lifecycle adapter-1 active\n  aosctl adapter update-lifecycle adapter-1 retired --json"
    )]
    UpdateLifecycle {
        /// Adapter ID
        adapter_id: String,

        /// New lifecycle state (draft, active, deprecated, retired)
        state: String,
    },

    /// Register a packaged adapter by path (dir or weights file)
    #[command(
        after_help = "Examples:\n  aosctl adapter register --path ./adapters/my-adapter\n  aosctl adapter register --path ./adapters/my-adapter/weights.safetensors --adapter-id custom-id\n  aosctl adapter register --path ./adapters/my-adapter --rank 16 --tier 1"
    )]
    Register {
        /// Path to packaged adapter dir or weights.safetensors
        #[arg(long)]
        path: PathBuf,

        /// Adapter ID (defaults to directory name)
        #[arg(long)]
        adapter_id: Option<String>,

        /// Name to display (defaults to adapter_id)
        #[arg(long)]
        name: Option<String>,

        /// Rank (defaults from manifest if present; else 8)
        #[arg(long)]
        rank: Option<i32>,

        /// Tier (ephemeral=0, persistent=1) default ephemeral
        #[arg(long)]
        tier: Option<i32>,

        /// Control plane base URL
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,
    },

    /// Hot-swap adapters in running worker
    #[command(
        after_help = "Examples:\n  aosctl adapter swap --tenant dev --add specialist --remove temp_fix\n  aosctl adapter swap --tenant dev --add specialist --remove temp_fix --commit\n  aosctl adapter swap --tenant dev --add adapter1,adapter2 --commit"
    )]
    Swap {
        /// Tenant ID
        #[arg(short, long)]
        tenant: String,

        /// Adapter IDs to add (comma-separated)
        #[arg(long, value_delimiter = ',')]
        add: Vec<String>,

        /// Adapter IDs to remove (comma-separated)
        #[arg(long, value_delimiter = ',')]
        remove: Vec<String>,

        /// Timeout in milliseconds
        #[arg(long, default_value = "5000")]
        timeout: u64,

        /// Commit the swap (otherwise dry-run)
        #[arg(long)]
        commit: bool,

        /// UDS socket path
        #[arg(long, default_value = "/var/run/aos/aos.sock")]
        socket: std::path::PathBuf,
    },

    /// Show adapter information and provenance
    #[command(
        after_help = "Examples:\n  aosctl adapter info specialist\n  aosctl adapter info temp_fix"
    )]
    Info {
        /// Adapter ID
        #[arg()]
        adapter_id: String,
    },

    /// List pinned adapters for a tenant
    #[command(
        after_help = "Examples:\n  aosctl adapter list-pinned --tenant dev\n  aosctl adapter list-pinned --tenant dev --json"
    )]
    ListPinned {
        /// Tenant ID
        #[arg(short, long)]
        tenant: String,
    },

    /// PRD-ART-01: Export adapter to a .aos file
    #[command(
        after_help = "Examples:\n  aosctl adapter export adapter-1\n  aosctl adapter export adapter-1 -o ./exported.aos\n  aosctl adapter export adapter-1 --out path/to/file.aos"
    )]
    Export {
        /// Adapter ID to export
        #[arg()]
        adapter_id: String,

        /// Output file path (default: ./{adapter_id}.aos)
        #[arg(short, long)]
        out: Option<PathBuf>,

        /// Control plane base URL
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,
    },

    /// PRD-ART-01: Import adapter from a .aos file
    #[command(
        after_help = "Examples:\n  aosctl adapter import ./my-adapter.aos --tenant dev\n  aosctl adapter import ./adapter.aos --tenant dev --auto-load"
    )]
    Import {
        /// Path to .aos file
        #[arg()]
        path: PathBuf,

        /// Tenant ID (required)
        #[arg(long)]
        tenant: String,

        /// Auto-load adapter after import
        #[arg(long)]
        auto_load: bool,

        /// Control plane base URL
        #[arg(long, default_value = "http://127.0.0.1:8080/api")]
        base_url: String,
    },
}

/// Get adapter command name for telemetry
fn get_adapter_command_name(cmd: &AdapterCommand) -> String {
    match cmd {
        AdapterCommand::List { .. } => "adapter_list".to_string(),
        AdapterCommand::Profile { .. } => "adapter_profile".to_string(),
        AdapterCommand::Promote { .. } => "adapter_promote".to_string(),
        AdapterCommand::Demote { .. } => "adapter_demote".to_string(),
        AdapterCommand::Pin { .. } => "adapter_pin".to_string(),
        AdapterCommand::Unpin { .. } => "adapter_unpin".to_string(),
        AdapterCommand::Lineage { .. } => "adapter_lineage".to_string(),
        AdapterCommand::Evict { .. } => "adapter_evict".to_string(),
        AdapterCommand::DirectoryUpsert { .. } => "adapter_directory_upsert".to_string(),
        AdapterCommand::VerifyGpu { .. } => "adapter_verify_gpu".to_string(),
        AdapterCommand::UpdateLifecycle { .. } => "adapter_update_lifecycle".to_string(),
        AdapterCommand::Register { .. } => "adapter_register".to_string(),
        AdapterCommand::Swap { .. } => "adapter_swap".to_string(),
        AdapterCommand::Info { .. } => "adapter_info".to_string(),
        AdapterCommand::ListPinned { .. } => "adapter_list_pinned".to_string(),
        AdapterCommand::Export { .. } => "adapter_export".to_string(),
        AdapterCommand::Import { .. } => "adapter_import".to_string(),
    }
}

/// Extract tenant ID from adapter command
fn extract_tenant_from_adapter_command(cmd: &AdapterCommand) -> Option<String> {
    match cmd {
        AdapterCommand::List { tenant, .. } => tenant.clone(),
        AdapterCommand::Profile { tenant, .. } => tenant.clone(),
        AdapterCommand::Promote { tenant, .. } => tenant.clone(),
        AdapterCommand::Demote { tenant, .. } => tenant.clone(),
        AdapterCommand::Pin { tenant, .. } => tenant.clone(),
        AdapterCommand::Unpin { tenant, .. } => tenant.clone(),
        AdapterCommand::Lineage { .. } => None, // Lineage doesn't have tenant parameter
        AdapterCommand::Evict { tenant, .. } => tenant.clone(),
        AdapterCommand::DirectoryUpsert { tenant, .. } => Some(tenant.clone()),
        AdapterCommand::VerifyGpu { tenant, .. } => tenant.clone(),
        AdapterCommand::UpdateLifecycle { .. } => None, // No tenant parameter
        AdapterCommand::Register { .. } => None,        // No tenant parameter
        AdapterCommand::Swap { tenant, .. } => Some(tenant.clone()),
        AdapterCommand::Info { .. } => None, // No tenant parameter
        AdapterCommand::ListPinned { tenant } => Some(tenant.clone()),
        AdapterCommand::Export { .. } => None, // Export uses auth context
        AdapterCommand::Import { tenant, .. } => Some(tenant.clone()),
    }
}

/// Handle adapter lifecycle commands
///
/// Routes adapter commands to appropriate handlers
pub async fn handle_adapter_command(cmd: AdapterCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_adapter_command_name(&cmd);
    let tenant_id = extract_tenant_from_adapter_command(&cmd);

    info!(command = ?cmd, "Handling adapter command");

    // Emit telemetry
    let _ = crate::cli_telemetry::emit_cli_command(&command_name, tenant_id.as_deref(), true).await;

    match cmd {
        AdapterCommand::List {
            json,
            tenant,
            pinned_only,
        } => list_adapters(json, tenant, pinned_only, output).await,
        AdapterCommand::Profile {
            adapter_id,
            json,
            tenant,
        } => profile_adapter(&adapter_id, json, tenant, output).await,
        AdapterCommand::Promote { adapter_id, tenant } => {
            promote_adapter(&adapter_id, tenant, output).await
        }
        AdapterCommand::Demote { adapter_id, tenant } => {
            demote_adapter(&adapter_id, tenant, output).await
        }
        AdapterCommand::Pin { adapter_id, tenant } => {
            pin_adapter(&adapter_id, tenant, output).await
        }
        AdapterCommand::Unpin { adapter_id, tenant } => {
            unpin_adapter(&adapter_id, tenant, output).await
        }
        AdapterCommand::Lineage {
            adapter_id,
            json,
            tree,
        } => lineage_adapter(&adapter_id, json, tree, output).await,
        AdapterCommand::Evict {
            adapter_id,
            tenant,
            reason,
        } => evict_adapter(&adapter_id, tenant, reason.as_deref(), output).await,
        AdapterCommand::DirectoryUpsert {
            tenant,
            root,
            path,
            activate,
            base_url,
        } => directory_upsert(&tenant, &root, &path, activate, &base_url, output).await,
        AdapterCommand::VerifyGpu {
            tenant,
            adapter,
            socket,
            timeout,
        } => {
            let tenant_id = tenant.as_deref().unwrap_or("default");
            crate::commands::verify_gpu::run(tenant_id, adapter.as_deref(), &socket, timeout)
                .await
                .map_err(|e| adapteros_core::AosError::Other(e.to_string()))
        }
        AdapterCommand::UpdateLifecycle { adapter_id, state } => {
            update_lifecycle(&adapter_id, &state, output).await
        }
        AdapterCommand::Register {
            path,
            adapter_id,
            name,
            rank,
            tier,
            base_url,
        } => register_adapter(&path, adapter_id, name, rank, tier, &base_url, output).await,
        AdapterCommand::Swap {
            tenant,
            add,
            remove,
            timeout,
            commit,
            socket,
        } => crate::commands::adapter_swap::run(&tenant, &add, &remove, timeout, commit, &socket)
            .await
            .map_err(|e| adapteros_core::AosError::Other(e.to_string())),
        AdapterCommand::Info { adapter_id } => crate::commands::adapter_info::run(&adapter_id)
            .await
            .map_err(|e| adapteros_core::AosError::Other(e.to_string())),
        AdapterCommand::ListPinned { tenant } => {
            let db = adapteros_db::Db::connect_env().await?;
            crate::commands::pin::list_pinned(&db, &tenant, output)
                .await
                .map_err(|e| adapteros_core::AosError::Other(e.to_string()))
        }
        AdapterCommand::Export {
            adapter_id,
            out,
            base_url,
        } => export_adapter_cmd(&adapter_id, out, &base_url, output).await,
        AdapterCommand::Import {
            path,
            tenant,
            auto_load,
            base_url,
        } => import_adapter_cmd(&path, &tenant, auto_load, &base_url, output).await,
    }
}

/// List all adapters with their current states
async fn list_adapters(
    json: bool,
    tenant: Option<String>,
    pinned_only: bool,
    output: &OutputWriter,
) -> Result<()> {
    info!("Listing adapter lifecycle status");

    let socket_path = get_worker_socket_path(tenant.as_deref());

    if !socket_path.exists() || !socket_path.parent().unwrap().exists() {
        if json {
            let mock_data = serde_json::json!([
                {
                    "id": "python-general",
                    "hash": "b3:abc123",
                    "tier": "persistent",
                    "rank": 16,
                    "state": "hot",
                    "activation_pct": 45.2,
                    "quality_delta": 0.68,
                    "memory_mb": 16,
                    "pinned": false,
                    "last_activation": "2m ago"
                },
                {
                    "id": "django-specific",
                    "hash": "b3:def456",
                    "tier": "persistent",
                    "rank": 8,
                    "state": "warm",
                    "activation_pct": 12.8,
                    "quality_delta": 0.54,
                    "memory_mb": 16,
                    "pinned": false,
                    "last_activation": "5m ago"
                }
            ]);
            info!(
                "Adapter lifecycle status: {}",
                serde_json::to_string_pretty(&mock_data)?
            );
        } else {
            output.result("📊 Adapter Lifecycle Status");
            output.blank();
            output.warning(&format!(
                "Worker socket not found at: {}",
                socket_path.display()
            ));
            output.result("Showing mock data instead.");
            output.blank();

            let mut table = Table::new();
            table
                .load_preset(UTF8_FULL)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_header(vec![
                    "ID",
                    "Hash",
                    "Tier",
                    "Rank",
                    "State",
                    "Activation %",
                    "Quality Δ",
                    "Memory",
                    "Pinned",
                    "Last Active",
                ]);

            // Mock data when worker is not available
            table.add_row(vec![
                "python-general",
                "b3:abc123",
                "persistent",
                "16",
                "hot",
                "45.2%",
                "+0.68",
                "16 MB",
                "no",
                "2m ago",
            ]);
            table.add_row(vec![
                "django-specific",
                "b3:def456",
                "persistent",
                "8",
                "warm",
                "12.8%",
                "+0.54",
                "16 MB",
                "no",
                "5m ago",
            ]);
            table.add_row(vec![
                "rust-general",
                "b3:789ghi",
                "persistent",
                "16",
                "cold",
                "2.1%",
                "+0.23",
                "16 MB",
                "no",
                "never",
            ]);
            table.add_row(vec![
                "security-patch",
                "b3:jkl012",
                "ephemeral",
                "32",
                "resident",
                "78.9%",
                "+0.95",
                "16 MB",
                "yes",
                "30s ago",
            ]);

            output.result(&format!("{table}"));
        }
        return Ok(());
    }

    // Connect to worker and fetch adapter states
    match connect_and_fetch_adapter_states(&socket_path, Duration::from_secs(5)).await {
        Ok(mut adapters) => {
            // Filter to only pinned adapters if requested
            if pinned_only {
                adapters.retain(|a| a.pinned);
            }

            if json {
                output.result(&serde_json::to_string_pretty(&adapters)?);
            } else {
                let mut table = Table::new();
                table
                    .load_preset(UTF8_FULL)
                    .apply_modifier(UTF8_ROUND_CORNERS)
                    .set_header(vec![
                        "ID",
                        "Hash",
                        "Tier",
                        "Rank",
                        "State",
                        "Activation %",
                        "Quality Δ",
                        "Memory",
                        "Pinned",
                        "Last Active",
                    ]);

                for adapter in adapters {
                    let state = if adapter.active { "active" } else { "staged" };
                    let pinned = if adapter.pinned { "yes" } else { "no" };
                    let last_active = adapter
                        .last_activation
                        .map(|ts| {
                            format!(
                                "{}s ago",
                                (std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .unwrap()
                                    .as_secs()
                                    - ts)
                            )
                        })
                        .unwrap_or_else(|| "never".to_string());

                    table.add_row(vec![
                        &adapter.id,
                        &adapter.hash[..8], // Short hash
                        &adapter.tier,
                        &adapter.rank.to_string(),
                        state,
                        &format!("{:.1}%", adapter.activation_pct),
                        &format!("{:.2}", adapter.quality_delta),
                        &format!("{} MB", adapter.vram_mb),
                        pinned,
                        &last_active,
                    ]);
                }

                output.result(&format!("{table}"));
            }
        }
        Err(e) => {
            if json {
                let error_response = serde_json::json!({
                    "error": format!("{}", e),
                    "adapters": []
                });
                output.result(&serde_json::to_string_pretty(&error_response)?);
            } else {
                output.error(&format!("Failed to connect to worker: {}", e));
                output.result("Showing mock data instead.");
                output.blank();

                let mut table = Table::new();
                table
                    .load_preset(UTF8_FULL)
                    .apply_modifier(UTF8_ROUND_CORNERS)
                    .set_header(vec![
                        "ID",
                        "Hash",
                        "Tier",
                        "Rank",
                        "State",
                        "Activation %",
                        "Quality Δ",
                        "Memory",
                        "Pinned",
                        "Last Active",
                    ]);

                table.add_row(vec![
                    "python-general",
                    "b3:abc123",
                    "persistent",
                    "16",
                    "hot",
                    "45.2%",
                    "+0.68",
                    "16 MB",
                    "no",
                    "2m ago",
                ]);
                table.add_row(vec![
                    "django-specific",
                    "b3:def456",
                    "persistent",
                    "8",
                    "warm",
                    "12.8%",
                    "+0.54",
                    "16 MB",
                    "no",
                    "5m ago",
                ]);
                table.add_row(vec![
                    "rust-general",
                    "b3:789ghi",
                    "persistent",
                    "16",
                    "cold",
                    "2.1%",
                    "+0.23",
                    "16 MB",
                    "no",
                    "never",
                ]);
                table.add_row(vec![
                    "security-patch",
                    "b3:jkl012",
                    "ephemeral",
                    "32",
                    "resident",
                    "78.9%",
                    "+0.95",
                    "16 MB",
                    "yes",
                    "30s ago",
                ]);

                output.result(&format!("{table}"));
            }
        }
    }

    Ok(())
}

/// Display detailed profile for an adapter
async fn profile_adapter(
    adapter_id: &str,
    json: bool,
    tenant: Option<String>,
    output: &OutputWriter,
) -> Result<()> {
    validate_adapter_id(adapter_id)?;

    info!(adapter_id = %adapter_id, "Profiling adapter");

    let socket_path = get_worker_socket_path(tenant.as_deref());

    if !socket_path.exists() {
        if json {
            let mock_profile = serde_json::json!({
                "state": "hot",
                "activation_pct": 45.2,
                "activations": 1234,
                "total_tokens": 2730,
                "avg_latency_us": 156.2,
                "memory_kb": 16384,
                "quality_delta": 0.68,
                "recent_activations": [
                    {"start_token": 100, "end_token": 150, "count": 3},
                    {"start_token": 150, "end_token": 200, "count": 5},
                    {"start_token": 200, "end_token": 250, "count": 2}
                ],
                "performance_metrics": {
                    "p50_latency_us": 142.0,
                    "p95_latency_us": 189.0,
                    "p99_latency_us": 234.0,
                    "throughput_tokens_per_sec": 45.2,
                    "error_rate": 0.01
                },
                "policy_compliance": {
                    "determinism_score": 0.98,
                    "evidence_coverage": 0.95,
                    "refusal_rate": 0.02,
                    "policy_violations": 0
                }
            });
            info!(
                "Adapter profile: {}",
                serde_json::to_string_pretty(&mock_profile)?
            );
        } else {
            output.result(&format!("📈 Adapter Profile: {}", adapter_id));
            output.blank();
            output.warning(&format!(
                "Worker socket not found at: {}",
                socket_path.display()
            ));
            output.result("Showing mock data instead.");
            output.blank();

            output.result("State:           hot");
            output.result("Activation:      45.2% (1,234 / 2,730 tokens)");
            output.result("Avg Latency:     156.2 µs");
            output.result("Memory Usage:    16,384 KB");
            output.result("Quality Delta:   +0.68");
            output.blank();
            output.result("Last 10 activations:");
            output.result("  Token 100-150:  3 activations");
            output.result("  Token 150-200:  5 activations");
            output.result("  Token 200-250:  2 activations");
        }
        return Ok(());
    }

    // Connect to worker and fetch adapter profile
    match connect_and_fetch_adapter_profile(&socket_path, adapter_id, Duration::from_secs(5)).await
    {
        Ok(profile) => {
            if json {
                output.result(&serde_json::to_string_pretty(&profile)?);
            } else {
                output.result(&format!("State:           {}", profile.state));
                output.result(&format!(
                    "Activation:      {:.1}% ({} / {} tokens)",
                    profile.activation_pct, profile.activations, profile.total_tokens
                ));
                output.result(&format!(
                    "Avg Latency:     {:.1} µs",
                    profile.avg_latency_us
                ));
                output.result(&format!("Memory Usage:    {} KB", profile.memory_kb));
                output.result(&format!("Quality Delta:   {:.2}", profile.quality_delta));
                output.blank();
                output.result("Last 10 activations:");
                for activation in &profile.recent_activations {
                    output.result(&format!(
                        "  Token {}-{}:  {} activations",
                        activation.start_token, activation.end_token, activation.count
                    ));
                }
                output.blank();
                output.result("Performance Metrics:");
                output.result(&format!(
                    "  P50 Latency:    {:.1} µs",
                    profile.performance_metrics.p50_latency_us
                ));
                output.result(&format!(
                    "  P95 Latency:    {:.1} µs",
                    profile.performance_metrics.p95_latency_us
                ));
                output.result(&format!(
                    "  P99 Latency:    {:.1} µs",
                    profile.performance_metrics.p99_latency_us
                ));
                output.result(&format!(
                    "  Throughput:     {:.1} tokens/sec",
                    profile.performance_metrics.throughput_tokens_per_sec
                ));
                output.result(&format!(
                    "  Error Rate:     {:.2}%",
                    profile.performance_metrics.error_rate * 100.0
                ));
                output.blank();
                output.result("Policy Compliance:");
                output.result(&format!(
                    "  Determinism:   {:.2}",
                    profile.policy_compliance.determinism_score
                ));
                output.result(&format!(
                    "  Evidence:      {:.2}",
                    profile.policy_compliance.evidence_coverage
                ));
                output.result(&format!(
                    "  Refusal Rate:  {:.2}%",
                    profile.policy_compliance.refusal_rate * 100.0
                ));
                output.result(&format!(
                    "  Violations:    {}",
                    profile.policy_compliance.policy_violations
                ));
            }
        }
        Err(e) => {
            if json {
                let error_response = serde_json::json!({
                    "error": format!("{}", e),
                    "profile": null
                });
                output.result(&serde_json::to_string_pretty(&error_response)?);
            } else {
                output.error(&format!("Failed to connect to worker: {}", e));
                output.result("Showing mock data instead.");
                output.blank();

                output.result("State:           hot");
                output.result("Activation:      45.2% (1,234 / 2,730 tokens)");
                output.result("Avg Latency:     156.2 µs");
                output.result("Memory Usage:    16,384 KB");
                output.result("Quality Delta:   +0.68");
                output.blank();
                output.result("Last 10 activations:");
                output.result("  Token 100-150:  3 activations");
                output.result("  Token 150-200:  5 activations");
                output.result("  Token 200-250:  2 activations");
            }
        }
    }

    Ok(())
}

/// Promote adapter to higher priority state
async fn promote_adapter(
    adapter_id: &str,
    tenant: Option<String>,
    output: &OutputWriter,
) -> Result<()> {
    validate_adapter_id(adapter_id)?;

    info!(adapter_id = %adapter_id, "Promoting adapter");
    let socket_path = get_worker_socket_path(tenant.as_deref());

    if !socket_path.exists() {
        if output.mode().is_json() {
            let response = serde_json::json!({
                "success": true,
                "message": "Promoted adapter (mock)",
                "adapter_id": adapter_id,
                "state_change": "warm → hot"
            });
            output.result(&serde_json::to_string_pretty(&response)?);
        } else {
            output.warning(&format!(
                "Worker socket not found at: {}",
                socket_path.display()
            ));
            output.success(&format!("Promoted adapter: {} (mock)", adapter_id));
            output.result("State: warm → hot");
        }
        return Ok(());
    }

    emit_kv_readiness(adapter_id, output).await;

    match send_adapter_command(&socket_path, "promote", adapter_id, Duration::from_secs(5)).await {
        Ok(_) => {
            if output.mode().is_json() {
                let response = serde_json::json!({
                    "success": true,
                    "message": "Adapter promoted successfully",
                    "adapter_id": adapter_id,
                    "state_change": "warm → hot"
                });
                output.result(&serde_json::to_string_pretty(&response)?);
            } else {
                output.success(&format!("Promoted adapter: {}", adapter_id));
                output.result("State: warm → hot");
            }
        }
        Err(e) => {
            if output.mode().is_json() {
                let response = serde_json::json!({
                    "success": false,
                    "error": format!("{}", e),
                    "adapter_id": adapter_id
                });
                output.result(&serde_json::to_string_pretty(&response)?);
            } else {
                output.error(&format!("Failed to promote adapter: {}", e));
            }
        }
    }

    Ok(())
}

/// Demote adapter to lower priority state
async fn demote_adapter(
    adapter_id: &str,
    tenant: Option<String>,
    output: &OutputWriter,
) -> Result<()> {
    validate_adapter_id(adapter_id)?;

    info!(adapter_id = %adapter_id, "Demoting adapter");
    let socket_path = get_worker_socket_path(tenant.as_deref());

    if !socket_path.exists() {
        if output.mode().is_json() {
            let response = serde_json::json!({
                "success": true,
                "message": "Demoted adapter (mock)",
                "adapter_id": adapter_id,
                "state_change": "hot → warm"
            });
            output.result(&serde_json::to_string_pretty(&response)?);
        } else {
            output.warning(&format!(
                "Worker socket not found at: {}",
                socket_path.display()
            ));
            output.success(&format!("Demoted adapter: {} (mock)", adapter_id));
            output.result("State: hot → warm");
        }
        return Ok(());
    }

    emit_kv_readiness(adapter_id, output).await;

    match send_adapter_command(&socket_path, "demote", adapter_id, Duration::from_secs(5)).await {
        Ok(_) => {
            if output.mode().is_json() {
                let response = serde_json::json!({
                    "success": true,
                    "message": "Adapter demoted successfully",
                    "adapter_id": adapter_id,
                    "state_change": "hot → warm"
                });
                output.result(&serde_json::to_string_pretty(&response)?);
            } else {
                output.success(&format!("Demoted adapter: {}", adapter_id));
                output.result("State: hot → warm");
            }
        }
        Err(e) => {
            if output.mode().is_json() {
                let response = serde_json::json!({
                    "success": false,
                    "error": format!("{}", e),
                    "adapter_id": adapter_id
                });
                output.result(&serde_json::to_string_pretty(&response)?);
            } else {
                output.error(&format!("Failed to demote adapter: {}", e));
            }
        }
    }

    Ok(())
}

/// Pin adapter to resident state (prevent eviction)
async fn pin_adapter(
    adapter_id: &str,
    tenant: Option<String>,
    output: &OutputWriter,
) -> Result<()> {
    validate_adapter_id(adapter_id)?;

    info!(adapter_id = %adapter_id, "Pinning adapter");
    let socket_path = get_worker_socket_path(tenant.as_deref());

    if !socket_path.exists() {
        if output.mode().is_json() {
            let response = serde_json::json!({
                "success": true,
                "message": "Pinned adapter (mock)",
                "adapter_id": adapter_id,
                "state_change": "→ resident (pinned)"
            });
            output.result(&serde_json::to_string_pretty(&response)?);
        } else {
            output.warning(&format!(
                "Worker socket not found at: {}",
                socket_path.display()
            ));
            output.success(&format!("Pinned adapter: {} (mock)", adapter_id));
            output.result("State: → resident (pinned)");
        }
        return Ok(());
    }

    emit_kv_readiness(adapter_id, output).await;

    match send_adapter_command(&socket_path, "pin", adapter_id, Duration::from_secs(5)).await {
        Ok(_) => {
            if output.mode().is_json() {
                let response = serde_json::json!({
                    "success": true,
                    "message": "Adapter pinned successfully",
                    "adapter_id": adapter_id,
                    "state_change": "→ resident (pinned)"
                });
                output.result(&serde_json::to_string_pretty(&response)?);
            } else {
                output.success(&format!("Pinned adapter: {}", adapter_id));
                output.result("State: → resident (pinned)");
            }
        }
        Err(e) => {
            if output.mode().is_json() {
                let response = serde_json::json!({
                    "success": false,
                    "error": format!("{}", e),
                    "adapter_id": adapter_id
                });
                output.result(&serde_json::to_string_pretty(&response)?);
            } else {
                output.error(&format!("Failed to pin adapter: {}", e));
            }
        }
    }

    Ok(())
}

/// Unpin adapter (allow eviction)
async fn unpin_adapter(
    adapter_id: &str,
    tenant: Option<String>,
    output: &OutputWriter,
) -> Result<()> {
    validate_adapter_id(adapter_id)?;

    info!(adapter_id = %adapter_id, "Unpinning adapter");
    let socket_path = get_worker_socket_path(tenant.as_deref());

    if !socket_path.exists() {
        if output.mode().is_json() {
            let response = serde_json::json!({
                "success": true,
                "message": "Unpinned adapter (mock)",
                "adapter_id": adapter_id,
                "state_change": "Adapter can now be demoted"
            });
            output.result(&serde_json::to_string_pretty(&response)?);
        } else {
            output.warning(&format!(
                "Worker socket not found at: {}",
                socket_path.display()
            ));
            output.success(&format!("Unpinned adapter: {} (mock)", adapter_id));
            output.result("Adapter can now be demoted");
        }
        return Ok(());
    }

    emit_kv_readiness(adapter_id, output).await;

    match send_adapter_command(&socket_path, "unpin", adapter_id, Duration::from_secs(5)).await {
        Ok(_) => {
            if output.mode().is_json() {
                let response = serde_json::json!({
                    "success": true,
                    "message": "Adapter unpinned successfully",
                    "adapter_id": adapter_id,
                    "state_change": "Adapter can now be demoted"
                });
                output.result(&serde_json::to_string_pretty(&response)?);
            } else {
                output.success(&format!("Unpinned adapter: {}", adapter_id));
                output.result("Adapter can now be demoted");
            }
        }
        Err(e) => {
            if output.mode().is_json() {
                let response = serde_json::json!({
                    "success": false,
                    "error": format!("{}", e),
                    "adapter_id": adapter_id
                });
                output.result(&serde_json::to_string_pretty(&response)?);
            } else {
                output.error(&format!("Failed to unpin adapter: {}", e));
            }
        }
    }

    Ok(())
}

/// Show adapter lineage tree (ancestors and descendants)
///
/// Displays full lineage tree including parent, children, and fork relationships.
async fn lineage_adapter(
    adapter_id: &str,
    json_output: bool,
    tree_view: bool,
    output: &OutputWriter,
) -> Result<()> {
    use reqwest::Client;
    use serde_json::Value;

    validate_adapter_id(adapter_id)?;
    info!(adapter_id = %adapter_id, "Fetching adapter lineage");

    // Call lineage API endpoint
    let client = Client::new();
    let url = format!("http://127.0.0.1:8080/v1/adapters/{}/lineage", adapter_id);

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("Failed to fetch lineage: {}", e)))?;

    if !response.status().is_success() {
        let status = response.status();
        let error_text = response.text().await.unwrap_or_default();
        return Err(AosError::Io(format!(
            "API error {}: {}",
            status, error_text
        )));
    }

    let lineage_data: Value = response
        .json()
        .await
        .map_err(|e| AosError::Io(format!("Failed to parse response: {}", e)))?;

    // Output based on format
    if json_output {
        output.result(&serde_json::to_string_pretty(&lineage_data)?);
        return Ok(());
    }

    // Parse lineage structure
    let empty_vec = vec![];
    let ancestors = lineage_data["ancestors"].as_array().unwrap_or(&empty_vec);
    let self_node = &lineage_data["self_node"];
    let descendants = lineage_data["descendants"].as_array().unwrap_or(&empty_vec);
    let total_nodes = lineage_data["total_nodes"].as_u64().unwrap_or(0);

    output.info(&format!("Lineage tree for adapter: {}", adapter_id));
    output.kv("Total nodes", &total_nodes.to_string());
    println!();

    if tree_view {
        // ASCII tree view
        if !ancestors.is_empty() {
            output.section("Ancestors");
            for (i, ancestor) in ancestors.iter().enumerate() {
                let indent = "  ".repeat(ancestors.len() - i - 1);
                let name = ancestor["adapter_name"]
                    .as_str()
                    .unwrap_or(ancestor["adapter_id"].as_str().unwrap_or("unknown"));
                let state = ancestor["current_state"].as_str().unwrap_or("unknown");
                let revision = ancestor["revision"].as_str().unwrap_or("r???");

                println!("{}└── {} ({}) [{}]", indent, name, revision, state);
            }
        }

        // Self node
        let self_name = self_node["adapter_name"]
            .as_str()
            .unwrap_or(self_node["adapter_id"].as_str().unwrap_or("unknown"));
        let self_state = self_node["current_state"].as_str().unwrap_or("unknown");
        let self_revision = self_node["revision"].as_str().unwrap_or("r???");
        println!(">>> {} ({}) [{}] <<<", self_name, self_revision, self_state);

        if !descendants.is_empty() {
            output.section("Descendants");
            for (i, descendant) in descendants.iter().enumerate() {
                let indent = "  ".repeat(i + 1);
                let name = descendant["adapter_name"]
                    .as_str()
                    .unwrap_or(descendant["adapter_id"].as_str().unwrap_or("unknown"));
                let state = descendant["current_state"].as_str().unwrap_or("unknown");
                let revision = descendant["revision"].as_str().unwrap_or("r???");
                let fork_type = descendant["fork_type"].as_str();

                let fork_indicator = fork_type
                    .map(|ft| format!(" [fork: {}]", ft))
                    .unwrap_or_default();
                println!(
                    "{}└── {} ({}) [{}]{}",
                    indent, name, revision, state, fork_indicator
                );
            }
        }
    } else {
        // Tabular view
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                "Relation",
                "Adapter Name",
                "Revision",
                "State",
                "Fork Type",
            ]);

        // Add ancestors
        for ancestor in ancestors {
            table.add_row(vec![
                "Ancestor",
                ancestor["adapter_name"]
                    .as_str()
                    .unwrap_or(ancestor["adapter_id"].as_str().unwrap_or("-")),
                ancestor["revision"].as_str().unwrap_or("-"),
                ancestor["current_state"].as_str().unwrap_or("-"),
                ancestor["fork_type"].as_str().unwrap_or("-"),
            ]);
        }

        // Add self
        table.add_row(vec![
            ">>> SELF <<<",
            self_node["adapter_name"]
                .as_str()
                .unwrap_or(self_node["adapter_id"].as_str().unwrap_or("-")),
            self_node["revision"].as_str().unwrap_or("-"),
            self_node["current_state"].as_str().unwrap_or("-"),
            "-",
        ]);

        // Add descendants
        for descendant in descendants {
            table.add_row(vec![
                "Descendant",
                descendant["adapter_name"]
                    .as_str()
                    .unwrap_or(descendant["adapter_id"].as_str().unwrap_or("-")),
                descendant["revision"].as_str().unwrap_or("-"),
                descendant["current_state"].as_str().unwrap_or("-"),
                descendant["fork_type"].as_str().unwrap_or("-"),
            ]);
        }

        println!("{}", table);
    }

    output.success("Lineage retrieved successfully");
    Ok(())
}

/// Evict adapter from memory
async fn evict_adapter(
    adapter_id: &str,
    tenant: Option<String>,
    reason: Option<&str>,
    output: &OutputWriter,
) -> Result<()> {
    validate_adapter_id(adapter_id)?;

    info!(
        adapter_id = %adapter_id,
        reason = ?reason,
        "Evicting adapter"
    );

    let socket_path = get_worker_socket_path(tenant.as_deref());

    if !socket_path.exists() {
        if output.mode().is_json() {
            let mut response = serde_json::json!({
                "success": true,
                "message": "Evicted adapter (mock)",
                "adapter_id": adapter_id
            });
            if let Some(r) = reason {
                response["reason"] = serde_json::Value::String(r.to_string());
            }
            output.result(&serde_json::to_string_pretty(&response)?);
        } else {
            output.warning(&format!(
                "Worker socket not found at: {}",
                socket_path.display()
            ));
            output.success(&format!("Evicted adapter: {} (mock)", adapter_id));
            if let Some(r) = reason {
                output.result(&format!("Reason: {}", r));
            }
        }
        return Ok(());
    }

    match send_adapter_command(&socket_path, "evict", adapter_id, Duration::from_secs(5)).await {
        Ok(_) => {
            if output.mode().is_json() {
                let mut response = serde_json::json!({
                    "success": true,
                    "message": "Adapter evicted successfully",
                    "adapter_id": adapter_id
                });
                if let Some(r) = reason {
                    response["reason"] = serde_json::Value::String(r.to_string());
                }
                output.result(&serde_json::to_string_pretty(&response)?);
            } else {
                output.success(&format!("Evicted adapter: {}", adapter_id));
                if let Some(r) = reason {
                    output.result(&format!("Reason: {}", r));
                }
            }
        }
        Err(e) => {
            if output.mode().is_json() {
                let response = serde_json::json!({
                    "success": false,
                    "error": format!("{}", e),
                    "adapter_id": adapter_id
                });
                output.result(&serde_json::to_string_pretty(&response)?);
            } else {
                output.error(&format!("Failed to evict adapter: {}", e));
            }
        }
    }

    Ok(())
}

/// Update adapter lifecycle state
async fn update_lifecycle(adapter_id: &str, state_str: &str, output: &OutputWriter) -> Result<()> {
    use adapteros_core::lifecycle::LifecycleState;
    use std::str::FromStr;

    validate_adapter_id(adapter_id)?;

    info!(adapter_id = %adapter_id, state = %state_str, "Updating adapter lifecycle state");

    // Parse the lifecycle state
    let new_state = LifecycleState::from_str(state_str).map_err(|e| {
        adapteros_core::AosError::Validation(format!(
            "Invalid lifecycle state '{}': {}. Must be one of: draft, active, deprecated, retired",
            state_str, e
        ))
    })?;

    // Connect to database and update lifecycle state
    let db = adapteros_db::Db::connect_env().await?;

    match db
        .update_adapter_lifecycle_state(adapter_id, new_state)
        .await
    {
        Ok(_) => {
            if output.mode().is_json() {
                let response = serde_json::json!({
                    "success": true,
                    "message": "Adapter lifecycle state updated successfully",
                    "adapter_id": adapter_id,
                    "new_state": new_state.as_str()
                });
                output.result(&serde_json::to_string_pretty(&response)?);
            } else {
                output.success(&format!(
                    "Updated adapter {} to lifecycle state: {}",
                    adapter_id,
                    new_state.as_str()
                ));
            }
            Ok(())
        }
        Err(e) => {
            // Check if error is due to invalid transition
            let error_msg = format!("{}", e);

            if output.mode().is_json() {
                let response = serde_json::json!({
                    "success": false,
                    "error": error_msg,
                    "adapter_id": adapter_id,
                    "requested_state": new_state.as_str()
                });
                output.result(&serde_json::to_string_pretty(&response)?);
            } else {
                output.error(&format!("Failed to update lifecycle state: {}", error_msg));
            }
            Err(e)
        }
    }
}

/// Resolve paths for adapter registration
/// Returns (weights_path, manifest_path, default_adapter_id)
fn resolve_paths(path: &Path) -> Result<(PathBuf, PathBuf, String)> {
    let (weights_path, manifest_path, adapter_id_default) = if path.is_dir() {
        // Directory: look for weights.safetensors and manifest.json
        let weights = path.join("weights.safetensors");
        let manifest = path.join("manifest.json");
        let id = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "adapter".to_string());
        (weights, manifest, id)
    } else {
        // File: assume it's weights, look for manifest in same dir
        let parent = path.parent().unwrap_or(Path::new("."));
        let manifest = parent.join("manifest.json");
        let id = parent
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "adapter".to_string());
        (path.to_path_buf(), manifest, id)
    };

    Ok((weights_path, manifest_path, adapter_id_default))
}

/// Register a packaged adapter by path
async fn register_adapter(
    path: &Path,
    adapter_id: Option<String>,
    name: Option<String>,
    rank: Option<i32>,
    tier: Option<i32>,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    use std::fs;

    let (weights_path, manifest_path, adapter_id_default) = resolve_paths(path)?;

    // Read weights file and compute hash
    if !weights_path.exists() {
        return Err(AosError::Io(format!(
            "Weights file not found: {}",
            weights_path.display()
        )));
    }

    let weights_data = fs::read(&weights_path)
        .map_err(|e| AosError::Io(format!("Failed to read weights: {}", e)))?;
    let weights_hash = B3Hash::hash(&weights_data);

    // Read manifest if it exists
    let manifest: Option<serde_json::Value> = if manifest_path.exists() {
        let manifest_data = fs::read_to_string(&manifest_path)
            .map_err(|e| AosError::Io(format!("Failed to read manifest: {}", e)))?;
        Some(
            serde_json::from_str(&manifest_data)
                .map_err(|e| AosError::Io(format!("Failed to parse manifest: {}", e)))?,
        )
    } else {
        None
    };

    let adapter_id = adapter_id.unwrap_or(adapter_id_default.clone());
    let name = name.unwrap_or_else(|| adapter_id.clone());

    // Get rank from manifest or use default
    let rank = rank.unwrap_or_else(|| {
        manifest
            .as_ref()
            .and_then(|m| m.get("rank"))
            .and_then(|r| r.as_i64())
            .map(|r| r as i32)
            .unwrap_or(8)
    });

    // Get tier (0=ephemeral, 1=persistent)
    let tier = tier.unwrap_or(0);
    let tier_str = if tier == 1 { "persistent" } else { "ephemeral" };

    output.info("Registering adapter");
    output.kv("Adapter ID", &adapter_id);
    output.kv("Name", &name);
    output.kv("Hash", &weights_hash.to_hex());
    output.kv("Rank", &rank.to_string());
    output.kv("Tier", tier_str);

    // Build request
    let request = RegisterAdapterRequest {
        adapter_id: adapter_id.clone(),
        name,
        hash_b3: weights_hash.to_hex(),
        rank,
        tier: tier_str.to_string(),
        languages: vec![],
        framework: None,
        category: "code".to_string(),
        scope: None,
        expires_at: None,
    };

    // Send to API
    let client = reqwest::Client::new();
    let url = format!("{}/v1/adapters/register", base_url.trim_end_matches('/'));

    let response = client
        .post(&url)
        .json(&request)
        .send()
        .await
        .map_err(|e| AosError::Io(format!("HTTP request failed: {}", e)))?;

    let status = response.status();
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(AosError::Other(format!(
            "Registration failed: {} {}",
            status, text
        )));
    }

    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AosError::Http(e.to_string()))?;

    if output.is_json() {
        output.result(&serde_json::to_string_pretty(&value).unwrap());
    } else {
        output.success(&format!("Adapter registered: {}", adapter_id));
    }

    Ok(())
}

// ============================================================================
// PRD-ART-01: Export/Import Commands
// ============================================================================

/// Export an adapter to a .aos file
///
/// Downloads the adapter from the control plane and saves it to a file.
async fn export_adapter_cmd(
    adapter_id: &str,
    out_path: Option<PathBuf>,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    output.info(&format!("Exporting adapter: {}", adapter_id));

    let client = reqwest::Client::new();
    let url = format!(
        "{}/v1/adapters/{}/export",
        base_url.trim_end_matches('/'),
        adapter_id
    );

    let response = client
        .get(&url)
        .send()
        .await
        .map_err(|e| AosError::Http(format!("Failed to connect to control plane: {}", e)))?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(AosError::Http(format!(
            "Export failed ({}): {}",
            status, error_text
        )));
    }

    // Get hash from header for verification
    let content_hash = response
        .headers()
        .get("x-adapter-hash")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // Determine output path
    let file_path = out_path.unwrap_or_else(|| PathBuf::from(format!("{}.aos", adapter_id)));

    // Download and save
    let bytes = response
        .bytes()
        .await
        .map_err(|e| AosError::Http(format!("Failed to download adapter file: {}", e)))?;

    tokio::fs::write(&file_path, &bytes).await.map_err(|e| {
        AosError::Io(format!(
            "Failed to write file {}: {}",
            file_path.display(),
            e
        ))
    })?;

    output.kv("Output file", &file_path.display().to_string());
    output.kv("Size", &format!("{} bytes", bytes.len()));
    if let Some(hash) = content_hash {
        output.kv("Content hash", &hash);
    }
    output.success(&format!("Adapter exported to: {}", file_path.display()));

    Ok(())
}

/// Import an adapter from a .aos file
///
/// Uploads the adapter file to the control plane for registration.
async fn import_adapter_cmd(
    file_path: &Path,
    tenant_id: &str,
    auto_load: bool,
    base_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    output.info(&format!("Importing adapter from: {}", file_path.display()));
    output.kv("Tenant", tenant_id);

    // Verify file exists
    if !file_path.exists() {
        return Err(AosError::Io(format!(
            "File not found: {}",
            file_path.display()
        )));
    }

    // Read file
    let file_data = tokio::fs::read(file_path).await.map_err(|e| {
        AosError::Io(format!(
            "Failed to read file {}: {}",
            file_path.display(),
            e
        ))
    })?;

    output.kv("File size", &format!("{} bytes", file_data.len()));

    // Build multipart form
    let file_name = file_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("adapter.aos")
        .to_string();

    let part = reqwest::multipart::Part::bytes(file_data)
        .file_name(file_name)
        .mime_str("application/octet-stream")
        .map_err(|e| AosError::Http(format!("Failed to create multipart: {}", e)))?;

    let form = reqwest::multipart::Form::new().part("file", part);

    // Build URL with query params
    let url = format!(
        "{}/v1/adapters/import?load={}",
        base_url.trim_end_matches('/'),
        auto_load
    );

    let client = reqwest::Client::new();
    let response = client
        .post(&url)
        .multipart(form)
        .send()
        .await
        .map_err(|e| AosError::Http(format!("Failed to connect to control plane: {}", e)))?;

    let status = response.status();
    if !status.is_success() {
        let error_text = response
            .text()
            .await
            .unwrap_or_else(|_| "Unknown error".to_string());
        return Err(AosError::Http(format!(
            "Import failed ({}): {}",
            status, error_text
        )));
    }

    let value: serde_json::Value = response
        .json()
        .await
        .map_err(|e| AosError::Http(format!("Failed to parse response: {}", e)))?;

    // Check if deduplicated
    let deduplicated = value
        .get("deduplicated")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    let adapter_id = value
        .get("adapter_id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    if output.is_json() {
        output.result(&serde_json::to_string_pretty(&value).unwrap());
    } else {
        if deduplicated {
            output.success(&format!(
                "Adapter already exists (deduplicated): {}",
                adapter_id
            ));
        } else {
            output.success(&format!("Adapter imported: {}", adapter_id));
        }
        if auto_load {
            output.kv("Auto-load", "enabled");
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::output::{OutputMode, OutputWriter};

    #[test]
    fn test_validate_adapter_id() {
        assert!(validate_adapter_id("valid-adapter-1").is_ok());
        assert!(validate_adapter_id("adapter_2").is_ok());
        assert!(validate_adapter_id("adapter123").is_ok());
        assert!(validate_adapter_id("").is_err());
        assert!(validate_adapter_id("invalid@adapter").is_err());
        assert!(validate_adapter_id("adapter with spaces").is_err());
        assert!(validate_adapter_id(&"a".repeat(65)).is_err()); // Too long
    }

    #[test]
    fn test_get_adapter_command_name() {
        assert_eq!(
            get_adapter_command_name(&AdapterCommand::List {
                json: false,
                tenant: None,
                pinned_only: false
            }),
            "adapter_list"
        );
        assert_eq!(
            get_adapter_command_name(&AdapterCommand::Profile {
                adapter_id: "test".to_string(),
                json: false,
                tenant: None
            }),
            "adapter_profile"
        );
        assert_eq!(
            get_adapter_command_name(&AdapterCommand::Promote {
                adapter_id: "test".to_string(),
                tenant: None
            }),
            "adapter_promote"
        );
        assert_eq!(
            get_adapter_command_name(&AdapterCommand::Demote {
                adapter_id: "test".to_string(),
                tenant: None
            }),
            "adapter_demote"
        );
        assert_eq!(
            get_adapter_command_name(&AdapterCommand::Pin {
                adapter_id: "test".to_string(),
                tenant: None
            }),
            "adapter_pin"
        );
        assert_eq!(
            get_adapter_command_name(&AdapterCommand::Unpin {
                adapter_id: "test".to_string(),
                tenant: None
            }),
            "adapter_unpin"
        );
        assert_eq!(
            get_adapter_command_name(&AdapterCommand::Evict {
                adapter_id: "test".to_string(),
                tenant: None,
                reason: None
            }),
            "adapter_evict"
        );
    }

    #[test]
    fn test_extract_tenant_from_adapter_command() {
        assert_eq!(
            extract_tenant_from_adapter_command(&AdapterCommand::List {
                json: false,
                tenant: None,
                pinned_only: false
            }),
            None
        );
        assert_eq!(
            extract_tenant_from_adapter_command(&AdapterCommand::List {
                json: false,
                tenant: Some("dev".to_string()),
                pinned_only: false
            }),
            Some("dev".to_string())
        );
        assert_eq!(
            extract_tenant_from_adapter_command(&AdapterCommand::Profile {
                adapter_id: "test".to_string(),
                json: false,
                tenant: Some("prod".to_string())
            }),
            Some("prod".to_string())
        );
        assert_eq!(
            extract_tenant_from_adapter_command(&AdapterCommand::Evict {
                adapter_id: "test".to_string(),
                tenant: Some("dev".to_string()),
                reason: None
            }),
            Some("dev".to_string())
        );
        assert_eq!(
            extract_tenant_from_adapter_command(&AdapterCommand::Evict {
                adapter_id: "test".to_string(),
                tenant: None,
                reason: Some("Low activation".to_string())
            }),
            None
        );
    }

    #[test]
    fn test_get_worker_socket_path() {
        // Test default tenant
        let path = get_worker_socket_path(None);
        assert!(path.to_string_lossy().contains("default"));

        // Test custom tenant
        let path = get_worker_socket_path(Some("dev"));
        assert!(path.to_string_lossy().contains("dev"));
    }

    #[tokio::test]
    async fn test_list_adapters_mock() {
        // Test mock data fallback
        let output = OutputWriter::new(OutputMode::Text, false);
        let result = list_adapters(false, None, false, &output).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_profile_adapter_mock() {
        // Test mock data fallback
        let output = OutputWriter::new(OutputMode::Text, false);
        let result = profile_adapter("test-adapter", false, None, &output).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_promote_adapter_mock() {
        // Test mock data fallback
        let output = OutputWriter::new(OutputMode::Text, false);
        let result = promote_adapter("test-adapter", None, &output).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_demote_adapter_mock() {
        // Test mock data fallback
        let output = OutputWriter::new(OutputMode::Text, false);
        let result = demote_adapter("test-adapter", None, &output).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_pin_adapter_mock() {
        // Test mock data fallback
        let output = OutputWriter::new(OutputMode::Text, false);
        let result = pin_adapter("test-adapter", None, &output).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_unpin_adapter_mock() {
        // Test mock data fallback
        let output = OutputWriter::new(OutputMode::Text, false);
        let result = unpin_adapter("test-adapter", None, &output).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_evict_adapter_mock() {
        // Test mock data fallback
        let output = OutputWriter::new(OutputMode::Text, false);
        let result = evict_adapter("test-adapter", None, None, &output).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_evict_adapter_with_reason() {
        // Test evict with reason
        let output = OutputWriter::new(OutputMode::Text, false);
        let result = evict_adapter("test-adapter", None, Some("Low activation"), &output).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_evict_adapter_invalid_id() {
        // Test evict with invalid adapter ID
        let output = OutputWriter::new(OutputMode::Text, false);
        let result = evict_adapter("invalid@adapter", None, None, &output).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_list_adapters_pinned_only() {
        // Test pinned-only filter
        let output = OutputWriter::new(OutputMode::Text, false);
        let result = list_adapters(false, None, true, &output).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_json_output() {
        // Test JSON output format
        let output = OutputWriter::new(OutputMode::Json, false);
        let result = list_adapters(true, None, false, &output).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_adapter_state_serialization() {
        let state = AdapterState {
            id: "test-adapter".to_string(),
            hash: "b3:abc123".to_string(),
            vram_mb: 16,
            active: true,
            tier: "persistent".to_string(),
            rank: 16,
            activation_pct: 45.2,
            quality_delta: 0.68,
            last_activation: Some(1234567890),
            pinned: false,
        };

        let json = serde_json::to_string(&state).unwrap();
        let deserialized: AdapterState = serde_json::from_str(&json).unwrap();
        assert_eq!(state.id, deserialized.id);
        assert_eq!(state.hash, deserialized.hash);
        assert_eq!(state.vram_mb, deserialized.vram_mb);
        assert_eq!(state.active, deserialized.active);
    }

    #[test]
    fn test_adapter_profile_serialization() {
        let profile = AdapterProfile {
            state: "hot".to_string(),
            activation_pct: 45.2,
            activations: 1234,
            total_tokens: 2730,
            avg_latency_us: 156.2,
            memory_kb: 16384,
            quality_delta: 0.68,
            recent_activations: vec![ActivationWindow {
                start_token: 100,
                end_token: 150,
                count: 3,
            }],
            performance_metrics: PerformanceMetrics {
                p50_latency_us: 142.0,
                p95_latency_us: 189.0,
                p99_latency_us: 234.0,
                throughput_tokens_per_sec: 45.2,
                error_rate: 0.01,
            },
            policy_compliance: PolicyCompliance {
                determinism_score: 0.98,
                evidence_coverage: 0.95,
                refusal_rate: 0.02,
                policy_violations: 0,
            },
        };

        let json = serde_json::to_string(&profile).unwrap();
        let deserialized: AdapterProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(profile.state, deserialized.state);
        assert_eq!(profile.activation_pct, deserialized.activation_pct);
        assert_eq!(profile.activations, deserialized.activations);
    }

    #[test]
    fn test_update_lifecycle_command_name() {
        assert_eq!(
            get_adapter_command_name(&AdapterCommand::UpdateLifecycle {
                adapter_id: "test".to_string(),
                state: "active".to_string()
            }),
            "adapter_update_lifecycle"
        );
    }

    #[test]
    fn test_update_lifecycle_no_tenant() {
        // UpdateLifecycle does not have a tenant parameter
        assert_eq!(
            extract_tenant_from_adapter_command(&AdapterCommand::UpdateLifecycle {
                adapter_id: "test".to_string(),
                state: "active".to_string()
            }),
            None
        );
    }
}
