//! Golden-run archive system for audit reproducibility
//!
//! This module provides the infrastructure to:
//! - Store deterministic run hashes + ε statistics in signed archives
//! - Compare new runs against golden baselines
//! - Track metadata including toolchain, adapter set, and device fingerprint
//! - Enable compliance auditing with cryptographic verification
//!
//! # Golden Run Archive Format
//!
//! A golden run archive contains:
//! - `manifest.json` - Metadata about the run (toolchain, adapters, device)
//! - `event_bundle.ndjson` - Complete event trace from the run
//! - `epsilon_stats.json` - Floating-point error statistics per layer
//! - `signature.sig` - Ed25519 signature over the archive
//!
//! # Usage
//!
//! ```no_run
//! use adapteros_verify::{GoldenRunArchive, create_golden_run};
//! use std::path::Path;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Create a golden run from a replay bundle
//! let archive = create_golden_run(
//!     Path::new("var/bundles/replay_12345.ndjson"),
//!     "v1.0.0",
//!     &["adapter-001", "adapter-002"],
//! ).await?;
//!
//! // Save to golden_runs/ directory
//! archive.save(Path::new("golden_runs/baseline-001"))?;
//! # Ok(())
//! # }
//! ```

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::info;

pub mod archive;
pub mod drift;
pub mod epsilon;
pub mod federation;
pub mod keys;
pub mod metadata;
pub mod rng_diff_viewer;
pub mod sysinfo;
pub mod verification;

pub use archive::{create_golden_run, GoldenRunArchive};
pub use drift::{DriftEvaluator, DriftReport, DriftSeverity, FieldDrift};
pub use epsilon::{EpsilonStatistics, EpsilonStats};
pub use federation::{verify_cross_host, FederationVerificationReport};
pub use keys::{get_fingerprint_public_key, get_or_create_fingerprint_keypair};
pub use metadata::{DeviceFingerprint, GoldenRunMetadata, ToolchainMetadata};
pub use rng_diff_viewer::{compare_rng_states, format_diff, RngState, RngStateDiff};
pub use verification::{verify_against_golden, VerificationReport};

/// Error types for golden-run verification
#[derive(Error, Debug)]
pub enum VerifyError {
    #[error("Golden run not found: {path}")]
    GoldenRunNotFound { path: String },

    #[error("Signature verification failed: {reason}")]
    SignatureVerificationFailed { reason: String },

    #[error(
        "Epsilon divergence detected: layer {layer}, ε={epsilon:.6e} (threshold: {threshold:.6e})"
    )]
    EpsilonDivergence {
        layer: String,
        epsilon: f64,
        threshold: f64,
    },

    #[error("Toolchain mismatch: golden={golden}, current={current}")]
    ToolchainMismatch { golden: String, current: String },

    #[error("Adapter set mismatch: {reason}")]
    AdapterSetMismatch { reason: String },

    #[error("Device fingerprint mismatch: {reason}")]
    DeviceMismatch { reason: String },

    #[error("Hash mismatch: expected {expected}, got {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("Archive corrupted: {reason}")]
    ArchiveCorrupted { reason: String },

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Anyhow error: {0}")]
    Anyhow(#[from] anyhow::Error),
}

/// Result type for verification operations
pub type VerifyResult<T> = std::result::Result<T, VerifyError>;

/// Verification strictness levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
#[serde(rename_all = "kebab-case")]
pub enum StrictnessLevel {
    /// Bit-for-bit identical (no floating-point tolerance)
    Bitwise,
    /// Epsilon tolerance for floating-point (default: 1e-6)
    #[default]
    EpsilonTolerant,
    /// Relaxed for statistical sampling (default: 1e-4)
    Statistical,
}

impl StrictnessLevel {
    /// Get the epsilon threshold for this strictness level
    pub fn epsilon_threshold(&self) -> f64 {
        match self {
            Self::Bitwise => 0.0,
            Self::EpsilonTolerant => 1e-6,
            Self::Statistical => 1e-4,
        }
    }
}

/// Golden run comparison configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonConfig {
    /// Strictness level for verification
    pub strictness: StrictnessLevel,
    /// Whether to verify toolchain match
    pub verify_toolchain: bool,
    /// Whether to verify adapter set match
    pub verify_adapters: bool,
    /// Whether to verify device fingerprint
    pub verify_device: bool,
    /// Whether to verify signature
    pub verify_signature: bool,
}

impl Default for ComparisonConfig {
    fn default() -> Self {
        Self {
            strictness: StrictnessLevel::default(),
            verify_toolchain: true,
            verify_adapters: true,
            verify_device: false, // Device can vary across runs
            verify_signature: true,
        }
    }
}

/// Initialize golden_runs/ directory structure
pub fn init_golden_runs_dir<P: AsRef<Path>>(base_path: P) -> VerifyResult<PathBuf> {
    let golden_runs_dir = base_path.as_ref().join("golden_runs");

    // Create directory structure
    fs::create_dir_all(&golden_runs_dir)?;
    fs::create_dir_all(golden_runs_dir.join("baselines"))?;
    fs::create_dir_all(golden_runs_dir.join("archive"))?;

    // Create README
    let readme_path = golden_runs_dir.join("README.md");
    if !readme_path.exists() {
        fs::write(&readme_path, include_str!("../templates/README.md"))?;
    }

    // Create .gitignore
    let gitignore_path = golden_runs_dir.join(".gitignore");
    if !gitignore_path.exists() {
        fs::write(
            &gitignore_path,
            "# Ignore large event bundles\n*.ndjson\n*.bin\n",
        )?;
    }

    info!(
        "Initialized golden_runs directory at: {}",
        golden_runs_dir.display()
    );
    Ok(golden_runs_dir)
}

/// List all golden runs in a directory
pub fn list_golden_runs<P: AsRef<Path>>(base_path: P) -> VerifyResult<Vec<String>> {
    let baselines_dir = base_path.as_ref().join("baselines");

    if !baselines_dir.exists() {
        return Ok(Vec::new());
    }

    let mut runs = Vec::new();

    for entry in fs::read_dir(baselines_dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                // Check if manifest exists
                let manifest_path = path.join("manifest.json");
                if manifest_path.exists() {
                    runs.push(name.to_string());
                }
            }
        }
    }

    runs.sort();
    Ok(runs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_init_golden_runs_dir() {
        let temp_dir = TempDir::new().unwrap();
        let golden_runs_dir = init_golden_runs_dir(temp_dir.path()).unwrap();

        assert!(golden_runs_dir.exists());
        assert!(golden_runs_dir.join("baselines").exists());
        assert!(golden_runs_dir.join("archive").exists());
        assert!(golden_runs_dir.join("README.md").exists());
        assert!(golden_runs_dir.join(".gitignore").exists());
    }

    #[test]
    fn test_list_golden_runs_empty() {
        let temp_dir = TempDir::new().unwrap();
        init_golden_runs_dir(temp_dir.path()).unwrap();

        let golden_runs_dir = temp_dir.path().join("golden_runs");
        let runs = list_golden_runs(&golden_runs_dir).unwrap();

        assert_eq!(runs.len(), 0);
    }

    #[test]
    fn test_strictness_level_thresholds() {
        assert_eq!(StrictnessLevel::Bitwise.epsilon_threshold(), 0.0);
        assert_eq!(StrictnessLevel::EpsilonTolerant.epsilon_threshold(), 1e-6);
        assert_eq!(StrictnessLevel::Statistical.epsilon_threshold(), 1e-4);
    }
}
