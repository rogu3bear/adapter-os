//! Policy Packs - Complete implementation of all 22 policy packs
//!
//! This module contains the complete implementation of all policy packs
//! for AdapterOS, including configuration, validation, and enforcement logic.

pub mod adapters;
pub mod artifacts;
pub mod build_release;
pub mod compliance;
pub mod determinism;
pub mod deterministic_io;
pub mod drift;
pub mod egress;
pub mod evidence;
pub mod incident;
pub mod isolation;
pub mod memory;
pub mod mplora;
pub mod naming_policy;
pub mod numeric;
pub mod output;
pub mod performance;
pub mod rag;
pub mod refusal;
pub mod retention;
pub mod router;
pub mod secrets;
pub mod telemetry;

// Re-export policy implementations
pub use adapters::{
    AdapterLifecycleConfig, AdapterLifecyclePolicy, AdapterMetadata, AdapterRegistrationRequest,
};
pub use artifacts::{ArtifactMetadata, ArtifactValidation, ArtifactsConfig, ArtifactsPolicy};
pub use build_release::{BuildMetrics, BuildReleaseConfig, BuildReleasePolicy, ReplayTestResults};
pub use compliance::{ComplianceConfig, CompliancePolicy, ControlMatrixEntry, EvidenceEntry};
pub use determinism::{
    DeterminismConfig, DeterminismPolicy, EpsilonBounds, RngSeedingMethod, TieBreakRule,
    ToolchainRequirements,
};
pub use deterministic_io::{
    DeterminismRequirements, DeterminismRule, DeterministicIoConfig, DeterministicIoPolicy,
    FilesystemConstraints, IoOperationContext, IoOperationType, IoPattern, IoValidation,
    NetworkConstraints,
};
pub use drift::{
    AlertChannel, AlertSeverity, AlertingConfig, BaselineConfig, BaselineStorage,
    BaselineValidationRule, DriftAlert as DriftAlertType, DriftConfig, DriftMeasurement,
    DriftPolicy, DriftThresholds, MonitoringConfig, MonitoringMetric, StorageBackend,
};
pub use egress::{DnsPolicy, EgressConfig, EgressMode, EgressPolicy, MediaImportConfig};
pub use evidence::{
    EvidenceConfig, EvidencePolicy, EvidenceSpan, EvidenceType, QualityThresholds, SourceInfo,
    SourceRequirements,
};
pub use incident::{IncidentConfig, IncidentEvent, IncidentPolicy, IncidentResponsePlan};
pub use isolation::{
    FileOperation, FilesystemIsolation, IsolationConfig, IsolationPolicy, KeyBackend, KeyConfig,
    NetworkIsolation, ProcessModel, RotationPolicy, RotationTrigger, TenantContext,
};
pub use memory::{AdapterMemoryInfo, EvictionDecision, MemoryConfig, MemoryPolicy, MemoryStats};
pub use mplora::{
    MploraConfig, MploraPath, MploraPerformanceMetrics, MploraPolicy, PathConstraints,
    PathSelectionStrategy, PerformanceConstraints as MploraPerformanceConstraints,
};
pub use naming_policy::{
    AdapterNameValidation, NamingConfig, NamingPolicy, NamingViolation, NamingViolationType,
    StackNameValidation,
};
pub use numeric::{
    NumericClaim, NumericConfig, NumericPolicy, PrecisionInfo, PrecisionRequirements, RangeLimit,
    RoundingMode, UnitConversion, ValidationRules,
};
pub use output::{LlmOutput, OutputConfig, OutputPolicy, SafetyCheckResult};
pub use performance::{InferenceMetrics, PerformanceConfig, PerformancePolicy, PerformanceStats};
pub use rag::{
    DocumentMetadata, IndexScope, IsolationRule, OrderingRule, RagConfig, RagPolicy,
    RetrievalResult, SupersessionAction, SupersessionConfig, SupersessionStatus, TenantIsolation,
};
pub use refusal::{
    RedactionRules, RefusalConfig, RefusalPolicy, RefusalReason, RefusalResponse, SafetyChecks,
    SafetyScores,
};
pub use retention::{
    BundleMetadata, BundleType, RetentionConfig, RetentionDecision, RetentionPolicy,
};
pub use router::{
    FeatureConfig, FeatureWeights, GateQuantization, RouterConfig, RouterPolicy,
    TieBreakRule as RouterTieBreakRule,
};
pub use secrets::{KeyRotationStatus, SecretMetadata, SecretsConfig, SecretsPolicy};
pub use telemetry::{
    BundleConfig, CompressionAlgorithm, CompressionConfig, PolicyTelemetryView,
    RetentionConfig as TelemetryRetentionConfig, SamplingConfig, TelemetryBundle, TelemetryConfig,
    TelemetryPolicy,
};

