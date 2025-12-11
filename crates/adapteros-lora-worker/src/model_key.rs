//! Model cache key for per-worker deduplication
//!
//! This module provides [`ModelKey`], the cache key type used to deduplicate
//! loaded models within a single worker process. A model is uniquely identified
//! by its backend type and manifest hash.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::attestation::BackendType;
use std::hash::{Hash, Hasher};
use std::path::Path;

/// Cache key for model deduplication: (backend_type, manifest_hash)
///
/// Two models are considered identical if they have the same backend type
/// and manifest hash. This ensures that:
/// - Different backends (Metal vs MLX) cache separately
/// - Different model versions (different config.json) cache separately
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelKey {
    /// The backend type (Metal, CoreML, MLX)
    pub backend_type: BackendType,
    /// BLAKE3 hash of the model identity (from config.json or path)
    pub manifest_hash: B3Hash,
}

impl ModelKey {
    /// Create a new model key from backend type and manifest hash
    ///
    /// This is the preferred constructor when you have the canonical manifest hash
    /// from `ManifestV3::compute_hash()` or from the database.
    pub fn new(backend_type: BackendType, manifest_hash: B3Hash) -> Self {
        Self {
            backend_type,
            manifest_hash,
        }
    }

    /// DEBUG/TEST ONLY: create a model key from manifest or path.
    ///
    /// This helper is reserved for offline tooling and test code paths and must
    /// never be used from HTTP handlers or worker UDS handlers. Online inference
    /// must supply a canonical manifest hash and use [`ModelKey::new`].
    #[cfg(any(test, debug_assertions))]
    pub fn from_manifest_or_path(
        backend_type: BackendType,
        manifest_hash: Option<&B3Hash>,
        model_path: &Path,
    ) -> Result<Self> {
        if let Some(hash) = manifest_hash {
            tracing::debug!(
                backend = %Self::backend_type_str_static(backend_type),
                manifest_hash = %hash.to_hex()[..12],
                "Using canonical manifest hash for model identity"
            );
            Ok(Self::new(backend_type, *hash))
        } else {
            tracing::warn!(
                backend = %Self::backend_type_str_static(backend_type),
                model_path = %model_path.display(),
                "DEBUG/TEST: falling back to path-based identity; not for online inference"
            );
            Self::from_path(backend_type, model_path)
        }
    }

    /// DEBUG/TEST ONLY: create a model key by computing a hash from the model path.
    ///
    /// Path-based identity is intended solely for offline tooling and tests and
    /// must not be reachable from production inference paths.
    #[cfg(any(test, debug_assertions))]
    pub fn from_path(backend_type: BackendType, model_path: &Path) -> Result<Self> {
        let config_path = model_path.join("config.json");
        let manifest_hash = if config_path.exists() {
            B3Hash::hash_file(&config_path).map_err(|e| {
                AosError::Config(format!(
                    "Failed to hash config.json at '{}': {}",
                    config_path.display(),
                    e
                ))
            })?
        } else {
            // Last resort: hash the path itself
            tracing::debug!(
                model_path = %model_path.display(),
                "DEBUG/TEST: using path hash as model identity"
            );
            B3Hash::hash(model_path.to_string_lossy().as_bytes())
        };

        Ok(Self {
            backend_type,
            manifest_hash,
        })
    }

    fn backend_type_str_static(backend_type: BackendType) -> &'static str {
        match backend_type {
            BackendType::Metal => "metal",
            BackendType::Mlx => "mlx",
            BackendType::CoreML => "coreml",
            BackendType::Mock => "mock",
        }
    }

    /// Get a short hex representation for logging
    pub fn short_hex(&self) -> String {
        format!(
            "{}:{}",
            self.backend_type_str(),
            &self.manifest_hash.to_hex()[..12]
        )
    }

    fn backend_type_str(&self) -> &'static str {
        match self.backend_type {
            BackendType::Metal => "metal",
            BackendType::Mlx => "mlx",
            BackendType::CoreML => "coreml",
            BackendType::Mock => "mock",
        }
    }
}

impl Hash for ModelKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Use discriminant value for backend type
        let backend_discriminant: u8 = match self.backend_type {
            BackendType::Metal => 0,
            BackendType::Mlx => 1,
            BackendType::CoreML => 2,
            BackendType::Mock => 3,
        };
        backend_discriminant.hash(state);
        self.manifest_hash.as_bytes().hash(state);
    }
}

impl std::fmt::Display for ModelKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}",
            self.backend_type_str(),
            self.manifest_hash.to_hex()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn test_model_key_equality() {
        let hash1 = B3Hash::hash(b"model1");
        let hash2 = B3Hash::hash(b"model2");

        let key1a = ModelKey::new(BackendType::Metal, hash1);
        let key1b = ModelKey::new(BackendType::Metal, hash1);
        let key2 = ModelKey::new(BackendType::Metal, hash2);
        let key3 = ModelKey::new(BackendType::Mlx, hash1);

        // Same backend + same hash = equal
        assert_eq!(key1a, key1b);

        // Same backend + different hash = not equal
        assert_ne!(key1a, key2);

        // Different backend + same hash = not equal
        assert_ne!(key1a, key3);
    }

    #[test]
    fn test_model_key_hash_in_collection() {
        let hash1 = B3Hash::hash(b"model1");
        let hash2 = B3Hash::hash(b"model2");

        let mut set = HashSet::new();

        let key1 = ModelKey::new(BackendType::Metal, hash1);
        let key2 = ModelKey::new(BackendType::Mlx, hash1);
        let key3 = ModelKey::new(BackendType::Metal, hash2);

        set.insert(key1.clone());
        assert!(set.contains(&key1));
        assert!(!set.contains(&key2)); // Different backend

        set.insert(key2.clone());
        set.insert(key3.clone());
        assert_eq!(set.len(), 3);
    }

    #[test]
    fn test_model_key_display() {
        let hash = B3Hash::hash(b"test");
        let key = ModelKey::new(BackendType::Metal, hash);

        let display = format!("{}", key);
        assert!(display.starts_with("metal:"));
        assert_eq!(display.len(), 5 + 1 + 64); // "metal" + ":" + 64 hex chars
    }

    #[test]
    fn test_model_key_short_hex() {
        let hash = B3Hash::hash(b"test");
        let key = ModelKey::new(BackendType::Mlx, hash);

        let short = key.short_hex();
        assert!(short.starts_with("mlx:"));
        assert_eq!(short.len(), 3 + 1 + 12); // "mlx" + ":" + 12 hex chars
    }
}
