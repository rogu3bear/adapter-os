//! Model cache key for per-worker deduplication
//!
//! This module provides [`ModelKey`], the cache key type used to deduplicate
//! loaded models within a single worker process. A model is uniquely identified
//! by backend, manifest hash, and additional context manifest fields to avoid
//! collisions across kernel builds or fusion/quantization modes.

use adapteros_core::{AosError, B3Hash, Result};
use adapteros_lora_kernel_api::attestation::BackendType;
use std::path::Path;

// =============================================================================
// PRD-06: QuantizationMode and FusionMode Enums
// =============================================================================

/// Quantization mode for model cache identity (PRD-06).
///
/// Used in canonical bytes encoding for ModelCacheIdentityV2 digest computation.
/// Each variant has a fixed u8 discriminant for deterministic serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum QuantizationMode {
    /// FP16/BF16 backend (Metal/CoreML) - "fp16_bf16_backend"
    Fp16Bf16 = 0,
    /// Model quantized (MLX) - "model_quantized"
    ModelQuantized = 1,
    /// 4-bit quantized - "q4"
    Q4 = 2,
    /// 8-bit quantized - "q8"
    Q8 = 3,
    /// Mock backend (testing) - "mock"
    Mock = 4,
    /// Unknown/custom quantization mode
    Custom = 255,
}

impl QuantizationMode {
    /// Convert to u8 for canonical bytes encoding
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Convert from u8 for canonical bytes decoding
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::Fp16Bf16,
            1 => Self::ModelQuantized,
            2 => Self::Q4,
            3 => Self::Q8,
            4 => Self::Mock,
            _ => Self::Custom,
        }
    }

    /// Get the quantization mode for a backend type
    pub fn for_backend(backend_type: BackendType) -> Self {
        match backend_type {
            BackendType::Metal | BackendType::CoreML => Self::Fp16Bf16,
            BackendType::Mlx => Self::ModelQuantized,
            BackendType::Mock => Self::Mock,
        }
    }

    /// Convert to canonical string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Fp16Bf16 => "fp16_bf16_backend",
            Self::ModelQuantized => "model_quantized",
            Self::Q4 => "q4",
            Self::Q8 => "q8",
            Self::Mock => "mock",
            Self::Custom => "custom",
        }
    }
}

impl std::fmt::Display for QuantizationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for QuantizationMode {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "fp16_bf16_backend" | "fp16" | "bf16" => Ok(Self::Fp16Bf16),
            "model_quantized" => Ok(Self::ModelQuantized),
            "q4" | "int4" => Ok(Self::Q4),
            "q8" | "int8" => Ok(Self::Q8),
            "mock" => Ok(Self::Mock),
            _ => Ok(Self::Custom),
        }
    }
}

/// Fusion mode for model cache identity (PRD-06).
///
/// Determines when adapter weights are fused during inference.
/// Each variant has a fixed u8 discriminant for deterministic serialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum FusionMode {
    /// Fuse once per request - "per_request"
    PerRequest = 0,
    /// Fuse for each token - "per_token"
    PerToken = 1,
    /// Fuse per segment - "per_segment"
    PerSegment = 2,
    /// Unknown/custom fusion mode
    Custom = 255,
}

impl FusionMode {
    /// Convert to u8 for canonical bytes encoding
    pub fn to_u8(self) -> u8 {
        self as u8
    }

    /// Convert from u8 for canonical bytes decoding
    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::PerRequest,
            1 => Self::PerToken,
            2 => Self::PerSegment,
            _ => Self::Custom,
        }
    }

    /// Get the default fusion mode from FusionInterval
    pub fn default_mode() -> Self {
        match adapteros_core::FusionInterval::default_mode() {
            adapteros_core::FusionInterval::PerRequest => Self::PerRequest,
            adapteros_core::FusionInterval::PerToken => Self::PerToken,
            adapteros_core::FusionInterval::PerSegment { .. } => Self::PerSegment,
        }
    }

    /// Convert to canonical string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PerRequest => "per_request",
            Self::PerToken => "per_token",
            Self::PerSegment => "per_segment",
            Self::Custom => "custom",
        }
    }
}

