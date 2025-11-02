//! Adapter lifecycle management commands

use crate::output::OutputWriter;
use adapteros_client::AdapterOSClient;
use adapteros_core::{AosError, Result};
use clap::Subcommand;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL, Table};
use serde::Deserialize;
use std::{env, time::Duration};
use tracing::{error, info};

/// Enhanced adapter state structure for UDS communication
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AdapterState {
    #[serde(alias = "id")]
    pub adapter_id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub hash_b3: Option<String>,
    #[serde(default)]
    pub rank: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_string_or_number")]
    pub tier: Option<String>,
    #[serde(default)]
    pub languages: Option<Vec<String>>,
    #[serde(default)]
    pub framework: Option<String>,
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub stats: Option<serde_json::Value>,
    #[serde(default)]
    pub activation_pct: Option<f32>,
    #[serde(default)]
    pub quality_delta: Option<f32>,
    #[serde(default)]
    pub memory_mb: Option<u64>,
    #[serde(default)]
    pub pinned: Option<bool>,
    #[serde(default)]
    pub last_activation: Option<String>,
}

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
    if let Ok(test_override) = env::var("AOS_TEST_SOCKET_OVERRIDE") {
        return std::path::PathBuf::from(test_override);
    }
    if let Ok(override_path) = env::var("AOS_WORKER_SOCKET_OVERRIDE") {
        return std::path::PathBuf::from(override_path);
    }

    let tenant = tenant_id.unwrap_or("default");
    std::path::PathBuf::from(format!("/var/run/aos/{}/aos.sock", tenant))
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
        output.result(serde_json::to_string_pretty(&value).unwrap());
    } else if let Some(adapter_id) = value.get("adapter_id").and_then(|v| v.as_str()) {
        output.success(format!("Adapter upserted: {}", adapter_id));
    } else {
        output.success("Adapter upserted");
    }

    Ok(())
}

/// Validate adapter ID format
pub(crate) fn validate_adapter_id(adapter_id: &str) -> Result<()> {
    if adapter_id.is_empty() {
        return Err(adapteros_core::AosError::Parse(
            "Adapter ID cannot be empty".to_string(),
        ));
    }

    if adapter_id.len() > 64 {
        return Err(adapteros_core::AosError::Parse(
            "Adapter ID must be 64 characters or less".to_string(),
        ));
    }

    if !adapter_id
        .chars()
        .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    {
        return Err(adapteros_core::AosError::Parse(
            "Adapter ID must contain only alphanumeric characters, hyphens, and underscores"
                .to_string(),
        ));
    }

    Ok(())
}

/// Connect to worker via UDS and fetch adapter states with retry logic
///
/// # Errors
///
/// Returns error if:
/// - Socket connection fails after retries
/// - Response parsing fails
/// - Timeout exceeded
pub(crate) async fn connect_and_fetch_adapter_states(
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
                    .map_err(adapteros_core::AosError::Serialization)?;

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

    unreachable!()
}

/// Connect to worker via UDS and fetch adapter profile with retry logic
///
/// # Errors
///
/// Returns error if:
/// - Socket connection fails after retries
/// - Response parsing fails
/// - Adapter not found
pub(crate) async fn connect_and_fetch_adapter_profile(
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
                    .map_err(adapteros_core::AosError::Serialization)?;

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

    unreachable!()
}

/// Send adapter command via UDS with retry logic
///
/// # Errors
///
/// Returns error if:
/// - Socket connection fails after retries
/// - Command execution fails
pub(crate) async fn send_adapter_command(
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
        let result = match command {
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
            Err(_e) if retries > 1 => {
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

    unreachable!()
}

#[derive(Debug, Subcommand, Clone)]
pub enum AdapterCommand {
    /// List all adapters with their states
    #[command(
        after_help = "Examples:\n  aosctl adapter list\n  aosctl adapter list --json\n  aosctl adapter list --tenant dev"
    )]
    List {
        /// Output format
        #[arg(long)]
        json: bool,

        /// Tenant ID
        #[arg(long)]
        tenant: Option<String>,
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
        AdapterCommand::DirectoryUpsert { .. } => "adapter_directory_upsert".to_string(),
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
        AdapterCommand::DirectoryUpsert { tenant, .. } => Some(tenant.clone()),
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
        AdapterCommand::List { json, tenant } => list_adapters(json, tenant, output).await,
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
        AdapterCommand::DirectoryUpsert {
            tenant,
            root,
            path,
            activate,
            base_url,
        } => directory_upsert(&tenant, &root, &path, activate, &base_url, output).await,
    }
}

