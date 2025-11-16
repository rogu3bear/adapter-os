//! Replay bundle for determinism testing

use crate::output::OutputWriter;
use adapteros_replay::{compare_traces, replay_trace, ReplaySession, VerificationMode};
use adapteros_telemetry::{find_divergence, format_divergence, load_replay_bundle};
use anyhow::Result;
use std::path::Path;

/// Compute Hamming distance between two f32 slices
fn hamming_distance(a: &[f32], b: &[f32]) -> usize {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let a_bits = x.to_bits();
            let b_bits = y.to_bits();
            (a_bits ^ b_bits).count_ones() as usize
        })
        .sum()
}

/// Extract logit-like data from events (simplified for demonstration)
fn extract_logits(events: &[adapteros_telemetry::replay::ReplayEvent]) -> Vec<Vec<f32>> {
    events
        .iter()
        .filter(|e| e.event_type == "inference.token" || e.event_type == "inference.logits")
        .filter_map(|e| {
            // Try to extract logits from payload
            // In real implementation, would parse actual logit arrays
            e.payload
                .get("logits")
                .and_then(|l| l.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_f64().map(|f| f as f32))
                        .collect()
                })
        })
        .collect()
}

/// Run diff between two bundles
pub async fn diff_bundles(bundle_a: &Path, bundle_b: &Path) -> Result<()> {
    println!("📊 Reproducibility Report");
    println!("========================\n");

    println!("Bundle A: {}", bundle_a.display());
    println!("Bundle B: {}", bundle_b.display());
    println!();

    // Load both bundles
    let events_a = load_replay_bundle(bundle_a)?;
    let events_b = load_replay_bundle(bundle_b)?;

    println!("Bundle A: {} events", events_a.events.len());
    println!("Bundle B: {} events", events_b.events.len());
    println!();

    // Extract logits
    let logits_a = extract_logits(&events_a.events);
    let logits_b = extract_logits(&events_b.events);

    // Sample every 10th token
    let sampled_a: Vec<_> = logits_a.iter().step_by(10).collect();
    let sampled_b: Vec<_> = logits_b.iter().step_by(10).collect();

    println!("Tokens sampled: {}", sampled_a.len().min(sampled_b.len()));

    // Compute bit differences
    let mut total_bits = 0;
    let mut divergences = Vec::new();

    for (idx, (a, b)) in sampled_a.iter().zip(sampled_b.iter()).enumerate() {
        if a.len() != b.len() {
            println!("⚠️  Token {} has different logit dimensions", idx);
            continue;
        }

        let bits = hamming_distance(a, b);
        total_bits += bits;

        if bits > 0 {
            divergences.push((idx * 10, bits)); // Scale back to original token index
        }
    }

    let exact_matches = sampled_a.len().min(sampled_b.len()) - divergences.len();
    let match_pct = if sampled_a.len() > 0 {
        (exact_matches as f64 / sampled_a.len() as f64) * 100.0
    } else {
        0.0
    };

    println!("Exact matches: {} ({:.1}%)", exact_matches, match_pct);
    println!("Bit differences: {} bits total", total_bits);

    if sampled_a.len() > 0 {
        let avg_hamming = total_bits as f64 / sampled_a.len() as f64;
        println!("Hamming distance (avg): {:.2} bits/token", avg_hamming);
    }

    if !divergences.is_empty() {
        println!();
        println!("Top divergences:");
        divergences.sort_by_key(|(_, bits)| std::cmp::Reverse(*bits));

        for (idx, bits) in divergences.iter().take(5) {
            println!("  Token {}: {} bits differ", idx, bits);
        }
    }

    println!();

    if divergences.is_empty() {
        println!("✅ Bit-for-bit identical");
    } else if match_pct >= 95.0 {
        println!("⚠️  Minor divergences detected ({:.1}% match)", match_pct);
    } else {
        println!("❌ Significant divergences ({:.1}% match)", match_pct);
    }

    Ok(())
}

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

/// Run replay with different verification modes
pub async fn run_with_mode(
    bundle: &Path,
    mode: VerificationMode,
    verbose: bool,
    output: &OutputWriter,
) -> Result<()> {
    output.info(format!(
        "Replaying bundle with {:?} verification: {}",
        mode,
        bundle.display()
    ));

    let mut session = ReplaySession::from_log_with_mode(bundle, mode)?;

    // Run with progress tracking
    session
        .run_with_progress(|stats| {
            if verbose {
                println!(
                    "Progress: {:.1}% ({} of {} events)",
                    stats.progress_percent, stats.current_step, stats.total_events
                );
            }
        })
        .await?;

    let final_stats = session.stats().await;

    println!("✓ Replay completed with {:?} verification", mode);
    println!("  Total events: {}", final_stats.total_events);
    println!("  Verified operations: {}", final_stats.verified_ops);
    println!(
        "  Success rate: {:.1}%",
        if final_stats.total_events > 0 {
            (final_stats.verified_ops as f64 / final_stats.total_events as f64) * 100.0
        } else {
            0.0
        }
    );

    Ok(())
}

/// Compare two trace files
pub async fn compare(
    trace_a: &Path,
    trace_b: &Path,
    verbose: bool,
    output: &OutputWriter,
) -> Result<()> {
    output.info(format!(
        "Comparing traces: {} vs {}",
        trace_a.display(),
        trace_b.display()
    ));

    let result = compare_traces(trace_a, trace_b).await?;

    match result {
        adapteros_replay::ComparisonResult::Identical => {
            println!("✓ Traces are identical");
            println!("  No divergences detected");
        }
        adapteros_replay::ComparisonResult::Divergent { reason, step } => {
            println!("❌ Traces diverge at step {}", step);
            println!("  Reason: {}", reason);

            if verbose {
                println!("\nDetailed comparison:");
                println!("  Trace A: {}", trace_a.display());
                println!("  Trace B: {}", trace_b.display());
                println!("  Divergence point: step {}", step);
            }

            std::process::exit(1);
        }
    }

    Ok(())
}