impl std::fmt::Display for FusionMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for FusionMode {
    type Err = ();

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "per_request" => Ok(Self::PerRequest),
            "per_token" => Ok(Self::PerToken),
            "per_segment" => Ok(Self::PerSegment),
            _ => Ok(Self::Custom),
        }
    }
}

/// Context identity fields that must be part of cache keys to align with the
/// context manifest (kernel + quantization + fusion cadence).
///
/// PRD-RECT-003: Implements Ord for deterministic cache eviction ordering.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelCacheIdentity {
    pub kernel_version_id: String,
    pub quantization_mode: String,
    pub fusion_mode: String,
    pub build_id: Option<String>,
    pub adapter_dir_hash: Option<B3Hash>,
}

impl ModelCacheIdentity {
    pub fn new(
        kernel_version_id: impl Into<String>,
        quantization_mode: impl Into<String>,
        fusion_mode: impl Into<String>,
    ) -> Self {
        Self {
            kernel_version_id: kernel_version_id.into(),
            quantization_mode: quantization_mode.into(),
            fusion_mode: fusion_mode.into(),
            build_id: None,
            adapter_dir_hash: None,
        }
    }

    pub fn with_build_id(mut self, build_id: impl Into<String>) -> Self {
        self.build_id = Some(build_id.into());
        self
    }

    pub fn with_adapter_dir_hash(mut self, adapter_dir_hash: B3Hash) -> Self {
        self.adapter_dir_hash = Some(adapter_dir_hash);
        self
    }

    /// Convenience for the common case where the cache identity is derived from
    /// the backend and the running kernel build.
    pub fn for_backend(backend_type: BackendType) -> Self {
        Self::new(
            adapteros_core::version::VERSION,
            quantization_tag_for_backend(backend_type),
            default_fusion_tag(),
        )
        .with_build_id(adapteros_core::version::VERSION)
    }
}

/// Cache key for model deduplication.
///
/// Two models are considered identical only if backend, manifest hash, and the
/// context identity fields match.
///
/// PRD-RECT-003: Implements Ord for deterministic cache eviction ordering.
/// Ordering: backend_type → manifest_hash → identity (lexicographic)
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModelKey {
    /// The backend type (Metal, CoreML, MLX)
    pub backend_type: BackendType,
    /// BLAKE3 hash of the model identity (from config.json or path)
    pub manifest_hash: B3Hash,
    /// Context identity superset
    pub identity: ModelCacheIdentity,
}

impl ModelKey {
    /// Create a new model key from backend type, manifest hash, and context identity
    pub fn new(
        backend_type: BackendType,
        manifest_hash: B3Hash,
        identity: ModelCacheIdentity,
    ) -> Self {
        Self {
            backend_type,
            manifest_hash,
            identity,
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
            Ok(Self::new(
                backend_type,
                *hash,
                ModelCacheIdentity::for_backend(backend_type),
            ))
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
            identity: ModelCacheIdentity::for_backend(backend_type),
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
            "{}:{}:{}:{}:{}",
            self.backend_type_str(),
            &self.manifest_hash.to_hex()[..12],
            self.identity.kernel_version_id,
            self.identity.quantization_mode,
            self.identity.fusion_mode
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

impl std::fmt::Display for ModelKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}:{}:{}",
            self.backend_type_str(),
            self.manifest_hash.to_hex(),
            self.identity.kernel_version_id,
            self.identity.quantization_mode,
            self.identity.fusion_mode
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

        let identity = ModelCacheIdentity::new("k1", "fp16", "per_request");
        let key1a = ModelKey::new(BackendType::Metal, hash1, identity.clone());
        let key1b = ModelKey::new(BackendType::Metal, hash1, identity.clone());
        let key2 = ModelKey::new(BackendType::Metal, hash2, identity.clone());
        let key3 = ModelKey::new(BackendType::Mlx, hash1, identity);

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

        let identity = ModelCacheIdentity::new("k1", "fp16", "per_request");
        let key1 = ModelKey::new(BackendType::Metal, hash1, identity.clone());
        let key2 = ModelKey::new(BackendType::Mlx, hash1, identity.clone());
        let key3 = ModelKey::new(BackendType::Metal, hash2, identity);

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
        let identity = ModelCacheIdentity::new("k1", "fp16", "per_request");
        let key = ModelKey::new(BackendType::Metal, hash, identity);

        let display = format!("{}", key);
        assert!(display.starts_with("metal:"));
        assert!(
            display.contains("k1"),
            "display should surface kernel version for identity"
        );
        assert!(
            display.contains("per_request"),
            "display should surface fusion mode for identity"
        );
    }