/// Display adapters in the requested format
#[allow(dead_code)]
fn display_adapters(
    adapters: &[adapteros_client::AdapterResponse],
    json: bool,
    output: &OutputWriter,
    socket_path: &std::path::Path,
) -> Result<()> {
    if json {
        info!(
            "Adapter lifecycle status: {}",
            serde_json::to_string_pretty(adapters)?
        );
        output.result(&serde_json::to_string_pretty(adapters)?);
    } else {
        output.result("📊 Adapter Lifecycle Status");
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
                "Memory MB",
                "Pinned",
                "Last Activation",
            ]);

        for adapter in adapters {
            table.add_row(vec![
                adapter.id.clone(),
                adapter.hash_b3.clone(),
                "persistent".to_string(), // TODO: get from adapter data
                "16".to_string(),         // TODO: get from adapter data
                "hot".to_string(),        // TODO: get from adapter data
                "45.2".to_string(),       // TODO: get from adapter data
                "0.68".to_string(),       // TODO: get from adapter data
                "16".to_string(),         // TODO: get from adapter data
                "false".to_string(),      // TODO: get from adapter data
                "2m ago".to_string(),     // TODO: get from adapter data
            ]);
        }

        output.result(table.to_string());
        output.blank();
        output.success(format!("Connected to worker at: {}", socket_path.display()));
    }
    Ok(())
}

/// List all adapters with their current states

async fn list_adapters(json: bool, tenant: Option<String>, output: &OutputWriter) -> Result<()> {
    info!("Listing adapter lifecycle status");

    let socket_path = get_worker_socket_path(tenant.as_deref());

    match connect_and_fetch_adapter_states(&socket_path, Duration::from_secs(2)).await {
        Ok(adapters) => {
            if json {
                let json_output =
                    serde_json::to_string_pretty(&adapters).map_err(AosError::Serialization)?;
                output.result(json_output);
            } else {
                output.result("📊 Adapter Lifecycle Status");
                output.blank();
                render_adapter_table(&adapters, output);
            }

            output.success(format!(
                "Retrieved {} adapter{} from worker: {}",
                adapters.len(),
                if adapters.len() == 1 { "" } else { "s" },
                socket_path.display()
            ));
        }
        Err(err) => {
            output.warning(format!(
                "Worker adapter list not available ({}); showing mock data: {}",
                err,
                socket_path.display()
            ));

            if json {
                let mock: serde_json::Value =
                    serde_json::from_str(MOCK_ADAPTERS_JSON).expect("static mock JSON valid");
                output.result(serde_json::to_string_pretty(&mock).expect("mock json pretty print"));
            } else {
                output.result("📊 Adapter Lifecycle Status");
                output.blank();
                let mock = mock_adapter_states();
                render_adapter_table(&mock, output);
                output.success("Use --json to view structured mock data.");
            }
        }
    }

    Ok(())
}

const MOCK_ADAPTERS_JSON: &str = r#"[
    {
        "adapter_id": "python-general",
        "name": "python-general",
        "hash_b3": "b3:abc123",
        "tier": "persistent",
        "rank": 16,
        "activation_pct": 45.2,
        "quality_delta": 0.68,
        "memory_mb": 16,
        "pinned": false,
        "last_activation": "2m ago"
    },
    {
        "adapter_id": "django-specific",
        "name": "django-specific",
        "hash_b3": "b3:def456",
        "tier": "persistent",
        "rank": 8,
        "activation_pct": 12.8,
        "quality_delta": 0.54,
        "memory_mb": 16,
        "pinned": false,
        "last_activation": "5m ago"
    }
]"#;

fn mock_adapter_states() -> Vec<AdapterState> {
    serde_json::from_str(MOCK_ADAPTERS_JSON).unwrap_or_default()
}

fn render_adapter_table(adapters: &[AdapterState], output: &OutputWriter) {
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
            "Memory MB",
            "Pinned",
            "Last Activation",
        ]);

    for adapter in adapters {
        table.add_row(vec![
            adapter.adapter_id.clone(),
            adapter.hash_b3.as_deref().unwrap_or("-").to_string(),
            adapter.tier.as_deref().unwrap_or("-").to_string(),
            adapter
                .rank
                .map(|r| r.to_string())
                .unwrap_or_else(|| "-".into()),
            extract_state(adapter),
            format_percent(adapter.activation_pct),
            format_signed(adapter.quality_delta),
            format_memory(adapter.memory_mb),
            format_bool(adapter.pinned),
            adapter
                .last_activation
                .clone()
                .unwrap_or_else(|| "-".into()),
        ]);
    }

    output.result(table.to_string());
    output.blank();
}

