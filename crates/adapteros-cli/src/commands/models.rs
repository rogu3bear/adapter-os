//! Model management commands

use anyhow::Result;
use clap::Subcommand;
use std::path::PathBuf;

use crate::commands::check_tokenizer::CheckTokenizerArgs;
use crate::commands::quantize_qwen::{self, GateMetrics, QuantizeQwen35Request};
use crate::output::OutputWriter;
use adapteros_db::{Db, SetupSeedOptions, SetupSeedStatus};
use tracing::info;

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

    /// Deterministic Qwen3.5-27B quantization pipeline (int4, MLX-oriented)
    #[command(after_help = r#"Examples:
  # Quantize Qwen3.5-27B with auto-resolved HF revision SHA
  aosctl models quantize-qwen35 \
    --input var/models/Qwen3.5-27B \
    --output . \
    --revision auto

  # Quantize with enforced gates and provided metrics
  aosctl models quantize-qwen35 \
    --input var/models/Qwen3.5-27B \
    --output . \
    --enforce-gates \
    --enable-native-probes \
    --probe-max-samples 8 \
    --guided \
    --metrics-from-flags \
    --golden-prompts data/golden_prompts.jsonl \
    --calibration data/calibration.jsonl \
    --baseline-fp16 artifacts/fp16/qwen3.5-27b \
    --logit-cosine-mean 0.989 \
    --ppl-delta-pct 5.4 \
    --task-proxy-delta-abs 1.2 \
    --tok-s-1k 28.1 \
    --tok-s-8k 13.7 \
    --rss-mb-peak 39000 \
    --human-critical-regressions 0
"#)]
    QuantizeQwen35 {
        /// Input model directory containing safetensors shards
        #[arg(long)]
        input: PathBuf,

        /// Output root where artifacts/models/... is created
        #[arg(long, default_value = ".")]
        output: PathBuf,

        /// Hugging Face repository (must resolve to Qwen3.5-27B lineage)
        #[arg(long, default_value = "Qwen/Qwen3.5-27B")]
        hf_repo: String,

        /// Revision SHA or "auto" (default)
        #[arg(long)]
        revision: Option<String>,

        /// Primary group size profile (default 64; fallback to 128 when enforcing gates)
        #[arg(long, default_value_t = 64)]
        group_size: usize,

        /// Default runtime context length
        #[arg(long, default_value_t = 8192)]
        context_default: usize,

        /// Max runtime context length
        #[arg(long, default_value_t = 16384)]
        context_max: usize,

        /// Determinism seed used for manifest/eval metadata
        #[arg(long, default_value_t = 42)]
        seed: u64,

        /// JSONL golden prompt set (expected 100 entries when enforced)
        #[arg(long)]
        golden_prompts: Option<PathBuf>,

        /// JSONL calibration set (expected 2k-5k entries when enforced)
        #[arg(long)]
        calibration: Option<PathBuf>,

        /// Baseline FP16 artifact directory
        #[arg(long)]
        baseline_fp16: Option<PathBuf>,

        /// Enforce acceptance gates and fallback ladder
        #[arg(long, default_value_t = false)]
        enforce_gates: bool,

        /// Interactive guided setup for missing inputs and beginner UX
        #[arg(long, default_value_t = false)]
        guided: bool,

        /// Preflight only: validate and resolve revision without quantizing
        #[arg(long, default_value_t = false)]
        dry_run: bool,

        /// Print beginner-friendly failure explanations on gate failure
        #[arg(long, default_value_t = true)]
        beginner_explain: bool,

        /// Compatibility mode: read gate metrics from CLI flags instead of computing in command
        #[arg(long, default_value_t = false)]
        metrics_from_flags: bool,

        /// Enable best-effort native MLX runtime probes (informational only in this phase)
        #[arg(long, default_value_t = false)]
        enable_native_probes: bool,

        /// Maximum deterministic probe samples when native probes are enabled
        #[arg(long)]
        probe_max_samples: Option<u32>,

        /// Measured mean logit cosine versus FP16 baseline
        #[arg(long)]
        logit_cosine_mean: Option<f64>,

        /// Measured perplexity delta percentage versus FP16 baseline
        #[arg(long)]
        ppl_delta_pct: Option<f64>,

        /// Measured task proxy absolute delta
        #[arg(long)]
        task_proxy_delta_abs: Option<f64>,

        /// Measured tokens/sec at 1k context
        #[arg(long)]
        tok_s_1k: Option<f64>,

        /// Measured tokens/sec at 8k context
        #[arg(long)]
        tok_s_8k: Option<f64>,

        /// Measured peak RSS in MB
        #[arg(long)]
        rss_mb_peak: Option<f64>,

        /// Human spot-check critical regression count
        #[arg(long)]
        human_critical_regressions: Option<u32>,
    },
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
        ModelsCommand::QuantizeQwen35 {
            input,
            output: output_root,
            hf_repo,
            revision,
            group_size,
            context_default,
            context_max,
            seed,
            golden_prompts,
            calibration,
            baseline_fp16,
            enforce_gates,
            guided,
            dry_run,
            beginner_explain,
            metrics_from_flags,
            enable_native_probes,
            probe_max_samples,
            logit_cosine_mean,
            ppl_delta_pct,
            task_proxy_delta_abs,
            tok_s_1k,
            tok_s_8k,
            rss_mb_peak,
            human_critical_regressions,
        } => {
            let req = QuantizeQwen35Request {
                input,
                output_root,
                hf_repo,
                revision,
                group_size,
                context_default,
                context_max,
                seed,
                golden_prompts,
                calibration,
                baseline_fp16,
                enforce_gates,
                metrics_from_flags,
                enable_native_probes,
                probe_max_samples,
                guided,
                dry_run,
                beginner_explain,
                metrics: GateMetrics {
                    logit_cosine_mean,
                    ppl_delta_pct,
                    task_proxy_delta_abs,
                    tok_s_1k,
                    tok_s_8k,
                    rss_mb_peak,
                    human_critical_regressions,
                },
                output_json: output.mode().is_json(),
            };
            let outcome = match quantize_qwen::run_qwen35_pipeline(req, output).await {
                Ok(v) => v,
                Err(e) => {
                    output.error(format!("quantize-qwen35 infrastructure/input failure: {e}"));
                    std::process::exit(3);
                }
            };
            if outcome.exit_code != 0 {
                std::process::exit(outcome.exit_code);
            }
            Ok(())
        }
    }
}

