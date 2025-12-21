//! Feature matrix testing for AdapterOS workspace
//!
//! This module implements comprehensive build checking across all supported
//! feature combinations to ensure workspace-wide compatibility.

use anyhow::{Context, Result};
use std::process::Command;

/// Supported feature combinations for the workspace
#[derive(Debug, Clone)]
pub struct FeatureSet {
    pub name: &'static str,
    pub features: Vec<&'static str>,
    pub exclude_crates: Vec<&'static str>,
    pub target_os: Option<&'static str>,
}

impl FeatureSet {
    /// Get all supported feature sets
    pub fn all() -> Vec<Self> {
        vec![
            // Default: deterministic-only, no backends (Linux-compatible)
            Self {
                name: "default",
                features: vec![],
                exclude_crates: vec!["adapteros-lora-mlx-ffi"],
                target_os: None,
            },
            // Full: all capabilities except backends
            Self {
                name: "full",
                features: vec!["full"],
                exclude_crates: vec!["adapteros-lora-mlx-ffi"],
                target_os: None,
            },
            // Metal backend: macOS only
            Self {
                name: "metal-backend",
                features: vec!["metal-backend"],
                exclude_crates: vec!["adapteros-lora-mlx-ffi"],
                target_os: Some("macos"),
            },
            // No-metal: Explicitly no Metal (for CI)
            Self {
                name: "no-metal",
                features: vec!["no-metal"],
                exclude_crates: vec!["adapteros-lora-mlx-ffi"],
                target_os: None,
            },
        ]
    }

    /// Check if this feature set is compatible with the current platform
    pub fn is_compatible(&self) -> bool {
        if let Some(required_os) = self.target_os {
            cfg!(target_os = "macos") && required_os == "macos"
        } else {
            true
        }
    }
}

/// Run cargo check for a specific feature set
fn check_feature_set(feature_set: &FeatureSet, verbose: bool) -> Result<()> {
    println!("\n=== Checking feature set: {} ===", feature_set.name);

    if !feature_set.is_compatible() {
        println!(
            "⏭️  Skipping {} (requires target_os = {:?})",
            feature_set.name, feature_set.target_os
        );
        return Ok(());
    }

    let mut cmd = Command::new("cargo");
    cmd.arg("check").arg("--workspace").arg("--all-targets");

    // Add features
    if !feature_set.features.is_empty() {
        cmd.arg("--features");
        cmd.arg(feature_set.features.join(","));
    }

    // Exclude crates
    for crate_name in &feature_set.exclude_crates {
        cmd.arg("--exclude").arg(crate_name);
    }

    if verbose {
        println!("Running: {:?}", cmd);
    }

    let output = cmd
        .output()
        .with_context(|| format!("Failed to run cargo check for {}", feature_set.name))?;

    if output.status.success() {
        println!("✅ {} passed", feature_set.name);
        Ok(())
    } else {
        eprintln!("❌ {} failed", feature_set.name);
        eprintln!("STDOUT:\n{}", String::from_utf8_lossy(&output.stdout));
        eprintln!("STDERR:\n{}", String::from_utf8_lossy(&output.stderr));
        anyhow::bail!("Feature set {} failed to compile", feature_set.name);
    }
}

/// Run all feature matrix checks
pub fn run(verbose: bool) -> Result<()> {
    println!("🔍 Running feature matrix checks...\n");

    let feature_sets = FeatureSet::all();
    let mut passed = 0;
    let mut skipped = 0;
    let mut failed = 0;

    for feature_set in &feature_sets {
        match check_feature_set(feature_set, verbose) {
            Ok(()) => {
                if feature_set.is_compatible() {
                    passed += 1;
                } else {
                    skipped += 1;
                }
            }
            Err(e) => {
                eprintln!("Error: {}", e);
                failed += 1;
            }
        }
    }

    println!("\n=== Summary ===");
    println!("✅ Passed:  {}", passed);
    println!("⏭️  Skipped: {}", skipped);
    println!("❌ Failed:  {}", failed);

    if failed > 0 {
        anyhow::bail!("{} feature set(s) failed to compile", failed);
    }

    Ok(())
}