    #[test]
    fn test_model_key_short_hex() {
        let hash = B3Hash::hash(b"test");
        let identity = ModelCacheIdentity::new("k1", "fp16", "per_request");
        let key = ModelKey::new(BackendType::Mlx, hash, identity);

        let short = key.short_hex();
        assert!(short.starts_with("mlx:"));
        assert!(
            short.contains("k1"),
            "short hex should include kernel version tag"
        );
        assert!(
            short.contains("per_request"),
            "short hex should include fusion tag"
        );
    }

    #[test]
    fn test_identity_fields_affect_equality() {
        let hash = B3Hash::hash(b"model");
        let base = ModelKey::new(
            BackendType::Metal,
            hash,
            ModelCacheIdentity::new("k1", "fp16", "per_request"),
        );
        let different_kernel = ModelKey::new(
            BackendType::Metal,
            hash,
            ModelCacheIdentity::new("k2", "fp16", "per_request"),
        );
        let different_quant = ModelKey::new(
            BackendType::Metal,
            hash,
            ModelCacheIdentity::new("k1", "int4", "per_request"),
        );
        let different_fusion = ModelKey::new(
            BackendType::Metal,
            hash,
            ModelCacheIdentity::new("k1", "fp16", "per_token"),
        );

        assert_ne!(
            base, different_kernel,
            "kernel version must alter model cache identity"
        );
        assert_ne!(
            base, different_quant,
            "quantization mode must alter model cache identity"
        );
        assert_ne!(
            base, different_fusion,
            "fusion mode must alter model cache identity"
        );
    }
}

pub fn quantization_tag_for_backend(backend_type: BackendType) -> &'static str {
    match backend_type {
        BackendType::Metal | BackendType::CoreML => "fp16_bf16_backend",
        BackendType::Mlx => "model_quantized",
        BackendType::Mock => "mock",
    }
}

pub fn default_fusion_tag() -> String {
    match adapteros_core::FusionInterval::default_mode() {
        adapteros_core::FusionInterval::PerRequest => "per_request".to_string(),
        adapteros_core::FusionInterval::PerToken => "per_token".to_string(),
        adapteros_core::FusionInterval::PerSegment { .. } => "per_segment".to_string(),
    }
}

// =============================================================================
// ModelCacheIdentityV2 (PRD-06: Canonical Bytes + Enforcement)
// =============================================================================

/// Extended cache identity with tokenizer binding for prefix KV caching (PRD-06).
///
/// V2 includes:
/// - All V1 fields (kernel, quantization, fusion, build_id, adapter_dir_hash)
/// - Tokenizer hashes for cache invalidation on tokenizer changes
/// - Tenant ID for multi-tenant isolation (PRD-06)
/// - Worker ID for worker-specific cache keying (PRD-06)
///
/// The canonical bytes of this struct are:
/// 1. Included in `prefix_kv_key_b3` computation
/// 2. Digested (BLAKE3-256) into `model_cache_identity_v2_digest_b3`
/// 3. Committed to run receipts and Merkle bundles
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelCacheIdentityV2 {
    // V1 fields (with enum types for quantization/fusion)
    pub kernel_version_id: String,
    /// Quantization mode (u8 enum for canonical encoding)
    pub quantization_mode: QuantizationMode,
    /// Fusion mode (u8 enum for canonical encoding)
    pub fusion_mode: FusionMode,
    pub build_id: Option<String>,
    pub adapter_dir_hash: Option<B3Hash>,

    // V2 additions: tokenizer binding
    /// BLAKE3 hash of tokenizer.json (vocabulary + merges)
    pub tokenizer_hash_b3: B3Hash,
    /// BLAKE3 hash of tokenizer_config.json (special tokens, template)
    pub tokenizer_cfg_hash_b3: B3Hash,

    // PRD-06 additions: tenant and worker identity
    /// Tenant ID for multi-tenant isolation (required, fail closed if missing)
    pub tenant_id: String,
    /// Worker ID for worker-specific cache keying
    pub worker_id: u32,
}

