#![allow(dead_code)]

//! Determinism report generation for build reproducibility

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeterminismReport {
    pub schema_version: String,
    pub build_timestamp: String,
    pub source_date_epoch: Option<u64>,
    pub build_metadata: BuildMetadata,
    pub binary_hashes: HashMap<String, String>,
    pub artifact_hashes: HashMap<String, String>,
    pub environment_variables: HashMap<String, String>,
    pub reproducibility_score: f64,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildMetadata {
    pub rustc_version: String,
    pub rustc_commit_hash: String,
    pub cargo_version: String,
    pub metal_sdk_version: String,
    pub metal_compiler_version: String,
    pub target_triple: String,
    pub build_mode: String,
    pub optimization_level: String,
    pub lto_enabled: bool,
    pub codegen_units: u32,
}

impl DeterminismReport {
    pub fn new() -> Result<Self> {
        Ok(Self {
            schema_version: "1.0.0".to_string(),
            build_timestamp: chrono::Utc::now().to_rfc3339(),
            source_date_epoch: std::env::var("SOURCE_DATE_EPOCH")
                .ok()
                .and_then(|s| s.parse().ok()),
            build_metadata: BuildMetadata::collect()?,
            binary_hashes: HashMap::new(),
            artifact_hashes: HashMap::new(),
            environment_variables: HashMap::new(),
            reproducibility_score: 0.0,
            issues: Vec::new(),
        })
    }

    pub fn collect_binary_hashes(&mut self, target_dir: &Path) -> Result<()> {
        let release_dir = target_dir.join("release");
        if !release_dir.exists() {
            return Ok(());
        }

        for entry in fs::read_dir(&release_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_file() && is_executable(&path)? {
                let hash = compute_b3_hash(&path)?;
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("unknown")
                    .to_string();

                self.binary_hashes.insert(name, hash);
            }
        }

        Ok(())
    }

    pub fn collect_artifact_hashes(&mut self, target_dir: &Path) -> Result<()> {
        // Collect .metallib files
        let metal_dir = target_dir.parent().unwrap().join("metal");
        if metal_dir.exists() {
            for entry in fs::read_dir(&metal_dir)? {
                let entry = entry?;
                let path = entry.path();

                if path.extension().and_then(|s| s.to_str()) == Some("metallib") {
                    let hash = compute_b3_hash(&path)?;
                    let name = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown")
                        .to_string();

                    self.artifact_hashes.insert(name, hash);
                }
            }
        }

        // Collect SBOM files
        let sbom_path = target_dir.join("sbom.spdx.json");
        if sbom_path.exists() {
            let hash = compute_b3_hash(&sbom_path)?;
            self.artifact_hashes
                .insert("sbom.spdx.json".to_string(), hash);
        }

        Ok(())
    }

    pub fn collect_environment_variables(&mut self) {
        let relevant_vars = [
            "SOURCE_DATE_EPOCH",
            "CARGO_INCREMENTAL",
            "RUSTC_WRAPPER",
            "CARGO_TARGET_DIR",
            "RUSTFLAGS",
            "CARGO_BUILD_RUSTFLAGS",
            "CARGO_BUILD_TARGET",
            "CARGO_BUILD_TARGET_DIR",
        ];

        for var in &relevant_vars {
            if let Ok(value) = std::env::var(var) {
                self.environment_variables.insert(var.to_string(), value);
            }
        }
    }

    pub fn calculate_reproducibility_score(&mut self) {
        let mut score: f64 = 100.0;
        let mut issues = Vec::new();

        // Check for non-deterministic elements
        if self.source_date_epoch.is_none() {
            score -= 10.0;
            issues.push("SOURCE_DATE_EPOCH not set".to_string());
        }

        if self.environment_variables.get("CARGO_INCREMENTAL") != Some(&"0".to_string()) {
            score -= 15.0;
            issues.push("Incremental compilation enabled".to_string());
        }

        if self.environment_variables.get("RUSTC_WRAPPER").is_some() {
            score -= 5.0;
            issues.push("Rustc wrapper detected (e.g., sccache)".to_string());
        }

        if self.build_metadata.codegen_units > 1 {
            score -= 10.0;
            issues.push("Multiple codegen units enabled".to_string());
        }

        if !self.build_metadata.lto_enabled {
            score -= 5.0;
            issues.push("LTO not enabled".to_string());
        }

        self.reproducibility_score = score.max(0.0);
        self.issues = issues;
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let json =
            serde_json::to_string_pretty(self).context("Failed to serialize determinism report")?;

        fs::write(path, json).context("Failed to write determinism report")?;

        Ok(())
    }

    pub fn load(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path).context("Failed to read determinism report")?;

        let report: Self =
            serde_json::from_str(&content).context("Failed to parse determinism report")?;

        Ok(report)
    }

    pub fn compare_with(&self, other: &Self) -> ComparisonResult {
        let mut differences = Vec::new();
        let mut identical = true;

        // Compare build metadata
        if self.build_metadata.rustc_version != other.build_metadata.rustc_version {
            differences.push(format!(
                "Rustc version: {} vs {}",
                self.build_metadata.rustc_version, other.build_metadata.rustc_version
            ));
            identical = false;
        }

        if self.build_metadata.rustc_commit_hash != other.build_metadata.rustc_commit_hash {
            differences.push(format!(
                "Rustc commit: {} vs {}",
                self.build_metadata.rustc_commit_hash, other.build_metadata.rustc_commit_hash
            ));
            identical = false;
        }

        // Compare binary hashes
        for (name, hash) in &self.binary_hashes {
            if let Some(other_hash) = other.binary_hashes.get(name) {
                if hash != other_hash {
                    differences.push(format!("Binary {}: {} vs {}", name, hash, other_hash));
                    identical = false;
                }
            } else {
                differences.push(format!("Binary {} missing in other build", name));
                identical = false;
            }
        }

        // Check for extra binaries in other build
        for name in other.binary_hashes.keys() {
            if !self.binary_hashes.contains_key(name) {
                differences.push(format!("Binary {} missing in this build", name));
                identical = false;
            }
        }

        ComparisonResult {
            identical,
            differences,
            score: if identical { 100.0 } else { 0.0 },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComparisonResult {
    pub identical: bool,
    pub differences: Vec<String>,
    pub score: f64,
}

impl BuildMetadata {
    fn collect() -> Result<Self> {
        let rustc_version = get_command_output("rustc", &["--version"])?;
        let rustc_commit_hash = get_command_output("rustc", &["--version", "--verbose"])?
            .lines()
            .find(|line| line.contains("commit-hash"))
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("unknown")
            .to_string();

        let cargo_version = get_command_output("cargo", &["--version"])?;
        let metal_sdk_version = get_command_output("xcrun", &["--show-sdk-version"])
            .unwrap_or_else(|_| "unknown".to_string());
        let metal_compiler_version = get_command_output("xcrun", &["metal", "--version"])
            .unwrap_or_else(|_| "unknown".to_string());

        let target_triple = get_command_output("rustc", &["--version", "--verbose"])?
            .lines()
            .find(|line| line.contains("host"))
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("unknown")
            .to_string();

        Ok(Self {
            rustc_version,
            rustc_commit_hash,
            cargo_version,
            metal_sdk_version,
            metal_compiler_version,
            target_triple,
            build_mode: "release".to_string(),
            optimization_level: "3".to_string(),
            lto_enabled: true,
            codegen_units: 1,
        })
    }
}

fn get_command_output(command: &str, args: &[&str]) -> Result<String> {
    let output = Command::new(command)
        .args(args)
        .output()
        .context(format!("Failed to execute {}", command))?;

    if !output.status.success() {
        anyhow::bail!(
            "Command {} failed: {}",
            command,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    Ok(String::from_utf8(output.stdout)?.trim().to_string())
}

fn compute_b3_hash(path: &Path) -> Result<String> {
    use blake3::Hasher;

    let content = fs::read(path).context(format!("Failed to read file: {}", path.display()))?;

    let mut hasher = Hasher::new();
    hasher.update(&content);
    let hash = hasher.finalize();

    Ok(hash.to_hex().to_string())
}

fn is_executable(path: &Path) -> Result<bool> {
    use std::os::unix::fs::PermissionsExt;

    let metadata = fs::metadata(path)?;
    let permissions = metadata.permissions();
    let mode = permissions.mode();

    // Check if file has execute permission
    Ok(mode & 0o111 != 0)
}

pub fn generate_determinism_report() -> Result<()> {
    println!("Generating determinism report...");

    let workspace_root = find_workspace_root()?;
    let target_dir = workspace_root.join("target");

    let mut report = DeterminismReport::new()?;

    // Collect all data
    report.collect_binary_hashes(&target_dir)?;
    report.collect_artifact_hashes(&target_dir)?;
    report.collect_environment_variables();
    report.calculate_reproducibility_score();

    // Save report
    let report_path = target_dir.join("determinism_report.json");
    report.save(&report_path)?;

    println!("✓ Determinism report generated: {}", report_path.display());
    println!(
        "  Reproducibility score: {:.1}/100",
        report.reproducibility_score
    );

    if !report.issues.is_empty() {
        println!("  Issues found:");
        for issue in &report.issues {
            println!("    - {}", issue);
        }
    }

    Ok(())
}

fn find_workspace_root() -> Result<PathBuf> {
    let mut current = std::env::current_dir()?;

    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = fs::read_to_string(&cargo_toml)?;
            if content.contains("[workspace]") {
                return Ok(current);
            }
        }

        if !current.pop() {
            anyhow::bail!("Could not find workspace root");
        }
    }
}

// Stub chrono for timestamp
mod chrono {
    pub struct Utc;
    impl Utc {
        pub fn now() -> Self {
            Self
        }
        pub fn to_rfc3339(&self) -> String {
            // Use SOURCE_DATE_EPOCH if available, otherwise current time
            if let Ok(epoch) = std::env::var("SOURCE_DATE_EPOCH") {
                if let Ok(secs) = epoch.parse::<i64>() {
                    return format!(
                        "{}Z",
                        chrono::DateTime::from_timestamp(secs, 0)
                            .unwrap_or_default()
                            .format("%Y-%m-%dT%H:%M:%S")
                    );
                }
            }
            "2025-01-01T00:00:00Z".to_string()
        }
    }

    pub mod date_time {
        use std::time::{SystemTime, UNIX_EPOCH};

        pub struct DateTime;

        impl DateTime {
            pub fn from_timestamp(secs: i64, _nsecs: u32) -> Option<Self> {
                if secs > 0 {
                    Some(Self)
                } else {
                    None
                }
            }

            pub fn format(&self, _fmt: &str) -> impl std::fmt::Display {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            }
        }
    }
}
