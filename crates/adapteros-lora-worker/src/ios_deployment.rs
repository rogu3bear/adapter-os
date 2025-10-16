//! iOS/macOS deployment helpers for AdapterOS
//!
//! These utilities package converted ML models and metadata into a
//! deterministic layout that satisfies App Store and enterprise
//! distribution requirements. The focus is on deterministic manifests so
//! that deployment can be audited and reproduced across CI environments.

use adapteros_core::{AosError, B3Hash, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
use tracing::{info, warn};

/// Maximum allowed entitlements for offline distribution.
const OFFLINE_ALLOWED_ENTITLEMENTS: &[&str] = &[
    "com.apple.security.application-groups",
    "com.apple.security.network.client",
    "com.apple.developer.networking.networkextension",
    "com.apple.developer.networking.wifi-info",
    "com.apple.developer.usernotifications.communication",
];

/// Configuration describing how a model bundle should be packaged for
/// iOS/macOS deployment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IosDeploymentConfig {
    pub bundle_identifier: String,
    pub app_id: String,
    pub team_id: String,
    pub model_package_path: PathBuf,
    pub output_path: PathBuf,
    #[serde(default)]
    pub allow_offline: bool,
    #[serde(default)]
    pub entitlements: Vec<String>,
    #[serde(default = "default_version")]
    pub version: String,
}

fn default_version() -> String {
    "1.0.0".to_string()
}

impl IosDeploymentConfig {
    pub fn validate(&self) -> Result<()> {
        if self.bundle_identifier.trim().is_empty() {
            return Err(AosError::Toolchain(
                "Bundle identifier must not be empty".to_string(),
            ));
        }

        if self.app_id.trim().is_empty() {
            return Err(AosError::Toolchain("App ID must not be empty".to_string()));
        }

        if !self.model_package_path.exists() {
            return Err(AosError::Toolchain(format!(
                "Model package path does not exist: {}",
                self.model_package_path.display()
            )));
        }

        Ok(())
    }
}

/// Manifest describing the packaged deployment bundle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeploymentManifest {
    pub generated_at: DateTime<Utc>,
    pub bundle_identifier: String,
    pub app_id: String,
    pub team_id: String,
    pub version: String,
    pub offline_supported: bool,
    pub entitlements: Vec<String>,
    pub model_checksum: B3Hash,
}

/// Package a CoreML/MLX bundle for deployment.
pub fn prepare_offline_bundle(config: &IosDeploymentConfig) -> Result<DeploymentManifest> {
    config.validate()?;

    if config.output_path.exists() {
        fs::remove_dir_all(&config.output_path).map_err(|e| {
            AosError::Toolchain(format!(
                "Failed to clear output directory {}: {}",
                config.output_path.display(),
                e
            ))
        })?;
    }

    fs::create_dir_all(&config.output_path).map_err(|e| {
        AosError::Toolchain(format!(
            "Failed to create deployment directory {}: {}",
            config.output_path.display(),
            e
        ))
    })?;

    let assets_path = config.output_path.join("Assets");
    fs::create_dir_all(&assets_path).map_err(|e| {
        AosError::Toolchain(format!(
            "Failed to create assets directory {}: {}",
            assets_path.display(),
            e
        ))
    })?;

    copy_directory(&config.model_package_path, &assets_path.join("model"))?;

    let checksum = hash_directory(&assets_path.join("model"))?;

    let manifest = DeploymentManifest {
        generated_at: Utc::now(),
        bundle_identifier: config.bundle_identifier.clone(),
        app_id: config.app_id.clone(),
        team_id: config.team_id.clone(),
        version: config.version.clone(),
        offline_supported: config.allow_offline,
        entitlements: config.entitlements.clone(),
        model_checksum: checksum,
    };

    let manifest_path = config.output_path.join("deployment_manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest).map_err(|e| {
        AosError::Toolchain(format!("Failed to serialize deployment manifest: {}", e))
    })?;
    fs::write(&manifest_path, manifest_json).map_err(|e| {
        AosError::Toolchain(format!(
            "Failed to write deployment manifest {}: {}",
            manifest_path.display(),
            e
        ))
    })?;

    info!(
        "Packaged deployment bundle at {}",
        config.output_path.display()
    );

    Ok(manifest)
}

/// Validate the entitlements for App Store submission.
pub fn validate_app_store_compliance(config: &IosDeploymentConfig) -> Result<()> {
    config.validate()?;

    if config.allow_offline {
        let allowed: BTreeSet<&str> = OFFLINE_ALLOWED_ENTITLEMENTS.iter().copied().collect();
        for entitlement in &config.entitlements {
            if !allowed.contains(entitlement.as_str()) {
                return Err(AosError::Toolchain(format!(
                    "Entitlement '{}' is not permitted for offline distribution",
                    entitlement
                )));
            }
        }
    }

    Ok(())
}

