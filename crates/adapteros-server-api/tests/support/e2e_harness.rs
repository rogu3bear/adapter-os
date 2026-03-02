use crate::common::clear_testkit_env;
use adapteros_core::{BackendKind, SeedMode};
use adapteros_db::models::ModelRegistrationBuilder;
use adapteros_db::Db;
use adapteros_lora_worker::memory::UmaPressureMonitor;
use adapteros_metrics_exporter::MetricsExporter;
use adapteros_server_api::config::PathsConfig;
use adapteros_server_api::state::{ApiConfig, AppState, MetricsConfig};
use adapteros_server_api::telemetry::MetricsRegistry;
use adapteros_telemetry::MetricsCollector;
use anyhow::{anyhow, Context, Result};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, RwLock};
use tokio::net::UnixStream;
use tokio::process::{Child, Command};
use tokio::time::{sleep, Duration};

const ENABLE_ENV: &str = "AOS_TEST_E2E_ENABLE";
const LEGACY_ENABLE_ENV: &str = "AOS_E2E_HARNESS";
const WORKER_MODE_ENV: &str = "AOS_TEST_WORKER_MODE";
const WORKER_UDS_ENV: &str = "AOS_TEST_WORKER_UDS_PATH";
const LEGACY_WORKER_UDS_ENV: &str = "AOS_E2E_UDS";
const MODEL_ID_ENV: &str = "AOS_TEST_MODEL_ID";
const MODEL_DIR_ENV: &str = "AOS_TEST_MODEL_DIR";
const LEGACY_MODEL_DIR_ENV: &str = "AOS_E2E_MODEL_PATH";
const LEGACY_MODEL_PATH_ENV: &str = "AOS_TEST_MODEL_PATH";
const TOKENIZER_DIR_ENV: &str = "AOS_TEST_TOKENIZER_DIR";
const WORKER_BACKEND_ENV: &str = "AOS_TEST_WORKER_BACKEND";
const LEGACY_WORKER_BACKEND_ENV: &str = "AOS_E2E_BACKEND";
const LEGACY_TRAINING_BACKEND_ENV: &str = "AOS_E2E_TRAINING_BACKEND";
const WORKER_MANIFEST_ENV: &str = "AOS_TEST_WORKER_MANIFEST";
const LEGACY_WORKER_MANIFEST_ENV: &str = "AOS_WORKER_MANIFEST";
const WORKER_BIN_ENV: &str = "AOS_TEST_WORKER_BIN";

#[allow(clippy::large_enum_variant)]
pub enum HarnessSetup {
    Skip { reason: String },
    Ready(E2eHarness),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum WorkerMode {
    Spawn,
    External,
}

#[derive(Clone, Debug)]
pub struct ModelConfig {
    #[allow(dead_code)]
    pub requested_id: String,
    #[allow(dead_code)]
    pub registered_id: String,
    pub model_dir: PathBuf,
    pub tokenizer_dir: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub struct HarnessPaths {
    #[allow(dead_code)]
    pub root: PathBuf,
    pub artifacts_root: PathBuf,
    pub bundles_root: PathBuf,
    pub adapters_root: PathBuf,
    pub plan_dir: PathBuf,
    pub datasets_root: PathBuf,
    pub documents_root: PathBuf,
}

pub struct E2eHarness {
    #[allow(dead_code)]
    pub state: AppState,
    #[allow(dead_code)]
    pub paths: HarnessPaths,
    pub uds_path: PathBuf,
    #[allow(dead_code)]
    pub worker_mode: WorkerMode,
    pub model: ModelConfig,
    worker: Option<Child>,
    env_cleanup: Vec<(&'static str, Option<String>)>,
}

impl E2eHarness {
    pub async fn from_env() -> Result<HarnessSetup> {
        if !env_flag_enabled(ENABLE_ENV, &[LEGACY_ENABLE_ENV]) {
            return Ok(HarnessSetup::Skip {
                reason: format!("{ENABLE_ENV} not set, skipping golden path harness"),
            });
        }

        clear_testkit_env();

        let worker_mode = match read_env_with_fallback(WORKER_MODE_ENV, &[])
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "spawn" => WorkerMode::Spawn,
            "external" => WorkerMode::External,
            "" => {
                return Ok(HarnessSetup::Skip {
                    reason: format!("{WORKER_MODE_ENV} not provided"),
                })
            }
            other => {
                return Ok(HarnessSetup::Skip {
                    reason: format!("Unsupported {WORKER_MODE_ENV} value: {other}"),
                })
            }
        };