/// Constant for missing build_id (PRD-06 edge case handling)
pub const BUILD_ID_UNKNOWN: &str = "unknown";

impl ModelCacheIdentityV2 {
    /// Schema version for canonical bytes format (PRD-06)
    pub const SCHEMA_VERSION: u8 = 2;

    /// Create a new V2 identity with all fields
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        kernel_version_id: String,
        quantization_mode: QuantizationMode,
        fusion_mode: FusionMode,
        build_id: Option<String>,
        adapter_dir_hash: Option<B3Hash>,
        tokenizer_hash_b3: B3Hash,
        tokenizer_cfg_hash_b3: B3Hash,
        tenant_id: String,
        worker_id: u32,
    ) -> Self {
        Self {
            kernel_version_id,
            quantization_mode,
            fusion_mode,
            build_id,
            adapter_dir_hash,
            tokenizer_hash_b3,
            tokenizer_cfg_hash_b3,
            tenant_id,
            worker_id,
        }
    }

    /// Create V2 from legacy V1 identity + new fields (PRD-06 migration helper)
    pub fn from_legacy(
        v1: ModelCacheIdentity,
        tokenizer_hash_b3: B3Hash,
        tokenizer_cfg_hash_b3: B3Hash,
        tenant_id: String,
        worker_id: u32,
    ) -> Self {
        Self {
            kernel_version_id: v1.kernel_version_id,
            quantization_mode: v1
                .quantization_mode
                .parse()
                .unwrap_or(QuantizationMode::Custom),
            fusion_mode: v1.fusion_mode.parse().unwrap_or(FusionMode::Custom),
            build_id: v1.build_id,
            adapter_dir_hash: v1.adapter_dir_hash,
            tokenizer_hash_b3,
            tokenizer_cfg_hash_b3,
            tenant_id,
            worker_id,
        }
    }

    /// Create a V2 identity from backend type with tokenizer hashes
    pub fn for_backend_with_tokenizer(
        backend_type: BackendType,
        tokenizer_hash_b3: B3Hash,
        tokenizer_cfg_hash_b3: B3Hash,
        tenant_id: String,
        worker_id: u32,
    ) -> Self {
        Self {
            kernel_version_id: adapteros_core::version::VERSION.to_string(),
            quantization_mode: QuantizationMode::for_backend(backend_type),
            fusion_mode: FusionMode::default_mode(),
            build_id: Some(adapteros_core::version::VERSION.to_string()),
            adapter_dir_hash: None,
            tokenizer_hash_b3,
            tokenizer_cfg_hash_b3,
            tenant_id,
            worker_id,
        }
    }

    /// Validate identity is complete (PRD-06 strict mode).
    ///
    /// Returns error if any required field is missing or invalid.
    /// Edge cases from PRD Section 8:
    /// - Missing kernel_version_id: fail closed
    /// - Missing tenant_id: fail closed
    /// - Missing build_id: use BUILD_ID_UNKNOWN constant
    pub fn validate_strict(&self) -> Result<()> {
        if self.kernel_version_id.is_empty() {
            return Err(AosError::Validation(
                "ModelCacheIdentityV2: kernel_version_id is required".to_string(),
            ));
        }
        if self.tenant_id.is_empty() {
            return Err(AosError::Validation(
                "ModelCacheIdentityV2: tenant_id is required".to_string(),
            ));
        }
        Ok(())
    }

    /// Get the effective build_id (uses BUILD_ID_UNKNOWN if None)
    pub fn effective_build_id(&self) -> &str {
        self.build_id.as_deref().unwrap_or(BUILD_ID_UNKNOWN)
    }

    /// Serialize to canonical bytes for hashing (PRD-06).
    ///
    /// Format (VERSIONED, all integers little-endian):
    /// ```text
    /// byte 0: SCHEMA_VERSION (0x02)
    /// bytes 1+:
    ///   - kernel_version_id: u32 len + UTF-8 bytes
    ///   - quantization_mode: u8 enum value
    ///   - fusion_mode: u8 enum value
    ///   - build_id: u8 present + (u32 len + UTF-8 bytes if present)
    ///   - adapter_dir_hash: u8 present + (32 bytes if present)
    ///   - tokenizer_hash_b3: 32 bytes
    ///   - tokenizer_cfg_hash_b3: 32 bytes
    ///   - tenant_id: u32 len + UTF-8 bytes (PRD-06)
    ///   - worker_id: u32 little-endian (PRD-06)
    /// ```
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(256);

        // Schema version (PRD-06)
        buf.push(Self::SCHEMA_VERSION);

        // kernel_version_id
        buf.extend_from_slice(&(self.kernel_version_id.len() as u32).to_le_bytes());
        buf.extend_from_slice(self.kernel_version_id.as_bytes());

        // quantization_mode (u8 enum)
        buf.push(self.quantization_mode.to_u8());

        // fusion_mode (u8 enum)
        buf.push(self.fusion_mode.to_u8());

        // build_id (optional)
        if let Some(ref build_id) = self.build_id {
            buf.push(1u8);
            buf.extend_from_slice(&(build_id.len() as u32).to_le_bytes());
            buf.extend_from_slice(build_id.as_bytes());
        } else {
            buf.push(0u8);
        }

        // adapter_dir_hash (optional)
        if let Some(ref hash) = self.adapter_dir_hash {
            buf.push(1u8);
            buf.extend_from_slice(hash.as_bytes());
        } else {
            buf.push(0u8);
        }

        // tokenizer_hash_b3 (required)
        buf.extend_from_slice(self.tokenizer_hash_b3.as_bytes());

        // tokenizer_cfg_hash_b3 (required)
        buf.extend_from_slice(self.tokenizer_cfg_hash_b3.as_bytes());

        // tenant_id (PRD-06, required)
        buf.extend_from_slice(&(self.tenant_id.len() as u32).to_le_bytes());
        buf.extend_from_slice(self.tenant_id.as_bytes());

        // worker_id (PRD-06)
        buf.extend_from_slice(&self.worker_id.to_le_bytes());

        buf
    }

    /// Compute BLAKE3-256 digest of canonical bytes (PRD-06)
    pub fn digest(&self) -> B3Hash {
        B3Hash::hash(&self.canonical_bytes())
    }
}