/// Generate an App Store ready manifest summarising entitlements and
/// security posture.
pub fn generate_deployment_manifest(
    config: &IosDeploymentConfig,
    manifest: &DeploymentManifest,
) -> Result<String> {
    validate_app_store_compliance(config)?;

    let mut summary = format!(
        "Bundle: {}\nTeam: {}\nVersion: {}\nOffline: {}\n",
        config.bundle_identifier, config.team_id, config.version, config.allow_offline
    );

    if config.entitlements.is_empty() {
        summary.push_str("Entitlements: none\n");
    } else {
        summary.push_str("Entitlements:\n");
        for entitlement in &config.entitlements {
            summary.push_str(&format!("- {}\n", entitlement));
        }
    }

    summary.push_str(&format!(
        "Model checksum: {}\nGenerated at: {}\n",
        hex::encode(manifest.model_checksum.as_bytes()),
        manifest.generated_at
    ));

    if !config.allow_offline {
        warn!("Offline mode disabled; App Store review will expect online telemetry");
    }

    Ok(summary)
}

fn copy_directory(src: &Path, dst: &Path) -> Result<()> {
    if src.is_file() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                AosError::Toolchain(format!(
                    "Failed to create directory {}: {}",
                    parent.display(),
                    e
                ))
            })?;
        }
        fs::copy(src, dst).map_err(|e| {
            AosError::Toolchain(format!(
                "Failed to copy file {} -> {}: {}",
                src.display(),
                dst.display(),
                e
            ))
        })?;
        return Ok(());
    }

    fs::create_dir_all(dst).map_err(|e| {
        AosError::Toolchain(format!(
            "Failed to create directory {}: {}",
            dst.display(),
            e
        ))
    })?;

    for entry in fs::read_dir(src).map_err(|e| {
        AosError::Toolchain(format!("Failed to read directory {}: {}", src.display(), e))
    })? {
        let entry = entry
            .map_err(|e| AosError::Toolchain(format!("Failed to read directory entry: {}", e)))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        copy_directory(&src_path, &dst_path)?;
    }

    Ok(())
}

fn hash_directory(path: &Path) -> Result<B3Hash> {
    if path.is_file() {
        return B3Hash::hash_file(path).map_err(|e| {
            AosError::Toolchain(format!("Failed to hash file {}: {}", path.display(), e))
        });
    }

    let mut entries: Vec<_> = fs::read_dir(path)
        .map_err(|e| {
            AosError::Toolchain(format!(
                "Failed to read directory {}: {}",
                path.display(),
                e
            ))
        })?
        .map(|entry| entry.map(|e| e.path()))
        .collect();
    entries.sort_by(|a, b| match (a, b) {
        (Ok(a_path), Ok(b_path)) => a_path.cmp(b_path),
        _ => std::cmp::Ordering::Equal,
    });

    let mut hasher = blake3::Hasher::new();
    for entry in entries {
        let entry = entry
            .map_err(|e| AosError::Toolchain(format!("Failed to access directory entry: {}", e)))?;
        if entry.is_dir() {
            let nested = hash_directory(&entry)?;
            hasher.update(entry.file_name().unwrap().to_string_lossy().as_bytes());
            hasher.update(nested.as_bytes());
        } else {
            let file_hash = B3Hash::hash_file(&entry).map_err(|e| {
                AosError::Toolchain(format!("Failed to hash file {}: {}", entry.display(), e))
            })?;
            hasher.update(entry.file_name().unwrap().to_string_lossy().as_bytes());
            hasher.update(file_hash.as_bytes());
        }
    }

    Ok(B3Hash::from_bytes(*hasher.finalize().as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_prepare_offline_bundle_creates_manifest() {
        let model_dir = tempdir().expect("model");
        fs::create_dir(model_dir.path().join("model.mlpackage")).expect("package");
        fs::write(
            model_dir.path().join("model.mlpackage/metadata.json"),
            b"{}",
        )
        .expect("metadata");

        let output = tempdir().expect("output");
        let config = IosDeploymentConfig {
            bundle_identifier: "com.adapter.test".to_string(),
            app_id: "ABC12345XY".to_string(),
            team_id: "TEAM12345".to_string(),
            model_package_path: model_dir.path().to_path_buf(),
            output_path: output.path().join("bundle"),
            allow_offline: true,
            entitlements: vec!["com.apple.security.application-groups".to_string()],
            version: "1.2.3".to_string(),
        };

        let manifest = prepare_offline_bundle(&config).expect("prepare bundle");
        assert_eq!(manifest.bundle_identifier, "com.adapter.test");
        assert!(config.output_path.join("deployment_manifest.json").exists());
    }

    #[test]
    fn test_generate_deployment_manifest_summary() {
        let model_dir = tempdir().expect("model");
        fs::write(model_dir.path().join("weights.bin"), b"weights").expect("weights");

        let config = IosDeploymentConfig {
            bundle_identifier: "com.adapter.test".to_string(),
            app_id: "ABC12345XY".to_string(),
            team_id: "TEAM12345".to_string(),
            model_package_path: model_dir.path().to_path_buf(),
            output_path: model_dir.path().join("bundle"),
            allow_offline: false,
            entitlements: vec![],
            version: "1.0.0".to_string(),
        };

        let manifest = DeploymentManifest {
            generated_at: Utc::now(),
            bundle_identifier: config.bundle_identifier.clone(),
            app_id: config.app_id.clone(),
            team_id: config.team_id.clone(),
            version: config.version.clone(),
            offline_supported: false,
            entitlements: vec![],
            model_checksum: B3Hash::hash(b"demo"),
        };

        let summary = generate_deployment_manifest(&config, &manifest).expect("summary");
        assert!(summary.contains("Bundle: com.adapter.test"));
        assert!(summary.contains("Offline: false"));
    }
}
