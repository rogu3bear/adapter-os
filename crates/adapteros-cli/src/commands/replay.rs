//! Replay bundle for determinism testing

use crate::output::OutputWriter;
use adapteros_replay::replay_trace;
use anyhow::{Context, Result};
use serde::Serialize;
use std::path::Path;

#[derive(Serialize)]
struct ReplayOutcome<'a> {
    outcome: &'a str,
    #[serde(flatten)]
    stats: serde_json::Value,
}

pub async fn run(bundle: &Path, verbose: bool, output: &OutputWriter) -> Result<()> {
    // Validate path upfront
    if !bundle.exists() || !bundle.is_file() {
        anyhow::bail!("bundle not found or invalid: {}", bundle.display());
    }

    output.info(format!("Replaying bundle: {}", bundle.display()));

    // Execute replay
    let stats = replay_trace(bundle)
        .await
        .with_context(|| format!("replay failed: {}", bundle.display()))?;

    // JSON output: emit machine-readable summary and return non-zero on incomplete
    if output.is_json() {
        let stats_json = serde_json::json!({
            "total_events": stats.total_events,
            "verified_ops": stats.verified_ops,
            "progress_percent": stats.progress_percent,
            "is_complete": stats.is_complete,
            "current_step": stats.current_step,
        });
        let outcome = if stats.is_complete { "complete" } else { "incomplete" };
        output.json(&ReplayOutcome { outcome, stats: stats_json })?;
        if !stats.is_complete {
            anyhow::bail!("replay incomplete");
        }
        return Ok(());
    }

    // Human output
    output.kv("Total events", &stats.total_events.to_string());
    output.kv("Verified operations", &stats.verified_ops.to_string());
    output.kv("Progress", &format!("{:.1}%", stats.progress_percent));
    output.kv("Complete", if stats.is_complete { "true" } else { "false" });

    if stats.is_complete {
        output.success("Replay completed successfully");
        output.kv("All operations verified", &stats.verified_ops.to_string());
    } else {
        output.warning("Replay incomplete");
        output.kv(
            "Verified",
            &format!("{} of {} operations", stats.verified_ops, stats.total_events),
        );
    }

    if verbose {
        output.section("Detailed statistics");
        output.kv("Current step", &stats.current_step.to_string());
        output.kv("Total steps", &stats.total_events.to_string());
        let rate = if stats.total_events > 0 {
            (stats.verified_ops as f64 / stats.total_events as f64) * 100.0
        } else {
            0.0
        };
        output.kv("Verification rate", &format!("{:.1}%", rate));
    }

    // Fail CI when incomplete
    if !stats.is_complete {
        anyhow::bail!("replay incomplete");
    }

    Ok(())
}
