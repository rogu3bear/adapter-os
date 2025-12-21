//! Router calibration commands

use adapteros_lora_router::{CalibrationDataset, Calibrator, OptimizationMethod, RouterWeights};
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Debug, Subcommand, Clone)]
pub enum RouterCmd {
    /// Calibrate router weights using a dataset
    Calibrate(CalibrateArgs),
    /// Validate router weights on a dataset
    Validate(ValidateArgs),
    /// Show current router weights
    Show(ShowArgs),
    /// Enable or disable safe mode (routes through safety adapter only)
    SafeMode(SafeModeArgs),
    /// Query routing decisions from database
    Decisions(DecisionsArgs),
}

#[derive(Debug, Parser, Clone)]
pub struct CalibrateArgs {
    /// Path to calibration dataset (JSON file)
    #[arg(short, long)]
    dataset: PathBuf,

    /// Output path for calibrated weights
    #[arg(short, long, default_value = "router_weights.json")]
    output: PathBuf,

    /// Optimization method: grid-search or gradient-descent
    #[arg(short, long, default_value = "grid-search")]
    method: String,

    /// Number of top adapters to select (K)
    #[arg(short, long, default_value = "3")]
    k: usize,

    /// Train/validation split ratio
    #[arg(long, default_value = "0.8")]
    train_ratio: f32,
}

#[derive(Debug, Parser, Clone)]
pub struct ValidateArgs {
    /// Path to calibration dataset (JSON file)
    #[arg(short, long)]
    dataset: PathBuf,

    /// Path to weights file (if not specified, use default weights)
    #[arg(short, long)]
    weights: Option<PathBuf>,
}

#[derive(Debug, Parser, Clone)]
pub struct ShowArgs {
    /// Path to weights file (if not specified, show default weights)
    #[arg(short, long)]
    weights: Option<PathBuf>,
}

#[derive(Debug, Parser, Clone)]
pub struct SafeModeArgs {
    /// Enable safe mode (true) or disable it (false)
    #[arg(short, long)]
    enable: bool,

    /// Path to router config file to update
    #[arg(short, long, default_value = "router_config.json")]
    config: PathBuf,
}

#[derive(Debug, Parser, Clone)]
pub struct DecisionsArgs {
    /// Database path (defaults to var/aos-cp.sqlite3)
    #[arg(long, default_value = "var/aos-cp.sqlite3")]
    db: PathBuf,

    /// Tenant ID to filter by
    #[arg(short, long, default_value = "default")]
    tenant: String,

    /// Stack ID to filter by
    #[arg(long)]
    stack: Option<String>,

    /// Adapter ID to filter by
    #[arg(long)]
    adapter: Option<String>,

    /// Filter to decisions since this ISO-8601 timestamp
    #[arg(long)]
    since: Option<String>,

    /// Maximum number of results to return
    #[arg(short, long, default_value = "50")]
    limit: usize,

    /// Output format: table (default) or json
    #[arg(short, long, default_value = "table")]
    format: String,

    /// Show only anomalies (high overhead or low entropy)
    #[arg(long)]
    anomalies: bool,
}

impl RouterCmd {
    pub fn run(self) -> Result<()> {
        match self {
            RouterCmd::Calibrate(args) => calibrate(args),
            RouterCmd::Validate(args) => validate(args),
            RouterCmd::Show(args) => show(args),
            RouterCmd::SafeMode(args) => safe_mode(args),
            RouterCmd::Decisions(args) => decisions(args),
        }
    }
}

