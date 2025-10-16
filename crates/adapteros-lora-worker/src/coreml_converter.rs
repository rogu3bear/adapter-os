//! CoreML conversion pipeline
//!
//! The implementation focuses on deterministic packaging and metadata
//! recording rather than performing the heavy model translation. It
//! prepares an `.mlpackage` bundle that records all source artifacts and
//! enables downstream tooling to substitute real converted weights.

use adapteros_core::{AosError, B3Hash, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tracing::info;

/// Compute unit preference used when compiling CoreML models.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ComputeUnits {
    All,
    CpuAndGpu,
    CpuOnly,
}

impl Default for ComputeUnits {
    fn default() -> Self {
        Self::All
    }
}

/// Precision to use for CoreML weights.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuantizationPrecision {
    Float32,
    Float16,
    Int8,
}

/// Configuration describing CoreML conversion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoremlConversionConfig {
    pub input_model: PathBuf,
    pub output_dir: PathBuf,
    pub model_name: String,
    #[serde(default = "default_min_ios_version")]
    pub minimum_ios_version: String,
    #[serde(default)]
    pub compute_units: ComputeUnits,
    #[serde(default)]
    pub quantization: Option<QuantizationPrecision>,
    #[serde(default)]
    pub allow_overwrite: bool,
}

fn default_min_ios_version() -> String {
    "15.0".to_string()
}

impl CoremlConversionConfig {
    pub fn validate(&self) -> Result<()> {
        if self.model_name.trim().is_empty() {
            return Err(AosError::Toolchain(
                "CoreML model name must not be empty".to_string(),
            ));
        }

        if !self.input_model.exists() {
            return Err(AosError::Toolchain(format!(
                "Input model path does not exist: {}",
                self.input_model.display()
            )));
        }

        if let Some(quant) = &self.quantization {
            match quant {
                QuantizationPrecision::Float32
                | QuantizationPrecision::Float16
                | QuantizationPrecision::Int8 => {}
            }
        }

        Ok(())
    }
}

/// Manifest describing the generated CoreML package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoremlConversionManifest {
    pub created_at: DateTime<Utc>,
    pub model_name: String,
    pub minimum_ios_version: String,
    pub compute_units: ComputeUnits,
    pub quantization: Option<QuantizationPrecision>,
    pub source_artifacts: BTreeMap<PathBuf, B3Hash>,
    pub package_artifacts: Vec<CoremlArtifact>,
}

/// Individual artifact in the CoreML package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoremlArtifact {
    pub relative_path: PathBuf,
    pub checksum: B3Hash,
    pub size_bytes: u64,
}

/// Report returned to callers once conversion completes.
#[derive(Debug, Clone)]
pub struct CoremlConversionReport {
    pub manifest_path: PathBuf,
    pub package_root: PathBuf,
    pub manifest: CoremlConversionManifest,
}

