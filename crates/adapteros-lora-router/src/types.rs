//! Core types for the router module
//!
//! This module contains type-safe newtypes and enums that replace stringly-typed
//! patterns in the router implementation.

use crate::constants::{
    LORA_TIER_MAX_BOOST, LORA_TIER_MICRO_BOOST, LORA_TIER_STANDARD_BOOST, TIER_0_BOOST,
    TIER_1_BOOST, TIER_2_BOOST,
};
use crate::policy_mask::PolicyOverrideFlags;
use crate::quantization::ROUTER_GATE_Q15_DENOM;
use adapteros_core::{AosError, B3Hash, Result};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use std::str::FromStr;

// =============================================================================
// Feature Vector
// =============================================================================

/// Expected dimensions for standard feature vectors
pub const FEATURE_VECTOR_STANDARD_LEN: usize = 22;

/// Expected dimensions for extended DIR feature vectors
pub const FEATURE_VECTOR_EXTENDED_LEN: usize = 25;

/// Feature vector layout indices
pub mod feature_indices {
    /// Language one-hot encoding (indices 0-7, 8 dimensions)
    pub const LANGUAGE_START: usize = 0;
    pub const LANGUAGE_END: usize = 8;

    /// Framework scores (indices 8-10, 3 dimensions)
    pub const FRAMEWORK_START: usize = 8;
    pub const FRAMEWORK_END: usize = 11;

    /// Symbol hits (index 11, 1 dimension)
    pub const SYMBOL_HITS: usize = 11;

    /// Path tokens (index 12, 1 dimension)
    pub const PATH_TOKENS: usize = 12;

    /// Prompt verb one-hot encoding (indices 13-20, 8 dimensions)
    pub const PROMPT_VERB_START: usize = 13;
    pub const PROMPT_VERB_END: usize = 21;

    /// Attention entropy (index 21, 1 dimension)
    pub const ATTN_ENTROPY: usize = 21;

    // Extended DIR features (indices 22-24)
    /// Orthogonal penalty (index 22)
    pub const ORTHOGONAL_PENALTY: usize = 22;
    /// Adapter diversity (index 23)
    pub const ADAPTER_DIVERSITY: usize = 23;
    /// Path similarity (index 24)
    pub const PATH_SIMILARITY: usize = 24;
}

/// Error type for feature vector validation
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeatureVectorError {
    /// Feature vector has invalid length
    InvalidLength { expected: &'static str, got: usize },
}

impl std::fmt::Display for FeatureVectorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FeatureVectorError::InvalidLength { expected, got } => {
                write!(
                    f,
                    "Invalid feature vector length: expected {}, got {}",
                    expected, got
                )
            }
        }
    }
}

impl std::error::Error for FeatureVectorError {}

/// Type-safe wrapper for feature vectors
///
/// Provides validated access to feature vector components without
/// magic index arithmetic scattered throughout the codebase.
#[derive(Debug, Clone)]
pub struct FeatureVector {
    data: Vec<f32>,
}

impl FeatureVector {
    /// Create a new feature vector from a slice
    ///
    /// Accepts vectors of length 21, 22, or 25 for backward compatibility.
    ///
    /// # Errors
    /// Returns `FeatureVectorError::InvalidLength` if the slice length is not 21, 22, or 25.
    #[must_use = "this returns a Result that should be checked"]
    pub fn from_slice(slice: &[f32]) -> std::result::Result<Self, FeatureVectorError> {
        match slice.len() {
            21 | 22 | 25 => Ok(Self {
                data: slice.to_vec(),
            }),
            len => Err(FeatureVectorError::InvalidLength {
                expected: "21, 22, or 25",
                got: len,
            }),
        }
    }

    /// Create a zero-initialized feature vector of standard length (22)
    pub fn zeros() -> Self {
        Self {
            data: vec![0.0; FEATURE_VECTOR_STANDARD_LEN],
        }
    }

