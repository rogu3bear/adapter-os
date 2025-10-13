//! Adapter Lifecycle Policy Pack
//!
//! Enforces adapter lifecycle management including registration,
//! activation thresholds, and quality requirements.

use crate::{Audit, Policy, PolicyContext, PolicyId, Severity, Violation};
use adapteros_core::{AosError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Adapter lifecycle policy configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterLifecycleConfig {
    /// Minimum activation percentage
    pub min_activation_pct: f64,
    /// Minimum quality delta
    pub min_quality_delta: f64,
    /// Whether registry admission is required
    pub require_registry_admit: bool,
    /// Maximum number of adapters per tenant
    pub max_adapters_per_tenant: u32,
    /// Adapter eviction thresholds
    pub eviction_thresholds: EvictionThresholds,
    /// Quality assessment criteria
    pub quality_criteria: QualityCriteria,
}

/// Eviction thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvictionThresholds {
    /// Minimum activation percentage before eviction
    pub min_activation_before_eviction: f64,
    /// Maximum age in days before eviction
    pub max_age_days: u64,
    /// Minimum quality score before eviction
    pub min_quality_score: f64,
    /// Maximum memory usage before eviction
    pub max_memory_usage_mb: u64,
}

/// Quality assessment criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityCriteria {
    /// Minimum accuracy threshold
    pub min_accuracy: f64,
    /// Minimum precision threshold
    pub min_precision: f64,
    /// Minimum recall threshold
    pub min_recall: f64,
    /// Minimum F1 score threshold
    pub min_f1_score: f64,
    /// Maximum latency threshold in milliseconds
    pub max_latency_ms: u64,
}

impl Default for EvictionThresholds {
    fn default() -> Self {
        Self {
            min_activation_before_eviction: 1.0,
            max_age_days: 365,
            min_quality_score: 0.5,
            max_memory_usage_mb: 1000,
        }
    }
}

impl Default for QualityCriteria {
    fn default() -> Self {
        Self {
            min_accuracy: 0.8,
            min_precision: 0.75,
            min_recall: 0.75,
            min_f1_score: 0.75,
            max_latency_ms: 1000,
        }
    }
}

impl Default for AdapterLifecycleConfig {
    fn default() -> Self {
        Self {
            min_activation_pct: 2.0,
            min_quality_delta: 0.5,
            require_registry_admit: true,
            max_adapters_per_tenant: 100,
            eviction_thresholds: EvictionThresholds::default(),
            quality_criteria: QualityCriteria::default(),
        }
    }
}

/// Adapter metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterMetadata {
    pub adapter_id: String,
    pub adapter_name: String,
    pub adapter_type: AdapterType,
    pub version: String,
    pub created_at: u64,
    pub last_accessed: u64,
    pub activation_count: u64,
    pub total_requests: u64,
    pub quality_metrics: QualityMetrics,
    pub registry_status: RegistryStatus,
    pub eviction_status: EvictionStatus,
}

/// Types of adapters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AdapterType {
    /// Ephemeral adapter
    Ephemeral,
    /// Directory-specific adapter
    DirectorySpecific,
    /// Framework adapter
    Framework,
    /// Code adapter
    Code,
    /// Base adapter
    Base,
}

/// Registry status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RegistryStatus {
    /// Adapter registered
    Registered,
    /// Adapter pending approval
    Pending,
    /// Adapter rejected
    Rejected,
    /// Adapter suspended
    Suspended,
    /// Adapter not registered
    NotRegistered,
}

/// Eviction status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvictionStatus {
    /// Adapter active
    Active,
    /// Adapter marked for eviction
    MarkedForEviction,
    /// Adapter evicted
    Evicted,
    /// Adapter protected from eviction
    Protected,
}

/// Quality metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityMetrics {
    pub accuracy: f64,
    pub precision: f64,
    pub recall: f64,
    pub f1_score: f64,
    pub latency_ms: u64,
    pub memory_usage_mb: u64,
    pub last_updated: u64,
}

/// Adapter registration request
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterRegistrationRequest {
    pub adapter_id: String,
    pub adapter_name: String,
    pub adapter_type: AdapterType,
    pub version: String,
    pub capability_card: CapabilityCard,
    pub dataset_lineage: DatasetLineage,
    pub quality_metrics: QualityMetrics,
    pub submitted_by: String,
    pub submitted_at: u64,
}

