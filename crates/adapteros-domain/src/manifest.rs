//! Manifest specification for domain adapters

use adapteros_core::B3Hash;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

use crate::error::{DomainAdapterError, Result};

/// Adapter manifest structure
///
/// This defines the configuration for a domain adapter, including
/// model paths, input/output formats, and determinism parameters.
///
/// # Example manifest (TOML)
///
/// ```toml
/// [adapter]
/// name = "text_adapter_v1"
/// version = "1.0.0"
/// model = "mlx_lora_base_v1"
/// hash = "b3d9c2a1e8f7..."
/// input_format = "UTF8 canonical"
/// output_format = "BPE deterministic"
/// epsilon_threshold = 1e-6
/// deterministic = true
///
/// [adapter.model_files]
/// weights = "path/to/weights.safetensors"
/// config = "path/to/config.json"
/// tokenizer = "path/to/tokenizer.json"
///
/// [adapter.parameters]
/// max_sequence_length = 2048
/// vocab_size = 32000
/// hidden_size = 4096
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterManifest {
    /// Adapter configuration
    pub adapter: AdapterConfig,
}

/// Core adapter configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterConfig {
    /// Unique name of the adapter
    pub name: String,

    /// Version string
    pub version: String,

    /// Model identifier
    pub model: String,

    /// BLAKE3 hash of the model (hex string)
    pub hash: String,

    /// Input format specification
    pub input_format: String,

    /// Output format specification
    pub output_format: String,

    /// Epsilon threshold for numerical drift
    #[serde(default = "default_epsilon")]
    pub epsilon_threshold: f64,

    /// Whether this adapter is deterministic
    #[serde(default = "default_deterministic")]
    pub deterministic: bool,

    /// Model file paths
    #[serde(default)]
    pub model_files: HashMap<String, String>,

    /// Adapter parameters
    #[serde(default)]
    pub parameters: HashMap<String, serde_json::Value>,

    /// Additional custom fields
    #[serde(default)]
    pub custom: HashMap<String, serde_json::Value>,
}

fn default_epsilon() -> f64 {
    1e-6
}

fn default_deterministic() -> bool {
    true
}

impl AdapterManifest {
    /// Create a new manifest
    pub fn new(name: String, version: String, model: String, hash: String) -> Self {
        Self {
            adapter: AdapterConfig {
                name,
                version,
                model,
                hash,
                input_format: "raw".to_string(),
                output_format: "raw".to_string(),
                epsilon_threshold: default_epsilon(),
                deterministic: default_deterministic(),
                model_files: HashMap::new(),
                parameters: HashMap::new(),
                custom: HashMap::new(),
            },
        }
    }

    /// Parse hash string to B3Hash
    pub fn parse_hash(&self) -> Result<B3Hash> {
        // For now, we'll create a hash from the hex string
        // In production, this would properly decode the hex string
        let hash_bytes = self.adapter.hash.as_bytes();
        if hash_bytes.len() < 32 {
            return Err(DomainAdapterError::InvalidManifest {
                reason: format!("Hash too short: {}", self.adapter.hash),
            });
        }

        Ok(B3Hash::hash(hash_bytes))
    }

    /// Validate manifest
    pub fn validate(&self) -> Result<()> {
        // Check required fields
        if self.adapter.name.is_empty() {
            return Err(DomainAdapterError::InvalidManifest {
                reason: "Adapter name is required".to_string(),
            });
        }

        if self.adapter.version.is_empty() {
            return Err(DomainAdapterError::InvalidManifest {
                reason: "Adapter version is required".to_string(),
            });
        }

        if self.adapter.hash.is_empty() {
            return Err(DomainAdapterError::InvalidManifest {
                reason: "Model hash is required".to_string(),
            });
        }

        // Validate epsilon threshold
        if self.adapter.epsilon_threshold <= 0.0 {
            return Err(DomainAdapterError::InvalidManifest {
                reason: "Epsilon threshold must be positive".to_string(),
            });
        }

        // Ensure deterministic flag is set for production adapters
        if !self.adapter.deterministic {
            tracing::warn!(
                "Adapter {} is marked as non-deterministic",
                self.adapter.name
            );
        }

        Ok(())
    }

    /// Get model file path
    pub fn get_model_file(&self, key: &str) -> Option<&String> {
        self.adapter.model_files.get(key)
    }