/// Policy pack factory for creating policy instances
pub struct PolicyPackFactory;

impl PolicyPackFactory {
    /// Create a retention policy with default configuration
    pub fn create_retention_policy() -> RetentionPolicy {
        RetentionPolicy::new(RetentionConfig::default())
    }

    /// Create a performance policy with default configuration
    pub fn create_performance_policy() -> PerformancePolicy {
        PerformancePolicy::new(PerformanceConfig::default())
    }

    /// Create a memory policy with default configuration
    pub fn create_memory_policy() -> MemoryPolicy {
        MemoryPolicy::new(MemoryConfig::default())
    }

    /// Create an artifacts policy with default configuration
    pub fn create_artifacts_policy() -> ArtifactsPolicy {
        ArtifactsPolicy::new(ArtifactsConfig::default())
    }

    /// Create a secrets policy with default configuration
    pub fn create_secrets_policy() -> SecretsPolicy {
        SecretsPolicy::new(SecretsConfig::default())
    }

    /// Create a build & release policy with default configuration
    pub fn create_build_release_policy() -> BuildReleasePolicy {
        BuildReleasePolicy::new(BuildReleaseConfig::default())
    }

    /// Create a compliance policy with default configuration
    pub fn create_compliance_policy() -> CompliancePolicy {
        CompliancePolicy::new(ComplianceConfig::default())
    }

    /// Create an incident policy with default configuration
    pub fn create_incident_policy() -> IncidentPolicy {
        IncidentPolicy::new(IncidentConfig::default())
    }

    /// Create an output policy with default configuration
    pub fn create_output_policy() -> OutputPolicy {
        OutputPolicy::new(OutputConfig::default())
    }

    /// Create an adapter lifecycle policy with default configuration
    pub fn create_adapter_lifecycle_policy() -> AdapterLifecyclePolicy {
        AdapterLifecyclePolicy::new(AdapterLifecycleConfig::default())
    }

    /// Create an egress policy with default configuration
    pub fn create_egress_policy() -> EgressPolicy {
        EgressPolicy::new(EgressConfig::default())
    }

    /// Create a determinism policy with default configuration
    pub fn create_determinism_policy() -> DeterminismPolicy {
        DeterminismPolicy::new(DeterminismConfig::default())
    }

    /// Create a router policy with default configuration
    pub fn create_router_policy() -> RouterPolicy {
        RouterPolicy::new(RouterConfig::default())
    }

    /// Create an evidence policy with default configuration
    pub fn create_evidence_policy() -> EvidencePolicy {
        EvidencePolicy::new(EvidenceConfig::default())
    }

    /// Create a refusal policy with default configuration
    pub fn create_refusal_policy() -> RefusalPolicy {
        RefusalPolicy::new(RefusalConfig::default())
    }

    /// Create a numeric policy with default configuration
    pub fn create_numeric_policy() -> NumericPolicy {
        NumericPolicy::new(NumericConfig::default())
    }

    /// Create a RAG policy with default configuration
    pub fn create_rag_policy() -> RagPolicy {
        RagPolicy::new(RagConfig::default())
    }

    /// Create an isolation policy with default configuration
    pub fn create_isolation_policy() -> IsolationPolicy {
        IsolationPolicy::new(IsolationConfig::default())
    }

    /// Create a telemetry policy with default configuration
    pub fn create_telemetry_policy() -> TelemetryPolicy {
        TelemetryPolicy::new(TelemetryConfig::default())
    }