/// Capability card
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityCard {
    pub capabilities: Vec<String>,
    pub limitations: Vec<String>,
    pub performance_characteristics: HashMap<String, serde_json::Value>,
    pub compatibility: Vec<String>,
}

/// Dataset lineage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetLineage {
    pub dataset_name: String,
    pub dataset_version: String,
    pub dataset_hash: String,
    pub training_samples: u64,
    pub validation_samples: u64,
    pub test_samples: u64,
    pub data_sources: Vec<String>,
}

/// Adapter lifecycle policy implementation
pub struct AdapterLifecyclePolicy {
    config: AdapterLifecycleConfig,
}

impl AdapterLifecyclePolicy {
    /// Create new adapter lifecycle policy
    pub fn new(config: AdapterLifecycleConfig) -> Self {
        Self { config }
    }

    /// Calculate adapter activation percentage
    pub fn calculate_activation_percentage(&self, adapter: &AdapterMetadata) -> f64 {
        if adapter.total_requests == 0 {
            0.0
        } else {
            (adapter.activation_count as f64 / adapter.total_requests as f64) * 100.0
        }
    }

    /// Check if adapter meets activation requirements
    pub fn check_activation_requirements(&self, adapter: &AdapterMetadata) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        let activation_pct = self.calculate_activation_percentage(adapter);

        if activation_pct < self.config.min_activation_pct {
            errors.push(format!(
                "Activation percentage {:.2}% below minimum {:.2}%",
                activation_pct, self.config.min_activation_pct
            ));
        }

        // Check quality metrics
        if adapter.quality_metrics.accuracy < self.config.quality_criteria.min_accuracy {
            errors.push(format!(
                "Accuracy {:.4} below minimum {:.4}",
                adapter.quality_metrics.accuracy, self.config.quality_criteria.min_accuracy
            ));
        }

        if adapter.quality_metrics.precision < self.config.quality_criteria.min_precision {
            errors.push(format!(
                "Precision {:.4} below minimum {:.4}",
                adapter.quality_metrics.precision, self.config.quality_criteria.min_precision
            ));
        }

        if adapter.quality_metrics.recall < self.config.quality_criteria.min_recall {
            errors.push(format!(
                "Recall {:.4} below minimum {:.4}",
                adapter.quality_metrics.recall, self.config.quality_criteria.min_recall
            ));
        }

        if adapter.quality_metrics.f1_score < self.config.quality_criteria.min_f1_score {
            errors.push(format!(
                "F1 score {:.4} below minimum {:.4}",
                adapter.quality_metrics.f1_score, self.config.quality_criteria.min_f1_score
            ));
        }

        if adapter.quality_metrics.latency_ms > self.config.quality_criteria.max_latency_ms {
            errors.push(format!(
                "Latency {}ms exceeds maximum {}ms",
                adapter.quality_metrics.latency_ms, self.config.quality_criteria.max_latency_ms
            ));
        }

