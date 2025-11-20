//! Core AOS file generators

use crate::test_utils::safetensors::SafetensorsBuilder;
use crate::test_utils::semantic_ids::SemanticIdGenerator;
use adapteros_core::{AosError, Result};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// AOS manifest version
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestVersion {
    /// Version 1.0 (legacy)
    V1_0,
    /// Version 2.0 (current)
    V2_0,
    /// Invalid version for testing
    Invalid,
}

impl ManifestVersion {
    pub fn as_str(&self) -> &'static str {
        match self {
            ManifestVersion::V1_0 => "1.0",
            ManifestVersion::V2_0 => "2.0",
            ManifestVersion::Invalid => "99.99",
        }
    }
}

/// Types of corruption for error testing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorruptionType {
    /// Corrupt the 8-byte header
    BadHeader,
    /// Corrupt the manifest JSON
    BadManifest,
    /// Corrupt the weights data
    BadWeights,
    /// Invalid manifest offset (points beyond file)
    InvalidOffset,
    /// Wrong BLAKE3 hash in manifest
    WrongHash,
    /// Truncated file (incomplete data)
    Truncated,
}

/// Edge cases for testing
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EdgeCaseType {
    /// Empty weights section
    EmptyWeights,
    /// Huge file (multi-megabyte)
    HugeFile,
    /// Missing manifest section
    MissingManifest,
    /// Zero-rank adapter
    ZeroRank,
    /// Single tensor only
    SingleTensor,
    /// Many tensors (100+)
    ManyTensors,
}

/// Configuration for AOS generator
#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    /// LoRA rank (default: 8)
    pub rank: u32,
    /// Hidden dimension (default: 512)
    pub hidden_dim: usize,
    /// Number of tensors to generate (default: 2 for lora_A and lora_B)
    pub num_tensors: usize,
    /// Random seed for deterministic generation (default: None = random)
    pub seed: Option<u64>,
    /// Manifest version (default: V2_0)
    pub version: ManifestVersion,
    /// Base model name (default: "llama-7b")
    pub base_model: String,
    /// Adapter ID (default: auto-generated)
    pub adapter_id: Option<String>,
    /// Learning rate (default: 1e-4)
    pub learning_rate: f32,
    /// Alpha scaling (default: 16.0)
    pub alpha: f32,
    /// Batch size (default: 4)
    pub batch_size: usize,
    /// Epochs (default: 3)
    pub epochs: usize,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            rank: 8,
            hidden_dim: 512,
            num_tensors: 2,
            seed: None,
            version: ManifestVersion::V2_0,
            base_model: "llama-7b".to_string(),
            adapter_id: None,
            learning_rate: 1e-4,
            alpha: 16.0,
            batch_size: 4,
            epochs: 3,
        }
    }
}

/// Standard test manifest structure compatible with AOS 2.0
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TestManifest {
    pub version: String,
    pub adapter_id: String,
    pub rank: u32,
    pub base_model: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weights_hash: Option<String>,
    pub training_config: TrainingConfig,
}

/// Training configuration in manifest
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct TrainingConfig {
    pub rank: usize,
    pub alpha: f32,
    pub learning_rate: f32,
    pub batch_size: usize,
    pub epochs: usize,
    pub hidden_dim: usize,
}

/// Main AOS generator
pub struct AosGenerator {
    config: GeneratorConfig,
    rng: ChaCha8Rng,
    id_generator: SemanticIdGenerator,
}

impl AosGenerator {
    /// Create a new generator with the given configuration
    pub fn new(config: GeneratorConfig) -> Self {
        let seed = config.seed.unwrap_or_else(|| {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        });

        let rng = ChaCha8Rng::seed_from_u64(seed);
        let id_generator = SemanticIdGenerator::new(seed);

        Self {
            config,
            rng,
            id_generator,
        }
    }

    /// Generate a valid AOS file in memory
    pub fn generate_valid(&mut self) -> Result<Vec<u8>> {
        let manifest = self.create_manifest();
        let weights = self.create_weights()?;

        self.write_aos(&manifest, &weights)
    }

