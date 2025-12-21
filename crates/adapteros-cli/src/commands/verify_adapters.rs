//! Adapter deliverable verification front-end for aosctl.
//!
//! Wraps `cargo xtask verify-agents` behind:
//!   aosctl verify-adapters [--json]
//!
//! This command is intended for CI and operator workflows to verify
//! that adapter deliverables A–F are complete.

use crate::output::OutputWriter;
use anyhow::{Context, Result};
use serde::Serialize;
use std::process::Command;

#[derive(Debug, Serialize)]
pub struct VerifyAdaptersResult {
    ok: bool,
    exit_code: i32,
    stdout_head: String,
    stderr_head: String,
}

/// Run adapter verification.
///
/// Returns process exit code from `cargo xtask verify-agents`.
pub async fn run(output: &OutputWriter) -> Result<i32> {
    let child = Command::new("cargo")
        .args(["xtask", "verify-agents"])
        .output()
        .context("failed to run cargo xtask verify-agents")?;

    let exit_code = child.status.code().unwrap_or(1);
    let ok = child.status.success();

    let stdout = String::from_utf8_lossy(&child.stdout);
    let stderr = String::from_utf8_lossy(&child.stderr);

    let stdout_head = stdout.lines().take(40).collect::<Vec<_>>().join("\n");
    let stderr_head = stderr.lines().take(40).collect::<Vec<_>>().join("\n");

    if output.is_json() {
        let result = VerifyAdaptersResult {
            ok,
            exit_code,
            stdout_head,
            stderr_head,
        };
        output.json(&result)?;
    } else {
        // For text mode, print the captured output verbatim.
        if !stdout.is_empty() && !output.is_quiet() {
            println!("{}", stdout);
        }
        if !stderr.is_empty() && !output.is_quiet() {
            eprintln!("{}", stderr);
        }

        if ok {
            output.success("Adapter verification passed");
        } else {
            output.error("Adapter verification failed");
        }
    }

    Ok(exit_code)
}