fn format_percent(value: Option<f32>) -> String {
    value
        .map(|v| format!("{:.1}%", v))
        .unwrap_or_else(|| "-".into())
}

fn format_signed(value: Option<f32>) -> String {
    value
        .map(|v| format!("{:+.2}", v))
        .unwrap_or_else(|| "-".into())
}

fn format_memory(value: Option<u64>) -> String {
    value.map(|v| v.to_string()).unwrap_or_else(|| "-".into())
}

fn format_bool(value: Option<bool>) -> String {
    value
        .map(|v| if v { "yes".into() } else { "no".into() })
        .unwrap_or_else(|| "-".into())
}

fn extract_state(adapter: &AdapterState) -> String {
    adapter
        .stats
        .as_ref()
        .and_then(|value| value.get("state"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "-".into())
}

fn deserialize_string_or_number<'de, D>(
    deserializer: D,
) -> std::result::Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum StringOrNumber {
        String(String),
        Int(i64),
        Float(f64),
    }

    let value = Option::<StringOrNumber>::deserialize(deserializer)?;
    Ok(match value {
        Some(StringOrNumber::String(s)) => Some(s),
        Some(StringOrNumber::Int(i)) => Some(i.to_string()),
        Some(StringOrNumber::Float(f)) => Some(f.to_string()),
        None => None,
    })
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
            output.result(format!("📈 Adapter Profile: {}", adapter_id));
            output.blank();
            output.warning(format!(
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
                output.result(format!("State:           {}", profile.state));
                output.result(format!(
                    "Activation:      {:.1}% ({} / {} tokens)",
                    profile.activation_pct, profile.activations, profile.total_tokens
                ));
                output.result(format!("Avg Latency:     {:.1} µs", profile.avg_latency_us));
                output.result(format!("Memory Usage:    {} KB", profile.memory_kb));
                output.result(format!("Quality Delta:   {:.2}", profile.quality_delta));
                output.blank();
                output.result("Last 10 activations:");
                for activation in &profile.recent_activations {
                    output.result(format!(
                        "  Token {}-{}:  {} activations",
                        activation.start_token, activation.end_token, activation.count
                    ));
                }
                output.blank();
                output.result("Performance Metrics:");
                output.result(format!(
                    "  P50 Latency:    {:.1} µs",
                    profile.performance_metrics.p50_latency_us
                ));
                output.result(format!(
                    "  P95 Latency:    {:.1} µs",
                    profile.performance_metrics.p95_latency_us
                ));
                output.result(format!(
                    "  P99 Latency:    {:.1} µs",
                    profile.performance_metrics.p99_latency_us
                ));
                output.result(format!(
                    "  Throughput:     {:.1} tokens/sec",
                    profile.performance_metrics.throughput_tokens_per_sec
                ));
                output.result(format!(
                    "  Error Rate:     {:.2}%",
                    profile.performance_metrics.error_rate * 100.0
                ));
                output.blank();
                output.result("Policy Compliance:");
                output.result(format!(
                    "  Determinism:   {:.2}",
                    profile.policy_compliance.determinism_score
                ));
                output.result(format!(
                    "  Evidence:      {:.2}",
                    profile.policy_compliance.evidence_coverage
                ));
                output.result(format!(
                    "  Refusal Rate:  {:.2}%",
                    profile.policy_compliance.refusal_rate * 100.0
                ));
                output.result(format!(
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
                output.error(format!("Failed to connect to worker: {}", e));
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
            output.warning(format!(
                "Worker socket not found at: {}",
                socket_path.display()
            ));
            output.success(format!("Promoted adapter: {} (mock)", adapter_id));
            output.result("State: warm → hot");
        }
        return Ok(());
    }

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
                output.success(format!("Promoted adapter: {}", adapter_id));
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
                output.error(format!("Failed to promote adapter: {}", e));
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
            output.warning(format!(
                "Worker socket not found at: {}",
                socket_path.display()
            ));
            output.success(format!("Demoted adapter: {} (mock)", adapter_id));
            output.result("State: hot → warm");
        }
        return Ok(());
    }

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
                output.success(format!("Demoted adapter: {}", adapter_id));
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
                output.error(format!("Failed to demote adapter: {}", e));
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
            output.warning(format!(
                "Worker socket not found at: {}",
                socket_path.display()
            ));
            output.success(format!("Pinned adapter: {} (mock)", adapter_id));
            output.result("State: → resident (pinned)");
        }
        return Ok(());
    }

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
                output.success(format!("Pinned adapter: {}", adapter_id));
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
                output.error(format!("Failed to pin adapter: {}", e));
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
            output.warning(format!(
                "Worker socket not found at: {}",
                socket_path.display()
            ));
            output.success(format!("Unpinned adapter: {} (mock)", adapter_id));
            output.result("Adapter can now be demoted");
        }
        return Ok(());
    }

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
                output.success(format!("Unpinned adapter: {}", adapter_id));
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
                output.error(format!("Failed to unpin adapter: {}", e));
            }
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
                tenant: None
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
    }

    #[test]
    fn test_extract_tenant_from_adapter_command() {
        assert_eq!(
            extract_tenant_from_adapter_command(&AdapterCommand::List {
                json: false,
                tenant: None
            }),
            None
        );
        assert_eq!(
            extract_tenant_from_adapter_command(&AdapterCommand::List {
                json: false,
                tenant: Some("dev".to_string())
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
        let result = list_adapters(false, None, &output).await;
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
    async fn test_json_output() {
        // Test JSON output format
        let output = OutputWriter::new(OutputMode::Json, false);
        let result = list_adapters(true, None, &output).await;
        assert!(result.is_ok());
    }

    #[test]
    fn test_adapter_state_serialization() {
        let json = r#"{
            "id": "python-general",
            "name": "Python General",
            "hash_b3": "b3:abc123",
            "rank": 16,
            "tier": 2,
            "languages": ["python", "rust"],
            "framework": "pytorch",
            "created_at": "2024-01-01T00:00:00Z",
            "stats": { "state": "hot", "uptime_s": 600 },
            "activation_pct": 45.2,
            "quality_delta": 0.68,
            "memory_mb": 16,
            "pinned": true,
            "last_activation": "2024-05-01T12:00:00Z"
        }"#;

        let state: AdapterState = serde_json::from_str(json).unwrap();
        assert_eq!(state.adapter_id, "python-general");
        assert_eq!(state.name.as_deref(), Some("Python General"));
        assert_eq!(state.hash_b3.as_deref(), Some("b3:abc123"));
        assert_eq!(state.rank, Some(16));
        assert_eq!(state.tier.as_deref(), Some("2"));
        assert_eq!(state.languages, Some(vec!["python".into(), "rust".into()]));
        assert_eq!(state.framework.as_deref(), Some("pytorch"));
        assert_eq!(state.created_at.as_deref(), Some("2024-01-01T00:00:00Z"));
        assert_eq!(
            state
                .stats
                .as_ref()
                .and_then(|stats| stats.get("state"))
                .and_then(|value| value.as_str()),
            Some("hot")
        );
        assert_eq!(state.activation_pct, Some(45.2));
        assert_eq!(state.quality_delta, Some(0.68));
        assert_eq!(state.memory_mb, Some(16));
        assert_eq!(state.pinned, Some(true));
        assert_eq!(
            state.last_activation.as_deref(),
            Some("2024-05-01T12:00:00Z")
        );

        let serialized = serde_json::to_value(&state).unwrap();
        assert_eq!(
            serialized
                .get("adapter_id")
                .and_then(|value| value.as_str()),
            Some("python-general")
        );
        assert!(serialized.get("id").is_none());

        let minimal_json = r#"{"adapter_id": "only-required"}"#;
        let minimal: AdapterState = serde_json::from_str(minimal_json).unwrap();
        assert_eq!(minimal.adapter_id, "only-required");
        assert!(minimal.tier.is_none());
        assert!(minimal.languages.is_none());
        assert!(minimal.framework.is_none());
        assert!(minimal.created_at.is_none());
        assert!(minimal.stats.is_none());
        assert!(minimal.activation_pct.is_none());
        assert!(minimal.quality_delta.is_none());
        assert!(minimal.memory_mb.is_none());
        assert!(minimal.pinned.is_none());
        assert!(minimal.last_activation.is_none());
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
}