impl std::fmt::Display for ModelCacheIdentityV2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "v2:{}:{}:{}:tok:{}:cfg:{}:t:{}:w:{}",
            self.kernel_version_id,
            self.quantization_mode,
            self.fusion_mode,
            &self.tokenizer_hash_b3.to_hex()[..12],
            &self.tokenizer_cfg_hash_b3.to_hex()[..12],
            &self.tenant_id,
            self.worker_id,
        )
    }
}

#[cfg(test)]
mod tests_v2 {
    use super::*;

    fn test_v2_identity() -> ModelCacheIdentityV2 {
        let tok_hash = B3Hash::hash(b"tokenizer.json");
        let cfg_hash = B3Hash::hash(b"tokenizer_config.json");
        ModelCacheIdentityV2::for_backend_with_tokenizer(
            BackendType::Mlx,
            tok_hash,
            cfg_hash,
            "tenant-123".to_string(),
            42,
        )
    }

    #[test]
    fn test_v2_canonical_bytes_deterministic() {
        let v2 = test_v2_identity();

        let bytes1 = v2.canonical_bytes();
        let bytes2 = v2.canonical_bytes();

        assert_eq!(bytes1, bytes2, "canonical bytes must be deterministic");
    }

    #[test]
    fn test_identity_v2_digest_stable() {
        // PRD-06 Acceptance Test: test_identity_v2_digest_stable
        let v2a = test_v2_identity();
        let v2b = test_v2_identity();

        assert_eq!(
            v2a.digest(),
            v2b.digest(),
            "same inputs must produce same digest"
        );

        // Multiple calls on same instance
        let digest1 = v2a.digest();
        let digest2 = v2a.digest();
        assert_eq!(digest1, digest2, "digest must be stable across calls");
    }