        let uds_path = match worker_mode {
            WorkerMode::Spawn => {
                let base_tempdir = tempfile::TempDir::with_prefix("aos-test-e2e-harness-run-")?;
                let base = base_tempdir.keep();
                base.join("worker.sock")
            }
            WorkerMode::External => {
                let raw = match read_env_with_fallback(WORKER_UDS_ENV, &[LEGACY_WORKER_UDS_ENV]) {
                    Some(val) if !val.is_empty() => val,
                    _ => {
                        return Ok(HarnessSetup::Skip {
                            reason: format!(
                                "{WORKER_UDS_ENV} must point to external worker socket"
                            ),
                        })
                    }
                };
                PathBuf::from(raw)
            }
        };

        let model_dir = match read_env_with_fallback(
            MODEL_DIR_ENV,
            &[LEGACY_MODEL_DIR_ENV, LEGACY_MODEL_PATH_ENV],
        ) {
            Some(path) if Path::new(&path).exists() => PathBuf::from(path),
            _ => {
                return Ok(HarnessSetup::Skip {
                    reason: format!("{MODEL_DIR_ENV} must point to a readable model directory"),
                })
            }
        };

        let model_id = match read_env_with_fallback(MODEL_ID_ENV, &[]) {
            Some(id) if !id.is_empty() => id,
            _ => {
                return Ok(HarnessSetup::Skip {
                    reason: format!("{MODEL_ID_ENV} must be set for harness to seed the model"),
                })
            }
        };

        let tokenizer_dir = read_env_with_fallback(TOKENIZER_DIR_ENV, &[])
            .map(PathBuf::from)
            .filter(|p| p.exists());

        let paths = build_paths()?;
        let mut env_cleanup = Vec::new();
        set_env(
            &mut env_cleanup,
            "AOS_SKIP_MIGRATION_SIGNATURES",
            "1".to_string(),
        );
        set_env(&mut env_cleanup, "AOS_DEV_NO_AUTH", "1".to_string());
        set_env(
            &mut env_cleanup,
            "AOS_WORKER_SOCKET",
            uds_path.display().to_string(),
        );

        let state = build_state(&paths).await?;
        let registered_id = seed_base_model(&state, &model_id, &model_dir).await?;
        let model = ModelConfig {
            requested_id: model_id.clone(),
            registered_id,
            model_dir: model_dir.clone(),
            tokenizer_dir,
        };

        let mut harness = E2eHarness {
            state,
            paths,
            uds_path: uds_path.clone(),
            worker_mode: worker_mode.clone(),
            model,
            worker: None,
            env_cleanup,
        };

        match worker_mode {
            WorkerMode::Spawn => {
                if resolve_worker_bin().is_none() {
                    return Ok(HarnessSetup::Skip {
                        reason: format!(
                            "{} not set and aos_worker binary not found",
                            WORKER_BIN_ENV
                        ),
                    });
                }
                if resolve_manifest_path().is_none() {
                    return Ok(HarnessSetup::Skip {
                        reason: format!(
                            "{} not set and manifests/reference.yaml missing",
                            WORKER_MANIFEST_ENV
                        ),
                    });
                }
                harness.spawn_worker().await?;
            }
            WorkerMode::External => {
                if let Err(err) = harness.wait_for_worker_ready(Duration::from_secs(5)).await {
                    return Ok(HarnessSetup::Skip {
                        reason: format!(
                            "External worker at {} not reachable: {}",
                            uds_path.display(),
                            err
                        ),
                    });
                }
            }
        }

        Ok(HarnessSetup::Ready(harness))
    }

    pub async fn wait_for_worker_ready(&self, timeout: Duration) -> Result<()> {
        let start = std::time::Instant::now();
        loop {
            match UnixStream::connect(&self.uds_path).await {
                Ok(_) => return Ok(()),
                Err(err) if start.elapsed() < timeout => {
                    sleep(Duration::from_millis(100)).await;
                    tracing::debug!("Waiting for worker socket: {}", err);
                }
                Err(err) => {
                    return Err(anyhow!(
                        "Timed out waiting for worker socket {}: {}",
                        self.uds_path.display(),
                        err
                    ))
                }
            }
        }
    }