fn get_models_command_name(cmd: &ModelsCommand) -> String {
    match cmd {
        ModelsCommand::Seed { .. } => "models_seed".to_string(),
        ModelsCommand::List { .. } => "models_list".to_string(),
        ModelsCommand::CheckTokenizer(..) => "models_check_tokenizer".to_string(),
        ModelsCommand::QuantizeQwen35 { .. } => "models_quantize_qwen35".to_string(),
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
    let discovered = Db::setup_discover_models(&model_path);

    if discovered.is_empty() {
        output.warning(format!(
            "No valid model directories found at: {} (must contain config.json)",
            model_path.display()
        ));
        return Ok(());
    }

    output.section("Seeding Models");
    let selected_paths: Vec<PathBuf> = discovered.into_iter().map(|m| m.path).collect();
    let summary = db
        .setup_seed_models(
            &selected_paths,
            SetupSeedOptions {
                force,
                tenant_id: "system",
                imported_by: "system",
            },
        )
        .await?;

    for item in summary.items {
        match item.status {
            SetupSeedStatus::Seeded => {
                output.kv(
                    "  Seeded",
                    &format!(
                        "{} (id: {})",
                        item.name,
                        item.model_id.as_deref().unwrap_or("unknown")
                    ),
                );
            }
            SetupSeedStatus::Skipped => {
                output.info(format!("  {} - already exists, skipping", item.name));
            }
            SetupSeedStatus::Failed => {
                output.error(format!(
                    "  {} - failed: {}",
                    item.name,
                    item.message.unwrap_or_else(|| "unknown error".to_string())
                ));
            }
        }
    }

    output.blank();
    if summary.seeded > 0 {
        output.result(format!(
            "Seeded {} model(s) ({} errors)",
            summary.seeded, summary.failed
        ));
    } else if summary.failed > 0 {
        output.error(format!("No models seeded ({} errors)", summary.failed));
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
