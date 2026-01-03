//! Model seeding utilities for populating the database from local cache.

use adapteros_db::Db;
use adapteros_model_hub::{ModelHubClient, ModelHubConfig};
use anyhow::Result;
use std::path::PathBuf;
use tracing::{info, warn};

/// Downloads priority models from HuggingFace Hub during server startup.
///
/// This function checks if the HF Hub integration is enabled via environment variables
/// and downloads a configured list of priority models during server startup.
/// Download failures are logged but do not block server startup.
pub async fn download_priority_models() {
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
        let default = std::path::PathBuf::from("var/model-cache");
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
        .fetch_one(db.pool())
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

    let mut seeded = 0usize;
    let mut errors = 0usize;

    for root in model_dirs {
        // If this root is a single model directory (like var/models/Qwen2.5...), seed it directly.
        let entries: Vec<PathBuf> = if root.join("config.json").exists() {
            vec![root.clone()]
        } else {
            std::fs::read_dir(&root)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .collect()
        };

        for path in entries {
            if !path.is_dir() {
                continue;
            }

            let Some(path_str) = path.to_str() else {
                errors += 1;
                warn!(path = ?path, "Skipping model dir with non-UTF8 path");
                continue;
            };

            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "model".to_string());
            let (format, backend) = detect_model_format_backend(&path);

            match db
                .import_model_from_path(&name, path_str, &format, &backend, "system", "system")
                .await
            {
                Ok(model_id) => {
                    if let Err(e) = db
                        .update_model_import_status(&model_id, "available", None)
                        .await
                    {
                        warn!(model_id = %model_id, error = %e, "Failed to mark model available");
                        errors += 1;
                    } else {
                        seeded += 1;
                    }
                }
                Err(e) => {
                    warn!(model = %name, error = %e, "Failed to seed cached model");
                    errors += 1;
                }
            }
        }
    }

    info!(
        seeded,
        errors,
        path = %primary_models_dir.display(),
        "Seeded cached base models into database"
    );

    Ok(())
}

/// Detects model format and backend from file extensions in a directory.
///
/// Scans the given path for model files and determines the format and backend:
/// - `.mlpackage` -> format="mlpackage", backend="coreml"
/// - `.gguf` -> format="gguf", backend="metal"
/// - Default -> format="safetensors", backend="mlx"
pub fn detect_model_format_backend(path: &std::path::Path) -> (String, String) {
    // Default to safetensors + mlx backend, override if we detect a CoreML package.
    let mut format = "safetensors".to_string();
    let mut backend = "mlx".to_string();

    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            let p = entry.path();
            if let Some(ext) = p.extension().and_then(|s| s.to_str()) {
                if ext.eq_ignore_ascii_case("mlpackage") {
                    format = "mlpackage".to_string();
                    backend = "coreml".to_string();
                    break;
                }
                if ext.eq_ignore_ascii_case("gguf") {
                    format = "gguf".to_string();
                    backend = "metal".to_string();
                }
            }
        }
    }

    (format, backend)
}