    #[test]
    fn test_cache_hit_requires_identity_v2_match() {
        // PRD-06 Acceptance Test: test_cache_hit_requires_identity_v2_match
        let tok_hash = B3Hash::hash(b"tokenizer.json");
        let cfg_hash = B3Hash::hash(b"tokenizer_config.json");

        let v2a = ModelCacheIdentityV2::for_backend_with_tokenizer(
            BackendType::Mlx,
            tok_hash,
            cfg_hash,
            "tenant-123".to_string(),
            42,
        );

        // Different tenant
        let v2_different_tenant = ModelCacheIdentityV2::for_backend_with_tokenizer(
            BackendType::Mlx,
            tok_hash,
            cfg_hash,
            "tenant-456".to_string(),
            42,
        );
        assert_ne!(
            v2a.digest(),
            v2_different_tenant.digest(),
            "different tenant must produce different digest"
        );

        // Different worker
        let v2_different_worker = ModelCacheIdentityV2::for_backend_with_tokenizer(
            BackendType::Mlx,
            tok_hash,
            cfg_hash,
            "tenant-123".to_string(),
            99,
        );
        assert_ne!(
            v2a.digest(),
            v2_different_worker.digest(),
            "different worker must produce different digest"
        );
    }

    #[test]
    fn test_v2_different_tokenizer_different_digest() {
        let tok_hash1 = B3Hash::hash(b"tokenizer_v1.json");
        let tok_hash2 = B3Hash::hash(b"tokenizer_v2.json");
        let cfg_hash = B3Hash::hash(b"tokenizer_config.json");

        let v2a = ModelCacheIdentityV2::for_backend_with_tokenizer(
            BackendType::Mlx,
            tok_hash1,
            cfg_hash,
            "tenant-123".to_string(),
            42,
        );

        let v2b = ModelCacheIdentityV2::for_backend_with_tokenizer(
            BackendType::Mlx,
            tok_hash2,
            cfg_hash,
            "tenant-123".to_string(),
            42,
        );

        assert_ne!(
            v2a.digest(),
            v2b.digest(),
            "different tokenizer hashes must produce different digests"
        );
    }

    #[test]
    fn test_v2_display_format() {
        let v2 = test_v2_identity();

        let display = format!("{}", v2);
        assert!(
            display.starts_with("v2:"),
            "display should start with version"
        );
        assert!(
            display.contains("tok:"),
            "display should include tokenizer marker"
        );
        assert!(
            display.contains("cfg:"),
            "display should include config marker"
        );
        assert!(
            display.contains("t:tenant-123"),
            "display should include tenant"
        );
        assert!(display.contains("w:42"), "display should include worker");
    }

    #[test]
    fn test_v2_schema_version_in_canonical_bytes() {
        let v2 = test_v2_identity();
        let bytes = v2.canonical_bytes();

        assert_eq!(
            bytes[0],
            ModelCacheIdentityV2::SCHEMA_VERSION,
            "first byte must be schema version"
        );
        assert_eq!(bytes[0], 2, "schema version must be 2 for PRD-06");
    }

    #[test]
    fn test_v2_validate_strict_success() {
        let v2 = test_v2_identity();
        assert!(
            v2.validate_strict().is_ok(),
            "valid identity should pass strict validation"
        );
    }

