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
}

#[derive(Debug, Parser, Clone)]
pub(crate) struct CalibrateArgs {
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
pub(crate) struct ValidateArgs {
    /// Path to calibration dataset (JSON file)
    #[arg(short, long)]
    dataset: PathBuf,

    /// Path to weights file (if not specified, use default weights)
    #[arg(short, long)]
    weights: Option<PathBuf>,
}

#[derive(Debug, Parser, Clone)]
pub(crate) struct ShowArgs {
    /// Path to weights file (if not specified, show default weights)
    #[arg(short, long)]
    weights: Option<PathBuf>,
}

impl RouterCmd {
    pub fn run(self) -> Result<()> {
        match self {
            RouterCmd::Calibrate(args) => calibrate(args),
            RouterCmd::Validate(args) => validate(args),
            RouterCmd::Show(args) => show(args),
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
