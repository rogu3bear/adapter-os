//! # AdapterOS Experimental Features
//!
//! This crate contains experimental features that are **NOT FOR PRODUCTION USE**.
//!
//! ## ⚠️ WARNING ⚠️
//!
//! All features in this crate are:
//! - **NOT production ready**
//! - **Subject to breaking changes**
//! - **May have incomplete implementations**
//! - **Should not be used in production systems**
//!
//! ## Experimental Feature Status
//!
//! | Feature | Status | Stability | Notes |
//! |---------|--------|-----------|-------|
//! | `aos-cli` | 🚧 In Development | Unstable | AOS CLI commands with TODO implementations |
//! | `error-recovery` | 🚧 In Development | Unstable | Placeholder retry logic |
//! | `migration-conflicts` | 🚧 In Development | Unstable | Schema alignment conflicts |
//! | `domain-adapters` | 🚧 In Development | Unstable | Domain adapter execution pipeline |
//!
//! ## Usage
//!
//! ```toml
//! [dependencies]
//! adapteros-experimental = { path = "../adapteros-experimental", features = ["experimental-aos-cli"] }
//! ```
//!
//! ## Feature Flags
//!
//! - `experimental-aos-cli` - AOS CLI experimental features
//! - `experimental-error-recovery` - Error recovery experimental features
//! - `experimental-migration-conflicts` - Migration conflict resolution
//! - `experimental-domain-adapters` - Domain adapter experimental features
//! - `experimental-all` - All experimental features
//!
//! ## Deterministic Tagging System
//!
//! Each experimental feature is tagged with:
//! - **Status**: Development stage (In Development, Experimental, Deprecated)
//! - **Stability**: Stability level (Unstable, Experimental, Deprecated)
//! - **Dependencies**: Required features and crates
//! - **Last Updated**: Date of last modification
//! - **Known Issues**: List of known problems
//!
//! ## Contributing
//!
//! When adding experimental features:
//! 1. Create a new module in `src/`
//! 2. Add feature flag to `Cargo.toml`
//! 3. Update this documentation
//! 4. Add deterministic tags
//! 5. Include comprehensive tests
//!
//! ## Migration Path
//!
//! Experimental features should eventually be:
//! 1. **Completed** and moved to production crates
//! 2. **Deprecated** and removed
//! 3. **Stabilized** and moved to stable APIs

#![warn(missing_docs)]
#![warn(clippy::all)]
#![warn(clippy::pedantic)]

// ============================================================================
// EXPERIMENTAL MODULE DECLARATIONS
// ============================================================================

/// Experimental AOS CLI features
///
/// # Status: 🚧 In Development
/// # Stability: Unstable
/// # Dependencies: adapteros-cli
/// # Last Updated: 2025-01-15
/// # Known Issues: TODO implementations, missing control plane registration
#[cfg(feature = "aos-cli")]
pub mod aos_cli;

/// Experimental error recovery features
///
/// # Status: 🚧 In Development  
/// # Stability: Unstable
/// # Dependencies: None
/// # Last Updated: 2025-01-15
/// # Known Issues: Placeholder retry logic
#[cfg(feature = "error-recovery")]
pub mod error_recovery;

/// Experimental migration conflict resolution
///
/// # Status: 🚧 In Development
/// # Stability: Unstable  
/// # Dependencies: adapteros-db
/// # Last Updated: 2025-01-15
/// # Known Issues: Schema alignment conflicts, duplicate migration numbers
#[cfg(feature = "migration-conflicts")]
pub mod migration_conflicts;

/// Experimental domain adapter features
///
/// # Status: 🚧 In Development
/// # Stability: Unstable
/// # Dependencies: adapteros-api-types
/// # Last Updated: 2025-01-15
/// # Known Issues: Merge conflicts, incomplete implementation
#[cfg(feature = "domain-adapters")]
pub mod domain_adapters;

// ============================================================================
// EXPERIMENTAL FEATURE REGISTRY
// ============================================================================

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Experimental feature metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentalFeature {
    /// Feature name
    pub name: String,
    /// Development status
    pub status: FeatureStatus,
    /// Stability level
    pub stability: StabilityLevel,
    /// Required dependencies
    pub dependencies: Vec<String>,
    /// Last updated date
    pub last_updated: String,
    /// Known issues
    pub known_issues: Vec<String>,
    /// Feature description
    pub description: String,
}

/// Feature development status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FeatureStatus {
    /// In development
    InDevelopment,
    /// Experimental
    Experimental,
    /// Deprecated
    Deprecated,
    /// Removed
    Removed,
}

/// Feature stability level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StabilityLevel {
    /// Unstable - may break at any time
    Unstable,
    /// Experimental - subject to change
    Experimental,
    /// Deprecated - will be removed
    Deprecated,
}

/// Experimental feature registry
pub struct ExperimentalRegistry {
    features: HashMap<String, ExperimentalFeature>,
}

impl ExperimentalRegistry {
    /// Create a new experimental feature registry
    pub fn new() -> Self {
        let mut registry = Self {
            features: HashMap::new(),
        };

        // Register all experimental features
        registry.register_features();
        registry
    }

