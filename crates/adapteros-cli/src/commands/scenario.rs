use crate::commands::dev::{self, DevCommand};
use crate::output::OutputWriter;
use adapteros_api_types::InferRequest;
use adapteros_core::{AosError, Result};
use adapteros_db::adapters::Adapter;
use adapteros_db::Db;
use adapteros_scenarios::{AdapterConfig, ScenarioConfig, ScenarioLoader};
use clap::Subcommand;
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use std::time::{Duration, Instant};
use tokio::time::sleep;

#[derive(Debug, Subcommand, Clone)]
pub struct ScenarioCommand {
    #[command(subcommand)]
    pub subcommand: ScenarioSubcommand,
}

#[derive(Debug, Subcommand, Clone)]
pub enum ScenarioSubcommand {
    /// List available scenarios from configs/scenarios
    List {
        /// Override scenario directory (default: configs/scenarios)
        #[arg(long)]
        scenario_dir: Option<String>,
    },
    /// Check if a scenario is ready to run
    Check {
        /// Scenario ID (file name without extension)
        #[arg(long)]
        name: String,

        /// Server URL for readiness polling
        #[arg(long, env = "AOS_SERVER_URL", default_value = "http://localhost:8080")]
        server_url: String,

        /// Maximum seconds to wait for /system/ready
        #[arg(long, default_value_t = 60)]
        ready_timeout: u64,

        /// Seconds between readiness polls
        #[arg(long, default_value_t = 2)]
        ready_interval: u64,

        /// Override scenario directory (default: configs/scenarios)
        #[arg(long)]
        scenario_dir: Option<String>,

        /// Require adapter to be preloaded (load_state != cold)
        #[arg(long)]
        require_loaded: bool,

        /// Run a 1-token inference probe
        #[arg(long)]
        chat_probe: bool,
    },
    /// Start dev stack then run scenario check
    Up {
        /// Scenario ID (file name without extension)
        #[arg(long)]
        name: String,

        /// Start UI dev server
        #[arg(long)]
        ui: bool,

        /// Reset database before starting
        #[arg(long)]
        db_reset: bool,

        /// Skip database migrations
        #[arg(long)]
        skip_migrations: bool,

        /// Server URL for readiness polling
        #[arg(long, env = "AOS_SERVER_URL", default_value = "http://localhost:8080")]
        server_url: String,

        /// Run a 1-token inference probe
        #[arg(long)]
        chat_probe: bool,

        /// Override scenario directory (default: configs/scenarios)
        #[arg(long)]
        scenario_dir: Option<String>,
    },
}

#[derive(Debug, Serialize)]
struct ScenarioCheckRow {
    check: String,
    status: String,
    detail: String,
}

#[derive(Debug, Serialize)]
struct ScenarioListRow {
    id: String,
    description: String,
    tenant: String,
    model: String,
    adapter: String,
}

#[derive(Debug, Deserialize)]
struct ReadyComponent {
    component: String,
    status: String,
    message: String,
}

#[derive(Debug, Deserialize)]
struct SystemReadyResponse {
    ready: bool,
    overall_status: Option<String>,
    reason: Option<String>,
    #[serde(default)]
    components: Vec<ReadyComponent>,
}

pub async fn run(cmd: ScenarioCommand, output: &OutputWriter) -> Result<()> {
    match cmd.subcommand {
        ScenarioSubcommand::List { scenario_dir } => list_scenarios(scenario_dir, output),
        ScenarioSubcommand::Check {
            name,
            server_url,
            ready_timeout,
            ready_interval,
            scenario_dir,
            require_loaded,
            chat_probe,
        } => {
            let loader = loader_from_arg(scenario_dir);
            run_check(
                &loader,
                &name,
                &server_url,
                ready_timeout,
                ready_interval,
                require_loaded,
                chat_probe,
                output,
            )
            .await
        }
        ScenarioSubcommand::Up {
            name,
            ui,
            db_reset,
            skip_migrations,
            server_url,
            chat_probe,
            scenario_dir,
        } => {
            dev::handle_dev_command(
                DevCommand::Up {
                    ui,
                    db_reset,
                    skip_migrations,
                },
                output,
            )
            .await?;

            let loader = loader_from_arg(scenario_dir);
            run_check(
                &loader,
                &name,
                &server_url,
                90,
                2,
                false,
                chat_probe,
                output,
            )
            .await
        }
    }
}