    #[test]
    fn test_v2_validate_strict_missing_kernel_version() {
        let tok_hash = B3Hash::hash(b"tokenizer.json");
        let cfg_hash = B3Hash::hash(b"tokenizer_config.json");

        let v2 = ModelCacheIdentityV2::new(
            "".to_string(), // Empty kernel version
            QuantizationMode::Fp16Bf16,
            FusionMode::PerRequest,
            None,
            None,
            tok_hash,
            cfg_hash,
            "tenant-123".to_string(),
            42,
        );

        let result = v2.validate_strict();
        assert!(
            result.is_err(),
            "missing kernel_version_id should fail validation"
        );
    }

    #[test]
    fn test_v2_validate_strict_missing_tenant_id() {
        let tok_hash = B3Hash::hash(b"tokenizer.json");
        let cfg_hash = B3Hash::hash(b"tokenizer_config.json");

        let v2 = ModelCacheIdentityV2::new(
            "1.0.0".to_string(),
            QuantizationMode::Fp16Bf16,
            FusionMode::PerRequest,
            None,
            None,
            tok_hash,
            cfg_hash,
            "".to_string(), // Empty tenant ID
            42,
        );

        let result = v2.validate_strict();
        assert!(result.is_err(), "missing tenant_id should fail validation");
    }

    #[test]
    fn test_v2_effective_build_id() {
        let tok_hash = B3Hash::hash(b"tokenizer.json");
        let cfg_hash = B3Hash::hash(b"tokenizer_config.json");

        // With build_id
        let v2_with_build = ModelCacheIdentityV2::new(
            "1.0.0".to_string(),
            QuantizationMode::Fp16Bf16,
            FusionMode::PerRequest,
            Some("build-abc".to_string()),
            None,
            tok_hash,
            cfg_hash,
            "tenant-123".to_string(),
            42,
        );
        assert_eq!(v2_with_build.effective_build_id(), "build-abc");

        // Without build_id
        let v2_no_build = ModelCacheIdentityV2::new(
            "1.0.0".to_string(),
            QuantizationMode::Fp16Bf16,
            FusionMode::PerRequest,
            None,
            None,
            tok_hash,
            cfg_hash,
            "tenant-123".to_string(),
            42,
        );
        assert_eq!(v2_no_build.effective_build_id(), BUILD_ID_UNKNOWN);
    }

    #[test]
    fn test_enum_from_string_roundtrip() {
        // QuantizationMode roundtrip
        let quant_str = "fp16_bf16_backend";
        let quant: QuantizationMode = quant_str.parse().unwrap();
        assert_eq!(quant, QuantizationMode::Fp16Bf16);
        assert_eq!(quant.as_str(), quant_str);

        // FusionMode roundtrip
        let fusion_str = "per_request";
        let fusion: FusionMode = fusion_str.parse().unwrap();
        assert_eq!(fusion, FusionMode::PerRequest);
        assert_eq!(fusion.as_str(), fusion_str);
    }

    #[test]
    fn test_enum_u8_roundtrip() {
        // QuantizationMode
        for mode in [
            QuantizationMode::Fp16Bf16,
            QuantizationMode::ModelQuantized,
            QuantizationMode::Q4,
            QuantizationMode::Q8,
            QuantizationMode::Mock,
        ] {
            let u8_val = mode.to_u8();
            let roundtrip = QuantizationMode::from_u8(u8_val);
            assert_eq!(
                mode, roundtrip,
                "QuantizationMode u8 roundtrip failed for {:?}",
                mode
            );
        }

        // FusionMode
        for mode in [
            FusionMode::PerRequest,
            FusionMode::PerToken,
            FusionMode::PerSegment,
        ] {
            let u8_val = mode.to_u8();
            let roundtrip = FusionMode::from_u8(u8_val);
            assert_eq!(
                mode, roundtrip,
                "FusionMode u8 roundtrip failed for {:?}",
                mode
            );
        }
    }

    #[test]
    fn test_from_legacy() {
        let tok_hash = B3Hash::hash(b"tokenizer.json");
        let cfg_hash = B3Hash::hash(b"tokenizer_config.json");

        let v1 = ModelCacheIdentity::new("1.0.0", "fp16_bf16_backend", "per_request");

        let v2 =
            ModelCacheIdentityV2::from_legacy(v1, tok_hash, cfg_hash, "tenant-123".to_string(), 42);

        assert_eq!(v2.kernel_version_id, "1.0.0");
        assert_eq!(v2.quantization_mode, QuantizationMode::Fp16Bf16);
        assert_eq!(v2.fusion_mode, FusionMode::PerRequest);
        assert_eq!(v2.tenant_id, "tenant-123");
        assert_eq!(v2.worker_id, 42);
    }
}

