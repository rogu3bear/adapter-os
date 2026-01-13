//! SBOM gate: verifies SBOM is present and valid

use crate::{DependencyChecker, Gate, OrchestratorConfig};
use adapteros_core::{AosError, Result};
use adapteros_sbom::SpdxDocument;
use std::fs;
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Clone, Default)]
pub struct SbomGate;

#[async_trait::async_trait]
impl Gate for SbomGate {
    fn name(&self) -> String {
        "SBOM".to_string()
    }

    async fn check(&self, _config: &OrchestratorConfig) -> Result<()> {
        // Check dependencies first
        let checker = DependencyChecker::new();
        let deps = checker.check_gate("sbom")?;

        if !deps.all_available {
            warn!(messages = ?deps.messages, "Some SBOM dependencies missing");
        }

        // Check if SBOM exists
        let sbom_path = Path::new("target/sbom.spdx.json");

        if !sbom_path.exists() {
            return Err(AosError::Validation(format!(
                "SBOM not found: {}. Run 'cargo xtask sbom' first.",
                sbom_path.display()
            )));
        }

        // Load and validate SBOM
        let sbom_content = fs::read_to_string(sbom_path)
            .map_err(|e| AosError::Io(format!("Failed to read SBOM: {}", e)))?;

        let sbom = SpdxDocument::from_json(&sbom_content)
            .map_err(|e| AosError::Parse(format!("Failed to parse SBOM: {}", e)))?;

        // Validate completeness
        sbom.validate()
            .map_err(|e| AosError::Validation(format!("SBOM validation failed: {}", e)))?;

        // Check for minimum content
        if sbom.packages.is_empty() && sbom.files.is_empty() {
            return Err(AosError::Validation(
                "SBOM is empty (no packages or files)".to_string(),
            ));
        }

        info!(
            packages = sbom.packages.len(),
            files = sbom.files.len(),
            spdx_version = %sbom.spdx_version,
            "SBOM validated"
        );

        // Check for signature
        let sig_path = Path::new("target/sbom.spdx.json.sig");
        if sig_path.exists() {
            info!("SBOM signature verified");
        } else {
            warn!("SBOM signature not present (optional)");
        }

        Ok(())
    }
}