        Ok(errors)
    }

    /// Check if adapter should be evicted
    pub fn should_evict_adapter(&self, adapter: &AdapterMetadata) -> bool {
        let activation_pct = self.calculate_activation_percentage(adapter);

        // Check activation threshold
        if activation_pct
            < self
                .config
                .eviction_thresholds
                .min_activation_before_eviction
        {
            return true;
        }

        // Check age
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let age_days = (now - adapter.created_at) / 86400;
        if age_days > self.config.eviction_thresholds.max_age_days {
            return true;
        }

        // Check quality score
        let quality_score = (adapter.quality_metrics.accuracy
            + adapter.quality_metrics.precision
            + adapter.quality_metrics.recall
            + adapter.quality_metrics.f1_score)
            / 4.0;

        if quality_score < self.config.eviction_thresholds.min_quality_score {
            return true;
        }

        // Check memory usage
        if adapter.quality_metrics.memory_usage_mb
            > self.config.eviction_thresholds.max_memory_usage_mb
        {
            return true;
        }

        false
    }

    /// Validate adapter registration request
    pub fn validate_registration_request(
        &self,
        request: &AdapterRegistrationRequest,
    ) -> Result<Vec<String>> {
        let mut errors = Vec::new();

        if self.config.require_registry_admit {
            // Check capability card
            if request.capability_card.capabilities.is_empty() {
                errors.push("Capability card must specify capabilities".to_string());
            }

            // Check dataset lineage
            if request.dataset_lineage.dataset_name.is_empty() {
                errors.push("Dataset lineage must specify dataset name".to_string());
            }

            if request.dataset_lineage.dataset_hash.is_empty() {
                errors.push("Dataset lineage must specify dataset hash".to_string());
            }

            if request.dataset_lineage.training_samples == 0 {
                errors.push("Dataset lineage must specify training samples".to_string());
            }
        }

        // Validate quality metrics
        if request.quality_metrics.accuracy < self.config.quality_criteria.min_accuracy {
            errors.push(format!(
                "Registration accuracy {:.4} below minimum {:.4}",
                request.quality_metrics.accuracy, self.config.quality_criteria.min_accuracy
            ));
        }

        if request.quality_metrics.latency_ms > self.config.quality_criteria.max_latency_ms {
            errors.push(format!(
                "Registration latency {}ms exceeds maximum {}ms",
                request.quality_metrics.latency_ms, self.config.quality_criteria.max_latency_ms
            ));
        }

        Ok(errors)
    }

    /// Validate adapter lifecycle configuration
    pub fn validate_config(&self) -> Result<()> {
        if self.config.min_activation_pct < 0.0 || self.config.min_activation_pct > 100.0 {
            return Err(AosError::PolicyViolation(
                "Minimum activation percentage must be between 0 and 100".to_string(),
            ));
        }

        if self.config.min_quality_delta < 0.0 || self.config.min_quality_delta > 1.0 {
            return Err(AosError::PolicyViolation(
                "Minimum quality delta must be between 0 and 1".to_string(),
            ));
        }

        if self.config.max_adapters_per_tenant == 0 {
            return Err(AosError::PolicyViolation(
                "Maximum adapters per tenant must be greater than 0".to_string(),
            ));
        }

        // Validate quality criteria
        if self.config.quality_criteria.min_accuracy < 0.0
            || self.config.quality_criteria.min_accuracy > 1.0
        {
            return Err(AosError::PolicyViolation(
                "Minimum accuracy must be between 0 and 1".to_string(),
            ));
        }

        if self.config.quality_criteria.min_precision < 0.0
            || self.config.quality_criteria.min_precision > 1.0
        {
            return Err(AosError::PolicyViolation(
                "Minimum precision must be between 0 and 1".to_string(),
            ));
        }

        if self.config.quality_criteria.min_recall < 0.0
            || self.config.quality_criteria.min_recall > 1.0
        {
            return Err(AosError::PolicyViolation(
                "Minimum recall must be between 0 and 1".to_string(),
            ));
        }

        if self.config.quality_criteria.min_f1_score < 0.0
            || self.config.quality_criteria.min_f1_score > 1.0
        {
            return Err(AosError::PolicyViolation(
                "Minimum F1 score must be between 0 and 1".to_string(),
            ));
        }

        Ok(())
    }
}

/// Context for adapter lifecycle policy enforcement
#[derive(Debug)]
pub struct AdapterLifecycleContext {
    pub adapters: Vec<AdapterMetadata>,
    pub registration_requests: Vec<AdapterRegistrationRequest>,
    pub tenant_id: String,
    pub operation: AdapterLifecycleOperation,
}

/// Types of adapter lifecycle operations
#[derive(Debug)]
pub enum AdapterLifecycleOperation {
    /// Adapter registration
    Registration,
    /// Adapter activation
    Activation,
    /// Adapter eviction
    Eviction,
    /// Adapter quality assessment
    QualityAssessment,
    /// Adapter lifecycle audit
    Audit,
}