    /// Get parameter value
    pub fn get_parameter(&self, key: &str) -> Option<&serde_json::Value> {
        self.adapter.parameters.get(key)
    }

    /// Get parameter as integer
    pub fn get_parameter_i64(&self, key: &str) -> Option<i64> {
        self.get_parameter(key).and_then(|v| v.as_i64())
    }

    /// Get parameter as float
    pub fn get_parameter_f64(&self, key: &str) -> Option<f64> {
        self.get_parameter(key).and_then(|v| v.as_f64())
    }

    /// Get parameter as string
    pub fn get_parameter_str(&self, key: &str) -> Option<&str> {
        self.get_parameter(key).and_then(|v| v.as_str())
    }
}

/// Load manifest from TOML file
///
/// # Arguments
/// * `path` - Path to the manifest file
///
/// # Returns
/// * `Result<AdapterManifest>` - Parsed and validated manifest
///
/// # Example
///
/// ```rust,no_run
/// use adapteros_policy::domain::manifest::load_manifest;
///
/// let manifest = load_manifest("adapter/manifest.toml").unwrap();
/// println!("Loaded adapter: {}", manifest.adapter.name);
/// ```
pub fn load_manifest<P: AsRef<Path>>(path: P) -> Result<AdapterManifest> {
    let path_ref = path.as_ref();
    let content =
        fs::read_to_string(path_ref).map_err(|e| DomainAdapterError::ManifestLoadError {
            path: path_ref.display().to_string(),
            source: e,
        })?;

    let manifest: AdapterManifest = toml::from_str(&content)?;
    manifest.validate()?;

    tracing::info!(
        "Loaded manifest for adapter: {} v{}",
        manifest.adapter.name,
        manifest.adapter.version
    );

    Ok(manifest)
}

/// Save manifest to TOML file
pub fn save_manifest<P: AsRef<Path>>(manifest: &AdapterManifest, path: P) -> Result<()> {
    manifest.validate()?;

    let content =
        toml::to_string_pretty(manifest).map_err(|e| DomainAdapterError::InvalidManifest {
            reason: format!("Failed to serialize manifest: {}", e),
        })?;

    fs::write(path.as_ref(), content).map_err(|e| DomainAdapterError::ManifestLoadError {
        path: path.as_ref().display().to_string(),
        source: e,
    })?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use tempfile::NamedTempFile;

    fn new_test_tempfile() -> NamedTempFile {
        let root = PathBuf::from("var").join("tmp");
        std::fs::create_dir_all(&root).expect("create var/tmp");
        NamedTempFile::new_in(&root).expect("create temp file")
    }

    fn create_test_manifest() -> AdapterManifest {
        let mut manifest = AdapterManifest::new(
            "test_adapter".to_string(),
            "1.0.0".to_string(),
            "test_model".to_string(),
            "b3d9c2a1e8f7d6b5a4938271605e4f3c2d1b0a9e8f7d6c5b4a3928170605".to_string(),
        );

        manifest.adapter.model_files.insert(
            "weights".to_string(),
            "path/to/weights.safetensors".to_string(),
        );

        manifest.adapter.parameters.insert(
            "max_sequence_length".to_string(),
            serde_json::Value::Number(2048.into()),
        );

        manifest
    }

    #[test]
    fn test_manifest_creation() {
        let manifest = create_test_manifest();
        assert_eq!(manifest.adapter.name, "test_adapter");
        assert_eq!(manifest.adapter.version, "1.0.0");
        assert!(manifest.adapter.deterministic);
    }

    #[test]
    fn test_manifest_validation() {
        let manifest = create_test_manifest();
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_manifest_validation_empty_name() {
        let mut manifest = create_test_manifest();
        manifest.adapter.name = "".to_string();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_manifest_get_parameter() {
        let manifest = create_test_manifest();
        assert_eq!(
            manifest.get_parameter_i64("max_sequence_length"),
            Some(2048)
        );
    }

    #[test]
    fn test_manifest_save_load() {
        let manifest = create_test_manifest();
        let temp_file = new_test_tempfile();

        save_manifest(&manifest, temp_file.path()).unwrap();
        let loaded = load_manifest(temp_file.path()).unwrap();

        assert_eq!(loaded.adapter.name, manifest.adapter.name);
        assert_eq!(loaded.adapter.version, manifest.adapter.version);
    }
}