/// Convert a model directory to a deterministic CoreML package layout.
pub fn convert_to_coreml(config: &CoremlConversionConfig) -> Result<CoremlConversionReport> {
    config.validate()?;

    if config.output_dir.exists() {
        if !config.allow_overwrite {
            return Err(AosError::Toolchain(format!(
                "Output directory already exists: {}",
                config.output_dir.display()
            )));
        }
    } else {
        fs::create_dir_all(&config.output_dir).map_err(|e| {
            AosError::Toolchain(format!(
                "Failed to create CoreML output directory {}: {}",
                config.output_dir.display(),
                e
            ))
        })?;
    }

    let package_root = config
        .output_dir
        .join(format!("{}.mlpackage", config.model_name));
    if package_root.exists() && !config.allow_overwrite {
        return Err(AosError::Toolchain(format!(
            "CoreML package already exists: {}",
            package_root.display()
        )));
    }
    fs::create_dir_all(&package_root).map_err(|e| {
        AosError::Toolchain(format!(
            "Failed to create package root {}: {}",
            package_root.display(),
            e
        ))
    })?;

    let source_artifacts = collect_artifacts(&config.input_model)?;
    let mut checksums = BTreeMap::new();
    for artifact in &source_artifacts {
        let checksum = B3Hash::hash(&fs::read(&artifact.source_path).map_err(|e| {
            AosError::Toolchain(format!(
                "Failed to read source artifact {}: {}",
                artifact.source_path.display(),
                e
            ))
        })?);
        checksums.insert(artifact.relative_path.clone(), checksum);
    }

    // Create the deterministic bundle structure
    let metadata_dir = package_root.join("Metadata");
    fs::create_dir_all(&metadata_dir).map_err(|e| {
        AosError::Toolchain(format!(
            "Failed to create metadata directory {}: {}",
            metadata_dir.display(),
            e
        ))
    })?;

    let model_data_dir = package_root.join("Data");
    fs::create_dir_all(&model_data_dir).map_err(|e| {
        AosError::Toolchain(format!(
            "Failed to create data directory {}: {}",
            model_data_dir.display(),
            e
        ))
    })?;

    let placeholder = build_placeholder_coreml(&checksums, config);
    let weights_path = model_data_dir.join("weights.bin");
    fs::write(&weights_path, placeholder).map_err(|e| {
        AosError::Toolchain(format!(
            "Failed to write placeholder weights {}: {}",
            weights_path.display(),
            e
        ))
    })?;

    let weights_hash = B3Hash::hash(&fs::read(&weights_path).map_err(|e| {
        AosError::Toolchain(format!(
            "Failed to read placeholder weights {}: {}",
            weights_path.display(),
            e
        ))
    })?);

    let manifest = CoremlConversionManifest {
        created_at: Utc::now(),
        model_name: config.model_name.clone(),
        minimum_ios_version: config.minimum_ios_version.clone(),
        compute_units: config.compute_units.clone(),
        quantization: config.quantization.clone(),
        source_artifacts: checksums,
        package_artifacts: vec![CoremlArtifact {
            relative_path: PathBuf::from("Data/weights.bin"),
            checksum: weights_hash,
            size_bytes: fs::metadata(&weights_path)
                .map_err(|e| {
                    AosError::Toolchain(format!(
                        "Failed to stat placeholder weights {}: {}",
                        weights_path.display(),
                        e
                    ))
                })?
                .len(),
        }],
    };

    let manifest_path = package_root.join("coreml_manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| AosError::Toolchain(format!("Failed to serialize CoreML manifest: {}", e)))?;
    fs::write(&manifest_path, manifest_json).map_err(|e| {
        AosError::Toolchain(format!(
            "Failed to write CoreML manifest {}: {}",
            manifest_path.display(),
            e
        ))
    })?;

    info!("Prepared CoreML package {}", package_root.display());

    Ok(CoremlConversionReport {
        manifest_path,
        package_root,
        manifest,
    })
}

#[derive(Debug)]
struct ArtifactRecord {
    source_path: PathBuf,
    relative_path: PathBuf,
}

fn collect_artifacts(path: &Path) -> Result<Vec<ArtifactRecord>> {
    if path.is_file() {
        let relative = path.file_name().map(PathBuf::from).ok_or_else(|| {
            AosError::Toolchain(format!(
                "Failed to determine file name for {}",
                path.display()
            ))
        })?;
        return Ok(vec![ArtifactRecord {
            source_path: path.to_path_buf(),
            relative_path: relative,
        }]);
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

    let mut artifacts = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.metadata().map(|m| m.is_dir()).unwrap_or(false) {
            for nested in collect_artifacts(&entry)? {
                artifacts.push(ArtifactRecord {
                    source_path: nested.source_path,
                    relative_path: entry
                        .strip_prefix(path)
                        .map(PathBuf::from)
                        .map_err(|e| {
                            AosError::Toolchain(format!(
                                "Failed to compute relative path for {}: {}",
                                entry.display(),
                                e
                            ))
                        })?
                        .join(nested.relative_path),
                });
            }
        } else if entry.metadata().map(|m| m.is_file()).unwrap_or(false) {
            let relative = entry.strip_prefix(path).map(PathBuf::from).map_err(|e| {
                AosError::Toolchain(format!(
                    "Failed to compute relative path for {}: {}",
                    entry.display(),
                    e
                ))
            })?;
            artifacts.push(ArtifactRecord {
                source_path: entry,
                relative_path: relative,
            });
        }
    }

    Ok(artifacts)
}

fn build_placeholder_coreml(
    checksums: &BTreeMap<PathBuf, B3Hash>,
    config: &CoremlConversionConfig,
) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    for (path, checksum) in checksums {
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(checksum.as_bytes());
    }
    hasher.update(config.model_name.as_bytes());
    hasher.update(config.minimum_ios_version.as_bytes());

    format!(
        "AdapterOS CoreML Placeholder\nmodel={}\nios={}\nartifacts={}\n",
        config.model_name,
        config.minimum_ios_version,
        checksums.len()
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_collect_artifacts_directory() {
        let dir = tempdir().expect("dir");
        fs::write(dir.path().join("model.onnx"), b"onnx").expect("write");
        fs::create_dir(dir.path().join("nested")).expect("nested");
        fs::write(dir.path().join("nested/weights.bin"), b"weights").expect("weights");

        let artifacts = collect_artifacts(dir.path()).expect("collect");
        assert_eq!(artifacts.len(), 2);
    }

    #[test]
    fn test_convert_to_coreml_creates_package() {
        let source = tempdir().expect("source");
        fs::write(source.path().join("model.onnx"), b"onnx").expect("write");
        fs::write(source.path().join("config.json"), b"{}").expect("write config");

        let output = tempdir().expect("output");
        let config = CoremlConversionConfig {
            input_model: source.path().to_path_buf(),
            output_dir: output.path().join("coreml"),
            model_name: "adapteros-test".to_string(),
            minimum_ios_version: "16.0".to_string(),
            compute_units: ComputeUnits::CpuAndGpu,
            quantization: Some(QuantizationPrecision::Float16),
            allow_overwrite: false,
        };

        let report = convert_to_coreml(&config).expect("conversion succeeds");
        assert!(report.package_root.exists());
        assert!(report.manifest_path.exists());
        assert_eq!(report.manifest.package_artifacts.len(), 1);
        assert_eq!(report.manifest.source_artifacts.len(), 2);
    }
}
