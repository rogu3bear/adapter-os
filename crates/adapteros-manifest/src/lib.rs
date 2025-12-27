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
//! ```ignore
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
use adapteros_types::coreml::CoreMLPlacementSpec;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// RoPE scaling configuration
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(rename = "RopeScaling")]
pub struct RoPEScaling {
    /// Scaling factor
    pub factor: f32,

    /// Original max position embeddings
    pub original_max_position_embeddings: u32,

    /// Scaling type (e.g., "yarn")
    pub scaling_type: String,
}

/// Deprecated alias for backwards compatibility
#[deprecated(
    since = "0.12.0",
    note = "Use `RoPEScaling` instead (correct RoPE casing)"
)]
pub type RopeScaling = RoPEScaling;

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
    #[serde(default)]
    pub coreml: Option<CoreMLSection>,
    #[serde(default)]
    pub fusion: Option<CoreMLFusion>,
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
    /// Routing bias to tilt adapter selection for this base model
    #[serde(default = "default_routing_bias")]
    pub routing_bias: f32,

    /// Config file hash
    pub config_hash: B3Hash,

    /// Tokenizer file hash
    pub tokenizer_hash: B3Hash,

    /// Tokenizer config file hash
    pub tokenizer_cfg_hash: B3Hash,

    /// License file hash (optional)
    pub license_hash: Option<B3Hash>,

    /// RoPE scaling override (optional)
    pub rope_scaling_override: Option<RoPEScaling>,
}

fn default_routing_bias() -> f32 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct Adapter {
    pub id: String,
    pub hash: B3Hash,
    /// Assurance tier for drift/determinism enforcement
    #[serde(default = "default_assurance_tier")]
    pub assurance_tier: AssuranceTier,
    pub tier: AdapterTier,
    pub rank: u32,
    pub alpha: f32,
    pub lora_strength: Option<f32>,
    pub target_modules: Vec<String>,
    #[serde(default)]
    pub ttl: Option<u32>,
    #[serde(default)]
    pub acl: Vec<String>,
    #[serde(default)]
    pub warmup_prompt: Option<String>,
    #[serde(default)]
    pub dependencies: Option<AdapterDependencies>,
    /// Determinism/drift metadata produced by harness runs (optional).
    #[serde(default)]
    pub determinism_seed: Option<u64>,
    #[serde(default)]
    pub determinism_backend: Option<String>,
    #[serde(default)]
    pub determinism_device: Option<String>,
    #[serde(default)]
    pub drift_reference_backend: Option<String>,
    #[serde(default)]
    pub drift_metric: Option<f32>,
    /// Backend used as the canonical baseline when computing drift
    #[serde(default)]
    pub drift_baseline_backend: Option<String>,
    /// Backend evaluated against the baseline in the last run
    #[serde(default)]
    pub drift_test_backend: Option<String>,
    /// Assurance tier recorded with the drift run
    #[serde(default)]
    pub drift_tier: Option<AssuranceTier>,
    /// Slice size used during the drift run (if any)
    #[serde(default)]
    pub drift_slice_size: Option<usize>,
    /// Slice offset used during the drift run (if any)
    #[serde(default)]
    pub drift_slice_offset: Option<usize>,
    /// Loss L∞ metric recorded during drift run (optional)
    #[serde(default)]
    pub drift_loss_metric: Option<f32>,

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
    /// Whether this adapter is recommended for MoE base models
    #[serde(default = "default_recommended_for_moe")]
    pub recommended_for_moe: bool,

    // State management hints
    #[serde(default = "default_auto_promote")]
    pub auto_promote: bool,
    #[serde(default = "default_eviction_priority")]
    pub eviction_priority: EvictionPriority,

    // MoE free token optimization hints
    /// Pre-computed free tokens for ultra-low-latency first tokens (MoE models)
    #[serde(default)]
    pub free_tokens: Option<Vec<FreeTokenHint>>,
    /// Hot expert hints for MoE pre-warming (layer_idx -> [expert_ids])
    #[serde(default)]
    pub hot_experts: Option<HashMap<usize, Vec<u8>>>,
}

