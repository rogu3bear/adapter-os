//! Core types for the router module
//!
//! This module contains type-safe newtypes and enums that replace stringly-typed
//! patterns in the router implementation.

use crate::constants::{
    LORA_TIER_MAX_BOOST, LORA_TIER_MICRO_BOOST, LORA_TIER_STANDARD_BOOST, TIER_0_BOOST,
    TIER_1_BOOST, TIER_2_BOOST,
};
use serde::{Deserialize, Serialize};
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
    pub fn from_slice(slice: &[f32]) -> Result<Self, FeatureVectorError> {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdapterTier {
    /// Highest priority tier
    Tier0,
    /// Standard tier
    Tier1,
    /// Lower priority tier
    Tier2,
    /// Default/unspecified tier
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
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "tier_0" => Self::Tier0,
            "tier_1" => Self::Tier1,
            "tier_2" => Self::Tier2,
            _ => Self::Default,
        })
    }
}

impl Default for AdapterTier {
    fn default() -> Self {
        Self::Default
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
    fn from_str(s: &str) -> Result<Self, Self::Err> {
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
}
