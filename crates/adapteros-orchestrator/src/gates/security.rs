//! Security gate: runs cargo-audit and cargo-deny

use crate::{DependencyChecker, Gate, OrchestratorConfig};
use anyhow::{Context, Result};
use std::process::Command;
use tracing::{info, warn};

#[derive(Debug, Clone, Default)]
pub struct SecurityGate;

#[async_trait::async_trait]
impl Gate for SecurityGate {
    fn name(&self) -> String {
        "Security".to_string()
    }

    async fn check(&self, _config: &OrchestratorConfig) -> Result<()> {
        // Check dependencies first
        let checker = DependencyChecker::new();
        let deps = checker.check_gate("security")?;

        // Check for deny.toml
        if let Some(status) = deps.required_paths.get("deny.toml") {
            if !status.exists {
                warn!("deny.toml not found - skipping cargo-deny check");
            }
        }

        // Run cargo-audit
        info!("Running cargo-audit...");
        let audit_status = Command::new("cargo").args(["audit", "--json"]).output();

        match audit_status {
            Ok(audit_output) => {
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

                info!("No vulnerabilities found");
            }
            Err(e) => {
                warn!(
                    "cargo-audit not available ({}), skipping vulnerability check",
                    e
                );
            }
        }

        // Run cargo-deny if deny.toml exists
        if let Some(status) = deps.required_paths.get("deny.toml") {
            if status.exists {
                info!("Running cargo-deny...");
                match Command::new("cargo")
                    .args(["deny", "check", "--config", "deny.toml"])
                    .output()
                {
                    Ok(deny_output) => {
                        if !deny_output.status.success() {
                            let stderr = String::from_utf8_lossy(&deny_output.stderr);
                            anyhow::bail!("cargo-deny failed: {}", stderr);
                        }
                        info!("Dependency policy checks passed");
                    }
                    Err(e) => {
                        warn!(
                            "cargo-deny not available ({}), skipping dependency policy check",
                            e
                        );
                    }
                }
            } else {
                info!("deny.toml not found - skipping cargo-deny");
            }
        }

        Ok(())
    }
}