/// Free token hint for MoE optimization
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct FreeTokenHint {
    /// Trigger pattern (prefix that activates these free tokens)
    pub trigger: String,
    /// Token sequence to emit immediately
    pub tokens: Vec<String>,
    /// Confidence score (0.0-1.0)
    #[serde(default = "default_free_token_confidence")]
    pub confidence: f32,
    /// Maximum temperature at which this hint is valid
    #[serde(default = "default_free_token_max_temp")]
    pub max_temperature: f32,
}

fn default_free_token_confidence() -> f32 {
    0.9
}

fn default_free_token_max_temp() -> f32 {
    0.3
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

fn default_recommended_for_moe() -> bool {
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

fn default_assurance_tier() -> AssuranceTier {
    AssuranceTier::Standard
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AdapterTier {
    Persistent,
    Ephemeral,
}

/// Assurance tier used for drift/determinism gates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum AssuranceTier {
    Low,
    Standard,
    High,
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

    // DIR (Deterministic Inference Runtime) enhancements
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

// DIR default functions
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
                missing_fields_templates: BTreeMap::new(),
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
                max_tokens: 1000,
                cpu_threshold_pct: 90.0,
                memory_threshold_pct: 95.0,
                circuit_breaker_threshold: 5,
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
    pub missing_fields_templates: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct NumericPolicy {
    pub canonical_units: BTreeMap<String, String>,
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
    pub max_tokens: usize,
    pub cpu_threshold_pct: f32,
    pub memory_threshold_pct: f32,
    pub circuit_breaker_threshold: usize,
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
pub struct Seeds {
    pub global: B3Hash,
    pub manifest_hash: B3Hash,
    pub parent_cpid: Option<CPID>,
}

/// CoreML-specific metadata, including LoRA placement for CoreML graphs.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct CoreMLSection {
    #[serde(default)]
    pub placement: Option<CoreMLPlacementSpec>,
}

/// Optional CoreML fusion metadata to bind the fused package to its source artifacts.
///
/// All fields are optional to keep backward compatibility; verification logic should
/// treat the presence of `fused_manifest_hash` as the primary signal.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Default)]
pub struct CoreMLFusion {
    #[serde(default)]
    pub fused_manifest_hash: Option<B3Hash>,
    #[serde(default)]
    pub coreml_package_hash: Option<B3Hash>,
    #[serde(default)]
    pub base_model_id: Option<String>,
    #[serde(default)]
    pub base_model_hash: Option<B3Hash>,
    #[serde(default)]
    pub adapter_id: Option<String>,
    #[serde(default)]
    pub adapter_hash: Option<B3Hash>,
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

        // DIR validation
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

        // CoreML placement constraints
        if let Some(coreml) = &self.coreml {
            if let Some(spec) = &coreml.placement {
                if spec.version == 0 {
                    return Err(AosError::InvalidManifest(
                        "coreml.placement.version must be > 0".into(),
                    ));
                }
                for binding in &spec.bindings {
                    if binding.rank == 0 {
                        return Err(AosError::InvalidManifest(format!(
                            "coreml placement binding {} has rank 0",
                            binding.binding_id
                        )));
                    }
                    if binding.shape.input_dim == 0 || binding.shape.output_dim == 0 {
                        return Err(AosError::InvalidManifest(format!(
                            "coreml placement binding {} has zero-dimension shape",
                            binding.binding_id
                        )));
                    }
                    if binding.target.layer.trim().is_empty() {
                        return Err(AosError::InvalidManifest(
                            "coreml placement binding missing target layer".into(),
                        ));
                    }
                }
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
                routing_bias: 1.0,
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
                    missing_fields_templates: BTreeMap::new(),
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
                    max_tokens: 1000,
                    cpu_threshold_pct: 90.0,
                    memory_threshold_pct: 95.0,
                    circuit_breaker_threshold: 5,
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
            },
            seeds: Seeds {
                global: B3Hash::hash(b"global_seed"),
                manifest_hash: B3Hash::hash(b"manifest"),
                parent_cpid: None,
            },
            coreml: None,
            fusion: None,
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