fn loader_from_arg(dir: Option<String>) -> ScenarioLoader {
    match dir {
        Some(path) => ScenarioLoader::with_root(path),
        None => ScenarioLoader::from_env(),
    }
}

fn list_scenarios(scenario_dir: Option<String>, output: &OutputWriter) -> Result<()> {
    let loader = loader_from_arg(scenario_dir);
    let scenarios = loader.list()?;

    if scenarios.is_empty() {
        output.warning("No scenarios found");
        return Ok(());
    }

    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["ID", "Description", "Tenant", "Model", "Adapter"]);

    let mut rows: Vec<ScenarioListRow> = Vec::new();
    for scenario in scenarios {
        let adapter_label = scenario
            .adapter
            .id
            .clone()
            .or(scenario.adapter.name.clone())
            .unwrap_or_else(|| "-".to_string());
        let row = ScenarioListRow {
            id: scenario.id.to_string(),
            description: scenario
                .description
                .clone()
                .unwrap_or_else(|| "Scenario".to_string()),
            tenant: scenario.tenant.id.clone(),
            model: scenario.model.id.clone(),
            adapter: adapter_label.clone(),
        };
        rows.push(row);
        table.add_row(vec![
            Cell::new(scenario.id.to_string()),
            Cell::new(scenario.description.unwrap_or_else(|| "-".to_string())),
            Cell::new(scenario.tenant.id),
            Cell::new(scenario.model.id.clone()),
            Cell::new(adapter_label),
        ]);
    }

    output.table(&table as &dyn std::fmt::Display, Some(&rows))?;
    Ok(())
}