    /// Create a zero-initialized extended feature vector (25 dimensions)
    pub fn zeros_extended() -> Self {
        Self {
            data: vec![0.0; FEATURE_VECTOR_EXTENDED_LEN],
        }
    }

    /// Get the raw slice
    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    /// Get the length
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get language one-hot encoding (8 dimensions)
    pub fn language(&self) -> &[f32] {
        &self.data[feature_indices::LANGUAGE_START..feature_indices::LANGUAGE_END]
    }

    /// Get the detected language index (max of one-hot)
    pub fn detected_language_idx(&self) -> Option<usize> {
        self.language()
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .filter(|(_, &val)| val > 0.0)
            .map(|(idx, _)| idx)
    }

    /// Get framework scores (3 dimensions)
    pub fn framework(&self) -> &[f32] {
        if self.data.len() >= feature_indices::FRAMEWORK_END {
            &self.data[feature_indices::FRAMEWORK_START..feature_indices::FRAMEWORK_END]
        } else {
            &[]
        }
    }

    /// Get framework strength (sum of framework scores)
    pub fn framework_strength(&self) -> f32 {
        self.framework().iter().sum()
    }

    /// Get symbol hits score
    pub fn symbol_hits(&self) -> f32 {
        if self.data.len() > feature_indices::SYMBOL_HITS {
            self.data[feature_indices::SYMBOL_HITS]
        } else {
            0.0
        }
    }

    /// Get path tokens score
    pub fn path_tokens(&self) -> f32 {
        if self.data.len() > feature_indices::PATH_TOKENS {
            self.data[feature_indices::PATH_TOKENS]
        } else {
            0.0
        }
    }

    /// Get prompt verb one-hot encoding (8 dimensions)
    pub fn prompt_verb(&self) -> &[f32] {
        if self.data.len() >= feature_indices::PROMPT_VERB_END {
            &self.data[feature_indices::PROMPT_VERB_START..feature_indices::PROMPT_VERB_END]
        } else {
            &[]
        }
    }

    /// Get prompt verb strength (max of one-hot)
    pub fn prompt_verb_strength(&self) -> f32 {
        self.prompt_verb().iter().fold(0.0f32, |a, &b| a.max(b))
    }

    /// Get attention entropy
    pub fn attn_entropy(&self) -> f32 {
        if self.data.len() > feature_indices::ATTN_ENTROPY {
            self.data[feature_indices::ATTN_ENTROPY]
        } else {
            0.0
        }
    }

    /// Check if this is an extended (DIR) feature vector
    pub fn is_extended(&self) -> bool {
        self.data.len() == FEATURE_VECTOR_EXTENDED_LEN
    }
}

// =============================================================================
// Adapter Tier
// =============================================================================

/// Adapter tier classification
///
/// Replaces stringly-typed tier matching with exhaustive enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum AdapterTier {
    /// Highest priority tier
    Tier0,
    /// Standard tier
    Tier1,
    /// Lower priority tier
    Tier2,
    /// Default/unspecified tier
    #[default]
    Default,
}

impl AdapterTier {
    /// Get the score boost for this tier
    pub fn boost(&self) -> f32 {
        match self {
            Self::Tier0 => TIER_0_BOOST,
            Self::Tier1 => TIER_1_BOOST,
            Self::Tier2 => TIER_2_BOOST,
            Self::Default => 0.0,
        }
    }

    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tier0 => "tier_0",
            Self::Tier1 => "tier_1",
            Self::Tier2 => "tier_2",
            Self::Default => "default",
        }
    }
}

impl FromStr for AdapterTier {
    type Err = std::convert::Infallible;

    /// Parse tier from string. Unknown values default to `AdapterTier::Default`.
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "tier_0" => Self::Tier0,
            "tier_1" => Self::Tier1,
            "tier_2" => Self::Tier2,
            _ => Self::Default,
        })
    }
}