    /// Generate a valid AOS file to disk
    pub fn generate_to_file<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let data = self.generate_valid()?;
        std::fs::write(path.as_ref(), &data)
            .map_err(|e| AosError::Io(format!("Failed to write file: {}", e)))?;
        Ok(())
    }

    /// Generate a corrupted AOS file
    pub fn generate_corrupted(&mut self, corruption_type: CorruptionType) -> Result<Vec<u8>> {
        let mut data = self.generate_valid()?;

        match corruption_type {
            CorruptionType::BadHeader => {
                // Corrupt the first 4 bytes (manifest_offset)
                data[0] = 0xFF;
                data[1] = 0xFF;
                data[2] = 0xFF;
                data[3] = 0xFF;
            }
            CorruptionType::BadManifest => {
                // Find manifest section and corrupt it
                let manifest_offset =
                    u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
                if manifest_offset < data.len() {
                    // Corrupt JSON by replacing a bracket
                    for i in manifest_offset..data.len() {
                        if data[i] == b'{' {
                            data[i] = b'X';
                            break;
                        }
                    }
                }
            }
            CorruptionType::BadWeights => {
                // Corrupt the weights section (bytes 8 to manifest_offset)
                let manifest_offset =
                    u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
                if manifest_offset > 8 {
                    let corrupt_pos = 8 + (manifest_offset - 8) / 2;
                    data[corrupt_pos] = data[corrupt_pos].wrapping_add(1);
                }
            }
            CorruptionType::InvalidOffset => {
                // Set manifest offset beyond file size
                let invalid_offset = (data.len() + 1000) as u32;
                data[0..4].copy_from_slice(&invalid_offset.to_le_bytes());
            }
            CorruptionType::WrongHash => {
                // Generate with correct hash, then change weights
                if data.len() > 100 {
                    data[50] = data[50].wrapping_add(1);
                }
            }
            CorruptionType::Truncated => {
                // Truncate file to 75% of original size
                let new_len = (data.len() * 3) / 4;
                data.truncate(new_len);
            }
        }

        Ok(data)
    }

    /// Generate an edge case AOS file
    pub fn generate_edge_case(&mut self, edge_case: EdgeCaseType) -> Result<Vec<u8>> {
        match edge_case {
            EdgeCaseType::EmptyWeights => {
                let manifest = self.create_manifest();
                let weights = vec![]; // Empty weights
                self.write_aos(&manifest, &weights)
            }
            EdgeCaseType::HugeFile => {
                let manifest = self.create_manifest();
                let weights = self.create_large_weights(5 * 1024 * 1024)?; // 5MB
                self.write_aos(&manifest, &weights)
            }
            EdgeCaseType::MissingManifest => {
                // Create a file with invalid header pointing to non-existent manifest
                let mut data = vec![0u8; 100];
                let manifest_offset = 1000u32; // Beyond file size
                let manifest_len = 100u32;
                data[0..4].copy_from_slice(&manifest_offset.to_le_bytes());
                data[4..8].copy_from_slice(&manifest_len.to_le_bytes());
                Ok(data)
            }
            EdgeCaseType::ZeroRank => {
                let mut config = self.config.clone();
                config.rank = 0;
                let mut gen = AosGenerator::new(config);
                gen.generate_valid()
            }
            EdgeCaseType::SingleTensor => {
                let mut config = self.config.clone();
                config.num_tensors = 1;
                let mut gen = AosGenerator::new(config);
                gen.generate_valid()
            }
            EdgeCaseType::ManyTensors => {
                let mut config = self.config.clone();
                config.num_tensors = 100;
                let mut gen = AosGenerator::new(config);
                gen.generate_valid()
            }
        }
    }

    /// Create a manifest with current config
    fn create_manifest(&mut self) -> TestManifest {
        let adapter_id = self
            .config
            .adapter_id
            .clone()
            .unwrap_or_else(|| self.id_generator.generate());

        TestManifest {
            version: self.config.version.as_str().to_string(),
            adapter_id,
            rank: self.config.rank,
            base_model: self.config.base_model.clone(),
            created_at: chrono::Utc::now().to_rfc3339(),
            weights_hash: None, // Will be computed if needed
            training_config: TrainingConfig {
                rank: self.config.rank as usize,
                alpha: self.config.alpha,
                learning_rate: self.config.learning_rate,
                batch_size: self.config.batch_size,
                epochs: self.config.epochs,
                hidden_dim: self.config.hidden_dim,
            },
        }
    }

    /// Create weights using SafetensorsBuilder
    fn create_weights(&mut self) -> Result<Vec<u8>> {
        let mut builder = SafetensorsBuilder::new();

        for i in 0..self.config.num_tensors {
            let tensor_name = if i == 0 {
                "lora_A".to_string()
            } else if i == 1 {
                "lora_B".to_string()
            } else {
                format!("tensor_{}", i)
            };

            // Generate random tensor data
            let size = if i == 0 {
                self.config.rank as usize * self.config.hidden_dim
            } else {
                self.config.hidden_dim * self.config.hidden_dim
            };

            let data: Vec<f32> = (0..size).map(|_| self.rng.gen::<f32>()).collect();
            let shape = if i == 0 {
                vec![self.config.rank as usize, self.config.hidden_dim]
            } else {
                vec![self.config.hidden_dim, self.config.hidden_dim]
            };

            builder.add_tensor(tensor_name, data, shape);
        }

        builder.build()
    }

    /// Create large weights for huge file testing
    fn create_large_weights(&mut self, target_size: usize) -> Result<Vec<u8>> {
        let mut builder = SafetensorsBuilder::new();

        // Create one large tensor
        let num_elements = target_size / 4; // f32 is 4 bytes
        let data: Vec<f32> = (0..num_elements).map(|_| self.rng.gen::<f32>()).collect();

        builder.add_tensor("large_tensor".to_string(), data, vec![num_elements]);
        builder.build()
    }

    /// Write AOS file using AOS2Writer
    fn write_aos(&self, manifest: &TestManifest, weights: &[u8]) -> Result<Vec<u8>> {
        use std::io::Cursor;

        let mut buffer = Vec::new();

        // Manually write the AOS format since we need in-memory output
        let manifest_json = serde_json::to_vec_pretty(manifest)?;

        let header_size = 8;
        let manifest_offset = header_size + weights.len();
        let manifest_len = manifest_json.len();

        // Validate sizes
        if manifest_offset > u32::MAX as usize {
            return Err(AosError::Validation(format!(
                "Archive too large: manifest_offset {} exceeds u32::MAX",
                manifest_offset
            )));
        }
        if manifest_len > u32::MAX as usize {
            return Err(AosError::Validation(format!(
                "Manifest too large: {} exceeds u32::MAX",
                manifest_len
            )));
        }

        // Write header
        buffer.extend_from_slice(&(manifest_offset as u32).to_le_bytes());
        buffer.extend_from_slice(&(manifest_len as u32).to_le_bytes());

        // Write weights
        buffer.extend_from_slice(weights);

        // Write manifest
        buffer.extend_from_slice(&manifest_json);

        Ok(buffer)
    }
}

