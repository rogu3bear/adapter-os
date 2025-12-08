//! Replay bundle commands: record, run, inspect, verify

use crate::output::OutputWriter;
use adapteros_core::B3Hash;
use adapteros_telemetry::BundleWriter;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tokio::fs;

#[derive(Debug, clap::Subcommand)]
pub enum ReplaySubcommand {
    /// Record execution of a command to a replay bundle
    Record {
        /// Output bundle path
        #[arg(short, long)]
        out: PathBuf,
        /// Command to record (remaining args)
        #[arg(allow_hyphen_values = true, trailing_var_arg = true)]
        cmd: Vec<String>,
    },
    /// Run a replay bundle
    Run {
        /// Input bundle path
        #[arg(short, long)]
        in_bundle: PathBuf,
    },
    /// Inspect bundle contents
    Inspect {
        /// Input bundle path
        #[arg(short, long)]
        in_bundle: PathBuf,
    },
    /// Verify determinism by running multiple times
    Verify {
        /// Input bundle path
        #[arg(short, long)]
        in_bundle: PathBuf,
        /// Number of runs to verify
        #[arg(short, long, default_value = "10")]
        runs: u32,
    },
}

pub async fn handle_replay_command(cmd: ReplaySubcommand, output: &OutputWriter) -> Result<()> {
    match cmd {
        ReplaySubcommand::Record { out, cmd } => record_command(&out, &cmd, output).await,
        ReplaySubcommand::Run { in_bundle } => run_bundle(&in_bundle, output).await,
        ReplaySubcommand::Inspect { in_bundle } => inspect_bundle(&in_bundle, output).await,
        ReplaySubcommand::Verify { in_bundle, runs } => {
            verify_determinism(&in_bundle, runs, output).await
        }
    }
}

async fn record_command(out: &Path, cmd: &[String], output: &OutputWriter) -> Result<()> {
    if cmd.is_empty() {
        anyhow::bail!("No command specified");
    }

    output.info(format!("Recording command: {:?}", cmd.join(" ")));
    output.info(format!("Output bundle: {}", out.display()));

    // Ensure output directory exists
    if let Some(parent) = out.parent() {
        fs::create_dir_all(parent).await?;
    }

    // Initialize bundle writer
    let mut bundle_writer = BundleWriter::new(
        out.parent().unwrap_or(Path::new(".")),
        10000,       // max_events
        100_000_000, // max_bytes (100MB)
    )?;

    // Record seed and metadata
    let seed = B3Hash::hash(cmd.join(" ").as_bytes());
    let metadata = serde_json::json!({
        "command": cmd,
        "seed": seed.to_string(),
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    bundle_writer.write_event(&metadata)?;

    // Execute command and capture output
    let mut process = Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let result = process.wait()?;
    let exit_code = result.code().unwrap_or(-1);

    // Record completion event
    let completion = serde_json::json!({
        "event_type": "command.complete",
        "exit_code": exit_code,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    });
    bundle_writer.write_event(&completion)?;

    bundle_writer.flush()?;

    output.success(format!(
        "Recorded execution to bundle: {} (exit code: {})",
        out.display(),
        exit_code
    ));

    Ok(())
}

async fn run_bundle(in_bundle: &Path, output: &OutputWriter) -> Result<()> {
    if !in_bundle.exists() {
        anyhow::bail!("Bundle not found: {}", in_bundle.display());
    }

    output.info(format!("Running replay bundle: {}", in_bundle.display()));

    // Load bundle (using adapteros_replay infrastructure)
    let bundle = adapteros_telemetry::load_replay_bundle(in_bundle)
        .with_context(|| format!("Failed to load bundle: {}", in_bundle.display()))?;

    output.kv("Bundle ID", &bundle.cpid);
    output.kv("Plan ID", &bundle.plan_id);
    output.kv("Seed", &bundle.seed_global.to_string());
    output.kv("Events", &bundle.events.len().to_string());

    // Execute replay
    let stats = adapteros_replay::replay_trace(in_bundle).await?;

    output.kv("Verified operations", &stats.verified_ops.to_string());
    output.kv("Progress", &format!("{:.1}%", stats.progress_percent));

    if stats.is_complete {
        output.success("Replay completed successfully");
    } else {
        output.warning("Replay incomplete");
    }

    Ok(())
}

async fn inspect_bundle(in_bundle: &Path, output: &OutputWriter) -> Result<()> {
    if !in_bundle.exists() {
        anyhow::bail!("Bundle not found: {}", in_bundle.display());
    }

    output.info(format!("Inspecting bundle: {}", in_bundle.display()));

    let bundle = adapteros_telemetry::load_replay_bundle(in_bundle)
        .with_context(|| format!("Failed to load bundle: {}", in_bundle.display()))?;

    if output.is_json() {
        output.json(&bundle)?;
    } else {
        output.section("Bundle Metadata");
        output.kv("CPID", &bundle.cpid);
        output.kv("Plan ID", &bundle.plan_id);
        output.kv("Seed", &bundle.seed_global.to_string());
        output.kv("Total events", &bundle.events.len().to_string());
        output.kv("RNG checkpoints", &bundle.rng_checkpoints.len().to_string());

        if !bundle.events.is_empty() {
            output.section("Event Summary");
            output.kv("First event", &bundle.events[0].event_type);
            output.kv(
                "Last event",
                &bundle.events[bundle.events.len() - 1].event_type,
            );
        }
    }

    Ok(())
}

async fn verify_determinism(in_bundle: &Path, runs: u32, output: &OutputWriter) -> Result<()> {
    if !in_bundle.exists() {
        anyhow::bail!("Bundle not found: {}", in_bundle.display());
    }

    output.info(format!(
        "Verifying determinism: {} runs of {}",
        runs,
        in_bundle.display()
    ));

    let mut results = Vec::new();

    for run in 1..=runs {
        output.info(format!("Run {}/{}", run, runs));
        let stats = adapteros_replay::replay_trace(in_bundle).await?;
        results.push(stats);
    }

    // Check all runs produced identical results
    let first_result = &results[0];
    let all_identical = results.iter().all(|r| {
        r.verified_ops == first_result.verified_ops
            && r.total_events == first_result.total_events
            && r.is_complete == first_result.is_complete
    });

    if output.is_json() {
        let summaries: Vec<_> = results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "verified_ops": r.verified_ops,
                    "total_events": r.total_events,
                    "is_complete": r.is_complete,
                    "progress_percent": r.progress_percent,
                })
            })
            .collect();
        output.json(&serde_json::json!({
            "deterministic": all_identical,
            "runs": runs,
            "results": summaries,
        }))?;
    } else {
        output.section("Verification Results");
        output.kv("Runs completed", &runs.to_string());
        output.kv("Deterministic", if all_identical { "yes" } else { "no" });

        if all_identical {
            output.success("All runs produced identical results");
        } else {
            output.error("Divergence detected across runs");
            anyhow::bail!("Determinism verification failed");
        }
    }

    Ok(())
}