impl std::fmt::Display for AdapterTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =============================================================================
// LoRA Tier
// =============================================================================

/// LoRA adapter tier classification
///
/// Represents the capacity/capability tier of a LoRA adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoraTier {
    /// Maximum capacity LoRA
    Max,
    /// Standard capacity LoRA
    Standard,
    /// Minimal/micro LoRA
    Micro,
}

/// Error returned when parsing an invalid LoRA tier string
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseLoraTierError {
    /// The invalid input string
    pub input: String,
}

impl std::fmt::Display for ParseLoraTierError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "invalid LoRA tier '{}': expected 'max', 'standard', or 'micro'",
            self.input
        )
    }
}

impl std::error::Error for ParseLoraTierError {}

impl LoraTier {
    /// Get the score boost for this tier
    pub fn boost(&self) -> f32 {
        match self {
            Self::Max => LORA_TIER_MAX_BOOST,
            Self::Standard => LORA_TIER_STANDARD_BOOST,
            Self::Micro => LORA_TIER_MICRO_BOOST,
        }
    }

    /// Convert to string representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Max => "max",
            Self::Standard => "standard",
            Self::Micro => "micro",
        }
    }
}

impl FromStr for LoraTier {
    type Err = ParseLoraTierError;

    /// Parse tier from string.
    ///
    /// # Errors
    /// Returns `ParseLoraTierError` if the string is not "max", "standard", or "micro".
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "max" => Ok(Self::Max),
            "standard" => Ok(Self::Standard),
            "micro" => Ok(Self::Micro),
            _ => Err(ParseLoraTierError {
                input: s.to_string(),
            }),
        }
    }
}

impl std::fmt::Display for LoraTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =============================================================================
// Router Types
// =============================================================================

/// Router determinism configuration
///
/// Controls deterministic floating-point behavior and decision hashing
/// to ensure reproducible routing decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterDeterminismConfig {
    /// Use IEEE 754 deterministic softmax with f64 intermediate precision and Kahan summation
    pub ieee754_deterministic: bool,
    /// Enable decision hashing with BLAKE3 for audit trail
    pub enable_decision_hashing: bool,
}

impl Default for RouterDeterminismConfig {
    fn default() -> Self {
        Self {
            ieee754_deterministic: true,   // Enabled by default for reproducibility
            enable_decision_hashing: true, // Enabled by default for audit trail
        }
    }
}

/// Decision hash for audit and reproducibility verification
///
/// Contains BLAKE3 hash of routing inputs and outputs, along with metadata
/// to enable determinism proofs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionHash {
    /// BLAKE3 hash of input features and priors
    pub input_hash: String,
    /// BLAKE3 hash of output indices and gates
    pub output_hash: String,
    /// Optional hash of reasoning buffer used for dynamic routing
    pub reasoning_hash: Option<String>,
    /// Combined hash of input + output for compact verification
    pub combined_hash: String,
    /// Tau (temperature) used in this decision
    pub tau: f32,
    /// Epsilon (entropy floor) used in this decision
    pub eps: f32,
    /// K (number of selected adapters)
    pub k: usize,
}

/// Router weights for feature importance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouterWeights {
    /// Weight for language detection (0.3 - strong signal)
    pub language_weight: f32,
    /// Weight for framework detection (0.25 - strong signal)
    pub framework_weight: f32,
    /// Weight for symbol hits (0.2 - moderate signal)
    pub symbol_hits_weight: f32,
    /// Weight for path tokens (0.15 - moderate signal)
    pub path_tokens_weight: f32,
    /// Weight for prompt verb (0.1 - weak signal)
    pub prompt_verb_weight: f32,

    // DIR (Deterministic Inference Runtime) additions
    // Reference: https://openreview.net/pdf?id=jqz6Msm3AF
    /// Weight for orthogonal constraints (0.05 - weak signal)
    pub orthogonal_weight: f32,
    /// Weight for adapter diversity (0.03 - weak signal)
    pub diversity_weight: f32,
    /// Weight for similarity penalty (0.02 - weak signal)
    pub similarity_penalty: f32,
}

