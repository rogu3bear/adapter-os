//! Security gate: runs cargo-audit and cargo-deny

use crate::{Gate, OrchestratorConfig};
use anyhow::{Context, Result};
use std::process::Command;

#[derive(Debug, Default)]
pub struct SecurityGate;

#[async_trait::async_trait]
impl Gate for SecurityGate {
    fn name(&self) -> String {
        "Security".to_string()
    }

    async fn check(&self, _config: &OrchestratorConfig) -> Result<()> {
        // Run cargo-audit
        println!("    Running cargo-audit...");
        let audit_output = Command::new("cargo")
            .args(&["audit", "--json"])
            .output()
            .context("Failed to run cargo-audit")?;

        if !audit_output.status.success() {
            let stderr = String::from_utf8_lossy(&audit_output.stderr);
            anyhow::bail!("cargo-audit failed: {}", stderr);
        }

        // Parse audit output
        let audit_json: serde_json::Value = serde_json::from_slice(&audit_output.stdout)
            .context("Failed to parse cargo-audit output")?;

        let vulnerabilities = audit_json["vulnerabilities"]["count"].as_u64().unwrap_or(0);

        if vulnerabilities > 0 {
            anyhow::bail!("Found {} vulnerabilities", vulnerabilities);
        }

        println!("      ✓ No vulnerabilities found");

        // Run cargo-deny
        println!("    Running cargo-deny...");
        let deny_output = Command::new("cargo")
            .args(&["deny", "check", "--config", "deny.toml"])
            .output()
            .context("Failed to run cargo-deny")?;

        if !deny_output.status.success() {
            let stderr = String::from_utf8_lossy(&deny_output.stderr);
            anyhow::bail!("cargo-deny failed: {}", stderr);
        }

        println!("      ✓ Dependency policy checks passed");

        Ok(())
    }
}
