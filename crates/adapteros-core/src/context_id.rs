//! Context identifier construction for inference run verification.
//!
//! Computes a unique identifier that captures the model and adapter configuration
//! for an inference run. The `context_id` enables verification that specific
//! model/adapter versions were used. Configuration changes produce different
//! context identifiers.
//!
//! # Canonical Formula
//!
//! 1. Collect `base_model_hash` from loaded model
//! 2. Collect adapter hashes sorted by `stable_id`
//! 3. Collect configuration parameters (temperature, top_k, max_tokens)
//! 4. Serialize as canonical binary (length-prefixed, little-endian)
//! 5. Compute `context_id` using BLAKE3
//!
//! # Usage
//!
//! ```rust
//! use adapteros_core::{B3Hash, context_id::{ContextIdBuilder, AdapterEntry}};
//!
//! let context_id = ContextIdBuilder::new(B3Hash::hash(b"model_weights"))
//!     .with_adapter(AdapterEntry {
//!         stable_id: 1,
//!         hash_b3: B3Hash::hash(b"adapter_1"),
//!     })
//!     .with_temperature(0.7)
//!     .with_top_k(40)
//!     .with_max_tokens(2048)
//!     .build();
//! ```

use crate::B3Hash;
use serde::{Deserialize, Serialize};

/// Error when constructing a context identifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextIdError {
    /// Base model hash is missing or invalid.
    MissingModelHash,
    /// An adapter hash is missing or invalid.
    MissingAdapterHash {
        /// The stable_id of the adapter with the missing hash.
        stable_id: u64,
    },
}

impl std::fmt::Display for ContextIdError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContextIdError::MissingModelHash => {
                write!(f, "Base model hash is missing or invalid")
            }
            ContextIdError::MissingAdapterHash { stable_id } => {
                write!(f, "Adapter hash missing for stable_id={}", stable_id)
            }
        }
    }
}

impl std::error::Error for ContextIdError {}

/// Adapter entry for context identifier computation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdapterEntry {
    /// Stable ID for deterministic ordering.
    pub stable_id: u64,
    /// BLAKE3 hash of adapter weights/content.
    pub hash_b3: B3Hash,
}

/// Configuration parameters included in the context identifier.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InferenceConfig {
    /// Sampling temperature (0.0 = deterministic, higher = more random).
    pub temperature: Option<f32>,
    /// Top-k sampling parameter.
    pub top_k: Option<u32>,
    /// Maximum output tokens.
    pub max_tokens: Option<u32>,
}

/// Builder for constructing context identifiers.
///
/// Ensures all required components are collected before computing
/// the final context_id hash.
#[derive(Debug, Clone)]
pub struct ContextIdBuilder {
    base_model_hash: Option<B3Hash>,
    adapters: Vec<AdapterEntry>,
    config: InferenceConfig,
}

impl ContextIdBuilder {
    /// Create a new builder with the base model hash.
    pub fn new(base_model_hash: B3Hash) -> Self {
        Self {
            base_model_hash: Some(base_model_hash),
            adapters: Vec::new(),
            config: InferenceConfig::default(),
        }
    }

    /// Create an empty builder (requires `set_model_hash` before build).
    pub fn empty() -> Self {
        Self {
            base_model_hash: None,
            adapters: Vec::new(),
            config: InferenceConfig::default(),
        }
    }

    /// Set the base model hash.
    pub fn set_model_hash(mut self, hash: B3Hash) -> Self {
        self.base_model_hash = Some(hash);
        self
    }

    /// Add an adapter entry.
    pub fn with_adapter(mut self, adapter: AdapterEntry) -> Self {
        self.adapters.push(adapter);
        self
    }

    /// Add multiple adapter entries.
    pub fn with_adapters(mut self, adapters: impl IntoIterator<Item = AdapterEntry>) -> Self {
        self.adapters.extend(adapters);
        self
    }

    /// Set the temperature parameter.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.config.temperature = Some(temperature);
        self
    }

    /// Set the top_k parameter.
    pub fn with_top_k(mut self, top_k: u32) -> Self {
        self.config.top_k = Some(top_k);
        self
    }

    /// Set the max_tokens parameter.
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.config.max_tokens = Some(max_tokens);
        self
    }

    /// Set all configuration parameters at once.
    pub fn with_config(mut self, config: InferenceConfig) -> Self {
        self.config = config;
        self
    }

    /// Build the context identifier.
    ///
    /// # Errors
    ///
    /// Returns `ContextIdError::MissingModelHash` if no base model hash was set.
    pub fn build(self) -> Result<B3Hash, ContextIdError> {
        let base_model_hash = self
            .base_model_hash
            .ok_or(ContextIdError::MissingModelHash)?;

        Ok(compute_context_id(
            &base_model_hash,
            &self.adapters,
            &self.config,
        ))
    }

    /// Try to build, returning None if model hash is missing.
    pub fn try_build(self) -> Option<B3Hash> {
        self.build().ok()
    }
}