async fn run_check(
    loader: &ScenarioLoader,
    name: &str,
    server_url: &str,
    ready_timeout: u64,
    ready_interval: u64,
    require_loaded_flag: bool,
    chat_probe_flag: bool,
    output: &OutputWriter,
) -> Result<()> {
    let scenario = loader.load(name)?;
    let mut rows: Vec<ScenarioCheckRow> = Vec::new();

    let ready = poll_system_ready(server_url, ready_timeout, ready_interval).await?;
    rows.push(ScenarioCheckRow {
        check: "system_ready".to_string(),
        status: status_label(ready.ready),
        detail: ready.reason.unwrap_or_else(|| {
            ready
                .overall_status
                .unwrap_or_else(|| "unknown".to_string())
        }),
    });

    if !ready.ready {
        let table = build_table(&rows);
        output.table(&table as &dyn std::fmt::Display, Some(&rows))?;
        return Err(AosError::Config(
            "System is not ready; aborting scenario check".to_string(),
        ));
    }

    let db = Db::connect_env().await?;

    // Base model check
    let model = match db.get_model(&scenario.model.id).await? {
        Some(m) => m,
        None => {
            rows.push(ScenarioCheckRow {
                check: "base_model".to_string(),
                status: status_label(false),
                detail: format!("Model {} not found", scenario.model.id),
            });
            let table = build_table(&rows);
            output.table(&table as &dyn std::fmt::Display, Some(&rows))?;
            return Err(AosError::Config(format!(
                "Base model {} missing",
                scenario.model.id
            )));
        }
    };
    let model_status = model
        .import_status
        .as_deref()
        .or(model.status.as_deref())
        .unwrap_or("unknown");
    let model_ok = model_status == "available";
    rows.push(ScenarioCheckRow {
        check: "base_model".to_string(),
        status: status_label(model_ok),
        detail: format!("status={}", model_status),
    });

    // Adapter check
    let adapters = db.list_all_adapters_system().await?;
    let adapter_record = find_adapter(&scenario.adapter, &scenario.tenant.id, &adapters);
    if let Some(adapter) = adapter_record {
        let lifecycle_required = scenario
            .adapter
            .lifecycle_state
            .as_deref()
            .unwrap_or("active");
        let lifecycle_ok = adapter.lifecycle_state == lifecycle_required;
        let require_loaded = require_loaded_flag
            || scenario.adapter.require_loaded.unwrap_or(false)
            || scenario
                .adapter
                .load_state
                .as_deref()
                .map(|s| s != "cold")
                .unwrap_or(false);
        let load_state = adapter.load_state.clone();
        let load_ok = !require_loaded || load_state != "cold";
        rows.push(ScenarioCheckRow {
            check: "adapter".to_string(),
            status: status_label(lifecycle_ok && load_ok),
            detail: format!(
                "lifecycle={}, load_state={}",
                adapter.lifecycle_state, adapter.load_state
            ),
        });
    } else {
        rows.push(ScenarioCheckRow {
            check: "adapter".to_string(),
            status: status_label(false),
            detail: "Adapter not found".to_string(),
        });
        let table = build_table(&rows);
        output.table(&table as &dyn std::fmt::Display, Some(&rows))?;
        return Err(AosError::Config("Adapter missing".to_string()));
    }

    // Optional probe
    if chat_probe_flag
        || scenario
            .chat
            .as_ref()
            .and_then(|c| c.probe_enabled)
            .unwrap_or(false)
    {
        let adapter_ids: Vec<String> = scenario
            .adapter
            .id
            .clone()
            .or(scenario.adapter.name.clone())
            .into_iter()
            .collect();
        let prompt = scenario
            .chat
            .as_ref()
            .and_then(|c| c.probe_prompt.clone())
            .unwrap_or_else(|| "ping".to_string());
        let seed = scenario.chat.as_ref().and_then(|c| c.seed);
        let max_tokens = scenario
            .chat
            .as_ref()
            .and_then(|c| c.probe_max_tokens)
            .unwrap_or(1);
        let probe_result = run_probe(
            server_url,
            &scenario.tenant.id,
            &scenario.model.id,
            adapter_ids,
            prompt,
            seed,
            max_tokens,
        )
        .await;
        let probe_ok = probe_result.is_ok();
        rows.push(ScenarioCheckRow {
            check: "probe".to_string(),
            status: status_label(probe_ok),
            detail: probe_result.unwrap_or_else(|e| format!("probe failed: {}", e)),
        });
    }

    let table = build_table(&rows);
    output.table(&table as &dyn std::fmt::Display, Some(&rows))?;

    if rows.iter().all(|r| r.status == "pass") {
        output.success(&format!("Scenario '{}' is ready", scenario.id));
        Ok(())
    } else {
        Err(AosError::Config(format!(
            "Scenario '{}' is not ready",
            scenario.id
        )))
    }
}

fn build_table(rows: &[ScenarioCheckRow]) -> Table {
    let mut table = Table::new();
    table.load_preset(UTF8_FULL);
    table.set_header(vec!["Check", "Status", "Detail"]);
    for row in rows {
        table.add_row(vec![
            Cell::new(&row.check),
            Cell::new(&row.status),
            Cell::new(&row.detail),
        ]);
    }
    table
}

fn status_label(ok: bool) -> String {
    if ok {
        "pass".to_string()
    } else {
        "fail".to_string()
    }
}

fn find_adapter<'a>(
    desired: &AdapterConfig,
    tenant: &str,
    adapters: &'a [Adapter],
) -> Option<&'a Adapter> {
    adapters.iter().find(|a| {
        a.tenant_id == tenant
            && (desired.id.as_ref().map(|id| a.id == *id).unwrap_or(false)
                || desired
                    .name
                    .as_ref()
                    .map(|name| a.name == *name || a.adapter_name.as_deref() == Some(name.as_str()))
                    .unwrap_or(false))
    })
}

