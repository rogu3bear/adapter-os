//! Verify and sign release artifacts.
//!
//! This is a thin Rust wrapper around the existing
//! `scripts/verify_artifacts.sh` script so that build
//! orchestration goes through `cargo xtask` instead of
//! invoking shell scripts directly.

use anyhow::{Context, Result};
use std::process::Command;

pub fn run() -> Result<()> {
    println!("🔐 Verifying artifacts and generating signatures via scripts/verify_artifacts.sh");

    let status = Command::new("bash")
        .arg("scripts/verify_artifacts.sh")
        .status()
        .context("failed to invoke scripts/verify_artifacts.sh")?;

    if !status.success() {
        anyhow::bail!("artifact verification script exited with status {}", status);
    }

    Ok(())
}

