//! MLX model conversion pipeline
//!
//! This module provides a deterministic conversion pipeline that
//! prepares PyTorch or ONNX checkpoints for MLX inference. The
//! implementation focuses on reproducibility by hashing every input
//! artifact and emitting a manifest that records the exact steps that
//! were executed during conversion.

use adapteros_core::{AosError, B3Hash, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use tracing::info;

/// Supported input model formats.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SourceFormat {
    Pytorch,
    Onnx,
    Unknown,
}

/// Quantization options for MLX export.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QuantizationConfig {
    pub num_bits: u8,
    pub group_size: usize,
    #[serde(default)]
    pub scheme: QuantizationScheme,
}

impl QuantizationConfig {
    fn validate(&self) -> Result<()> {
        match self.num_bits {
            4 | 8 | 16 => {}
            other => {
                return Err(AosError::Mlx(format!(
                    "Unsupported quantization bit width: {}",
                    other
                )))
            }
        }

        if self.group_size == 0 {
            return Err(AosError::Mlx(
                "Quantization group size must be greater than zero".to_string(),
            ));
        }

        Ok(())
    }
}

/// Quantization scheme controls rounding behaviour.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum QuantizationScheme {
    #[default]
    Symmetric,
    Asymmetric,
}

/// Conversion configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlxConversionConfig {
    pub input_model: PathBuf,
    pub output_dir: PathBuf,
    #[serde(default)]
    pub quantization: Option<QuantizationConfig>,
    #[serde(default)]
    pub optimize_for_apple_silicon: bool,
    #[serde(default)]
    pub allow_overwrite: bool,
}

impl MlxConversionConfig {
    /// Validate the configuration before running conversion.
    pub fn validate(&self) -> Result<()> {
        if !self.input_model.exists() {
            return Err(AosError::Mlx(format!(
                "Input model path does not exist: {}",
                self.input_model.display()
            )));
        }

        if let Some(quant) = &self.quantization {
            quant.validate()?;
        }

        Ok(())
    }
}

/// Metadata describing generated MLX artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MlxConversionManifest {
    pub source_format: SourceFormat,
    pub created_at: DateTime<Utc>,
    pub input_checksums: BTreeMap<PathBuf, B3Hash>,
    pub output_artifacts: Vec<ConvertedArtifact>,
    pub quantization: Option<QuantizationConfig>,
    pub optimize_for_apple_silicon: bool,
}

/// Representation of a produced artifact.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvertedArtifact {
    pub relative_path: PathBuf,
    pub checksum: B3Hash,
    pub size_bytes: u64,
}

/// Report returned after successful conversion.
#[derive(Debug, Clone)]
pub struct MlxConversionReport {
    pub manifest_path: PathBuf,
    pub model_path: PathBuf,
    pub manifest: MlxConversionManifest,
}

/// Convert a PyTorch or ONNX model into the deterministic MLX layout.
///
/// The conversion currently performs the following operations:
/// - Validates the configuration and creates the output directory
/// - Hashes every source artifact for provenance tracking
/// - Emits a placeholder MLX model file that encodes the metadata hash
/// - Writes a manifest describing the conversion
pub fn convert_to_mlx(config: &MlxConversionConfig) -> Result<MlxConversionReport> {
    config.validate()?;

    if config.output_dir.exists() {
        if !config.allow_overwrite {
            return Err(AosError::Mlx(format!(
                "Output directory already exists: {}",
                config.output_dir.display()
            )));
        }
    } else {
        fs::create_dir_all(&config.output_dir).map_err(|e| {
            AosError::Mlx(format!(
                "Failed to create MLX output directory {}: {}",
                config.output_dir.display(),
                e
            ))
        })?;
    }

    let source_format = detect_source_format(&config.input_model);
    let inputs = collect_source_artifacts(&config.input_model)?;

    let mut checksums = BTreeMap::new();
    for artifact in &inputs {
        let checksum = B3Hash::hash(&fs::read(&artifact.source_path).map_err(|e| {
            AosError::Mlx(format!(
                "Failed to read source artifact {}: {}",
                artifact.source_path.display(),
                e
            ))
        })?);
        checksums.insert(artifact.relative_path.clone(), checksum);
    }

    let placeholder_contents = build_placeholder_model(&checksums, config);
    let model_path = config.output_dir.join("model.mlx");
    fs::write(&model_path, placeholder_contents).map_err(|e| {
        AosError::Mlx(format!(
            "Failed to write MLX placeholder model {}: {}",
            model_path.display(),
            e
        ))
    })?;

    let placeholder_hash = B3Hash::hash(&fs::read(&model_path).map_err(|e| {
        AosError::Mlx(format!(
            "Failed to re-open MLX model {}: {}",
            model_path.display(),
            e
        ))
    })?);

    let manifest = MlxConversionManifest {
        source_format,
        created_at: Utc::now(),
        input_checksums: checksums,
        output_artifacts: vec![ConvertedArtifact {
            relative_path: PathBuf::from("model.mlx"),
            checksum: placeholder_hash,
            size_bytes: fs::metadata(&model_path)
                .map_err(|e| {
                    AosError::Mlx(format!(
                        "Failed to stat MLX model {}: {}",
                        model_path.display(),
                        e
                    ))
                })?
                .len(),
        }],
        quantization: config.quantization.clone(),
        optimize_for_apple_silicon: config.optimize_for_apple_silicon,
    };

    let manifest_path = config.output_dir.join("mlx_manifest.json");
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| AosError::Mlx(format!("Failed to serialize MLX manifest: {}", e)))?;
    fs::write(&manifest_path, manifest_json).map_err(|e| {
        AosError::Mlx(format!(
            "Failed to write MLX manifest {}: {}",
            manifest_path.display(),
            e
        ))
    })?;

    info!(
        "Converted model to MLX format at {}",
        config.output_dir.display()
    );

    Ok(MlxConversionReport {
        manifest_path,
        model_path,
        manifest,
    })
}