impl PolicyContext for AdapterLifecycleContext {
    fn context_type(&self) -> &str {
        "adapter_lifecycle"
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Policy for AdapterLifecyclePolicy {
    fn id(&self) -> PolicyId {
        PolicyId::Adapters
    }

    fn name(&self) -> &'static str {
        "Adapter Lifecycle"
    }

    fn severity(&self) -> Severity {
        Severity::Medium
    }

    fn enforce(&self, ctx: &dyn PolicyContext) -> Result<Audit> {
        let adapter_ctx = ctx
            .as_any()
            .downcast_ref::<AdapterLifecycleContext>()
            .ok_or_else(|| {
                AosError::PolicyViolation("Invalid adapter lifecycle context".to_string())
            })?;

        // Validate configuration
        self.validate_config()?;

        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Check adapter count limit
        if adapter_ctx.adapters.len() > self.config.max_adapters_per_tenant as usize {
            violations.push(Violation {
                severity: Severity::High,
                message: format!(
                    "Adapter count {} exceeds maximum {}",
                    adapter_ctx.adapters.len(),
                    self.config.max_adapters_per_tenant
                ),
                details: Some("Too many adapters for tenant".to_string()),
            });
        }

        // Validate adapters
        for adapter in &adapter_ctx.adapters {
            match self.check_activation_requirements(adapter) {
                Ok(errors) => {
                    for error in errors {
                        violations.push(Violation {
                            severity: Severity::Medium,
                            message: format!(
                                "Adapter {} activation requirements failed: {}",
                                adapter.adapter_id, error
                            ),
                            details: Some(format!("Adapter type: {:?}", adapter.adapter_type)),
                        });
                    }
                }
                Err(e) => {
                    violations.push(Violation {
                        severity: Severity::Medium,
                        message: format!("Adapter {} validation error", adapter.adapter_id),
                        details: Some(e.to_string()),
                    });
                }
            }

            // Check if adapter should be evicted
            if self.should_evict_adapter(adapter) {
                warnings.push(format!("Adapter {} should be evicted", adapter.adapter_id));
            }

            // Check registry status
            match adapter.registry_status {
                RegistryStatus::NotRegistered => {
                    if self.config.require_registry_admit {
                        violations.push(Violation {
                            severity: Severity::High,
                            message: format!("Adapter {} not registered", adapter.adapter_id),
                            details: Some("Registry admission required".to_string()),
                        });
                    }
                }
                RegistryStatus::Rejected => {
                    violations.push(Violation {
                        severity: Severity::High,
                        message: format!("Adapter {} was rejected", adapter.adapter_id),
                        details: Some("Adapter should not be in use".to_string()),
                    });
                }
                RegistryStatus::Suspended => {
                    warnings.push(format!("Adapter {} is suspended", adapter.adapter_id));
                }
                _ => {}
            }
        }

        // Validate registration requests
        for request in &adapter_ctx.registration_requests {
            match self.validate_registration_request(request) {
                Ok(errors) => {
                    for error in errors {
                        violations.push(Violation {
                            severity: Severity::Medium,
                            message: format!(
                                "Registration request {} validation failed: {}",
                                request.adapter_id, error
                            ),
                            details: Some(format!("Submitted by: {}", request.submitted_by)),
                        });
                    }
                }
                Err(e) => {
                    violations.push(Violation {
                        severity: Severity::Medium,
                        message: format!(
                            "Registration request {} validation error",
                            request.adapter_id
                        ),
                        details: Some(e.to_string()),
                    });
                }
            }
        }

        Ok(Audit {
            policy_id: PolicyId::Adapters,
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
    fn test_adapter_lifecycle_config_default() {
        let config = AdapterLifecycleConfig::default();
        assert_eq!(config.min_activation_pct, 2.0);
        assert_eq!(config.min_quality_delta, 0.5);
        assert!(config.require_registry_admit);
        assert_eq!(config.max_adapters_per_tenant, 100);
    }

    #[test]
    fn test_adapter_lifecycle_policy_creation() {
        let config = AdapterLifecycleConfig::default();
        let policy = AdapterLifecyclePolicy::new(config);
        assert_eq!(policy.id(), PolicyId::Adapters);
    }

    #[test]
    fn test_activation_percentage_calculation() {
        let config = AdapterLifecycleConfig::default();
        let policy = AdapterLifecyclePolicy::new(config);

        let adapter = AdapterMetadata {
            adapter_id: "test_adapter".to_string(),
            adapter_name: "Test Adapter".to_string(),
            adapter_type: AdapterType::Code,
            version: "1.0.0".to_string(),
            created_at: 1000,
            last_accessed: 2000,
            activation_count: 20,
            total_requests: 100,
            quality_metrics: QualityMetrics {
                accuracy: 0.9,
                precision: 0.8,
                recall: 0.8,
                f1_score: 0.8,
                latency_ms: 500,
                memory_usage_mb: 100,
                last_updated: 2000,
            },
            registry_status: RegistryStatus::Registered,
            eviction_status: EvictionStatus::Active,
        };

        let activation_pct = policy.calculate_activation_percentage(&adapter);
        assert_eq!(activation_pct, 20.0);
    }

    #[test]
    fn test_activation_requirements_check() {
        let config = AdapterLifecycleConfig::default();
        let policy = AdapterLifecyclePolicy::new(config);

        let adapter = AdapterMetadata {
            adapter_id: "test_adapter".to_string(),
            adapter_name: "Test Adapter".to_string(),
            adapter_type: AdapterType::Code,
            version: "1.0.0".to_string(),
            created_at: 1000,
            last_accessed: 2000,
            activation_count: 1, // Low activation
            total_requests: 100,
            quality_metrics: QualityMetrics {
                accuracy: 0.5, // Below minimum
                precision: 0.8,
                recall: 0.8,
                f1_score: 0.8,
                latency_ms: 500,
                memory_usage_mb: 100,
                last_updated: 2000,
            },
            registry_status: RegistryStatus::Registered,
            eviction_status: EvictionStatus::Active,
        };

        let errors = policy.check_activation_requirements(&adapter).unwrap();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("Activation percentage")));
        assert!(errors.iter().any(|e| e.contains("Accuracy")));
    }

    #[test]
    fn test_adapter_eviction_check() {
        let config = AdapterLifecycleConfig::default();
        let policy = AdapterLifecyclePolicy::new(config);

        let adapter = AdapterMetadata {
            adapter_id: "test_adapter".to_string(),
            adapter_name: "Test Adapter".to_string(),
            adapter_type: AdapterType::Code,
            version: "1.0.0".to_string(),
            created_at: 1000,
            last_accessed: 2000,
            activation_count: 1,
            total_requests: 100,
            quality_metrics: QualityMetrics {
                accuracy: 0.5, // Low quality
                precision: 0.5,
                recall: 0.5,
                f1_score: 0.5,
                latency_ms: 500,
                memory_usage_mb: 100,
                last_updated: 2000,
            },
            registry_status: RegistryStatus::Registered,
            eviction_status: EvictionStatus::Active,
        };

        assert!(policy.should_evict_adapter(&adapter));
    }

    #[test]
    fn test_registration_request_validation() {
        let config = AdapterLifecycleConfig::default();
        let policy = AdapterLifecyclePolicy::new(config);

        let request = AdapterRegistrationRequest {
            adapter_id: "test_adapter".to_string(),
            adapter_name: "Test Adapter".to_string(),
            adapter_type: AdapterType::Code,
            version: "1.0.0".to_string(),
            capability_card: CapabilityCard {
                capabilities: vec![], // Empty - should fail
                limitations: vec![],
                performance_characteristics: HashMap::new(),
                compatibility: vec![],
            },
            dataset_lineage: DatasetLineage {
                dataset_name: "".to_string(), // Empty - should fail
                dataset_version: "1.0.0".to_string(),
                dataset_hash: "".to_string(), // Empty - should fail
                training_samples: 0,          // Zero - should fail
                validation_samples: 100,
                test_samples: 100,
                data_sources: vec![],
            },
            quality_metrics: QualityMetrics {
                accuracy: 0.5, // Below minimum
                precision: 0.8,
                recall: 0.8,
                f1_score: 0.8,
                latency_ms: 500,
                memory_usage_mb: 100,
                last_updated: 2000,
            },
            submitted_by: "test_user".to_string(),
            submitted_at: 2000,
        };

        let errors = policy.validate_registration_request(&request).unwrap();
        assert!(!errors.is_empty());
        assert!(errors.iter().any(|e| e.contains("capabilities")));
        assert!(errors.iter().any(|e| e.contains("dataset name")));
        assert!(errors.iter().any(|e| e.contains("dataset hash")));
        assert!(errors.iter().any(|e| e.contains("training samples")));
        assert!(errors.iter().any(|e| e.contains("accuracy")));
    }

    #[test]
    fn test_adapter_lifecycle_config_validation() {
        let mut config = AdapterLifecycleConfig::default();
        config.min_activation_pct = 150.0; // Invalid
        let policy = AdapterLifecyclePolicy::new(config);

        assert!(policy.validate_config().is_err());
    }
}
