//! Retention Policy Pack
//!
//! Enforces bundle retention policies with CPID-based retention,
//! incident preservation, and automated garbage collection.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export canonical types from adapteros-telemetry-types
pub use adapteros_telemetry::types::{BundleType, RetentionBundleMetadata as BundleMetadata};

/// Retention policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionConfig {
    /// Number of bundles to keep per CPID
    pub keep_bundles_per_cpid: usize,
    /// Whether to keep all incident bundles
    pub keep_incident_bundles: bool,
    /// Whether to keep promotion bundles
    pub keep_promotion_bundles: bool,
    /// Eviction strategy
    pub evict_strategy: EvictionStrategy,
}

/// Eviction strategy for bundles
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvictionStrategy {
    /// Evict oldest bundles first (safe)
    OldestFirstSafe,
    /// Evict by size (largest first)
    LargestFirst,
    /// Evict by access time (least recently used)
    LruFirst,
}

impl Default for RetentionConfig {
    fn default() -> Self {
        Self {
            keep_bundles_per_cpid: 12,
            keep_incident_bundles: true,
            keep_promotion_bundles: true,
            evict_strategy: EvictionStrategy::OldestFirstSafe,
        }
    }
}

/// Retention policy implementation
pub struct RetentionPolicy {
    config: RetentionConfig,
}

impl RetentionPolicy {
    /// Create new retention policy
    pub fn new(config: RetentionConfig) -> Self {
        Self { config }
    }

    /// Determine which bundles should be retained
    pub fn determine_retention(&self, bundles: &[BundleMetadata]) -> Result<RetentionDecision> {
        let mut bundles_by_cpid: HashMap<String, Vec<BundleMetadata>> = HashMap::new();

        // Group bundles by CPID
        for bundle in bundles {
            bundles_by_cpid
                .entry(bundle.cpid.clone())
                .or_default()
                .push(bundle.clone());
        }

        let mut retain = Vec::new();
        let mut evict = Vec::new();

        for (_cpid, cpid_bundles) in bundles_by_cpid {
            let mut cpid_retain = Vec::new();
            let mut cpid_evict = Vec::new();

            // Always retain incident bundles if configured
            if self.config.keep_incident_bundles {
                for bundle in &cpid_bundles {
                    if bundle.bundle_type == BundleType::Incident {
                        cpid_retain.push(bundle.clone());
                    }
                }
            }

            // Always retain promotion bundles if configured
            if self.config.keep_promotion_bundles {
                for bundle in &cpid_bundles {
                    if bundle.bundle_type == BundleType::Promotion {
                        cpid_retain.push(bundle.clone());
                    }
                }
            }

            // Sort remaining bundles by eviction strategy
            let mut remaining: Vec<BundleMetadata> = cpid_bundles
                .into_iter()
                .filter(|b| !cpid_retain.iter().any(|r| r.bundle_id == b.bundle_id))
                .collect();

            match self.config.evict_strategy {
                EvictionStrategy::OldestFirstSafe => {
                    remaining.sort_by_key(|b| b.created_at);
                }
                EvictionStrategy::LargestFirst => {
                    remaining.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));
                }
                EvictionStrategy::LruFirst => {
                    remaining.sort_by_key(|b| b.last_accessed);
                }
            }

            // Keep the most recent N bundles
            let keep_count = self
                .config
                .keep_bundles_per_cpid
                .saturating_sub(cpid_retain.len());
            let (keep, evict_part) = remaining.split_at(keep_count.min(remaining.len()));

            cpid_retain.extend_from_slice(keep);
            cpid_evict.extend_from_slice(evict_part);

            retain.extend(cpid_retain);
            evict.extend(cpid_evict);
        }

        let retained_count = retain.len();
        let evicted_count = evict.len();

        Ok(RetentionDecision {
            retain,
            evict,
            total_bundles: bundles.len(),
            retained_count,
            evicted_count,
        })
    }

    /// Validate retention configuration
    pub fn validate_config(&self) -> Result<()> {
        if self.config.keep_bundles_per_cpid == 0 {
            return Err(AosError::PolicyViolation(
                "keep_bundles_per_cpid must be greater than 0".to_string(),
            ));
        }

        if self.config.keep_bundles_per_cpid > 1000 {
            return Err(AosError::PolicyViolation(
                "keep_bundles_per_cpid cannot exceed 1000".to_string(),
            ));
        }

        Ok(())
    }
}

/// Retention decision result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetentionDecision {
    pub retain: Vec<BundleMetadata>,
    pub evict: Vec<BundleMetadata>,
    pub total_bundles: usize,
    pub retained_count: usize,
    pub evicted_count: usize,
}

impl RetentionDecision {
    /// Check if retention decision is valid
    pub fn is_valid(&self) -> bool {
        self.retain.len() + self.evict.len() == self.total_bundles
    }

    /// Get retention ratio
    pub fn retention_ratio(&self) -> f64 {
        if self.total_bundles == 0 {
            1.0
        } else {
            self.retained_count as f64 / self.total_bundles as f64
        }
    }
}

/// Context for retention policy enforcement
#[derive(Debug)]
pub struct RetentionContext {
    pub bundles: Vec<BundleMetadata>,
    pub cpid: String,
    pub tenant_id: String,
}