fn calibrate(args: CalibrateArgs) -> Result<()> {
    println!("Loading calibration dataset from {:?}...", args.dataset);
    let dataset =
        CalibrationDataset::load(&args.dataset).context("Failed to load calibration dataset")?;

    println!("Dataset loaded: {} samples", dataset.samples.len());

    // Split into train/val
    let (train_dataset, val_dataset) = dataset.train_val_split(args.train_ratio);
    println!(
        "Split: {} training samples, {} validation samples",
        train_dataset.samples.len(),
        val_dataset.samples.len()
    );

    // Parse optimization method
    let method = match args.method.to_lowercase().as_str() {
        "grid-search" | "grid" => OptimizationMethod::GridSearch,
        "gradient-descent" | "gradient" => OptimizationMethod::GradientDescent,
        _ => {
            anyhow::bail!(
                "Invalid optimization method: {}. Use 'grid-search' or 'gradient-descent'",
                args.method
            );
        }
    };

    println!("Starting calibration with {:?} method...", method);
    let calibrator = Calibrator::new(train_dataset.clone(), method, args.k);

    let weights = calibrator.train().context("Calibration failed")?;

    println!("\nCalibrated weights:");
    println!("  Language:   {:.4}", weights.language_weight);
    println!("  Framework:  {:.4}", weights.framework_weight);
    println!("  Symbols:    {:.4}", weights.symbol_hits_weight);
    println!("  Paths:      {:.4}", weights.path_tokens_weight);
    println!("  Verb:       {:.4}", weights.prompt_verb_weight);
    println!("  Total:      {:.4}", weights.total_weight());

    // Validate on training set
    println!("\nTraining set metrics:");
    let train_calibrator = Calibrator::new(train_dataset, method, args.k);
    let train_metrics = train_calibrator.validate(&weights);
    print_metrics(&train_metrics);

    // Validate on validation set
    if !val_dataset.samples.is_empty() {
        println!("\nValidation set metrics:");
        let val_calibrator = Calibrator::new(val_dataset, method, args.k);
        let val_metrics = val_calibrator.validate(&weights);
        print_metrics(&val_metrics);
    }

    // Save weights
    weights
        .save(&args.output)
        .context("Failed to save weights")?;
    println!("\nWeights saved to {:?}", args.output);

    Ok(())
}

fn validate(args: ValidateArgs) -> Result<()> {
    println!("Loading calibration dataset from {:?}...", args.dataset);
    let dataset =
        CalibrationDataset::load(&args.dataset).context("Failed to load calibration dataset")?;

    let weights = if let Some(weights_path) = args.weights {
        println!("Loading weights from {:?}...", weights_path);
        RouterWeights::load(weights_path).context("Failed to load weights")?
    } else {
        println!("Using default weights...");
        RouterWeights::default()
    };

    println!("\nWeights:");
    println!("  Language:   {:.4}", weights.language_weight);
    println!("  Framework:  {:.4}", weights.framework_weight);
    println!("  Symbols:    {:.4}", weights.symbol_hits_weight);
    println!("  Paths:      {:.4}", weights.path_tokens_weight);
    println!("  Verb:       {:.4}", weights.prompt_verb_weight);

    let calibrator = Calibrator::new(dataset, OptimizationMethod::GridSearch, 3);
    let metrics = calibrator.validate(&weights);

    println!("\nValidation metrics:");
    print_metrics(&metrics);

    Ok(())
}

fn show(args: ShowArgs) -> Result<()> {
    let weights = if let Some(weights_path) = args.weights {
        println!("Loading weights from {:?}...\n", weights_path);
        RouterWeights::load(weights_path).context("Failed to load weights")?
    } else {
        println!("Default router weights:\n");
        RouterWeights::default()
    };

    println!("Router Weights:");
    println!("  Language:   {:.4}", weights.language_weight);
    println!("  Framework:  {:.4}", weights.framework_weight);
    println!("  Symbols:    {:.4}", weights.symbol_hits_weight);
    println!("  Paths:      {:.4}", weights.path_tokens_weight);
    println!("  Verb:       {:.4}", weights.prompt_verb_weight);
    println!("\nTotal weight: {:.4}", weights.total_weight());

    Ok(())
}

fn print_metrics(metrics: &adapteros_lora_router::ValidationMetrics) {
    println!("  Accuracy:  {:.4}", metrics.accuracy);
    println!("  Precision: {:.4}", metrics.precision);
    println!("  Recall:    {:.4}", metrics.recall);
    println!("  F1 Score:  {:.4}", metrics.f1_score);
    println!("  MRR:       {:.4}", metrics.mrr);
    println!("  Score:     {:.4}", metrics.score());
}

fn safe_mode(args: SafeModeArgs) -> Result<()> {
    // Note: RouterConfig does not currently have a safe_mode field.
    // This command is a placeholder for future implementation.

    println!(
        "Safe mode {} requested for config {:?}",
        if args.enable { "enable" } else { "disable" },
        args.config
    );

    println!("\n⚠ Note: Safe mode is not yet implemented in RouterConfig.");
    println!("This feature will be added in a future release.");

    if args.enable {
        println!("\nWhen implemented, safe mode will:");
        println!("  - Only use safety adapters for routing");
        println!("  - Filter all queries through the safety layer");
        println!("  - May reduce response quality for non-safety queries");
    } else {
        println!("\nWhen implemented, disabling safe mode will:");
        println!("  - Make all adapters available for routing");
        println!("  - Use standard K-sparse selection");
    }

    Ok(())
}