impl Default for RouterWeights {
    fn default() -> Self {
        Self {
            language_weight: 0.27272728,
            framework_weight: 0.22727273,
            symbol_hits_weight: 0.18181819,
            path_tokens_weight: 0.13636364,
            prompt_verb_weight: 0.09090909,
            orthogonal_weight: 0.04545455,
            diversity_weight: 0.02727273,
            similarity_penalty: 0.01818182,
        }
    }
}

impl RouterWeights {
    /// Create custom weights
    pub fn new(language: f32, framework: f32, symbols: f32, paths: f32, verb: f32) -> Self {
        Self {
            language_weight: language,
            framework_weight: framework,
            symbol_hits_weight: symbols,
            path_tokens_weight: paths,
            prompt_verb_weight: verb,
            orthogonal_weight: 0.04545455,
            diversity_weight: 0.02727273,
            similarity_penalty: 0.01818182,
        }
    }

    /// Create custom weights with DIR (Deterministic Inference Runtime) parameters
    pub fn new_with_dir_weights(
        language: f32,
        framework: f32,
        symbols: f32,
        paths: f32,
        verb: f32,
        orthogonal: f32,
        diversity: f32,
        similarity: f32,
    ) -> Self {
        Self {
            language_weight: language,
            framework_weight: framework,
            symbol_hits_weight: symbols,
            path_tokens_weight: paths,
            prompt_verb_weight: verb,
            orthogonal_weight: orthogonal,
            diversity_weight: diversity,
            similarity_penalty: similarity,
        }
    }

    /// Get total weight (for normalization check)
    pub fn total_weight(&self) -> f32 {
        self.language_weight
            + self.framework_weight
            + self.symbol_hits_weight
            + self.path_tokens_weight
            + self.prompt_verb_weight
            + self.orthogonal_weight
            + self.diversity_weight
            + self.similarity_penalty
    }

    /// Load weights from JSON file
    pub fn load(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let content =
            std::fs::read_to_string(path.as_ref()).map_err(|e| AosError::Io(e.to_string()))?;
        serde_json::from_str(&content).map_err(|e| AosError::Io(e.to_string()))
    }

    /// Load weights from TOML file
    pub fn load_toml(path: impl AsRef<std::path::Path>) -> Result<Self> {
        let content =
            std::fs::read_to_string(path.as_ref()).map_err(|e| AosError::Io(e.to_string()))?;
        toml::from_str(&content).map_err(|e| AosError::Io(e.to_string()))
    }

    /// Save weights to JSON file
    pub fn save(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let content =
            serde_json::to_string_pretty(&self).map_err(|e| AosError::Io(e.to_string()))?;
        std::fs::write(path.as_ref(), content).map_err(|e| AosError::Io(e.to_string()))
    }

    /// Save weights to TOML file
    pub fn save_toml(&self, path: impl AsRef<std::path::Path>) -> Result<()> {
        let content = toml::to_string_pretty(&self).map_err(|e| AosError::Io(e.to_string()))?;
        std::fs::write(path.as_ref(), content).map_err(|e| AosError::Io(e.to_string()))
    }
}

/// Scoring explanation for debugging and audit
#[derive(Debug, Clone)]
pub struct ScoringExplanation {
    pub language_score: f32,
    pub framework_score: f32,
    pub symbol_hits_score: f32,
    pub path_tokens_score: f32,
    pub prompt_verb_score: f32,
    pub total_score: f32,
}