    /// Create a deterministic I/O policy with default configuration
    pub fn create_deterministic_io_policy() -> DeterministicIoPolicy {
        DeterministicIoPolicy::new(DeterministicIoConfig::default())
    }

    /// Create a drift policy with default configuration
    pub fn create_drift_policy() -> DriftPolicy {
        DriftPolicy::new(DriftConfig::default())
    }

    /// Create an MPLoRA policy with default configuration
    pub fn create_adapteros_policy() -> MploraPolicy {
        MploraPolicy::new(MploraConfig::default())
    }

    /// Create a naming policy with default configuration
    pub fn create_naming_policy() -> NamingPolicy {
        NamingPolicy::new(NamingConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::{Policy, PolicyId};

    #[test]
    fn test_policy_pack_factory() {
        let retention_policy = PolicyPackFactory::create_retention_policy();
        let performance_policy = PolicyPackFactory::create_performance_policy();
        let memory_policy = PolicyPackFactory::create_memory_policy();
        let artifacts_policy = PolicyPackFactory::create_artifacts_policy();
        let secrets_policy = PolicyPackFactory::create_secrets_policy();
        let build_release_policy = PolicyPackFactory::create_build_release_policy();
        let compliance_policy = PolicyPackFactory::create_compliance_policy();
        let incident_policy = PolicyPackFactory::create_incident_policy();
        let output_policy = PolicyPackFactory::create_output_policy();
        let adapter_lifecycle_policy = PolicyPackFactory::create_adapter_lifecycle_policy();
        let egress_policy = PolicyPackFactory::create_egress_policy();
        let determinism_policy = PolicyPackFactory::create_determinism_policy();
        let router_policy = PolicyPackFactory::create_router_policy();
        let evidence_policy = PolicyPackFactory::create_evidence_policy();
        let refusal_policy = PolicyPackFactory::create_refusal_policy();
        let numeric_policy = PolicyPackFactory::create_numeric_policy();
        let rag_policy = PolicyPackFactory::create_rag_policy();
        let isolation_policy = PolicyPackFactory::create_isolation_policy();
        let telemetry_policy = PolicyPackFactory::create_telemetry_policy();
        let deterministic_io_policy = PolicyPackFactory::create_deterministic_io_policy();
        let drift_policy = PolicyPackFactory::create_drift_policy();
        let adapteros_policy = PolicyPackFactory::create_adapteros_policy();
        let naming_policy = PolicyPackFactory::create_naming_policy();

        // Verify all policies can be created
        assert_eq!(retention_policy.id(), PolicyId::Retention);
        assert_eq!(performance_policy.id(), PolicyId::Performance);
        assert_eq!(memory_policy.id(), PolicyId::Memory);
        assert_eq!(artifacts_policy.id(), PolicyId::Artifacts);
        assert_eq!(secrets_policy.id(), PolicyId::Secrets);
        assert_eq!(build_release_policy.id(), PolicyId::BuildRelease);
        assert_eq!(compliance_policy.id(), PolicyId::Compliance);
        assert_eq!(incident_policy.id(), PolicyId::Incident);
        assert_eq!(output_policy.id(), PolicyId::Output);
        assert_eq!(adapter_lifecycle_policy.id(), PolicyId::Adapters);
        assert_eq!(egress_policy.id(), PolicyId::Egress);
        assert_eq!(determinism_policy.id(), PolicyId::Determinism);
        assert_eq!(router_policy.id(), PolicyId::Router);
        assert_eq!(evidence_policy.id(), PolicyId::Evidence);
        assert_eq!(refusal_policy.id(), PolicyId::Refusal);
        assert_eq!(numeric_policy.id(), PolicyId::Numeric);
        assert_eq!(rag_policy.id(), PolicyId::Rag);
        assert_eq!(isolation_policy.id(), PolicyId::Isolation);
        assert_eq!(telemetry_policy.id(), PolicyId::Telemetry);
        assert_eq!(deterministic_io_policy.id(), PolicyId::DeterministicIo);
        assert_eq!(drift_policy.id(), PolicyId::Drift);
        assert_eq!(adapteros_policy.id(), PolicyId::Mplora);
        assert_eq!(naming_policy.id(), PolicyId::Naming);
    }
}
