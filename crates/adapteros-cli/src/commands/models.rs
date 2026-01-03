//! Model management commands

use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

use crate::output::OutputWriter;
use adapteros_db::{sqlx, Db};
use tracing::{info, warn};

#[derive(Debug, Clone, Subcommand)]
pub enum ModelsCommand {
    /// Seed models from local cache into database
    #[command(after_help = r#"Examples:
  # Seed models from AOS_MODEL_PATH environment variable
  aosctl models seed

  # Seed a specific model path
  aosctl models seed --model-path ./var/models/Qwen2.5-7B-Instruct-4bit

  # Force re-seed even if models exist
  aosctl models seed --force
"#)]
    Seed {
        /// Model directory path (defaults to AOS_MODEL_PATH env var or var/models)
        #[arg(long)]
        model_path: Option<PathBuf>,

        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Force re-seed even if models exist
        #[arg(long)]
        force: bool,
    },

    /// List registered models
    #[command(after_help = r#"Examples:
  # List all models
  aosctl models list

  # Output as JSON
  aosctl models list --json
"#)]
    List {
        /// Database path (defaults to DATABASE_URL or ./var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },
}

/// Handle models commands
pub async fn handle_models_command(cmd: ModelsCommand, output: &OutputWriter) -> Result<()> {
    let command_name = get_models_command_name(&cmd);
    info!(command = ?cmd, "Handling models command");
    if let Err(e) = crate::cli_telemetry::emit_cli_command(&command_name, None, true).await {
        tracing::debug!(error = %e, command = %command_name, "Telemetry emit failed (non-fatal)");
    }

    match cmd {
        ModelsCommand::Seed {
            model_path,
            db_path,
            force,
        } => run_seed(model_path, db_path, force, output).await,
        ModelsCommand::List { db_path, json } => run_list(db_path, json, output).await,
    }
}

fn get_models_command_name(cmd: &ModelsCommand) -> String {
    match cmd {
        ModelsCommand::Seed { .. } => "models_seed".to_string(),
        ModelsCommand::List { .. } => "models_list".to_string(),
    }
}

async fn run_seed(
    model_path: Option<PathBuf>,
    db_path: Option<PathBuf>,
    force: bool,
    output: &OutputWriter,
) -> Result<()> {
    // Resolve model path: CLI arg > AOS_MODEL_PATH env > default
    let model_path = model_path
        .or_else(|| std::env::var("AOS_MODEL_PATH").ok().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("var/models"));

    if !model_path.exists() {
        output.warning(format!(
            "Model path does not exist: {}",
            model_path.display()
        ));
        return Ok(());
    }

    // Resolve DB URL
    let db_url = if let Some(path) = db_path {
        format!("sqlite://{}", path.display())
    } else if let Ok(url) = std::env::var("DATABASE_URL") {
        url
    } else {
        "sqlite://./var/aos-cp.sqlite3".to_string()
    };

    output.progress(format!(
        "Connecting to database: {}",
        db_url.replace("sqlite://", "")
    ));
    let db = Db::connect(&db_url).await?;

    // Check if models already exist
    let existing: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models")
        .fetch_one(db.pool())
        .await?;

    if existing > 0 && !force {
        output.info(format!(
            "{} model(s) already registered. Use --force to re-seed.",
            existing
        ));
        return Ok(());
    }

    // Collect model directories to seed
    let model_dirs: Vec<PathBuf> = if model_path.join("config.json").exists() {
        // Single model directory
        vec![model_path.clone()]
    } else if model_path.is_dir() {
        // Directory containing multiple models
        std::fs::read_dir(&model_path)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir() && p.join("config.json").exists())
            .collect()
    } else {
        vec![]
    };

    if model_dirs.is_empty() {
        output.warning(format!(
            "No valid model directories found at: {} (must contain config.json)",
            model_path.display()
        ));
        return Ok(());
    }

    output.section("Seeding Models");
    let mut seeded = 0usize;
    let mut errors = 0usize;

    for path in model_dirs {
        let Some(path_str) = path.to_str() else {
            warn!(path = ?path, "Skipping model dir with non-UTF8 path");
            errors += 1;
            continue;
        };

        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "model".to_string());

        // Check if model with this name already exists
        if !force {
            let exists: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM models WHERE name = ?")
                .bind(&name)
                .fetch_one(db.pool())
                .await?;
            if exists > 0 {
                output.info(format!("  {} - already exists, skipping", name));
                continue;
            }
        }

        let (format, backend) = detect_model_format_backend(&path);
        output.progress(format!(
            "  {} (format: {}, backend: {})",
            name, format, backend
        ));

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
                    output.kv("  Seeded", &format!("{} (id: {})", name, model_id));
                    seeded += 1;
                }
            }
            Err(e) => {
                warn!(model = %name, error = %e, "Failed to seed model");
                output.error(format!("  {} - failed: {}", name, e));
                errors += 1;
            }
        }
    }

    output.blank();
    if seeded > 0 {
        output.result(format!("Seeded {} model(s) ({} errors)", seeded, errors));
    } else if errors > 0 {
        output.error(format!("No models seeded ({} errors)", errors));
    } else {
        output.info("No new models to seed");
    }

    Ok(())
}

async fn run_list(db_path: Option<PathBuf>, json: bool, output: &OutputWriter) -> Result<()> {
    // Resolve DB URL
    let db_url = if let Some(path) = db_path {
        format!("sqlite://{}", path.display())
    } else if let Ok(url) = std::env::var("DATABASE_URL") {
        url
    } else {
        "sqlite://./var/aos-cp.sqlite3".to_string()
    };

    let db = Db::connect(&db_url).await?;
    let models = db.list_models("default").await?;

    if json {
        let json_output: Vec<serde_json::Value> = models
            .iter()
            .map(|m| {
                serde_json::json!({
                    "id": m.id,
                    "name": m.name,
                    "model_path": m.model_path,
                    "format": m.format,
                    "backend": m.backend,
                    "import_status": m.import_status,
                    "size_bytes": m.size_bytes,
                    "created_at": m.created_at,
                })
            })
            .collect();
        if let Err(e) = output.json(&serde_json::json!({
            "models": json_output,
            "count": models.len()
        })) {
            tracing::debug!(error = %e, "JSON output failed (non-fatal)");
        }
    } else {
        output.section("Registered Models");
        if models.is_empty() {
            output.info("No models registered");
        } else {
            for model in &models {
                output.kv(
                    &model.name,
                    &format!(
                        "{} ({}, {})",
                        model.import_status.as_deref().unwrap_or("unknown"),
                        model.format.as_deref().unwrap_or("?"),
                        model.backend.as_deref().unwrap_or("?")
                    ),
                );
                if let Some(path) = &model.model_path {
                    output.info(format!("  Path: {}", path));
                }
            }
        }
        output.blank();
        output.result(format!("{} model(s) registered", models.len()));
    }

    Ok(())
}

/// Detect model format and backend from directory contents
fn detect_model_format_backend(path: &std::path::Path) -> (String, String) {
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