impl ScoringExplanation {
    /// Format as human-readable string
    pub fn format(&self) -> String {
        format!(
            "Scoring Breakdown:\n\
             - Language:     {:.3} (weight: 0.30)\n\
             - Framework:    {:.3} (weight: 0.25)\n\
             - Symbol Hits:  {:.3} (weight: 0.20)\n\
             - Path Tokens:  {:.3} (weight: 0.15)\n\
             - Prompt Verb:  {:.3} (weight: 0.10)\n\
             = Total Score:  {:.3}",
            self.language_score,
            self.framework_score,
            self.symbol_hits_score,
            self.path_tokens_score,
            self.prompt_verb_score,
            self.total_score,
        )
    }
}

/// Adapter information for routing
#[derive(Debug, Clone)]
pub struct AdapterInfo {
    pub id: String,
    /// Stable ID for deterministic tie-breaking across filter/selection operations.
    ///
    /// This ID is assigned at registration time and never changes. Unlike array
    /// indices which shift when adapters are filtered or reordered, stable_id
    /// provides a consistent tie-break key for reproducible routing decisions.
    ///
    /// # Determinism Invariant
    /// When two adapters have equal scores, the one with lower stable_id is chosen.
    /// This ensures identical selection across:
    /// - Policy mask filtering
    /// - Top-k selection
    /// - Adapter hot-swap/reload
    pub stable_id: u64,
    pub framework: Option<String>,
    pub languages: Vec<usize>, // Language indices
    pub tier: String,
    pub scope_path: Option<String>,
    pub lora_tier: Option<String>,
    pub base_model: Option<String>,
    pub recommended_for_moe: bool,
    /// Optional reasoning specialties (e.g., math, logic) for dynamic routing
    pub reasoning_specialties: Vec<String>,
    /// Adapter type: "standard", "codebase", or "core"
    pub adapter_type: Option<String>,
    /// Session binding for codebase adapters (exclusive)
    pub stream_session_id: Option<String>,
    /// Base adapter ID for codebase adapters (the core adapter they extend)
    pub base_adapter_id: Option<String>,
}

impl AdapterInfo {
    /// Check if adapter supports a language
    pub fn supports_language(&self, lang_idx: usize) -> bool {
        self.languages.contains(&lang_idx)
    }

    /// Check if this is a codebase adapter
    pub fn is_codebase_adapter(&self) -> bool {
        self.adapter_type.as_deref() == Some("codebase")
    }

    /// Check if this is a core adapter
    pub fn is_core_adapter(&self) -> bool {
        self.adapter_type.as_deref() == Some("core")
    }

    /// Check if this is a standard adapter (default)
    pub fn is_standard_adapter(&self) -> bool {
        !matches!(
            self.adapter_type.as_deref(),
            Some("codebase") | Some("core")
        )
    }

    /// Check if this adapter is bound to a specific session
    pub fn is_bound_to_session(&self, session_id: &str) -> bool {
        self.stream_session_id.as_deref() == Some(session_id)
    }
}

impl Default for AdapterInfo {
    fn default() -> Self {
        Self {
            id: String::new(),
            stable_id: 0,
            framework: None,
            languages: Vec::new(),
            tier: "default".to_string(),
            scope_path: None,
            lora_tier: None,
            base_model: None,
            recommended_for_moe: true,
            reasoning_specialties: Vec::new(),
            adapter_type: None,
            stream_session_id: None,
            base_adapter_id: None,
        }
    }
}

/// Candidate adapter selected by the router with raw score and gate
#[derive(Debug, Clone)]
pub struct DecisionCandidate {
    pub adapter_idx: u16,
    pub raw_score: f32,
    pub gate_q15: i16,
}

/// Router decision with indices and quantized gates
#[derive(Debug, Clone)]
pub struct Decision {
    pub indices: SmallVec<[u16; 8]>,
    pub gates_q15: SmallVec<[i16; 8]>,
    pub entropy: f32,
    pub candidates: Vec<DecisionCandidate>,
    /// Optional decision hash for audit and reproducibility verification
    pub decision_hash: Option<DecisionHash>,
    /// Digest binding routing policy context to the applied mask (if any).
    pub policy_mask_digest_b3: Option<B3Hash>,
    /// Flags indicating which policy overrides were applied for this decision.
    pub policy_overrides_applied: Option<PolicyOverrideFlags>,
}

