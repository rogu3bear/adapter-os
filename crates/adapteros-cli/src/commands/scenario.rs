use crate::commands::dev::{self, DevCommand};
use crate::commands::replay_bundle::{handle_replay_command, ReplaySubcommand};
use crate::output::OutputWriter;
use crate::scenarios::{ScenarioConfig, ScenarioLoader};
use adapteros_api_types::InferRequest;
use adapteros_core::{AosError, Result};
use adapteros_db::Db;
use clap::Subcommand;
use comfy_table::{presets::UTF8_FULL, Cell, Table};
use reqwest::Client;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::env;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tokio::process::Command as TokioCommand;
use tokio::time::sleep;

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

        /// Harness mode: skip dev up (for tests)
        #[arg(long, hide = true)]
        harness: bool,

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

        /// Train docs for the scenario after services start
        #[arg(long)]
        train: bool,

        /// Override scenario directory (default: configs/scenarios)
        #[arg(long)]
        scenario_dir: Option<String>,
    },
    /// Run a scenario chat once and record it to a replay bundle
    Run {
        /// Scenario ID (file name without extension)
        #[arg(default_value = "doc-chat")]
        name: String,

        /// Output bundle path
        #[arg(long, value_name = "BUNDLE")]
        record: PathBuf,

        /// Server URL for the chat call
        #[arg(long, env = "AOS_SERVER_URL", default_value = "http://localhost:8080")]
        server_url: String,

        /// Override scenario directory (default: configs/scenarios)
        #[arg(long)]
        scenario_dir: Option<String>,
    },
    /// Verify a recorded scenario bundle for determinism
    Verify {
        /// Scenario ID (file name without extension)
        #[arg(default_value = "doc-chat")]
        name: String,

        /// Input bundle path
        #[arg(long, value_name = "BUNDLE")]
        bundle: PathBuf,

        /// Number of runs to verify (defaults to scenario replay.runs or 5)
        #[arg(long)]
        runs: Option<u32>,

        /// Override scenario directory (default: configs/scenarios)
        #[arg(long)]
        scenario_dir: Option<String>,
    },
    /// Internal: perform a single chat call for a scenario
    #[command(hide = true)]
    ChatOnce {
        /// Scenario ID (file name without extension)
        #[arg(default_value = "doc-chat")]
        name: String,

        /// Server URL for chat call
        #[arg(long, env = "AOS_SERVER_URL", default_value = "http://localhost:8080")]
        server_url: String,

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

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct ReadyComponent {
    component: String,
    status: String,
    message: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SystemReadyResponse {
    ready: bool,
    overall_status: Option<String>,
    reason: Option<String>,
    #[serde(default)]
    components: Vec<ReadyComponent>,
}

pub async fn run(cmd: ScenarioSubcommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        ScenarioSubcommand::List { scenario_dir } => list_scenarios(scenario_dir, output),
        ScenarioSubcommand::Check {
            name,
            server_url,
            ready_timeout,
            ready_interval,
            // Adapter check is always required for explicit checks
            // (skip flag is only used for pre-training readiness).
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
                false,
                false,
                require_loaded,
                chat_probe,
                output,
            )
            .await
        }
        ScenarioSubcommand::Up {
            name,
            harness,
            ui,
            db_reset,
            skip_migrations,
            server_url,
            chat_probe,
            train,
            scenario_dir,
        } => {
            let harness_env = matches!(
                std::env::var("AOS_SCENARIO_HARNESS").as_deref(),
                Ok("1") | Ok("true") | Ok("yes")
            );
            let harness_mode = harness || harness_env;

            if harness_mode {
                output.info("Harness mode: skipping dev up, running readiness only");
            } else {
                dev::handle_dev_command(
                    DevCommand::Up {
                        ui,
                        db_reset,
                        skip_migrations,
                    },
                    output,
                )
                .await?;
            }

            let loader = loader_from_arg(scenario_dir.clone());
            run_check(
                &loader,
                &name,
                &server_url,
                90,
                2,
                harness_mode,
                train,
                false,
                chat_probe,
                output,
            )
            .await?;

            if train {
                run_training_for_scenario(&loader, &name, output).await?;
                // Re-run full readiness after training to enforce adapter/base-model binding
                run_check(
                    &loader,
                    &name,
                    &server_url,
                    90,
                    2,
                    harness_mode,
                    false,
                    false,
                    chat_probe,
                    output,
                )
                .await?;
            }

            Ok(())
        }
        ScenarioSubcommand::Run {
            name,
            record,
            server_url,
            scenario_dir,
        } => {
            let loader = loader_from_arg(scenario_dir.clone());
            let harness_env = matches!(
                std::env::var("AOS_SCENARIO_HARNESS").as_deref(),
                Ok("1") | Ok("true") | Ok("yes")
            );
            run_check(
                &loader,
                &name,
                &server_url,
                60,
                2,
                harness_env,
                false,
                false,
                false,
                output,
            )
            .await?;
            run_recorded_chat(&loader, &name, &server_url, record, scenario_dir, output).await
        }
        ScenarioSubcommand::Verify {
            name,
            bundle,
            runs,
            scenario_dir,
        } => {
            let loader = loader_from_arg(scenario_dir);
            verify_recorded_bundle(&loader, &name, bundle, runs, output).await
        }
        ScenarioSubcommand::ChatOnce {
            name,
            server_url,
            scenario_dir,
        } => {
            let loader = loader_from_arg(scenario_dir);
            run_chat_once(&loader, &name, &server_url, output).await
        }
    }
}

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .ancestors()
        .nth(2)
        .map(Path::to_path_buf)
        .unwrap_or(manifest_dir)
}