async fn poll_system_ready(
    server_url: &str,
    timeout_secs: u64,
    interval_secs: u64,
) -> Result<SystemReadyResponse> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| AosError::Http(e.to_string()))?;

    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    let url = format!("{}/system/ready", server_url.trim_end_matches('/'));

    loop {
        let resp = client.get(&url).send().await;
        if let Ok(res) = resp {
            if res.status().is_success() {
                let parsed: SystemReadyResponse = res
                    .json()
                    .await
                    .map_err(|e| AosError::Http(format!("decode /system/ready: {}", e)))?;
                if parsed.ready {
                    return Ok(parsed);
                }
            }
        }

        if Instant::now() >= deadline {
            return Err(AosError::Config(format!("Timed out waiting for {}", url)));
        }

        sleep(Duration::from_secs(interval_secs)).await;
    }
}

async fn run_probe(
    server_url: &str,
    tenant_id: &str,
    model_id: &str,
    adapters: Vec<String>,
    prompt: String,
    seed: Option<u64>,
    max_tokens: usize,
) -> Result<String> {
    let client = Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| AosError::Http(e.to_string()))?;

    let base = server_url.trim_end_matches('/');
    let body = InferRequest {
        prompt,
        model: Some(model_id.to_string()),
        adapters: Some(adapters),
        max_tokens: Some(max_tokens),
        seed,
        tenant_id: Some(tenant_id.to_string()),
        ..Default::default()
    };

    let resp = client
        .post(&format!("{}/v1/infer", base))
        .json(&body)
        .send()
        .await
        .map_err(|e| AosError::Http(format!("probe request failed: {}", e)))?;

    let status = resp.status();
    let text = resp.text().await.unwrap_or_default();
    if !status.is_success() {
        return Err(AosError::Config(format!(
            "probe failed: {} {}",
            status, text
        )));
    }

    Ok("probe succeeded".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_adapter(id: &str, name: &str, tenant: &str) -> Adapter {
        Adapter {
            id: id.to_string(),
            tenant_id: tenant.to_string(),
            name: name.to_string(),
            tier: "tier".to_string(),
            hash_b3: "hash".to_string(),
            rank: 0,
            alpha: 1.0,
            targets_json: "[]".to_string(),
            acl_json: None,
            adapter_id: None,
            languages_json: None,
            framework: None,
            active: 1,
            category: "cat".to_string(),
            scope: "scope".to_string(),
            framework_id: None,
            framework_version: None,
            repo_id: None,
            commit_sha: None,
            intent: None,
            current_state: "active".to_string(),
            pinned: 0,
            memory_bytes: 0,
            last_activated: None,
            activation_count: 0,
            expires_at: None,
            load_state: "warm".to_string(),
            last_loaded_at: None,
            aos_file_path: None,
            aos_file_hash: None,
            adapter_name: None,
            tenant_namespace: None,
            domain: None,
            purpose: None,
            revision: None,
            parent_id: None,
            fork_type: None,
            fork_reason: None,
            version: "v1".to_string(),
            lifecycle_state: "active".to_string(),
            archived_at: None,
            archived_by: None,
            archive_reason: None,
            purged_at: None,
            base_model_id: None,
            manifest_schema_version: None,
            content_hash_b3: None,
            provenance_json: None,
            created_at: "now".to_string(),
            updated_at: "now".to_string(),
        }
    }

    #[test]
    fn status_label_formats() {
        assert_eq!(status_label(true), "pass");
        assert_eq!(status_label(false), "fail");
    }

    #[test]
    fn find_adapter_matches_by_id_or_name() {
        let adapters = vec![
            dummy_adapter("a", "adapter-a", "tenant-1"),
            dummy_adapter("b", "adapter-b", "tenant-2"),
        ];

        let config = AdapterConfig {
            id: Some("a".to_string()),
            name: None,
            base_model_id: None,
            require_loaded: None,
            lifecycle_state: None,
            load_state: None,
        };
        assert!(find_adapter(&config, "tenant-1", &adapters).is_some());

        let config_by_name = AdapterConfig {
            id: None,
            name: Some("adapter-b".to_string()),
            base_model_id: None,
            require_loaded: None,
            lifecycle_state: None,
            load_state: None,
        };
        assert!(find_adapter(&config_by_name, "tenant-2", &adapters).is_some());
    }
}