impl Decision {
    /// Convert Q15 gates back to float
    pub fn gates_f32(&self) -> Vec<f32> {
        self.gates_q15
            .iter()
            .map(|&q| q as f32 / ROUTER_GATE_Q15_DENOM)
            .collect()
    }

    /// Convert to canonical RouterRing for kernel execution
    pub fn to_router_ring(&self) -> adapteros_lora_kernel_api::RouterRing {
        let k = self.indices.len();
        assert!(k <= 8, "Decision has too many adapters (k={}), max is 8", k);

        let mut ring = adapteros_lora_kernel_api::RouterRing::new(k);
        ring.set(&self.indices[..], &self.gates_q15[..]);
        ring
    }
}

/// Convert Decision to canonical RouterRing for kernel interface
impl From<Decision> for adapteros_lora_kernel_api::RouterRing {
    fn from(decision: Decision) -> Self {
        decision.to_router_ring()
    }
}

/// Convert Decision reference to canonical RouterRing
impl From<&Decision> for adapteros_lora_kernel_api::RouterRing {
    fn from(decision: &Decision) -> Self {
        decision.to_router_ring()
    }
}

/// Router decision that can represent either a concrete selection or an abstain outcome.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum RoutingDecision {
    /// The router selected one or more adapters.
    Selected(Decision),
    /// The router abstained from making a selection.
    Abstain(RouterAbstainReason),
}

/// Reasons the router can abstain from routing.
#[derive(Debug, Clone)]
pub enum RouterAbstainReason {
    /// No adapters are configured or available.
    EmptyRouterConfig,
    /// All computed scores fall below the abstention threshold.
    ScoresBelowThreshold { threshold: f32, max_score: f32 },
    /// Input validation failed (NaN/Inf detected).
    InvalidNumerics(String),
}

impl RoutingDecision {
    /// Access the selected decision if routing succeeded.
    pub fn as_selected(&self) -> Option<&Decision> {
        match self {
            RoutingDecision::Selected(decision) => Some(decision),
            RoutingDecision::Abstain(_) => None,
        }
    }

    /// Consume the routing decision and return the selected decision if present.
    pub fn into_selected(self) -> Option<Decision> {
        match self {
            RoutingDecision::Selected(decision) => Some(decision),
            RoutingDecision::Abstain(_) => None,
        }
    }
}

