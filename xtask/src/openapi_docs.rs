//! OpenAPI documentation generation entrypoint.
//!
//! This wraps the existing shell-based workflow so that
//! developers use `cargo xtask openapi-docs` instead of
//! calling scripts directly.

use anyhow::{Context, Result};
use std::process::Command;

pub fn run() -> Result<()> {
    println!("🔍 Generating OpenAPI documentation via scripts/generate_openapi_simple.sh");

    let status = Command::new("bash")
        .arg("scripts/generate_openapi_simple.sh")
        .status()
        .context("failed to invoke scripts/generate_openapi_simple.sh")?;

    if !status.success() {
        anyhow::bail!(
            "OpenAPI documentation script exited with status {}",
            status
        );
    }

    Ok(())
}

