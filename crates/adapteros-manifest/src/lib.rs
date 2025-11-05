//! Manifest schema and validation for AdapterOS
//!
//! This crate provides:
//! - Manifest V3 schema definition
//! - Adapter configuration structures
//! - Policy configuration types
//! - Validation and serialization support
//!
//! # Examples
//!
//! ```rust
//! use adapteros_manifest::{ManifestV3, Adapter, AdapterTier};
//! use adapteros_core::B3Hash;
//!
//! // Create a manifest
//! let manifest = ManifestV3 {
//!     schema: "adapteros.manifest.v3".to_string(),
//!     base: Base { /* ... */ },
//!     adapters: vec![Adapter {
//!         id: "test-adapter".to_string(),
//!         hash: B3Hash::hash(b"test"),
//!         tier: AdapterTier::Persistent,
//!         rank: 16,
//!         alpha: 32.0,
//!         target_modules: vec!["q_proj".to_string()],
//!         ttl: None,
//!         acl: vec![],
//!         warmup_prompt: None,
//!         dependencies: None,
//!     }],
//!     router: RouterCfg { /* ... */ },
//!     telemetry: TelemetryCfg { /* ... */ },
//!     policies: Policies { /* ... */ },
//!     seeds: Seeds { /* ... */ },
//! };
//!
//! // Validate the manifest
//! manifest.validate()?;
//! ```