impl From<Decision> for RoutingDecision {
    fn from(decision: Decision) -> Self {
        RoutingDecision::Selected(decision)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_feature_vector_from_slice() {
        let data = vec![0.0; 22];
        let fv = FeatureVector::from_slice(&data).unwrap();
        assert_eq!(fv.len(), 22);
    }

    #[test]
    fn test_feature_vector_invalid_length() {
        let data = vec![0.0; 10];
        let result = FeatureVector::from_slice(&data);
        assert!(result.is_err());
    }

    #[test]
    fn test_feature_vector_language_detection() {
        let mut data = vec![0.0; 22];
        data[2] = 1.0; // TypeScript
        let fv = FeatureVector::from_slice(&data).unwrap();
        assert_eq!(fv.detected_language_idx(), Some(2));
    }

    #[test]
    fn test_adapter_tier_parsing() {
        assert_eq!("tier_0".parse::<AdapterTier>().unwrap(), AdapterTier::Tier0);
        assert_eq!("tier_1".parse::<AdapterTier>().unwrap(), AdapterTier::Tier1);
        assert_eq!("tier_2".parse::<AdapterTier>().unwrap(), AdapterTier::Tier2);
        assert_eq!(
            "unknown".parse::<AdapterTier>().unwrap(),
            AdapterTier::Default
        );
    }

    #[test]
    fn test_adapter_tier_boost() {
        assert_eq!(AdapterTier::Tier0.boost(), TIER_0_BOOST);
        assert_eq!(AdapterTier::Default.boost(), 0.0);
    }

    #[test]
    fn test_lora_tier_parsing() {
        assert_eq!("max".parse::<LoraTier>().unwrap(), LoraTier::Max);
        assert_eq!("standard".parse::<LoraTier>().unwrap(), LoraTier::Standard);
        assert_eq!("micro".parse::<LoraTier>().unwrap(), LoraTier::Micro);
        assert!("invalid".parse::<LoraTier>().is_err());
    }

    #[test]
    fn test_lora_tier_parse_error() {
        let err = "invalid".parse::<LoraTier>().unwrap_err();
        assert_eq!(err.input, "invalid");
        assert!(err.to_string().contains("invalid LoRA tier"));
    }

    #[test]
    fn test_lora_tier_boost() {
        assert_eq!(LoraTier::Max.boost(), LORA_TIER_MAX_BOOST);
        assert_eq!(LoraTier::Standard.boost(), LORA_TIER_STANDARD_BOOST);
    }

    #[test]
    fn test_codebase_adapter_detection() {
        let mut adapter = AdapterInfo::default();
        assert!(adapter.is_standard_adapter());
        assert!(!adapter.is_codebase_adapter());
        assert!(!adapter.is_core_adapter());

        adapter.adapter_type = Some("codebase".to_string());
        assert!(adapter.is_codebase_adapter());
        assert!(!adapter.is_standard_adapter());

        adapter.adapter_type = Some("core".to_string());
        assert!(adapter.is_core_adapter());
        assert!(!adapter.is_codebase_adapter());
    }

    #[test]
    fn test_codebase_exclusivity_validation() {
        let adapters = vec![
            AdapterInfo {
                id: "codebase-1".to_string(),
                adapter_type: Some("codebase".to_string()),
                ..Default::default()
            },
            AdapterInfo {
                id: "standard-1".to_string(),
                adapter_type: Some("standard".to_string()),
                ..Default::default()
            },
        ];

        // Single codebase should pass
        let result = validate_codebase_exclusivity(&[0], &adapters);
        assert!(result.is_ok());

        // Standard only should pass
        let result = validate_codebase_exclusivity(&[1], &adapters);
        assert!(result.is_ok());

        // Mix is fine
        let result = validate_codebase_exclusivity(&[0, 1], &adapters);
        assert!(result.is_ok());
    }

    #[test]
    fn test_codebase_exclusivity_multiple_fails() {
        let adapters = vec![
            AdapterInfo {
                id: "codebase-1".to_string(),
                adapter_type: Some("codebase".to_string()),
                ..Default::default()
            },
            AdapterInfo {
                id: "codebase-2".to_string(),
                adapter_type: Some("codebase".to_string()),
                ..Default::default()
            },
        ];

        // Multiple codebase adapters should fail
        let result = validate_codebase_exclusivity(&[0, 1], &adapters);
        assert!(result.is_err());
        if let Err(CodebaseExclusivityError::MultipleCodebaseAdapters { count, ids }) = result {
            assert_eq!(count, 2);
            assert!(ids.contains(&"codebase-1".to_string()));
            assert!(ids.contains(&"codebase-2".to_string()));
        } else {
            panic!("Expected MultipleCodebaseAdapters error");
        }
    }
}

// =============================================================================
// Codebase Adapter Exclusivity Validation
// =============================================================================

/// Error type for codebase adapter exclusivity validation
#[derive(Debug, Clone)]
pub enum CodebaseExclusivityError {
    /// Multiple codebase adapters selected in a single routing decision
    MultipleCodebaseAdapters {
        /// Number of codebase adapters found
        count: usize,
        /// IDs of the conflicting adapters
        ids: Vec<String>,
    },
    /// Codebase adapter bound to a different session
    SessionMismatch {
        /// The adapter ID
        adapter_id: String,
        /// Expected session ID
        expected_session: String,
        /// Actual bound session ID
        actual_session: Option<String>,
    },
}

impl std::fmt::Display for CodebaseExclusivityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CodebaseExclusivityError::MultipleCodebaseAdapters { count, ids } => {
                write!(
                    f,
                    "Multiple codebase adapters ({}) selected in single decision: {:?}. \
                     Only one codebase adapter is allowed per routing decision.",
                    count, ids
                )
            }
            CodebaseExclusivityError::SessionMismatch {
                adapter_id,
                expected_session,
                actual_session,
            } => {
                write!(
                    f,
                    "Codebase adapter '{}' session mismatch: expected '{}', \
                     got {:?}",
                    adapter_id, expected_session, actual_session
                )
            }
        }
    }
}