#[derive(Debug)]
struct SourceArtifact {
    source_path: PathBuf,
    relative_path: PathBuf,
}

fn collect_source_artifacts(input_model: &Path) -> Result<Vec<SourceArtifact>> {
    if input_model.is_file() {
        let relative_path = input_model.file_name().map(PathBuf::from).ok_or_else(|| {
            AosError::Mlx(format!(
                "Failed to determine file name for {}",
                input_model.display()
            ))
        })?;
        return Ok(vec![SourceArtifact {
            source_path: input_model.to_path_buf(),
            relative_path,
        }]);
    }

    let mut artifacts = Vec::new();
    for entry in walk_directory_sorted(input_model)? {
        let path = entry?;
        if path.metadata().map(|m| m.is_file()).unwrap_or(false) {
            let rel = path
                .strip_prefix(input_model)
                .map(PathBuf::from)
                .map_err(|e| {
                    AosError::Mlx(format!(
                        "Failed to compute relative path for {}: {}",
                        path.display(),
                        e
                    ))
                })?;
            artifacts.push(SourceArtifact {
                source_path: path,
                relative_path: rel,
            });
        }
    }

    Ok(artifacts)
}

fn walk_directory_sorted(dir: &Path) -> Result<Vec<std::io::Result<PathBuf>>> {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .map_err(|e| AosError::Mlx(format!("Failed to read directory {}: {}", dir.display(), e)))?
        .map(|entry| entry.map(|e| e.path()))
        .collect();

    entries.sort_by(|a, b| match (a, b) {
        (Ok(a_path), Ok(b_path)) => a_path.cmp(b_path),
        _ => std::cmp::Ordering::Equal,
    });

    Ok(entries)
}

fn detect_source_format(input: &Path) -> SourceFormat {
    if input.is_dir() {
        for candidate in ["model.onnx", "model.safetensors", "pytorch_model.bin"] {
            if input.join(candidate).exists() {
                return match candidate {
                    "model.onnx" => SourceFormat::Onnx,
                    _ => SourceFormat::Pytorch,
                };
            }
        }
    } else if let Some(ext) = input.extension().and_then(|e| e.to_str()) {
        return match ext {
            "onnx" => SourceFormat::Onnx,
            "pt" | "bin" | "safetensors" => SourceFormat::Pytorch,
            _ => SourceFormat::Unknown,
        };
    }

    SourceFormat::Unknown
}

fn build_placeholder_model(
    checksums: &BTreeMap<PathBuf, B3Hash>,
    config: &MlxConversionConfig,
) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    for (path, hash) in checksums {
        hasher.update(path.to_string_lossy().as_bytes());
        hasher.update(hash.as_bytes());
    }

    if let Some(quant) = &config.quantization {
        hasher.update(&[quant.num_bits]);
        hasher.update(&quant.group_size.to_le_bytes());
    }

    hasher.update(&[config.optimize_for_apple_silicon as u8]);

    let digest = hasher.finalize();
    format!(
        "AdapterOS MLX Placeholder\ninputs={}\nquantization={:?}\noptimize={}\ndigest={}\n",
        checksums.len(),
        config.quantization,
        config.optimize_for_apple_silicon,
        hex::encode(digest.as_bytes())
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_collect_source_artifacts_file() {
        let dir = tempdir().expect("tempdir");
        let file = dir.path().join("model.onnx");
        fs::write(&file, b"onnx").expect("write");

        let artifacts = collect_source_artifacts(&file).expect("collect");
        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].relative_path, PathBuf::from("model.onnx"));
    }

    #[test]
    fn test_convert_to_mlx_creates_manifest() {
        let source_dir = tempdir().expect("source");
        fs::write(source_dir.path().join("model.onnx"), b"onnx").expect("write model");
        fs::write(source_dir.path().join("config.json"), b"{}").expect("write config");

        let output_dir = tempdir().expect("output");
        let config = MlxConversionConfig {
            input_model: source_dir.path().to_path_buf(),
            output_dir: output_dir.path().join("mlx"),
            quantization: Some(QuantizationConfig {
                num_bits: 8,
                group_size: 32,
                scheme: QuantizationScheme::Symmetric,
            }),
            optimize_for_apple_silicon: true,
            allow_overwrite: false,
        };

        let report = convert_to_mlx(&config).expect("conversion should succeed");
        assert!(report.model_path.exists());
        assert!(report.manifest_path.exists());
        assert_eq!(report.manifest.output_artifacts.len(), 1);
        assert_eq!(report.manifest.source_format, SourceFormat::Onnx);
        assert_eq!(report.manifest.input_checksums.len(), 2);
    }
}
