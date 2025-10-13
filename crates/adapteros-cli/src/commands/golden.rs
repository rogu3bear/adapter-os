//! Golden run archive management

use crate::output::OutputWriter;
use clap::{Parser, Subcommand};
use adapteros_verify::{
    archive::{create_golden_run, GoldenRunArchive},
    init_golden_runs_dir, list_golden_runs,
    verification::verify_against_golden,
    ComparisonConfig, StrictnessLevel,
};
use anyhow::{Context, Result};
use adapteros_crypto::Keypair;
use std::path::{Path, PathBuf};

/// Golden run commands
#[derive(Debug, Subcommand, Clone)]
pub enum GoldenCmd {
    /// Initialize golden_runs directory
    Init,
    /// Create a golden run from a replay bundle
    Create(CreateArgs),
    /// Verify a bundle against a golden run
    Verify(VerifyArgs),
    /// List available golden runs
    List,
    /// Show details of a golden run
    Show(ShowArgs),
}

/// Arguments for 'golden create'
#[derive(Debug, Parser, Clone)]
pub struct CreateArgs {
    /// Path to replay bundle
    #[arg(short, long)]
    pub bundle: PathBuf,

    /// Name for the golden run
    #[arg(short, long)]
    pub name: String,

    /// Toolchain version (defaults to current)
    #[arg(short, long)]
    pub toolchain: Option<String>,

    /// Adapter IDs (comma-separated)
    #[arg(short, long, value_delimiter = ',')]
    pub adapters: Vec<String>,

    /// Sign the golden run
    #[arg(short, long)]
    pub sign: bool,
}

/// Arguments for 'golden verify'
#[derive(Debug, Parser, Clone)]
pub struct VerifyArgs {
    /// Name of golden run to verify against
    #[arg(short, long)]
    pub golden: String,

    /// Path to bundle to verify
    #[arg(short, long)]
    pub bundle: PathBuf,

    /// Strictness level (bitwise, epsilon-tolerant, statistical)
    #[arg(short, long, value_enum)]
    pub strictness: Option<StrictnessLevel>,

    /// Skip toolchain verification
    #[arg(long)]
    pub skip_toolchain: bool,

    /// Skip signature verification
    #[arg(long)]
    pub skip_signature: bool,
}

/// Arguments for 'golden show'
#[derive(Debug, Parser, Clone)]
pub struct ShowArgs {
    /// Name of golden run to show
    pub name: String,
}

/// Execute golden command
pub async fn execute(cmd: &GoldenCmd, output: &OutputWriter) -> Result<()> {
    match cmd {
        GoldenCmd::Init => init(output).await,
        GoldenCmd::Create(args) => {
            let adapter_refs: Vec<&str> = args.adapters.iter().map(|s| s.as_str()).collect();
            create(
                &args.bundle,
                &args.name,
                args.toolchain.as_deref(),
                adapter_refs,
                args.sign,
                output,
            )
            .await
        }
        GoldenCmd::Verify(args) => {
            verify(
                &args.golden,
                &args.bundle,
                args.strictness,
                args.skip_toolchain,
                args.skip_signature,
                output,
            )
            .await
        }
        GoldenCmd::List => list(output).await,
        GoldenCmd::Show(args) => show(&args.name, output).await,
    }
}

/// Create a new golden run from a replay bundle
pub async fn create(
    bundle_path: &Path,
    name: &str,
    toolchain_version: Option<&str>,
    adapters: Vec<&str>,
    sign: bool,
    output: &OutputWriter,
) -> Result<()> {
    output.info(format!("Creating golden run from bundle: {}", bundle_path.display()));

    // Initialize golden_runs directory if needed
    let golden_runs_dir = init_golden_runs_dir(".")
        .context("Failed to initialize golden_runs directory")?;

    // Determine toolchain version
    let toolchain = toolchain_version.unwrap_or(env!("CARGO_PKG_RUST_VERSION"));

    output.progress("Extracting epsilon statistics from bundle");

    // Create the golden run archive
    let mut archive = create_golden_run(bundle_path, toolchain, &adapters)
        .await
        .context("Failed to create golden run")?;

    output.progress_done(true);

    output.kv("Run ID", &archive.metadata.run_id);
    output.kv("CPID", &archive.metadata.cpid);
    output.kv("Plan ID", &archive.metadata.plan_id);
    output.kv("Toolchain", &archive.metadata.toolchain.summary());
    output.kv("Adapters", &format!("{} adapters", archive.metadata.adapters.len()));
    output.kv(
        "Epsilon stats",
        &format!("{} layers", archive.epsilon_stats.layer_stats.len()),
    );
    output.kv(
        "Max epsilon",
        &format!("{:.6e}", archive.epsilon_stats.max_epsilon()),
    );

    // Sign if requested
    if sign {
        output.progress("Signing golden run");

        // Generate or load keypair (in production, would use Secure Enclave)
        let keypair = Keypair::generate();

        archive.sign(&keypair)
            .context("Failed to sign golden run")?;

        output.progress_done(true);
        output.success("Golden run signed");
    }

    // Save to baselines directory
    let baseline_dir = golden_runs_dir.join("baselines").join(name);
    output.progress(format!("Saving to {}", baseline_dir.display()));

    archive.save(&baseline_dir)
        .context("Failed to save golden run")?;

    output.progress_done(true);
    output.blank();
    output.success(format!("Golden run created: {}", name));
    output.kv("Location", &baseline_dir.display().to_string());

    if output.is_json() {
        output.json(&serde_json::json!({
            "status": "success",
            "name": name,
            "run_id": archive.metadata.run_id,
            "location": baseline_dir.display().to_string(),
            "signed": sign,
        }))?;
    }

    Ok(())
}