    /// Register all experimental features
    fn register_features(&mut self) {
        // AOS CLI experimental features
        self.register_feature(ExperimentalFeature {
            name: "aos-cli".to_string(),
            status: FeatureStatus::InDevelopment,
            stability: StabilityLevel::Unstable,
            dependencies: vec!["adapteros-cli".to_string()],
            last_updated: "2025-01-15".to_string(),
            known_issues: vec![
                "TODO: Register with control plane".to_string(),
                "Missing control plane registration".to_string(),
            ],
            description: "AOS CLI commands with incomplete implementations".to_string(),
        });

        // Error recovery experimental features
        self.register_feature(ExperimentalFeature {
            name: "error-recovery".to_string(),
            status: FeatureStatus::InDevelopment,
            stability: StabilityLevel::Unstable,
            dependencies: vec![],
            last_updated: "2025-01-15".to_string(),
            known_issues: vec![
                "Placeholder retry logic".to_string(),
                "Incomplete error recovery implementation".to_string(),
            ],
            description: "Error recovery with placeholder retry logic".to_string(),
        });

        // Migration conflicts experimental features
        self.register_feature(ExperimentalFeature {
            name: "migration-conflicts".to_string(),
            status: FeatureStatus::InDevelopment,
            stability: StabilityLevel::Unstable,
            dependencies: vec!["adapteros-db".to_string()],
            last_updated: "2025-01-15".to_string(),
            known_issues: vec![
                "Schema alignment conflicts".to_string(),
                "Duplicate migration numbers".to_string(),
                "FOREIGN KEY conflicts".to_string(),
            ],
            description: "Migration conflict resolution with schema alignment issues".to_string(),
        });

        // Domain adapters experimental features
        self.register_feature(ExperimentalFeature {
            name: "domain-adapters".to_string(),
            status: FeatureStatus::InDevelopment,
            stability: StabilityLevel::Unstable,
            dependencies: vec!["adapteros-api-types".to_string()],
            last_updated: "2025-01-15".to_string(),
            known_issues: vec![
                "Merge conflicts".to_string(),
                "Incomplete implementation".to_string(),
            ],
            description: "Domain adapter execution pipeline with incomplete implementation"
                .to_string(),
        });
    }

    /// Register a single experimental feature
    fn register_feature(&mut self, feature: ExperimentalFeature) {
        self.features.insert(feature.name.clone(), feature);
    }

    /// Get feature metadata by name
    pub fn get_feature(&self, name: &str) -> Option<&ExperimentalFeature> {
        self.features.get(name)
    }

    /// List all experimental features
    pub fn list_features(&self) -> Vec<&ExperimentalFeature> {
        self.features.values().collect()
    }

    /// Check if a feature is experimental
    pub fn is_experimental(&self, name: &str) -> bool {
        self.features.contains_key(name)
    }

    /// Get features by status
    pub fn get_features_by_status(&self, status: FeatureStatus) -> Vec<&ExperimentalFeature> {
        self.features
            .values()
            .filter(|f| std::mem::discriminant(&f.status) == std::mem::discriminant(&status))
            .collect()
    }
}

impl Default for ExperimentalRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================================
// EXPERIMENTAL FEATURE MACROS
// ============================================================================

/// Macro to mark experimental features
#[macro_export]
macro_rules! experimental_feature {
    ($name:ident, $status:expr, $stability:expr) => {
        #[cfg(feature = stringify!($name))]
        #[doc = concat!("Experimental feature: ", stringify!($name))]
        #[doc = concat!("Status: ", $status)]
        #[doc = concat!("Stability: ", $stability)]
        #[doc = "⚠️ NOT FOR PRODUCTION USE ⚠️"]
        pub mod $name;
    };
}

/// Macro to warn about experimental features
#[macro_export]
macro_rules! experimental_warning {
    ($feature:expr) => {
        compile_warning!(concat!(
            "Using experimental feature: ",
            $feature,
            " - NOT FOR PRODUCTION USE"
        ));
    };
}

// ============================================================================
// EXPERIMENTAL FEATURE TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_experimental_registry() {
        let registry = ExperimentalRegistry::new();

        // Test feature registration
        assert!(registry.is_experimental("aos-cli"));
        assert!(registry.is_experimental("error-recovery"));
        assert!(registry.is_experimental("migration-conflicts"));
        assert!(registry.is_experimental("domain-adapters"));

        // Test feature metadata
        let aos_cli = registry.get_feature("aos-cli").unwrap();
        assert_eq!(aos_cli.name, "aos-cli");
        assert!(matches!(aos_cli.status, FeatureStatus::InDevelopment));
        assert!(matches!(aos_cli.stability, StabilityLevel::Unstable));

        // Test feature listing
        let features = registry.list_features();
        assert_eq!(features.len(), 4);

        // Test status filtering
        let in_development = registry.get_features_by_status(FeatureStatus::InDevelopment);
        assert_eq!(in_development.len(), 4);
    }

    #[test]
    fn test_experimental_feature_serialization() {
        let feature = ExperimentalFeature {
            name: "test-feature".to_string(),
            status: FeatureStatus::Experimental,
            stability: StabilityLevel::Experimental,
            dependencies: vec!["test-dependency".to_string()],
            last_updated: "2025-01-15".to_string(),
            known_issues: vec!["test-issue".to_string()],
            description: "Test feature".to_string(),
        };

        // Test serialization
        let json = serde_json::to_string(&feature).unwrap();
        assert!(json.contains("test-feature"));

        // Test deserialization
        let deserialized: ExperimentalFeature = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.name, feature.name);
    }
}