fn decisions(args: DecisionsArgs) -> Result<()> {
    use adapteros_db::{Db, RoutingDecisionFilters};
    use tokio::runtime::Runtime;

    // Create async runtime to run database queries
    let rt = Runtime::new().context("Failed to create Tokio runtime")?;

    rt.block_on(async {
        // Connect to database
        println!("Connecting to database: {:?}", args.db);
        let db_path_str = args.db.to_str().ok_or_else(|| {
            anyhow::anyhow!(
                "Database path contains invalid UTF-8: {}",
                args.db.display()
            )
        })?;
        let db = Db::connect(db_path_str)
            .await
            .context("Failed to connect to database")?;

        // Build query filters
        let filters = RoutingDecisionFilters {
            tenant_id: Some(args.tenant.clone()),
            stack_id: args.stack.clone(),
            adapter_id: args.adapter.clone(),
            request_id: None,
            source_type: None,
            since: args.since.clone(),
            until: None,
            min_entropy: if args.anomalies { Some(0.0) } else { None },
            max_overhead_pct: None,
            limit: Some(args.limit),
            offset: None,
        };

        // Query routing decisions
        let decisions = if args.anomalies {
            db.get_low_entropy_decisions(Some(args.tenant.clone()), args.limit)
                .await
                .context("Failed to query anomalous routing decisions")?
        } else {
            db.query_routing_decisions(&filters)
                .await
                .context("Failed to query routing decisions")?
        };

        if decisions.is_empty() {
            println!("\nNo routing decisions found matching the filters.");
            return Ok(());
        }

        println!("\nFound {} routing decisions:\n", decisions.len());

        // Output results
        match args.format.as_str() {
            "json" => {
                // JSON output
                let json = serde_json::to_string_pretty(&decisions)
                    .context("Failed to serialize decisions to JSON")?;
                println!("{}", json);
            }
            _ => {
                // Table output (default)
                println!(
                    "{:<12} {:<6} {:<10} {:<8} {:<8} {:<12} {:<20}",
                    "Request", "Step", "Entropy", "K", "Overhead", "Latency(us)", "Stack"
                );
                println!("{}", "-".repeat(90));

                for decision in &decisions {
                    let request_short = decision
                        .request_id
                        .as_ref()
                        .map(|r| {
                            if r.len() > 12 {
                                format!("{}...", &r[..9])
                            } else {
                                r.clone()
                            }
                        })
                        .unwrap_or_else(|| "N/A".to_string());

                    let stack_short = decision
                        .stack_hash
                        .as_ref()
                        .map(|h| {
                            if h.len() > 16 {
                                format!("{}...", &h[..13])
                            } else {
                                h.clone()
                            }
                        })
                        .unwrap_or_else(|| "N/A".to_string());

                    println!(
                        "{:<12} {:<6} {:<10.2} {:<8} {:<8.2} {:<12} {:<20}",
                        request_short,
                        decision.step,
                        decision.entropy,
                        decision.k_value.unwrap_or(0),
                        decision.overhead_pct.unwrap_or(0.0),
                        decision.router_latency_us.unwrap_or(0),
                        stack_short
                    );
                }

                println!(
                    "\n{} decisions displayed (limit: {})",
                    decisions.len(),
                    args.limit
                );

                // Show summary statistics
                let avg_entropy: f64 =
                    decisions.iter().map(|d| d.entropy).sum::<f64>() / decisions.len() as f64;
                let avg_k: f64 = decisions.iter().filter_map(|d| d.k_value).sum::<i64>() as f64
                    / decisions.len() as f64;
                let avg_latency: f64 = decisions
                    .iter()
                    .filter_map(|d| d.router_latency_us)
                    .sum::<i64>() as f64
                    / decisions
                        .iter()
                        .filter(|d| d.router_latency_us.is_some())
                        .count() as f64;

                println!("\nSummary Statistics:");
                println!("  Average Entropy:        {:.3}", avg_entropy);
                println!("  Average K:              {:.1}", avg_k);
                println!("  Average Router Latency: {:.0} μs", avg_latency);

                if args.anomalies {
                    println!("\n⚠ Showing only anomalous decisions (low entropy or high overhead)");
                }
            }
        }

        Ok(())
    })
}
