//! Replay bundle for determinism testing

use crate::output::OutputWriter;
use adapteros_replay::replay_trace;
use anyhow::Result;
use std::path::Path;

pub async fn run(bundle: &Path, verbose: bool, output: &OutputWriter) -> Result<()> {
    output.info(format!("Replaying bundle: {}", bundle.display()));

    // Use the new replay system
    let stats = replay_trace(bundle).await?;

    println!("  Total events: {}", stats.total_events);
    println!("  Verified operations: {}", stats.verified_ops);
    println!("  Progress: {:.1}%", stats.progress_percent);
    println!("  Complete: {}", stats.is_complete);

    if stats.is_complete {
        println!("✓ Replay completed successfully");
        println!("  All {} operations verified", stats.verified_ops);
    } else {
        println!("⚠️  Replay incomplete");
        println!(
            "  {} of {} operations verified",
            stats.verified_ops, stats.total_events
        );
    }

    if verbose {
        println!("\nDetailed statistics:");
        println!("  Current step: {}", stats.current_step);
        println!("  Total steps: {}", stats.total_events);
        println!(
            "  Verification rate: {:.1}%",
            if stats.total_events > 0 {
                (stats.verified_ops as f64 / stats.total_events as f64) * 100.0
            } else {
                0.0
            }
        );
    }

    Ok(())
}

