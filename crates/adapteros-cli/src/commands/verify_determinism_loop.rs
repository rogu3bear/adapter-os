//! Determinism Loop verification front-end for aosctl.
//!
//! Wraps the legacy `scripts/verify-determinism-loop.sh` checks in a
//! structured, JSON-aware CLI command:
//!   aosctl verify-determinism-loop [--json]
//!
//! Checks:
//! - Required source files and migrations exist
//! - Key crates compile (`cargo check`)
//! - Optional `cargo xtask determinism-report` execution
//!
//! [source: scripts/verify-determinism-loop.sh L1-L140]

use crate::output::OutputWriter;
use anyhow::Result;
use serde::Serialize;
use std::path::Path;
use std::process::Command;

#[derive(Debug, Serialize)]
pub struct DeterminismCheck {
    name: String,
    ok: bool,
    detail: String,
}

#[derive(Debug, Serialize)]
pub struct DeterminismLoopResult {
    ok: bool,
    checks: Vec<DeterminismCheck>,
}

/// Run Determinism Loop verification.
///
/// Returns process exit code (0 = all checks passed, 1 = failure).
pub async fn run(output: &OutputWriter) -> Result<i32> {
    let mut checks = Vec::new();

    // Phase 1: Core Proofs
    check_file(
        "crates/adapteros-federation/src/lib.rs",
        "Federation crate implementation",
        &mut checks,
    );
    check_file(
        "crates/adapteros-federation/src/peer.rs",
        "Peer registry module",
        &mut checks,
    );
    check_file(
        "crates/adapteros-federation/src/output_hash.rs",
        "Output hash comparison module",
        &mut checks,
    );
    check_file(
        "crates/adapteros-federation/src/signature.rs",
        "Signature exchange module",
        &mut checks,
    );
    check_file(
        "migrations/0030_federation.sql",
        "Federation database migration",
        &mut checks,
    );
    check_compile(
        "adapteros-federation",
        "Federation crate compilation",
        &mut checks,
    );

    // Policy Hash Watcher Integration
    check_file(
        "crates/adapteros-policy/src/hash_watcher.rs",
        "Policy hash watcher implementation",
        &mut checks,
    );
    check_file(
        "crates/adapteros-policy/src/quarantine.rs",
        "Quarantine manager implementation",
        &mut checks,
    );
    check_file(
        "crates/adapteros-db/src/policy_hash.rs",
        "Policy hash database operations",
        &mut checks,
    );
    check_file(
        "migrations/0029_policy_hashes.sql",
        "Policy hash migration",
        &mut checks,
    );
    check_file(
        "docs/policy-hash-watcher.md",
        "Policy hash watcher documentation",
        &mut checks,
    );
    check_compile("adapteros-policy", "Policy crate compilation", &mut checks);

    // Phase 2: System Integrity
    check_file(
        "crates/adapteros-deterministic-exec/src/global_ledger.rs",
        "Global tick ledger implementation",
        &mut checks,
    );
    check_file(
        "migrations/0032_tick_ledger.sql",
        "Tick ledger migration",
        &mut checks,
    );
    check_compile(
        "adapteros-deterministic-exec",
        "Deterministic executor compilation",
        &mut checks,
    );

    check_file(
        "crates/adapteros-telemetry/src/uds_exporter.rs",
        "UDS metrics exporter implementation",
        &mut checks,
    );
    check_file(
        "scripts/metrics-bridge.sh",
        "Metrics bridge script",
        &mut checks,
    );
    check_compile(
        "adapteros-telemetry",
        "Telemetry crate compilation",
        &mut checks,
    );

    // Phase 3: Governance
    check_file(
        "migrations/0033_cab_lineage.sql",
        "CAB lineage migration",
        &mut checks,
    );

    check_file(
        "crates/adapteros-secd/src/host_identity.rs",
        "Host identity implementation",
        &mut checks,
    );
    check_compile(
        "adapteros-secd",
        "Secure Enclave daemon compilation",
        &mut checks,
    );

    check_file(
        "crates/adapteros-orchestrator/src/supervisor.rs",
        "Supervisor daemon implementation",
        &mut checks,
    );
    check_compile(
        "adapteros-orchestrator",
        "Orchestrator compilation",
        &mut checks,
    );

    // Integration files
    check_file(
        "crates/adapteros-lora-worker/src/inference_pipeline.rs",
        "Inference pipeline integration",
        &mut checks,
    );
    check_file(
        "crates/adapteros-cli/src/commands/policy.rs",
        "CLI policy commands",
        &mut checks,
    );

    // Documentation
    check_file(
        "DETERMINISM_LOOP_IMPLEMENTATION_SUMMARY.md",
        "Implementation summary",
        &mut checks,
    );

    // Optional: determinism-report
    run_determinism_report(&mut checks);

    let ok = checks.iter().all(|c| c.ok);
    let result = DeterminismLoopResult { ok, checks };

    if output.is_json() {
        output.json(&result)?;
    } else {
        if ok {
            output.success("Determinism Loop checks passed");
        } else {
            output.error("Determinism Loop checks failed");
        }
    }

    Ok(if ok { 0 } else { 1 })
}

fn check_file(path: &str, name: &str, checks: &mut Vec<DeterminismCheck>) {
    let exists = Path::new(path).is_file();
    checks.push(DeterminismCheck {
        name: name.to_string(),
        ok: exists,
        detail: if exists {
            format!("OK ({})", path)
        } else {
            format!("Missing: {}", path)
        },
    });
}

fn check_compile(package: &str, name: &str, checks: &mut Vec<DeterminismCheck>) {
    let status = Command::new("cargo")
        .args(["check", "--quiet", "--package", package])
        .output();

    match status {
        Ok(output) if output.status.success() => checks.push(DeterminismCheck {
            name: name.to_string(),
            ok: true,
            detail: "Compilation succeeded".to_string(),
        }),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            checks.push(DeterminismCheck {
                name: name.to_string(),
                ok: false,
                detail: format!(
                    "Compilation failed: {}",
                    stderr.lines().next().unwrap_or("")
                ),
            });
        }
        Err(e) => checks.push(DeterminismCheck {
            name: name.to_string(),
            ok: false,
            detail: format!("Failed to run cargo check: {}", e),
        }),
    }
}

fn run_determinism_report(checks: &mut Vec<DeterminismCheck>) {
    let status = Command::new("cargo")
        .args(["xtask", "determinism-report"])
        .output();

    match status {
        Ok(output) if output.status.success() => checks.push(DeterminismCheck {
            name: "determinism-report".to_string(),
            ok: true,
            detail: "cargo xtask determinism-report succeeded".to_string(),
        }),
        Ok(output) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            checks.push(DeterminismCheck {
                name: "determinism-report".to_string(),
                ok: false,
                detail: format!(
                    "cargo xtask determinism-report failed: {}",
                    stderr.lines().next().unwrap_or("")
                ),
            });
        }
        Err(e) => checks.push(DeterminismCheck {
            name: "determinism-report".to_string(),
            ok: false,
            detail: format!("Failed to run cargo xtask determinism-report: {}", e),
        }),
    }
}
