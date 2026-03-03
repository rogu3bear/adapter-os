//! Model seeding utilities for populating the database from local cache.

use adapteros_db::{Db, SetupSeedOptions};
use adapteros_model_hub::{ModelHubClient, ModelHubConfig};
use adapteros_server_api::boot_state::BootStateManager;
use anyhow::Result;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Downloads priority models from HuggingFace Hub during server startup.
///
/// This function checks if the HF Hub integration is enabled via environment variables
/// and downloads a configured list of priority models during server startup.
/// Download failures are logged but do not block server startup.
///
/// If a `BootStateManager` is provided, download progress will be tracked and
/// exposed via the boot progress SSE endpoint.
pub async fn download_priority_models(boot_state: Option<&BootStateManager>) {
    // Check if HF Hub is enabled
    let hf_enabled = std::env::var("AOS_HF_HUB_ENABLED")
        .map(|v| v == "true" || v == "1")
        .unwrap_or(false);

    if !hf_enabled {
        info!("HF Hub integration disabled, skipping priority model downloads");
        return;
    }

    // Get priority models from environment variable
    let priority_models_str = match std::env::var("AOS_PRIORITY_MODELS") {
        Ok(models) => models,
        Err(_) => {
            info!("No priority models configured (AOS_PRIORITY_MODELS not set)");
            return;
        }
    };

    let priority_models: Vec<String> = priority_models_str
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    if priority_models.is_empty() {
        info!("No priority models configured");
        return;
    }

    info!(
        count = priority_models.len(),
        models = ?priority_models,
        "Starting priority model downloads"
    );

    // Create ModelHub client configuration
    let cache_dir = std::env::var("AOS_MODEL_CACHE_DIR").unwrap_or_else(|_| {
        let default = adapteros_core::rebase_var_path("var/model-cache");
        default.to_string_lossy().to_string()
    });

    let hf_token = std::env::var("HF_TOKEN").ok();

    let config = ModelHubConfig {
        registry_url: std::env::var("AOS_HF_REGISTRY_URL")
            .unwrap_or_else(|_| "https://huggingface.co".to_string()),
        cache_dir: PathBuf::from(cache_dir),
        max_concurrent_downloads: {
            let raw = std::env::var("AOS_MAX_CONCURRENT_DOWNLOADS").ok();
            let parsed = raw
                .as_deref()
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(4);
            let clamped = parsed.clamp(1, 10);
            if clamped != parsed {
                warn!(
                    env = "AOS_MAX_CONCURRENT_DOWNLOADS",
                    raw = ?raw,
                    parsed,
                    clamped,
                    "Value out of bounds; clamping to safe range"
                );
            }
            clamped
        },
        timeout_secs: {
            let raw = std::env::var("AOS_DOWNLOAD_TIMEOUT_SECS").ok();
            let parsed = raw
                .as_deref()
                .and_then(|s| s.parse::<u64>().ok())
                .unwrap_or(300);
            let clamped = parsed.clamp(30, 3600);
            if clamped != parsed {
                warn!(
                    env = "AOS_DOWNLOAD_TIMEOUT_SECS",
                    raw = ?raw,
                    parsed,
                    clamped,
                    "Value out of bounds; clamping to safe range"
                );
            }
            clamped
        },
        hf_token,
    };

    // Create ModelHub client
    let client = match ModelHubClient::new(config) {
        Ok(c) => c,
        Err(e) => {
            warn!(
                error = %e,
                "Failed to create ModelHub client, skipping model downloads"
            );
            return;
        }
    };

    // Set up download progress tracking if boot_state is provided
    let progress_handle = if let Some(bs) = boot_state {
        let mut rx = client.subscribe_progress();
        let bs = bs.clone();
        Some(tokio::spawn(async move {
            let mut last_downloaded: std::collections::HashMap<String, u64> =
                std::collections::HashMap::new();
            while let Ok(progress) = rx.recv().await {
                // Track incremental bytes downloaded per file
                let key = format!("{}:{}", progress.model_id, progress.filename);
                let prev = last_downloaded.get(&key).copied().unwrap_or(0);
                if progress.downloaded_bytes > prev {
                    let delta = progress.downloaded_bytes - prev;
                    bs.add_download_bytes(delta);
                    debug!(
                        model_id = %progress.model_id,
                        filename = %progress.filename,
                        delta_bytes = delta,
                        total_bytes = progress.downloaded_bytes,
                        "Download progress tracked"
                    );
                }
                last_downloaded.insert(key, progress.downloaded_bytes);
            }
        }))
    } else {
        None
    };

    // Download each priority model
    for model_id in priority_models {
        info!(model_id = %model_id, "Attempting to download priority model");

        match client.download_model(&model_id).await {
            Ok(path) => {
                info!(
                    model_id = %model_id,
                    path = %path.display(),
                    "Priority model downloaded successfully"
                );
            }
            Err(e) => {
                warn!(
                    model_id = %model_id,
                    error = %e,
                    "Failed to download priority model (continuing with boot)"
                );
                // Don't fail boot - continue with other models
            }
        }
    }

    // Clean up progress tracking task
    if let Some(handle) = progress_handle {
        handle.abort();
    }

    info!("Priority model downloads complete");
}

/// Dev helper: register cached base models from var/model-cache into DB when empty.
///
/// This runs only when explicitly enabled or in debug builds, and only if the
/// `models` table is currently empty. It scans `AOS_MODEL_CACHE_DIR/models`
/// (defaults to `var/model-cache/models`) and registers each directory as a
/// base model so the UI can surface them without manual import.
pub async fn seed_models_from_cache_if_empty(db: &Db) -> Result<()> {
    let seed_enabled = std::env::var("AOS_SEED_MODEL_CACHE")
        .ok()
        .map(|v| matches!(v.to_ascii_lowercase().as_str(), "1" | "true" | "yes"))
        .unwrap_or(cfg!(debug_assertions));

    if !seed_enabled {
        return Ok(());
    }

    if db.pool_opt().is_none() {
        info!("Skipping model cache seed: SQL pool not available");
        return Ok(());
    }

    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models")
        .fetch_one(db.pool_result()?)
        .await?;
    if existing > 0 {
        return Ok(());
    }

    let cache_root = std::env::var("AOS_MODEL_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/model-cache"));
    let primary_models_dir = cache_root.join("models");
    let fallback_dir = std::env::var("AOS_MODEL_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("var/models"));

    // Collect candidate model directories to seed.
    let mut model_dirs = Vec::new();
    if primary_models_dir.exists() {
        model_dirs.push(primary_models_dir.clone());
    } else if fallback_dir.exists() {
        model_dirs.push(fallback_dir.clone());
    } else {
        info!(
            path = %primary_models_dir.display(),
            fallback = %fallback_dir.display(),
            "Model cache directory not found, skipping seed"
        );
        return Ok(());
    }

    let mut selected_paths = Vec::new();
    for root in model_dirs {
        selected_paths.extend(Db::setup_discover_models(&root).into_iter().map(|m| m.path));
    }

    if selected_paths.is_empty() {
        info!(
            path = %primary_models_dir.display(),
            fallback = %fallback_dir.display(),
            "No discoverable cached models found, skipping seed"
        );
        return Ok(());
    }

    let summary = db
        .setup_seed_models(
            &selected_paths,
            SetupSeedOptions {
                force: false,
                tenant_id: "system",
                imported_by: "system",
            },
        )
        .await?;

    info!(
        seeded = summary.seeded,
        skipped = summary.skipped,
        errors = summary.failed,
        path = %primary_models_dir.display(),
        "Seeded cached base models into database"
    );

    Ok(())
}