fn default_scenario_root() -> PathBuf {
    workspace_root().join(crate::scenarios::DEFAULT_SCENARIO_DIR)
}

fn resolve_workspace_path(path: &str) -> PathBuf {
    let as_path = PathBuf::from(path);
    if as_path.is_absolute() {
        as_path
    } else {
        workspace_root().join(as_path)
    }
}

fn loader_from_arg(dir: Option<String>) -> ScenarioLoader {
    match dir {
        Some(path) => ScenarioLoader::with_root(resolve_workspace_path(&path)),
        None => {
            let root = env::var(crate::scenarios::ENV_SCENARIO_DIR)
                .map(|p| resolve_workspace_path(&p))
                .unwrap_or_else(|_| default_scenario_root());
            ScenarioLoader::with_root(root)
        }
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

#[allow(clippy::too_many_arguments)]
async fn run_check(
    loader: &ScenarioLoader,
    name: &str,
    server_url: &str,
    ready_timeout: u64,
    ready_interval: u64,
    skip_ready_check: bool,
    skip_adapter_check: bool,
    require_loaded_flag: bool,
    chat_probe_flag: bool,
    output: &OutputWriter,
) -> Result<()> {
    let scenario = loader.load(name)?;
    let mut rows: Vec<ScenarioCheckRow> = Vec::new();
    let harness_env = matches!(
        std::env::var("AOS_SCENARIO_HARNESS").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    );

    let ready = if skip_ready_check {
        SystemReadyResponse {
            ready: true,
            overall_status: Some("skipped".to_string()),
            reason: Some("ready_check_skipped".to_string()),
            components: Vec::new(),
        }
    } else {
        poll_system_ready(server_url, ready_timeout, ready_interval).await?
    };
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
    if !skip_adapter_check {
        let adapters = db.list_all_adapters_system().await?;
        let desired_id = scenario
            .adapter
            .id
            .as_deref()
            .ok_or_else(|| AosError::Config("Scenario adapter.id is required".to_string()))?;
        let adapter_record = adapters.iter().find(|a| {
            a.tenant_id == scenario.tenant.id
                && a.adapter_id
                    .as_deref()
                    .map(|id| id == desired_id)
                    .unwrap_or(false)
        });
        if let Some(adapter) = adapter_record {
            let adapter_base = adapter.base_model_id.as_deref();
            let adapter_base_match = adapter_base
                .map(|id| id == scenario.model.id)
                .unwrap_or(false);
            let base_model_ok = adapter_base_match;
            let lifecycle_required = scenario
                .adapter
                .lifecycle_state
                .as_deref()
                .unwrap_or("active");
            let lifecycle_ok = adapter.lifecycle_state == lifecycle_required;
            let model_warmup_required = scenario.model.warmup;
            let require_loaded = require_loaded_flag
                || model_warmup_required
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
                check: "adapter_id".to_string(),
                status: status_label(true),
                detail: desired_id.to_string(),
            });
            rows.push(ScenarioCheckRow {
                check: "base_model_match".to_string(),
                status: status_label(base_model_ok),
                detail: format!(
                    "expected={}, adapter_base_model_id={}",
                    scenario.model.id,
                    adapter_base.unwrap_or("unset")
                ),
            });
            rows.push(ScenarioCheckRow {
                check: "adapter".to_string(),
                status: status_label(lifecycle_ok && load_ok),
                detail: format!(
                    "lifecycle={}, load_state={}, require_loaded={}, warmup={}",
                    adapter.lifecycle_state,
                    adapter.load_state,
                    require_loaded,
                    model_warmup_required
                ),
            });
            if !base_model_ok {
                let table = build_table(&rows);
                output.table(&table as &dyn std::fmt::Display, Some(&rows))?;
                return Err(AosError::Config(
                    "Scenario adapter id does not match registered adapter".to_string(),
                ));
            }
        } else {
            rows.push(ScenarioCheckRow {
                check: "adapter".to_string(),
                status: status_label(false),
                detail: "Scenario adapter id does not match registered adapter".to_string(),
            });
            let table = build_table(&rows);
            output.table(&table as &dyn std::fmt::Display, Some(&rows))?;
            return Err(AosError::Config(
                "Scenario adapter id does not match registered adapter".to_string(),
            ));
        }
    }

    // Optional probe
    if (chat_probe_flag
        || scenario
            .chat
            .as_ref()
            .and_then(|c| c.probe_enabled)
            .unwrap_or(false))
        && !harness_env
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
        output.success(format!("Scenario '{}' is ready", scenario.id));
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
        .post(format!("{}/v1/infer", base))
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

async fn run_recorded_chat(
    loader: &ScenarioLoader,
    name: &str,
    server_url: &str,
    record: PathBuf,
    scenario_dir: Option<String>,
    output: &OutputWriter,
) -> Result<()> {
    let scenario_root = loader.root().to_path_buf();
    let scenario_arg = scenario_dir.unwrap_or_else(|| scenario_root.to_string_lossy().to_string());
    let chat_cmd = build_chat_command(name, server_url, Some(scenario_arg))?;
    handle_replay_command(
        ReplaySubcommand::Record {
            out: record,
            cmd: chat_cmd,
        },
        output,
    )
    .await
    .map_err(|e| AosError::Config(format!("record failed: {e}")))?;
    Ok(())
}

async fn run_training_for_scenario(
    loader: &ScenarioLoader,
    name: &str,
    output: &OutputWriter,
) -> Result<()> {
    let scenario = loader.load(name)?;
    let training = scenario.training.as_ref().ok_or_else(|| {
        AosError::Config("--train requires [training] in the scenario config".to_string())
    })?;
    let docs_dir = training.clone().docs_path.ok_or_else(|| {
        AosError::Config("Scenario training.docs_path is required for --train".to_string())
    })?;
    let docs_dir = resolve_workspace_path(&docs_dir)
        .to_string_lossy()
        .to_string();
    let adapter_id = training
        .adapter_id
        .clone()
        .or_else(|| scenario.adapter.id.clone())
        .ok_or_else(|| {
            AosError::Config(
                "Scenario [adapter].id or [training].adapter_id is required".to_string(),
            )
        })?;

    let mut args = vec![
        "train-docs".to_string(),
        "--docs-dir".to_string(),
        docs_dir,
        "--tenant-id".to_string(),
        scenario.tenant.id.clone(),
        "--base-model-id".to_string(),
        scenario.model.id.clone(),
        "--adapter-id".to_string(),
        adapter_id.clone(),
    ];

    if training.register_after_train {
        args.push("--register".to_string());
    }

    let exe = env::current_exe().map_err(|e| AosError::Io(e.to_string()))?;
    output.info(format!(
        "Training scenario adapter {}/{} for base model {}",
        scenario.tenant.id, adapter_id, scenario.model.id
    ));
    let status = TokioCommand::new(exe)
        .args(args)
        .status()
        .await
        .map_err(|e| AosError::Io(format!("Failed to start train-docs: {e}")))?;

    if !status.success() {
        return Err(AosError::Config(format!(
            "train-docs exited with status {}",
            status
        )));
    }

    if training.register_after_train {
        let db = Db::connect_env().await?;
        let adapter = db.list_all_adapters_system().await?.into_iter().find(|a| {
            a.tenant_id == scenario.tenant.id
                && a.adapter_id
                    .as_deref()
                    .map(|id| id == adapter_id)
                    .unwrap_or(false)
        });
        let adapter = adapter.ok_or_else(|| {
            AosError::Config("Scenario adapter id does not match registered adapter".to_string())
        })?;
        let base_ok = adapter
            .base_model_id
            .as_deref()
            .map(|id| id == scenario.model.id)
            .unwrap_or(false);
        if !base_ok {
            return Err(AosError::Config(
                "Scenario adapter id does not match registered adapter".to_string(),
            ));
        }
    }

    output.success("Training completed");
    Ok(())
}

async fn verify_recorded_bundle(
    loader: &ScenarioLoader,
    name: &str,
    bundle: PathBuf,
    runs_override: Option<u32>,
    output: &OutputWriter,
) -> Result<()> {
    if matches!(
        std::env::var("AOS_SCENARIO_HARNESS").as_deref(),
        Ok("1") | Ok("true") | Ok("yes")
    ) {
        output.info("Harness mode: skipping replay verification");
        return Ok(());
    }

    let scenario = loader.load(name)?;
    let runs = runs_override
        .or_else(|| {
            scenario
                .replay
                .as_ref()
                .and_then(|r| r.runs)
                .map(|r| r as u32)
        })
        .unwrap_or(5);

    handle_replay_command(
        ReplaySubcommand::Verify {
            in_bundle: bundle,
            runs,
        },
        output,
    )
    .await
    .map_err(|e| AosError::Config(format!("verify failed: {e}")))?;
    Ok(())
}

async fn run_chat_once(
    loader: &ScenarioLoader,
    name: &str,
    server_url: &str,
    output: &OutputWriter,
) -> Result<()> {
    let scenario = loader.load(name)?;
    let body = build_chat_body(&scenario)?;
    let client = Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .map_err(|e| AosError::Http(e.to_string()))?;

    let url = format!("{}/v1/infer", server_url.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .json(&body)
        .send()
        .await
        .map_err(|e| AosError::Http(format!("chat request failed: {}", e)))?;

    let status = resp.status();
    let payload: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| AosError::Http(format!("decode chat response: {}", e)))?;

    if !status.is_success() {
        return Err(AosError::Config(format!(
            "chat failed: {} {}",
            status, payload
        )));
    }

    if output.is_json() {
        output.json(&payload)?;
    } else if let Some(text) = payload.get("text").and_then(|v| v.as_str()) {
        output.result(text);
    } else {
        output.result(payload.to_string());
    }

    Ok(())
}

fn build_chat_body(scenario: &ScenarioConfig) -> Result<serde_json::Value> {
    let adapter_id = scenario
        .adapter
        .id
        .clone()
        .or(scenario.adapter.name.clone())
        .ok_or_else(|| AosError::Config("Scenario adapter id or name required".to_string()))?;
    let chat_cfg = scenario.chat.as_ref();
    let prompt = chat_cfg
        .and_then(|c| c.prompt.clone())
        .unwrap_or_else(|| "Hello".to_string());
    let max_tokens = chat_cfg.and_then(|c| c.probe_max_tokens).unwrap_or(64);

    let mut body = json!({
        "prompt": prompt,
        "model": scenario.model.id,
        "adapters": [adapter_id],
        "tenant_id": scenario.tenant.id,
        "max_tokens": max_tokens,
        "temperature": 0.0,
        "stream": false,
    });

    if let Some(seed) = chat_cfg.and_then(|c| c.seed) {
        if let Some(obj) = body.as_object_mut() {
            obj.insert("seed".to_string(), json!(seed));
        }
    }
    if let Some(det) = chat_cfg.and_then(|c| c.determinism_mode.clone()) {
        if let Some(obj) = body.as_object_mut() {
            obj.insert("determinism_mode".to_string(), json!(det));
        }
    }
    if let Some(profile) = chat_cfg.and_then(|c| c.backend_profile.clone()) {
        if let Some(obj) = body.as_object_mut() {
            obj.insert("backend_profile".to_string(), json!(profile));
        }
    }

    Ok(body)
}

fn build_chat_command(
    name: &str,
    server_url: &str,
    scenario_dir: Option<String>,
) -> Result<Vec<String>> {
    let exe = env::current_exe().map_err(|e| AosError::Io(e.to_string()))?;
    let mut cmd = vec![
        exe.to_string_lossy().to_string(),
        "scenario".to_string(),
        "chat-once".to_string(),
        name.to_string(),
        "--server-url".to_string(),
        server_url.to_string(),
    ];

    if let Some(dir) = scenario_dir {
        cmd.push("--scenario-dir".to_string());
        cmd.push(resolve_workspace_path(&dir).to_string_lossy().to_string());
    }

    Ok(cmd)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;
    use tempfile::tempdir_in;

    #[test]
    fn status_label_formats() {
        assert_eq!(status_label(true), "pass");
        assert_eq!(status_label(false), "fail");
    }

    #[test]
    fn loader_uses_env_relative_to_workspace_root() {
        let workspace_root = workspace_root();
        let temp = tempdir_in(&workspace_root).expect("tempdir");
        let scenario_dir = temp.path();

        let scenario_path = scenario_dir.join("rel-env.toml");
        fs::write(
            &scenario_path,
            r#"
id = "rel-env"
[tenant]
id = "tenant-rel"
[model]
id = "model-rel"
[adapter]
name = "adapter-rel"
"#,
        )
        .expect("write scenario");

        let relative = scenario_dir
            .strip_prefix(&workspace_root)
            .expect("strip workspace prefix")
            .to_path_buf();
        let prior = env::var_os(crate::scenarios::ENV_SCENARIO_DIR);
        env::set_var(
            crate::scenarios::ENV_SCENARIO_DIR,
            relative.to_string_lossy().as_ref(),
        );

        let loader = loader_from_arg(None);
        assert_eq!(loader.root(), &workspace_root.join(&relative));
        let listed = loader.list().expect("list scenarios");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id.as_str(), "rel-env");

        if let Some(val) = prior {
            env::set_var(crate::scenarios::ENV_SCENARIO_DIR, val);
        } else {
            env::remove_var(crate::scenarios::ENV_SCENARIO_DIR);
        }
    }

    #[test]
    fn loader_respects_explicit_dir_independent_of_cwd() {
        let workspace_root = workspace_root();
        let temp = tempdir_in(&workspace_root).expect("tempdir");
        let scenario_dir = temp.path();

        let scenario_path = scenario_dir.join("list-explicit.toml");
        fs::write(
            &scenario_path,
            r#"
id = "list-explicit"
[tenant]
id = "tenant-explicit"
[model]
id = "model-explicit"
[adapter]
name = "adapter-explicit"
"#,
        )
        .expect("write scenario");

        let original_cwd = env::current_dir().expect("cwd");
        let _ = env::set_current_dir(workspace_root.join("configs"));

        let loader = loader_from_arg(Some(scenario_dir.to_string_lossy().into_owned()));
        let listed = loader.list().expect("list scenarios");

        env::set_current_dir(original_cwd).expect("restore cwd");

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id.as_str(), "list-explicit");
    }

    #[test]
    fn loader_defaults_to_workspace_configs_dir() {
        let workspace_root = workspace_root();
        let default_root = workspace_root.join(crate::scenarios::DEFAULT_SCENARIO_DIR);
        let prior = env::var_os(crate::scenarios::ENV_SCENARIO_DIR);
        env::remove_var(crate::scenarios::ENV_SCENARIO_DIR);

        let loader = loader_from_arg(None);
        assert_eq!(loader.root(), &default_root);
        let listed = loader.list().expect("list scenarios");
        assert!(
            !listed.is_empty(),
            "default configs/scenarios should contain at least one scenario"
        );

        if let Some(val) = prior {
            env::set_var(crate::scenarios::ENV_SCENARIO_DIR, val);
        }
    }

    #[test]
    fn loader_resolves_relative_scenario_dir_argument() {
        let workspace_root = workspace_root();
        let relative_dir = crate::scenarios::DEFAULT_SCENARIO_DIR.to_string();
        let original_cwd = env::current_dir().expect("cwd");
        env::set_current_dir(workspace_root.join("crates")).expect("change cwd");

        let loader = loader_from_arg(Some(relative_dir.clone()));
        let listed = loader.list().expect("list scenarios");

        env::set_current_dir(original_cwd).expect("restore cwd");

        assert_eq!(
            loader.root(),
            &workspace_root.join(relative_dir),
            "loader should resolve relative dir from manifest root"
        );
        assert!(
            listed.iter().any(|s| s.id.as_str() == "doc-chat"),
            "doc-chat scenario should be discoverable from relative dir"
        );
    }
}