/// Helper function: Generate a valid AOS file with default config
pub fn generate_valid_aos() -> Result<Vec<u8>> {
    let mut generator = AosGenerator::new(GeneratorConfig::default());
    generator.generate_valid()
}

/// Helper function: Generate a valid AOS file with custom parameters
pub fn generate_valid_aos_with_params(rank: u32, hidden_dim: usize) -> Result<Vec<u8>> {
    let config = GeneratorConfig {
        rank,
        hidden_dim,
        ..Default::default()
    };
    let mut generator = AosGenerator::new(config);
    generator.generate_valid()
}

/// Helper function: Generate a corrupted AOS file
pub fn generate_corrupted_aos(corruption_type: CorruptionType) -> Result<Vec<u8>> {
    let mut generator = AosGenerator::new(GeneratorConfig::default());
    generator.generate_corrupted(corruption_type)
}

/// Helper function: Generate an edge case AOS file
pub fn generate_edge_case_aos(edge_case: EdgeCaseType) -> Result<Vec<u8>> {
    let mut generator = AosGenerator::new(GeneratorConfig::default());
    generator.generate_edge_case(edge_case)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_generate_valid_aos() -> Result<()> {
        let data = generate_valid_aos()?;
        assert!(data.len() > 8, "Should have at least header");

        // Verify header
        let manifest_offset = u32::from_le_bytes([data[0], data[1], data[2], data[3]]) as usize;
        let manifest_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;

        assert!(manifest_offset > 8, "Manifest should be after header");
        assert!(manifest_len > 0, "Manifest should have content");
        assert!(
            manifest_offset + manifest_len <= data.len(),
            "Manifest should fit in file"
        );

        Ok(())
    }

    #[test]
    fn test_deterministic_generation() -> Result<()> {
        let config1 = GeneratorConfig {
            seed: Some(42),
            ..Default::default()
        };
        let config2 = GeneratorConfig {
            seed: Some(42),
            ..Default::default()
        };

        let mut gen1 = AosGenerator::new(config1);
        let mut gen2 = AosGenerator::new(config2);

        let data1 = gen1.generate_valid()?;
        let data2 = gen2.generate_valid()?;

        assert_eq!(
            data1.len(),
            data2.len(),
            "Same seed should produce same size"
        );

        Ok(())
    }

    #[test]
    fn test_corruption_types() -> Result<()> {
        let corruption_types = [
            CorruptionType::BadHeader,
            CorruptionType::BadManifest,
            CorruptionType::BadWeights,
            CorruptionType::InvalidOffset,
            CorruptionType::WrongHash,
            CorruptionType::Truncated,
        ];

        for corruption_type in &corruption_types {
            let data = generate_corrupted_aos(*corruption_type)?;
            assert!(data.len() > 0, "Corrupted data should not be empty");
        }

        Ok(())
    }

    #[test]
    fn test_edge_cases() -> Result<()> {
        let edge_cases = [
            EdgeCaseType::EmptyWeights,
            EdgeCaseType::ZeroRank,
            EdgeCaseType::SingleTensor,
            EdgeCaseType::ManyTensors,
        ];

        for edge_case in &edge_cases {
            let data = generate_edge_case_aos(*edge_case)?;
            assert!(data.len() >= 8, "Should have at least header");
        }

        Ok(())
    }

    #[test]
    fn test_generate_to_file() -> Result<()> {
        let temp_file = NamedTempFile::new()
            .map_err(|e| AosError::Io(format!("Failed to create temp file: {}", e)))?;

        let mut generator = AosGenerator::new(GeneratorConfig::default());
        generator.generate_to_file(temp_file.path())?;

        assert!(temp_file.path().exists(), "File should be created");

        let metadata = std::fs::metadata(temp_file.path())
            .map_err(|e| AosError::Io(format!("Failed to get metadata: {}", e)))?;
        assert!(metadata.len() > 8, "File should have content");

        Ok(())
    }

    #[test]
    fn test_custom_parameters() -> Result<()> {
        let data = generate_valid_aos_with_params(4, 256)?;
        assert!(data.len() > 8, "Should have content");
        Ok(())
    }
}
