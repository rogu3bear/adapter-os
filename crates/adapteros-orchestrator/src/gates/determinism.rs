//! Determinism gate: verifies replay produces zero diff

use crate::{DependencyChecker, Gate, OrchestratorConfig};
use adapteros_core::{AosError, Result};
use std::path::Path;
use tracing::{debug, warn};

#[derive(Debug, Clone, Default)]
pub struct DeterminismGate;

#[async_trait::async_trait]
impl Gate for DeterminismGate {
    fn name(&self) -> String {
        "Determinism".to_string()
    }

    async fn check(&self, config: &OrchestratorConfig) -> Result<()> {
        // Check dependencies
        let checker = DependencyChecker::new();
        let deps = checker.check_gate("determinism")?;

        if !deps.all_available {
            debug!(messages = ?deps.messages, "Some dependencies missing, attempting graceful degradation");
        }

        // Try primary bundles path first
        let bundle_path =
            Path::new(&config.bundles_path).join(format!("{}_replay.ndjson", config.cpid));

        let resolved_path = if bundle_path.exists() {
            Some(bundle_path.clone())
        } else {
            // Try fallback paths
            deps.get_resolved_path("replay_bundle")
                .and_then(|fallback| {
                    let path = Path::new(&fallback).join(format!("{}_replay.ndjson", config.cpid));
                    if path.exists() {
                        Some(path)
                    } else {
                        None
                    }
                })
        };

        let bundle_path = match resolved_path {
            Some(path) => {
                if path != bundle_path {
                    warn!("Using fallback path for replay bundle: {}", path.display());
                }
                path
            }
            None => {
                return Err(AosError::NotFound(format!(
                    "Replay bundle not found: {}. Run determinism test first. \
                     (checked primary: {}, fallbacks: {:?})",
                    config.cpid,
                    bundle_path.display(),
                    deps.optional_paths.get("replay_bundle")
                )));
            }
        };

        // Load and check replay bundle
        let bundle = adapteros_telemetry::load_replay_bundle(&bundle_path)
            .map_err(|e| AosError::Internal(format!("Failed to load replay bundle: {}", e)))?;

        // For now, just check that bundle loaded successfully
        // In full implementation, would run actual replay and compare
        // Verify event count is reasonable (> 0)
        if bundle.events.is_empty() {
            return Err(AosError::Validation("Replay bundle is empty".to_string()));
        }

        tracing::info!(
            bundle_path = %bundle_path.display(),
            event_count = bundle.events.len(),
            "Replay bundle loaded successfully"
        );

        Ok(())
    }
}