/// Verify a bundle against a golden run
pub async fn verify(
    golden_name: &str,
    bundle_path: &Path,
    strictness: Option<StrictnessLevel>,
    skip_toolchain: bool,
    skip_signature: bool,
    output: &OutputWriter,
) -> Result<()> {
    output.info(format!("Verifying bundle: {}", bundle_path.display()));
    output.info(format!("Against golden run: {}", golden_name));

    // Locate golden run
    let golden_dir = Path::new("golden_runs/baselines").join(golden_name);

    if !golden_dir.exists() {
        anyhow::bail!("Golden run not found: {}", golden_name);
    }

    // Configure comparison
    let mut config = ComparisonConfig::default();
    if let Some(s) = strictness {
        config.strictness = s;
    }
    if skip_toolchain {
        config.verify_toolchain = false;
    }
    if skip_signature {
        config.verify_signature = false;
    }

    output.progress("Loading golden run archive");

    // Run verification
    let report = verify_against_golden(&golden_dir, bundle_path, &config)
        .await
        .context("Verification failed")?;

    output.progress_done(true);
    output.blank();

    // Print report
    if output.is_json() {
        output.json(&report)?;
    } else {
        println!("{}", report.summary());
    }

    // Exit with error code if verification failed
    if !report.passed {
        std::process::exit(1);
    }

    Ok(())
}

/// List available golden runs
pub async fn list(output: &OutputWriter) -> Result<()> {
    output.info("Available golden runs:");

    let golden_runs_dir = Path::new("golden_runs");
    if !golden_runs_dir.exists() {
        output.warning("No golden_runs directory found");
        output.verbose("Run 'aosctl golden init' to create it");
        return Ok(());
    }

    let runs = list_golden_runs(golden_runs_dir)
        .context("Failed to list golden runs")?;

    if runs.is_empty() {
        output.warning("No golden runs found");
        output.verbose("Create a golden run with: aosctl golden create --bundle <path> --name <name>");
        return Ok(());
    }

    if output.is_json() {
        output.json(&serde_json::json!({
            "count": runs.len(),
            "runs": runs,
        }))?;
    } else {
        for run_name in &runs {
            // Try to load metadata
            let run_dir = golden_runs_dir.join("baselines").join(run_name);
            if let Ok(archive) = GoldenRunArchive::load(&run_dir) {
                output.kv(run_name, "");
                output.verbose(format!("  CPID: {}", archive.metadata.cpid));
                output.verbose(format!("  Plan: {}", archive.metadata.plan_id));
                output.verbose(format!("  Created: {}", archive.metadata.created_at.format("%Y-%m-%d %H:%M UTC")));
                output.verbose(format!("  Toolchain: {}", archive.metadata.toolchain.summary()));
                output.verbose(format!("  Signed: {}", if archive.signature.is_some() { "yes" } else { "no" }));
                output.blank();
            } else {
                output.kv(run_name, "(failed to load)");
            }
        }

        output.info(format!("Total: {} golden runs", runs.len()));
    }

    Ok(())
}

/// Show details of a golden run
pub async fn show(golden_name: &str, output: &OutputWriter) -> Result<()> {
    let golden_dir = Path::new("golden_runs/baselines").join(golden_name);

    if !golden_dir.exists() {
        anyhow::bail!("Golden run not found: {}", golden_name);
    }

    output.info(format!("Loading golden run: {}", golden_name));

    let archive = GoldenRunArchive::load(&golden_dir)
        .context("Failed to load golden run")?;

    if output.is_json() {
        output.json(&archive)?;
    } else {
        println!("{}", archive.metadata.summary());
        println!();
        println!("Epsilon Statistics:");
        println!("  Layers: {}", archive.epsilon_stats.layer_stats.len());
        println!("  Max epsilon: {:.6e}", archive.epsilon_stats.max_epsilon());
        println!("  Mean epsilon: {:.6e}", archive.epsilon_stats.mean_epsilon());
        println!();
        println!("Bundle Hash: {}", archive.bundle_hash);
        println!("Signed: {}", if archive.signature.is_some() { "yes" } else { "no" });
    }

    Ok(())
}

/// Initialize golden_runs directory structure
pub async fn init(output: &OutputWriter) -> Result<()> {
    output.info("Initializing golden_runs directory");

    let golden_runs_dir = init_golden_runs_dir(".")
        .context("Failed to initialize golden_runs directory")?;

    output.success(format!("Initialized: {}", golden_runs_dir.display()));
    output.kv("baselines/", "Active golden run baselines");
    output.kv("archive/", "Archived golden runs");
    output.kv("README.md", "Documentation");

    Ok(())
}