/// Compute a deterministic context identifier.
///
/// This is the canonical implementation for context_id computation.
///
/// # Arguments
///
/// * `base_model_hash` - BLAKE3 hash of the base model weights
/// * `adapters` - Adapter entries (will be sorted by stable_id)
/// * `config` - Inference configuration parameters
///
/// # Returns
///
/// BLAKE3 hash representing the complete inference context.
pub fn compute_context_id(
    base_model_hash: &B3Hash,
    adapters: &[AdapterEntry],
    config: &InferenceConfig,
) -> B3Hash {
    // Sort adapters by stable_id for deterministic ordering
    let mut sorted_adapters = adapters.to_vec();
    sorted_adapters.sort_by_key(|a| a.stable_id);

    // Build canonical binary representation
    let mut buf = Vec::with_capacity(256);

    // 1. Base model hash (32 bytes)
    buf.extend_from_slice(base_model_hash.as_bytes());

    // 2. Adapter count (u32 LE) + sorted adapter entries
    buf.extend_from_slice(&(sorted_adapters.len() as u32).to_le_bytes());
    for adapter in &sorted_adapters {
        // stable_id (u64 LE) + hash (32 bytes)
        buf.extend_from_slice(&adapter.stable_id.to_le_bytes());
        buf.extend_from_slice(adapter.hash_b3.as_bytes());
    }

    // 3. Configuration parameters with presence markers (avoids sentinel ambiguity)
    // Temperature: 1-byte presence marker + f32 bits if present
    match config.temperature {
        Some(t) => {
            buf.push(1u8);
            buf.extend_from_slice(&t.to_bits().to_le_bytes());
        }
        None => buf.push(0u8),
    }

    // Top-k: 1-byte presence marker + u32 if present
    match config.top_k {
        Some(k) => {
            buf.push(1u8);
            buf.extend_from_slice(&k.to_le_bytes());
        }
        None => buf.push(0u8),
    }

    // Max tokens: 1-byte presence marker + u32 if present
    match config.max_tokens {
        Some(m) => {
            buf.push(1u8);
            buf.extend_from_slice(&m.to_le_bytes());
        }
        None => buf.push(0u8),
    }

    B3Hash::hash(&buf)
}