impl std::error::Error for CodebaseExclusivityError {}

/// Validate that at most one codebase adapter is selected in a routing decision.
///
/// This enforces the codebase adapter exclusivity rule: only one codebase adapter
/// can be active per stream/session.
///
/// # Arguments
///
/// * `selected_indices` - Indices of selected adapters from routing decision
/// * `adapters` - Full list of available adapters
///
/// # Returns
///
/// Returns `Ok(())` if validation passes (0 or 1 codebase adapter selected),
/// or `Err(CodebaseExclusivityError)` if multiple codebase adapters are selected.
pub fn validate_codebase_exclusivity(
    selected_indices: &[usize],
    adapters: &[AdapterInfo],
) -> std::result::Result<(), CodebaseExclusivityError> {
    let codebase_adapters: Vec<_> = selected_indices
        .iter()
        .filter_map(|&i| adapters.get(i))
        .filter(|a| a.is_codebase_adapter())
        .collect();

    if codebase_adapters.len() > 1 {
        return Err(CodebaseExclusivityError::MultipleCodebaseAdapters {
            count: codebase_adapters.len(),
            ids: codebase_adapters.iter().map(|a| a.id.clone()).collect(),
        });
    }

    Ok(())
}

/// Validate that selected codebase adapters match the expected session.
///
/// # Arguments
///
/// * `selected_indices` - Indices of selected adapters from routing decision
/// * `adapters` - Full list of available adapters
/// * `session_id` - The expected session ID for codebase adapters
///
/// # Returns
///
/// Returns `Ok(())` if all codebase adapters are bound to the expected session,
/// or `Err(CodebaseExclusivityError::SessionMismatch)` if a mismatch is found.
pub fn validate_codebase_session_binding(
    selected_indices: &[usize],
    adapters: &[AdapterInfo],
    session_id: &str,
) -> std::result::Result<(), CodebaseExclusivityError> {
    for &idx in selected_indices {
        if let Some(adapter) = adapters.get(idx) {
            if adapter.is_codebase_adapter() {
                // Codebase adapters should be bound to the expected session
                if !adapter.is_bound_to_session(session_id) {
                    return Err(CodebaseExclusivityError::SessionMismatch {
                        adapter_id: adapter.id.clone(),
                        expected_session: session_id.to_string(),
                        actual_session: adapter.stream_session_id.clone(),
                    });
                }
            }
        }
    }

    Ok(())
}

/// Count codebase adapters in a selection
pub fn count_codebase_adapters(selected_indices: &[usize], adapters: &[AdapterInfo]) -> usize {
    selected_indices
        .iter()
        .filter_map(|&i| adapters.get(i))
        .filter(|a| a.is_codebase_adapter())
        .count()
}

/// Count core adapters in a selection
pub fn count_core_adapters(selected_indices: &[usize], adapters: &[AdapterInfo]) -> usize {
    selected_indices
        .iter()
        .filter_map(|&i| adapters.get(i))
        .filter(|a| a.is_core_adapter())
        .count()
}
