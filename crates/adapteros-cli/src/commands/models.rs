//! Model management commands

use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

use crate::commands::check_tokenizer::CheckTokenizerArgs;
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
  aosctl models seed --model-path /var/models/Llama-3.2-3B-Instruct-4bit

  # Force re-seed even if models exist
  aosctl models seed --force
"#)]
    Seed {
        /// Database path (defaults to DATABASE_URL or var/aos-cp.sqlite3)
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
        /// Database path (defaults to DATABASE_URL or var/aos-cp.sqlite3)
        #[arg(long)]
        db_path: Option<PathBuf>,

        /// Output in JSON format
        #[arg(long)]
        json: bool,
    },

    /// Validate a tokenizer.json file
    #[command(after_help = r#"Examples:
  # Check a tokenizer file
  aosctl models check-tokenizer /var/models/Llama-3.2-3B-Instruct-4bit/tokenizer.json

  # Validate tokenizer with JSON output
  aosctl models check-tokenizer ./tokenizer.json --json
"#)]
    CheckTokenizer(CheckTokenizerArgs),
}

/// Handle models commands
pub async fn handle_models_command(
    cmd: ModelsCommand,
    output: &OutputWriter,
    model_path_override: Option<PathBuf>,
) -> Result<()> {
    let command_name = get_models_command_name(&cmd);
    info!(command = ?cmd, "Handling models command");
    if let Err(e) = crate::cli_telemetry::emit_cli_command(&command_name, None, true).await {
        tracing::debug!(error = %e, command = %command_name, "Telemetry emit failed (non-fatal)");
    }

    match cmd {
        ModelsCommand::Seed { db_path, force } => {
            run_seed(model_path_override, db_path, force, output).await
        }
        ModelsCommand::List { db_path, json } => run_list(db_path, json, output).await,
        ModelsCommand::CheckTokenizer(args) => args.execute(output).await,
    }
}

fn get_models_command_name(cmd: &ModelsCommand) -> String {
    match cmd {
        ModelsCommand::Seed { .. } => "models_seed".to_string(),
        ModelsCommand::List { .. } => "models_list".to_string(),
        ModelsCommand::CheckTokenizer { .. } => "models_check_tokenizer".to_string(),
    }
}

async fn run_seed(
    model_path_override: Option<PathBuf>,
    db_path: Option<PathBuf>,
    force: bool,
    output: &OutputWriter,
) -> Result<()> {
    // Resolve model path: CLI/global override > AOS_MODEL_PATH env > default
    let model_path = model_path_override
        .or_else(|| std::env::var("AOS_MODEL_PATH").ok().map(PathBuf::from))
        .unwrap_or_else(|| adapteros_core::rebase_var_path("var/models"));

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
        "sqlite://var/aos-cp.sqlite3".to_string()
    };

    output.progress(format!(
        "Connecting to database: {}",
        db_url.replace("sqlite://", "")
    ));
    let db = Db::connect(&db_url).await?;

    // Collect model directories to seed
    let model_dirs = adapteros_core::discover_model_dirs(&model_path);

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

        let format = adapteros_core::ModelFormat::detect_from_dir(&path);
        let backend = format.default_backend();
        output.progress(format!(
            "  {} (format: {}, backend: {})",
            name, format, backend
        ));

        let result = if force {
            db.upsert_model_from_path(
                &name,
                path_str,
                format.as_str(),
                backend.as_str(),
                "system",
                "system",
                adapteros_core::ModelImportStatus::Available,
            )
            .await
        } else {
            db.import_model_from_path(
                &name,
                path_str,
                format.as_str(),
                backend.as_str(),
                "system",
                "system",
                adapteros_core::ModelImportStatus::Available,
            )
            .await
        };
        match result {
            Ok(model_id) => {
                output.kv("  Seeded", &format!("{} (id: {})", name, model_id));
                seeded += 1;
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
        "sqlite://var/aos-cp.sqlite3".to_string()
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