/// Validate that all adapters have valid hashes.
///
/// Call this before `compute_context_id` when adapter hashes may be missing.
///
/// # Errors
///
/// Returns the first adapter with a zero hash.
pub fn validate_adapter_hashes(adapters: &[AdapterEntry]) -> Result<(), ContextIdError> {
    for adapter in adapters {
        if adapter.hash_b3 == B3Hash::zero() {
            return Err(ContextIdError::MissingAdapterHash {
                stable_id: adapter.stable_id,
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_id_deterministic() {
        let model_hash = B3Hash::hash(b"model_weights");
        let adapters = vec![
            AdapterEntry {
                stable_id: 2,
                hash_b3: B3Hash::hash(b"adapter_2"),
            },
            AdapterEntry {
                stable_id: 1,
                hash_b3: B3Hash::hash(b"adapter_1"),
            },
        ];
        let config = InferenceConfig {
            temperature: Some(0.7),
            top_k: Some(40),
            max_tokens: Some(2048),
        };

        let id1 = compute_context_id(&model_hash, &adapters, &config);
        let id2 = compute_context_id(&model_hash, &adapters, &config);

        assert_eq!(id1, id2, "Same inputs must produce same context_id");
    }

    #[test]
    fn test_context_id_order_independent() {
        let model_hash = B3Hash::hash(b"model_weights");
        let adapters1 = vec![
            AdapterEntry {
                stable_id: 1,
                hash_b3: B3Hash::hash(b"adapter_1"),
            },
            AdapterEntry {
                stable_id: 2,
                hash_b3: B3Hash::hash(b"adapter_2"),
            },
        ];
        let adapters2 = vec![
            AdapterEntry {
                stable_id: 2,
                hash_b3: B3Hash::hash(b"adapter_2"),
            },
            AdapterEntry {
                stable_id: 1,
                hash_b3: B3Hash::hash(b"adapter_1"),
            },
        ];
        let config = InferenceConfig::default();

        let id1 = compute_context_id(&model_hash, &adapters1, &config);
        let id2 = compute_context_id(&model_hash, &adapters2, &config);

        assert_eq!(
            id1, id2,
            "Adapter input order must not affect context_id (sorted by stable_id)"
        );
    }

    #[test]
    fn test_context_id_different_model_hash() {
        let adapters = vec![AdapterEntry {
            stable_id: 1,
            hash_b3: B3Hash::hash(b"adapter_1"),
        }];
        let config = InferenceConfig::default();

        let id1 = compute_context_id(&B3Hash::hash(b"model_a"), &adapters, &config);
        let id2 = compute_context_id(&B3Hash::hash(b"model_b"), &adapters, &config);

        assert_ne!(id1, id2, "Different model hashes must produce different context_id");
    }

    #[test]
    fn test_context_id_different_adapters() {
        let model_hash = B3Hash::hash(b"model_weights");
        let config = InferenceConfig::default();

        let adapters1 = vec![AdapterEntry {
            stable_id: 1,
            hash_b3: B3Hash::hash(b"adapter_1"),
        }];
        let adapters2 = vec![AdapterEntry {
            stable_id: 1,
            hash_b3: B3Hash::hash(b"adapter_2"),
        }];

        let id1 = compute_context_id(&model_hash, &adapters1, &config);
        let id2 = compute_context_id(&model_hash, &adapters2, &config);

        assert_ne!(
            id1, id2,
            "Different adapter hashes must produce different context_id"
        );
    }

    #[test]
    fn test_context_id_different_config() {
        let model_hash = B3Hash::hash(b"model_weights");
        let adapters = vec![];

        let config1 = InferenceConfig {
            temperature: Some(0.7),
            top_k: Some(40),
            max_tokens: Some(2048),
        };
        let config2 = InferenceConfig {
            temperature: Some(0.8),
            top_k: Some(40),
            max_tokens: Some(2048),
        };

        let id1 = compute_context_id(&model_hash, &adapters, &config1);
        let id2 = compute_context_id(&model_hash, &adapters, &config2);

        assert_ne!(
            id1, id2,
            "Different config must produce different context_id"
        );
    }

    #[test]
    fn test_context_id_none_vs_some_config() {
        let model_hash = B3Hash::hash(b"model_weights");
        let adapters = vec![];

        let config1 = InferenceConfig {
            temperature: None,
            top_k: None,
            max_tokens: None,
        };
        let config2 = InferenceConfig {
            temperature: Some(1.0),
            top_k: None,
            max_tokens: None,
        };

        let id1 = compute_context_id(&model_hash, &adapters, &config1);
        let id2 = compute_context_id(&model_hash, &adapters, &config2);

        assert_ne!(id1, id2, "None vs Some config must produce different context_id");
    }

    #[test]
    fn test_builder_basic() {
        let model_hash = B3Hash::hash(b"model_weights");

        let context_id = ContextIdBuilder::new(model_hash)
            .with_adapter(AdapterEntry {
                stable_id: 1,
                hash_b3: B3Hash::hash(b"adapter_1"),
            })
            .with_temperature(0.7)
            .with_top_k(40)
            .with_max_tokens(2048)
            .build()
            .expect("Should build successfully");

        assert_ne!(context_id, B3Hash::zero());
    }

    #[test]
    fn test_builder_missing_model_hash() {
        let result = ContextIdBuilder::empty()
            .with_adapter(AdapterEntry {
                stable_id: 1,
                hash_b3: B3Hash::hash(b"adapter_1"),
            })
            .build();

        assert!(matches!(result, Err(ContextIdError::MissingModelHash)));
    }

    #[test]
    fn test_validate_adapter_hashes() {
        let valid_adapters = vec![
            AdapterEntry {
                stable_id: 1,
                hash_b3: B3Hash::hash(b"adapter_1"),
            },
            AdapterEntry {
                stable_id: 2,
                hash_b3: B3Hash::hash(b"adapter_2"),
            },
        ];
        assert!(validate_adapter_hashes(&valid_adapters).is_ok());

        let invalid_adapters = vec![
            AdapterEntry {
                stable_id: 1,
                hash_b3: B3Hash::hash(b"adapter_1"),
            },
            AdapterEntry {
                stable_id: 2,
                hash_b3: B3Hash::zero(),
            },
        ];
        let result = validate_adapter_hashes(&invalid_adapters);
        assert!(matches!(
            result,
            Err(ContextIdError::MissingAdapterHash { stable_id: 2 })
        ));
    }

    #[test]
    fn test_empty_adapters() {
        let model_hash = B3Hash::hash(b"model_weights");
        let config = InferenceConfig::default();

        let id = compute_context_id(&model_hash, &[], &config);
        assert_ne!(id, B3Hash::zero(), "Empty adapters should still produce valid hash");
    }

    #[test]
    fn test_builder_with_multiple_adapters() {
        let model_hash = B3Hash::hash(b"model_weights");
        let adapters = vec![
            AdapterEntry {
                stable_id: 1,
                hash_b3: B3Hash::hash(b"adapter_1"),
            },
            AdapterEntry {
                stable_id: 2,
                hash_b3: B3Hash::hash(b"adapter_2"),
            },
        ];

        let context_id = ContextIdBuilder::new(model_hash)
            .with_adapters(adapters)
            .build()
            .expect("Should build successfully");

        assert_ne!(context_id, B3Hash::zero());
    }
}