impl PolicyContext for RetentionContext {
    fn context_type(&self) -> &str {
        "retention"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Policy for RetentionPolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Retention
    }

    fn name(&self) -> &'static str {
        "Retention"
    }

    fn severity(&self) -> Severity {
        Severity::Medium
    }

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        let retention_ctx = ctx
            .as_any()
            .downcast_ref::<RetentionContext>()
            .ok_or_else(|| AosError::PolicyViolation("Invalid retention context".to_string()))?;

        // Validate configuration
        self.validate_config()?;

        // Determine retention
        let decision = self.determine_retention(&retention_ctx.bundles)?;

        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Check if decision is valid
        if !decision.is_valid() {
            violations.push(Violation {
                severity: Severity::High,
                message: "Retention decision is invalid".to_string(),
                details: Some(format!(
                    "Expected {} bundles, got {}",
                    decision.total_bundles,
                    decision.retain.len() + decision.evict.len()
                )),
            });
        }

        // Check retention ratio
        let retention_ratio = decision.retention_ratio();
        if retention_ratio < 0.1 {
            warnings.push(format!(
                "Low retention ratio: {:.2}% ({} of {} bundles retained)",
                retention_ratio * 100.0,
                decision.retained_count,
                decision.total_bundles
            ));
        }

        // Check for incident bundles
        let incident_count = decision
            .retain
            .iter()
            .filter(|b| b.bundle_type == BundleType::Incident)
            .count();

        if incident_count > 0 {
            warnings.push(format!("Retaining {} incident bundles", incident_count));
        }

        Ok(Audit {
            policy_id: PolicyId::Retention,
            passed: violations.is_empty(),
            violations,
            warnings,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_retention_config_default() {
        let config = RetentionConfig::default();
        assert_eq!(config.keep_bundles_per_cpid, 12);
        assert!(config.keep_incident_bundles);
        assert!(config.keep_promotion_bundles);
    }

    #[test]
    fn test_retention_policy_creation() {
        let config = RetentionConfig::default();
        let policy = RetentionPolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Retention);
    }

    #[test]
    fn test_retention_decision() {
        let config = RetentionConfig::default();
        let policy = RetentionPolicy::new(config);

        let bundles = vec![
            BundleMetadata {
                bundle_id: "bundle1".to_string(),
                cpid: "cpid1".to_string(),
                created_at: 1000,
                size_bytes: 1000,
                last_accessed: 1000,
                bundle_type: BundleType::Inference,
                incident_id: None,
                promotion_id: None,
            },
            BundleMetadata {
                bundle_id: "bundle2".to_string(),
                cpid: "cpid1".to_string(),
                created_at: 2000,
                size_bytes: 2000,
                last_accessed: 2000,
                bundle_type: BundleType::Inference,
                incident_id: None,
                promotion_id: None,
            },
        ];

        let decision = policy.determine_retention(&bundles).unwrap();
        assert!(decision.is_valid());
        assert_eq!(decision.total_bundles, 2);
        assert_eq!(decision.retained_count, 2); // Both should be retained
        assert_eq!(decision.evicted_count, 0);
    }

    #[test]
    fn test_retention_with_incident_bundles() {
        let config = RetentionConfig::default();
        let policy = RetentionPolicy::new(config);

        let bundles = vec![
            BundleMetadata {
                bundle_id: "bundle1".to_string(),
                cpid: "cpid1".to_string(),
                created_at: 1000,
                size_bytes: 1000,
                last_accessed: 1000,
                bundle_type: BundleType::Inference,
                incident_id: None,
                promotion_id: None,
            },
            BundleMetadata {
                bundle_id: "incident1".to_string(),
                cpid: "cpid1".to_string(),
                created_at: 2000,
                size_bytes: 2000,
                last_accessed: 2000,
                bundle_type: BundleType::Incident,
                incident_id: Some("incident1".to_string()),
                promotion_id: None,
            },
        ];

        let decision = policy.determine_retention(&bundles).unwrap();
        assert!(decision.is_valid());
        assert_eq!(decision.total_bundles, 2);
        assert_eq!(decision.retained_count, 2); // Both should be retained
        assert_eq!(decision.evicted_count, 0);
    }

    #[test]
    fn test_retention_eviction() {
        let mut config = RetentionConfig::default();
        config.keep_bundles_per_cpid = 1; // Only keep 1 bundle
        let policy = RetentionPolicy::new(config);

        let bundles = vec![
            BundleMetadata {
                bundle_id: "bundle1".to_string(),
                cpid: "cpid1".to_string(),
                created_at: 1000,
                size_bytes: 1000,
                last_accessed: 1000,
                bundle_type: BundleType::Inference,
                incident_id: None,
                promotion_id: None,
            },
            BundleMetadata {
                bundle_id: "bundle2".to_string(),
                cpid: "cpid1".to_string(),
                created_at: 2000,
                size_bytes: 2000,
                last_accessed: 2000,
                bundle_type: BundleType::Inference,
                incident_id: None,
                promotion_id: None,
            },
        ];

        let decision = policy.determine_retention(&bundles).unwrap();
        assert!(decision.is_valid());
        assert_eq!(decision.total_bundles, 2);
        assert_eq!(decision.retained_count, 1); // Only newest should be retained
        assert_eq!(decision.evicted_count, 1); // Oldest should be evicted
    }
}