use adapteros_core::{AosError, B3Hash, Result, CPID};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// RoPE scaling configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RopeScaling {
    /// Scaling factor
    pub factor: f32,

    /// Original max position embeddings
    pub original_max_position_embeddings: u32,

    /// Scaling type (e.g., "yarn")
    pub scaling_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub struct ManifestV3 {
    pub schema: String,
    pub base: Base,
    pub adapters: Vec<Adapter>,
    pub router: RouterCfg,
    pub telemetry: TelemetryCfg,
    pub policies: Policies,
    pub seeds: Seeds,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Base {
    pub model_id: String,
    pub model_hash: B3Hash,
    pub arch: String,
    pub vocab_size: u32,
    pub hidden_dim: u32,
    pub n_layers: u32,
    pub n_heads: u32,

    /// Config file hash
    pub config_hash: B3Hash,

    /// Tokenizer file hash
    pub tokenizer_hash: B3Hash,

    /// Tokenizer config file hash
    pub tokenizer_cfg_hash: B3Hash,

    /// License file hash (optional)
    pub license_hash: Option<B3Hash>,

    /// RoPE scaling override (optional)
    pub rope_scaling_override: Option<RopeScaling>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Adapter {
    pub id: String,
    pub hash: B3Hash,
    pub tier: AdapterTier,
    pub rank: u32,
    pub alpha: f32,
    pub target_modules: Vec<String>,
    #[serde(default)]
    pub ttl: Option<u32>,
    #[serde(default)]
    pub acl: Vec<String>,
    #[serde(default)]
    pub warmup_prompt: Option<String>,
    #[serde(default)]
    pub dependencies: Option<AdapterDependencies>,

    // Code intelligence fields
    #[serde(default = "default_category")]
    pub category: AdapterCategory,
    #[serde(default = "default_scope")]
    pub scope: AdapterScope,
    #[serde(default)]
    pub framework_id: Option<String>,
    #[serde(default)]
    pub framework_version: Option<String>,
    #[serde(default)]
    pub repo_id: Option<String>,
    #[serde(default)]
    pub commit_sha: Option<String>,
    #[serde(default)]
    pub intent: Option<String>,

    // State management hints
    #[serde(default = "default_auto_promote")]
    pub auto_promote: bool,
    #[serde(default = "default_eviction_priority")]
    pub eviction_priority: EvictionPriority,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AdapterCategory {
    Code,
    Framework,
    Codebase,
    Ephemeral,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AdapterScope {
    Global,
    Tenant,
    Repo,
    Commit,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum EvictionPriority {
    Never,
    Low,
    Normal,
    High,
    Critical,
}

fn default_category() -> AdapterCategory {
    AdapterCategory::Code
}

fn default_scope() -> AdapterScope {
    AdapterScope::Global
}

fn default_auto_promote() -> bool {
    true
}

fn default_eviction_priority() -> EvictionPriority {
    EvictionPriority::Normal
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct AdapterDependencies {
    pub base_model: Option<String>,
    #[serde(default)]
    pub requires_adapters: Vec<String>,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AdapterTier {
    Persistent,
    Ephemeral,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RouterCfg {
    pub k_sparse: usize,
    pub gate_quant: String, // "q15"
    pub entropy_floor: f32,
    pub tau: f32,
    pub sample_tokens_full: usize,
    #[serde(default)]
    pub warmup: bool,
    #[serde(default = "default_algorithm")]
    pub algorithm: String, // "weighted" | "entropy_floor" | "plugin:<name>"

    // MPLoRA enhancements
    // Reference: https://openreview.net/pdf?id=jqz6Msm3AF
    #[serde(default = "default_orthogonal_penalty")]
    pub orthogonal_penalty: f32, // Default: 0.1
    #[serde(default)]
    pub shared_downsample: bool, // Default: false
    #[serde(default = "default_compression_ratio")]
    pub compression_ratio: f32, // Default: 0.8
    #[serde(default)]
    pub multi_path_enabled: bool, // Default: false
    #[serde(default = "default_diversity_threshold")]
    pub diversity_threshold: f32, // Default: 0.05
    #[serde(default)]
    pub orthogonal_constraints: bool, // Default: false
}

fn default_algorithm() -> String {
    "weighted".to_string()
}

// MPLoRA default functions
fn default_orthogonal_penalty() -> f32 {
    0.1
}

fn default_compression_ratio() -> f32 {
    0.8
}

fn default_diversity_threshold() -> f32 {
    0.05
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct TelemetryCfg {
    pub schema_hash: B3Hash,
    pub sampling: Sampling,
    pub router_full_tokens: usize,
    pub bundle: BundleCfg,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Sampling {
    pub token: f32,
    pub router: f32,
    pub inference: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct BundleCfg {
    pub max_events: usize,
    pub max_bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DriftPolicy {
    /// OS build tolerance (0 = exact match required)
    pub os_build_tolerance: u8,
    /// GPU driver tolerance (0 = exact match required)
    pub gpu_driver_tolerance: u8,
    /// Environment hash tolerance (0 = exact match required)
    pub env_hash_tolerance: u8,
    /// Allow drift warnings without blocking
    pub allow_warnings: bool,
    /// Block inference on critical drift
    pub block_on_critical: bool,
}

impl Default for DriftPolicy {
    fn default() -> Self {
        Self {
            os_build_tolerance: 0,
            gpu_driver_tolerance: 0,
            env_hash_tolerance: 0,
            allow_warnings: true,
            block_on_critical: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Policies {
    pub egress: EgressPolicy,
    pub determinism: DeterminismPolicy,
    pub evidence: EvidencePolicy,
    pub refusal: RefusalPolicy,
    pub numeric: NumericPolicy,
    pub rag: RagPolicy,
    pub isolation: IsolationPolicy,
    pub performance: PerformancePolicy,
    pub memory: MemoryPolicy,
    pub artifacts: ArtifactsPolicy,
    pub drift: DriftPolicy,
    pub lazy_loading: LazyLoadingPolicy,
}

impl Default for Policies {
    fn default() -> Self {
        Self {
            egress: EgressPolicy {
                mode: "deny_all".into(),
                serve_requires_pf: true,
                allow_tcp: false,
                allow_udp: false,
                uds_paths: vec!["/var/run/aos/<tenant>/*.sock".into()],
            },
            determinism: DeterminismPolicy {
                require_metallib_embed: true,
                require_kernel_hash_match: true,
                rng: "hkdf_seeded".into(),
                retrieval_tie_break: vec!["score_desc".into(), "doc_id_asc".into()],
            },
            evidence: EvidencePolicy {
                require_open_book: true,
                min_spans: 1,
                prefer_latest_revision: true,
                warn_on_superseded: true,
            },
            refusal: RefusalPolicy {
                abstain_threshold: 0.55,
                missing_fields_templates: HashMap::new(),
            },
            numeric: NumericPolicy {
                canonical_units: [
                    ("torque".into(), "in_lbf".into()),
                    ("pressure".into(), "psi".into()),
                ]
                .into_iter()
                .collect(),
                max_rounding_error: 0.5,
                require_units_in_trace: true,
            },
            rag: RagPolicy {
                index_scope: "per_tenant".into(),
                doc_tags_required: vec![
                    "doc_id".into(),
                    "rev".into(),
                    "effectivity".into(),
                    "source_type".into(),
                ],
                embedding_model_hash: B3Hash::hash(b"embedding"),
                topk: 5,
                order: vec!["score_desc".into(), "doc_id_asc".into()],
            },
            isolation: IsolationPolicy {
                process_model: "per_tenant".into(),
                uds_root: "/var/run/aos/<tenant>".into(),
                forbid_shm: true,
            },
            performance: PerformancePolicy {
                latency_p95_ms: 24,
                router_overhead_pct_max: 8,
                throughput_tokens_per_s_min: 40,
            },
            memory: MemoryPolicy {
                min_headroom_pct: 15,
                evict_order: vec!["ephemeral_ttl".into(), "cold_lru".into(), "warm_lru".into()],
                k_reduce_before_evict: true,
            },
            artifacts: ArtifactsPolicy {
                require_signature: true,
                require_sbom: true,
                cas_only: true,
            },
            drift: DriftPolicy::default(),
            lazy_loading: LazyLoadingPolicy {
                enabled: true,
                max_load_time_secs: 30,
                max_concurrent_loads: 3,
                preload_related: false,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EgressPolicy {
    pub mode: String,
    pub serve_requires_pf: bool,
    pub allow_tcp: bool,
    pub allow_udp: bool,
    pub uds_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct DeterminismPolicy {
    pub require_metallib_embed: bool,
    pub require_kernel_hash_match: bool,
    pub rng: String,
    pub retrieval_tie_break: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EvidencePolicy {
    pub require_open_book: bool,
    pub min_spans: usize,
    pub prefer_latest_revision: bool,
    pub warn_on_superseded: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RefusalPolicy {
    pub abstain_threshold: f32,
    pub missing_fields_templates: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NumericPolicy {
    pub canonical_units: HashMap<String, String>,
    pub max_rounding_error: f32,
    pub require_units_in_trace: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct RagPolicy {
    pub index_scope: String,
    pub doc_tags_required: Vec<String>,
    pub embedding_model_hash: B3Hash,
    pub topk: usize,
    pub order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct IsolationPolicy {
    pub process_model: String,
    pub uds_root: String,
    pub forbid_shm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct PerformancePolicy {
    pub latency_p95_ms: u32,
    pub router_overhead_pct_max: u8,
    pub throughput_tokens_per_s_min: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct MemoryPolicy {
    pub min_headroom_pct: u8,
    pub evict_order: Vec<String>,
    pub k_reduce_before_evict: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct ArtifactsPolicy {
    pub require_signature: bool,
    pub require_sbom: bool,
    pub cas_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct LazyLoadingPolicy {
    /// Enable lazy loading of adapters on first request
    pub enabled: bool,
    /// Maximum time to wait for lazy loading (seconds)
    pub max_load_time_secs: u64,
    /// Maximum concurrent adapter loads
    pub max_concurrent_loads: usize,
    /// Whether to preload adapters after first lazy load
    pub preload_related: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Seeds {
    pub global: B3Hash,
    pub manifest_hash: B3Hash,
    pub parent_cpid: Option<CPID>,
}

impl ManifestV3 {
    /// Parse manifest from JSON
    pub fn from_json(json: &str) -> Result<Self> {
        serde_json::from_str(json)
            .map_err(|e| AosError::InvalidManifest(format!("Parse error: {}", e)))
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self)
            .map_err(|e| AosError::InvalidManifest(format!("Serialize error: {}", e)))
    }

    /// Validate manifest constraints
    pub fn validate(&self) -> Result<()> {
        // Schema version check
        if self.schema != "adapteros.manifest.v3" {
            return Err(AosError::InvalidManifest(format!(
                "Unknown schema: {}",
                self.schema
            )));
        }

        // Router constraints
        if self.router.k_sparse == 0 || self.router.k_sparse > 8 {
            return Err(AosError::InvalidManifest(
                "k_sparse must be between 1 and 8".into(),
            ));
        }

        if self.router.gate_quant != "q15" {
            return Err(AosError::InvalidManifest(format!(
                "Unknown gate quantization: {}",
                self.router.gate_quant
            )));
        }

        if self.router.entropy_floor < 0.0 || self.router.entropy_floor > 1.0 {
            return Err(AosError::InvalidManifest(
                "entropy_floor must be between 0 and 1".into(),
            ));
        }

        // MPLoRA validation
        // Reference: https://openreview.net/pdf?id=jqz6Msm3AF
        if self.router.orthogonal_penalty < 0.0 || self.router.orthogonal_penalty > 1.0 {
            return Err(AosError::InvalidManifest(
                "orthogonal_penalty must be between 0 and 1".into(),
            ));
        }

        if self.router.compression_ratio <= 0.0 || self.router.compression_ratio > 1.0 {
            return Err(AosError::InvalidManifest(
                "compression_ratio must be between 0 and 1".into(),
            ));
        }

        if self.router.diversity_threshold < 0.0 || self.router.diversity_threshold > 1.0 {
            return Err(AosError::InvalidManifest(
                "diversity_threshold must be between 0 and 1".into(),
            ));
        }

        // Adapter constraints
        for adapter in &self.adapters {
            if adapter.rank == 0 {
                return Err(AosError::InvalidManifest(format!(
                    "Adapter {} has rank 0",
                    adapter.id
                )));
            }
            if adapter.alpha <= 0.0 {
                return Err(AosError::InvalidManifest(format!(
                    "Adapter {} has non-positive alpha",
                    adapter.id
                )));
            }
        }

        // Telemetry constraints
        if self.telemetry.bundle.max_events == 0 {
            return Err(AosError::InvalidManifest(
                "max_events must be positive".into(),
            ));
        }

        Ok(())
    }

    /// Compute manifest hash
    pub fn compute_hash(&self) -> Result<B3Hash> {
        let json = self.to_json()?;
        Ok(B3Hash::hash(json.as_bytes()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_manifest() -> ManifestV3 {
        ManifestV3 {
            schema: "adapteros.manifest.v3".into(),
            base: Base {
                model_id: "test-model".into(),
                model_hash: B3Hash::hash(b"model"),
                arch: "llama".into(),
                vocab_size: 32000,
                hidden_dim: 4096,
                n_layers: 32,
                n_heads: 32,
                config_hash: B3Hash::hash(b"config"),
                tokenizer_hash: B3Hash::hash(b"tokenizer"),
                tokenizer_cfg_hash: B3Hash::hash(b"tokenizer_cfg"),
                license_hash: None,
                rope_scaling_override: None,
            },
            adapters: vec![],
            router: RouterCfg {
                k_sparse: 3,
                gate_quant: "q15".into(),
                entropy_floor: 0.02,
                tau: 1.0,
                sample_tokens_full: 128,
                warmup: false,
                algorithm: "weighted".into(),
                orthogonal_penalty: 0.1,
                shared_downsample: false,
                compression_ratio: 0.8,
                multi_path_enabled: false,
                diversity_threshold: 0.05,
                orthogonal_constraints: false,
            },
            telemetry: TelemetryCfg {
                schema_hash: B3Hash::hash(b"schema"),
                sampling: Sampling {
                    token: 0.05,
                    router: 1.0,
                    inference: 1.0,
                },
                router_full_tokens: 128,
                bundle: BundleCfg {
                    max_events: 500000,
                    max_bytes: 268435456,
                },
            },
            policies: Policies {
                egress: EgressPolicy {
                    mode: "deny_all".into(),
                    serve_requires_pf: true,
                    allow_tcp: false,
                    allow_udp: false,
                    uds_paths: vec!["/var/run/aos/<tenant>/*.sock".into()],
                },
                determinism: DeterminismPolicy {
                    require_metallib_embed: true,
                    require_kernel_hash_match: true,
                    rng: "hkdf_seeded".into(),
                    retrieval_tie_break: vec!["score_desc".into(), "doc_id_asc".into()],
                },
                evidence: EvidencePolicy {
                    require_open_book: true,
                    min_spans: 1,
                    prefer_latest_revision: true,
                    warn_on_superseded: true,
                },
                refusal: RefusalPolicy {
                    abstain_threshold: 0.55,
                    missing_fields_templates: HashMap::new(),
                },
                numeric: NumericPolicy {
                    canonical_units: [
                        ("torque".into(), "in_lbf".into()),
                        ("pressure".into(), "psi".into()),
                    ]
                    .into_iter()
                    .collect(),
                    max_rounding_error: 0.5,
                    require_units_in_trace: true,
                },
                rag: RagPolicy {
                    index_scope: "per_tenant".into(),
                    doc_tags_required: vec![
                        "doc_id".into(),
                        "rev".into(),
                        "effectivity".into(),
                        "source_type".into(),
                    ],
                    embedding_model_hash: B3Hash::hash(b"embedding"),
                    topk: 5,
                    order: vec!["score_desc".into(), "doc_id_asc".into()],
                },
                isolation: IsolationPolicy {
                    process_model: "per_tenant".into(),
                    uds_root: "/var/run/aos/<tenant>".into(),
                    forbid_shm: true,
                },
                performance: PerformancePolicy {
                    latency_p95_ms: 24,
                    router_overhead_pct_max: 8,
                    throughput_tokens_per_s_min: 40,
                },
                memory: MemoryPolicy {
                    min_headroom_pct: 15,
                    evict_order: vec!["ephemeral_ttl".into(), "cold_lru".into(), "warm_lru".into()],
                    k_reduce_before_evict: true,
                },
                artifacts: ArtifactsPolicy {
                    require_signature: true,
                    require_sbom: true,
                    cas_only: true,
                },
                drift: DriftPolicy::default(),
                lazy_loading: LazyLoadingPolicy {
                    enabled: true,
                    max_load_time_secs: 30,
                    max_concurrent_loads: 3,
                    preload_related: false,
                },
            },
            seeds: Seeds {
                global: B3Hash::hash(b"global_seed"),
                manifest_hash: B3Hash::hash(b"manifest"),
                parent_cpid: None,
            },
        }
    }

    #[test]
    fn test_manifest_validation() {
        let manifest = example_manifest();
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_manifest_serialization() {
        let manifest = example_manifest();
        let json = manifest
            .to_json()
            .expect("Test manifest should serialize to JSON");
        let manifest2 =
            ManifestV3::from_json(&json).expect("Test manifest should deserialize from JSON");
        assert_eq!(manifest.schema, manifest2.schema);
    }

    #[test]
    fn test_manifest_hash_deterministic() {
        let manifest = example_manifest();
        let hash1 = manifest
            .compute_hash()
            .expect("Test manifest hash computation should succeed");
        let hash2 = manifest
            .compute_hash()
            .expect("Test manifest hash computation should succeed");
        assert_eq!(hash1, hash2);
    }
}