    #[allow(dead_code)]
    pub async fn trust_dataset_version(&self, dataset_version_id: &str) -> Result<()> {
        self.state
            .db
            .update_dataset_version_safety_status(
                dataset_version_id,
                Some("clean"),
                Some("clean"),
                Some("clean"),
                Some("clean"),
            )
            .await
            .context("update dataset version safety status")?;
        Ok(())
    }

    async fn spawn_worker(&mut self) -> Result<()> {
        let worker_bin = resolve_worker_bin().ok_or_else(|| {
            anyhow!(
                "Worker binary not found. Set {} or build aos_worker",
                WORKER_BIN_ENV
            )
        })?;

        let manifest = resolve_manifest_path().ok_or_else(|| {
            anyhow!(
                "Worker manifest missing. Set {} or place manifests/reference.yaml",
                WORKER_MANIFEST_ENV
            )
        })?;

        let mut cmd = Command::new(worker_bin);
        let backend = resolve_worker_backend().unwrap_or_else(|| "auto".to_string());
        cmd.kill_on_drop(true);
        cmd.arg("--uds-path")
            .arg(&self.uds_path)
            .arg("--manifest")
            .arg(&manifest)
            .arg("--model-path")
            .arg(&self.model.model_dir)
            .arg("--backend")
            .arg(backend)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("AOS_DEV_NO_AUTH", "1")
            .env("AOS_WORKER_SOCKET", &self.uds_path)
            .env(
                "AOS_MODEL_CACHE_MAX_MB",
                std::env::var("AOS_MODEL_CACHE_MAX_MB").unwrap_or_else(|_| "1024".to_string()),
            );

        if let Some(tokenizer) = &self.model.tokenizer_dir {
            cmd.env("AOS_TOKENIZER_PATH", tokenizer);
        }

        let child = cmd
            .spawn()
            .with_context(|| "Failed to spawn aos_worker process")?;
        self.worker = Some(child);

        if let Err(e) = self.wait_for_worker_ready(Duration::from_secs(15)).await {
            if let Some(child) = self.worker.take() {
                if let Ok(out) = child.wait_with_output().await {
                    eprintln!(
                        "worker stdout:\n{}\nworker stderr:\n{}",
                        String::from_utf8_lossy(&out.stdout),
                        String::from_utf8_lossy(&out.stderr)
                    );
                }
            }
            return Err(e);
        }

        Ok(())
    }
}

impl Drop for E2eHarness {
    fn drop(&mut self) {
        if let Some(child) = self.worker.as_mut() {
            let _ = child.start_kill();
        }
        for (key, original) in self.env_cleanup.drain(..).rev() {
            match original {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        let _ = std::fs::remove_file(&self.uds_path);
    }
}

fn resolve_worker_bin() -> Option<PathBuf> {
    if let Ok(path) = std::env::var(WORKER_BIN_ENV) {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_aos_worker") {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    [
        PathBuf::from("target/debug/aos_worker"),
        PathBuf::from("target/release/aos_worker"),
    ]
    .into_iter()
    .find(|candidate| candidate.exists())
}

fn resolve_manifest_path() -> Option<PathBuf> {
    if let Some(path) = read_env_with_fallback(WORKER_MANIFEST_ENV, &[LEGACY_WORKER_MANIFEST_ENV]) {
        let candidate = PathBuf::from(path);
        if candidate.exists() {
            return Some(candidate);
        }
    }
    let fallback = PathBuf::from("manifests/reference.yaml");
    fallback.exists().then_some(fallback)
}

fn build_paths() -> Result<HarnessPaths> {
    let root_tempdir = tempfile::TempDir::with_prefix("aos-test-e2e-harness-")?;
    let root = root_tempdir.keep();
    let artifacts_root = root.join("artifacts");
    let bundles_root = root.join("bundles");
    let adapters_root = root.join("adapters");
    let plan_dir = root.join("plan");
    let datasets_root = root.join("datasets");
    let documents_root = root.join("documents");

    for dir in [
        &artifacts_root,
        &bundles_root,
        &adapters_root,
        &plan_dir,
        &datasets_root,
        &documents_root,
    ] {
        std::fs::create_dir_all(dir)
            .with_context(|| format!("failed to create {}", dir.display()))?;
    }

    Ok(HarnessPaths {
        root,
        artifacts_root,
        bundles_root,
        adapters_root,
        plan_dir,
        datasets_root,
        documents_root,
    })
}

async fn build_state(paths: &HarnessPaths) -> Result<AppState> {
    let db = Db::new_in_memory().await?;

    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('default', 'Default Tenant')",
    )
    .execute(db.pool_result()?)
    .await?;

    let jwt_secret = b"test-jwt-secret-for-e2e-harness-32bytes!".to_vec();

    let config = Arc::new(RwLock::new(ApiConfig {
        metrics: MetricsConfig {
            enabled: true,
            bearer_token: "test-bearer-token".to_string(),
        },
        directory_analysis_timeout_secs: 120,
        use_session_stack_for_routing: false,
        capacity_limits: Default::default(),
        general: None,
        server: Default::default(),
        security: Default::default(),
        auth: Default::default(),
        self_hosting: Default::default(),
        performance: Default::default(),
        streaming: Default::default(),
        paths: PathsConfig {
            artifacts_root: paths.artifacts_root.to_string_lossy().to_string(),
            bundles_root: paths.bundles_root.to_string_lossy().to_string(),
            adapters_root: paths.adapters_root.to_string_lossy().to_string(),
            plan_dir: paths.plan_dir.to_string_lossy().to_string(),
            datasets_root: paths.datasets_root.to_string_lossy().to_string(),
            documents_root: paths.documents_root.to_string_lossy().to_string(),
            synthesis_model_path: None,
        },
        chat_context: Default::default(),
        seed_mode: SeedMode::BestEffort,
        backend_profile: BackendKind::Auto,
        worker_id: 0,
        timeouts: Default::default(),
        rate_limit: None,
        inference_cache: Default::default(),
    }));

    let histogram_buckets = vec![
        0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
    ];
    let metrics_exporter = Arc::new(MetricsExporter::new(histogram_buckets)?);
    let metrics_collector = Arc::new(MetricsCollector::new(Default::default()));
    let metrics_registry = Arc::new(MetricsRegistry::new());
    let uma_monitor = Arc::new(UmaPressureMonitor::new(15, None));

    Ok(AppState::new(
        db,
        jwt_secret,
        config,
        metrics_exporter,
        metrics_collector,
        metrics_registry,
        uma_monitor,
    ))
}

async fn seed_base_model(state: &AppState, model_id: &str, model_dir: &Path) -> Result<String> {
    let params = ModelRegistrationBuilder::new()
        .name(model_id)
        .hash_b3(format!("b3:{}", model_id))
        .config_hash_b3(format!("config:{}", model_id))
        .tokenizer_hash_b3(format!("tokenizer:{}", model_id))
        .tokenizer_cfg_hash_b3(format!("tokenizer-cfg:{}", model_id))
        .build()?;

    let registered_id = state.db.register_model(params).await?;
    if registered_id != model_id {
        adapteros_db::sqlx::query("UPDATE models SET id = ? WHERE id = ?")
            .bind(model_id)
            .bind(&registered_id)
            .execute(state.db.pool_result()?)
            .await?;
    }
    state
        .db
        .update_model_path(model_id, model_dir.to_string_lossy().as_ref())
        .await?;
    state
        .db
        .update_base_model_status("default", model_id, "ready", None, Some(1024))
        .await
        .context("update base model status")?;

    Ok(model_id.to_string())
}

fn set_env(cleanup: &mut Vec<(&'static str, Option<String>)>, key: &'static str, value: String) {
    cleanup.push((key, std::env::var(key).ok()));
    std::env::set_var(key, value);
}

fn env_flag_enabled(primary: &str, fallbacks: &[&str]) -> bool {
    if env_truthy(primary) {
        return true;
    }
    fallbacks.iter().any(|name| env_truthy(name))
}

fn env_truthy(key: &str) -> bool {
    std::env::var(key)
        .map(|value| {
            let lower = value.to_ascii_lowercase();
            matches!(lower.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

fn read_env_with_fallback(primary: &str, fallbacks: &[&str]) -> Option<String> {
    std::env::var(primary)
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            fallbacks
                .iter()
                .find_map(|name| std::env::var(name).ok())
                .filter(|value| !value.trim().is_empty())
        })
}

fn resolve_worker_backend() -> Option<String> {
    read_env_with_fallback(
        WORKER_BACKEND_ENV,
        &[LEGACY_WORKER_BACKEND_ENV, LEGACY_TRAINING_BACKEND_ENV],
    )
    .map(|value| value.trim().to_ascii_lowercase())
}