// =============================================================================
// Proptest for V2 Identity Canonical Bytes (PRD-06)
// =============================================================================

#[cfg(test)]
mod proptest_v2 {
    use super::*;
    use proptest::prelude::*;

    prop_compose! {
        fn v2_identity_strategy()(
            kernel_version in "[a-z0-9\\-\\.]{1,20}".prop_map(|s| s.to_string()),
            quant_mode in prop_oneof![
                Just(QuantizationMode::Fp16Bf16),
                Just(QuantizationMode::ModelQuantized),
                Just(QuantizationMode::Q4),
                Just(QuantizationMode::Q8),
                Just(QuantizationMode::Mock),
            ],
            fusion_mode in prop_oneof![
                Just(FusionMode::PerRequest),
                Just(FusionMode::PerToken),
                Just(FusionMode::PerSegment),
            ],
            build_id in prop::option::of("[a-z0-9\\-]{1,16}".prop_map(|s| s.to_string())),
            adapter_hash_bytes in any::<[u8; 32]>(),
            has_adapter_hash in any::<bool>(),
            tokenizer_hash_bytes in any::<[u8; 32]>(),
            tokenizer_cfg_bytes in any::<[u8; 32]>(),
            tenant_id in "[a-z0-9\\-]{1,32}".prop_map(|s| s.to_string()),
            worker_id in any::<u32>(),
        ) -> ModelCacheIdentityV2 {
            ModelCacheIdentityV2 {
                kernel_version_id: kernel_version,
                quantization_mode: quant_mode,
                fusion_mode,
                build_id,
                adapter_dir_hash: if has_adapter_hash {
                    Some(B3Hash::from_bytes(adapter_hash_bytes))
                } else {
                    None
                },
                tokenizer_hash_b3: B3Hash::from_bytes(tokenizer_hash_bytes),
                tokenizer_cfg_hash_b3: B3Hash::from_bytes(tokenizer_cfg_bytes),
                tenant_id,
                worker_id,
            }
        }
    }

    proptest! {
        /// PRD-06: Verify canonical_bytes() is deterministic across multiple calls
        #[test]
        fn v2_canonical_bytes_stable(identity in v2_identity_strategy()) {
            let bytes_a = identity.canonical_bytes();
            let bytes_b = identity.canonical_bytes();
            prop_assert_eq!(&bytes_a, &bytes_b, "canonical_bytes must be deterministic");
        }

        /// PRD-06: Verify digest() is deterministic and matches canonical bytes hash
        #[test]
        fn v2_digest_matches_canonical_bytes_hash(identity in v2_identity_strategy()) {
            let digest = identity.digest();
            let expected = B3Hash::hash(&identity.canonical_bytes());
            prop_assert_eq!(digest, expected, "digest must equal hash of canonical_bytes");
        }

        /// PRD-06: Verify schema version is always byte 0
        #[test]
        fn v2_schema_version_is_first_byte(identity in v2_identity_strategy()) {
            let bytes = identity.canonical_bytes();
            prop_assert!(!bytes.is_empty(), "canonical_bytes must not be empty");
            prop_assert_eq!(
                bytes[0],
                ModelCacheIdentityV2::SCHEMA_VERSION,
                "first byte must be schema version"
            );
        }

        /// PRD-06: Different identities produce different digests
        #[test]
        fn v2_different_inputs_different_digests(
            identity_a in v2_identity_strategy(),
            identity_b in v2_identity_strategy()
        ) {
            // Only assert difference if the identities are actually different
            if identity_a != identity_b {
                prop_assert_ne!(
                    identity_a.digest(),
                    identity_b.digest(),
                    "different identities must produce different digests"
                );
            }
        }
    }
}
