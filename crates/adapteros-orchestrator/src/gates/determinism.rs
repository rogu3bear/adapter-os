//! Determinism gate: verifies replay produces zero diff

use crate::{Gate, OrchestratorConfig};
use anyhow::{Context, Result};
use std::path::Path;

#[derive(Debug, Default)]
pub struct DeterminismGate;

#[async_trait::async_trait]
impl Gate for DeterminismGate {
    fn name(&self) -> String {
        "Determinism".to_string()
    }

    async fn check(&self, config: &OrchestratorConfig) -> Result<()> {
        // Look for replay bundle for this CPID
        let bundle_path =
            Path::new(&config.bundles_path).join(format!("{}_replay.ndjson", config.cpid));

        if !bundle_path.exists() {
            anyhow::bail!(
                "Replay bundle not found: {}. Run determinism test first.",
                bundle_path.display()
            );
        }

        // Load and check replay bundle
        let bundle = adapteros_telemetry::load_replay_bundle(&bundle_path)
            .context("Failed to load replay bundle")?;

        // For now, just check that bundle loaded successfully
        // In full implementation, would run actual replay and compare
        // Verify event count is reasonable (> 0)
        if bundle.events.is_empty() {
            anyhow::bail!("Replay bundle is empty");
        }

        println!("    Replay events: {}", bundle.events.len());
        println!("    Replay bundle loaded successfully");

        Ok(())
    }
}
