//! Implementation of all 20 policy packs for AdapterOS
//!
//! This module provides concrete implementations of each policy pack
//! with validation logic, enforcement rules, and compliance reporting.
//!
//! # Citations
//! - Policy Pack #1-20: Complete implementation of all policy packs
//! - CLAUDE.md L142: "Policy Engine: Enforces 20 policy packs"
//! - .cursor/rules/global.mdc: Policy pack definitions and enforcement rules

use adapteros_core::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

/// Policy pack identifier
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum PolicyPackId {
    Egress,
    Determinism,
    Router,
    Evidence,
    Refusal,
    NumericUnits,
    RagIndex,
    Isolation,
    Telemetry,
    Retention,
    Performance,
    Memory,
    Artifacts,
    Secrets,
    BuildRelease,
    Compliance,
    Incident,
    LlmOutput,
    AdapterLifecycle,
    FullPack,
}

impl PolicyPackId {
    /// Get all policy pack IDs
    pub fn all() -> Vec<PolicyPackId> {
        vec![
            PolicyPackId::Egress,
            PolicyPackId::Determinism,
            PolicyPackId::Router,
            PolicyPackId::Evidence,
            PolicyPackId::Refusal,
            PolicyPackId::NumericUnits,
            PolicyPackId::RagIndex,
            PolicyPackId::Isolation,
            PolicyPackId::Telemetry,
            PolicyPackId::Retention,
            PolicyPackId::Performance,
            PolicyPackId::Memory,
            PolicyPackId::Artifacts,
            PolicyPackId::Secrets,
            PolicyPackId::BuildRelease,
            PolicyPackId::Compliance,
            PolicyPackId::Incident,
            PolicyPackId::LlmOutput,
            PolicyPackId::AdapterLifecycle,
            PolicyPackId::FullPack,
        ]
    }

    /// Get policy pack name
    pub fn name(&self) -> &'static str {
        match self {
            PolicyPackId::Egress => "Egress Ruleset",
            PolicyPackId::Determinism => "Determinism Ruleset",
            PolicyPackId::Router => "Router Ruleset",
            PolicyPackId::Evidence => "Evidence Ruleset",
            PolicyPackId::Refusal => "Refusal Ruleset",
            PolicyPackId::NumericUnits => "Numeric & Units Ruleset",
            PolicyPackId::RagIndex => "RAG Index Ruleset",
            PolicyPackId::Isolation => "Isolation Ruleset",
            PolicyPackId::Telemetry => "Telemetry Ruleset",
            PolicyPackId::Retention => "Retention Ruleset",
            PolicyPackId::Performance => "Performance Ruleset",
            PolicyPackId::Memory => "Memory Ruleset",
            PolicyPackId::Artifacts => "Artifacts Ruleset",
            PolicyPackId::Secrets => "Secrets Ruleset",
            PolicyPackId::BuildRelease => "Build & Release Ruleset",
            PolicyPackId::Compliance => "Compliance Ruleset",
            PolicyPackId::Incident => "Incident Ruleset",
            PolicyPackId::LlmOutput => "LLM Output Ruleset",
            PolicyPackId::AdapterLifecycle => "Adapter Lifecycle Ruleset",
            PolicyPackId::FullPack => "Full Pack Example",
        }
    }

    /// Get policy pack ID from name
    pub fn from_name(name: &str) -> Option<PolicyPackId> {
        match name {
            "Egress Ruleset" => Some(PolicyPackId::Egress),
            "Determinism Ruleset" => Some(PolicyPackId::Determinism),
            "Router Ruleset" => Some(PolicyPackId::Router),
            "Evidence Ruleset" => Some(PolicyPackId::Evidence),
            "Refusal Ruleset" => Some(PolicyPackId::Refusal),
            "Numeric & Units Ruleset" => Some(PolicyPackId::NumericUnits),
            "RAG Index Ruleset" => Some(PolicyPackId::RagIndex),
            "Isolation Ruleset" => Some(PolicyPackId::Isolation),
            "Telemetry Ruleset" => Some(PolicyPackId::Telemetry),
            "Retention Ruleset" => Some(PolicyPackId::Retention),
            "Performance Ruleset" => Some(PolicyPackId::Performance),
            "Memory Ruleset" => Some(PolicyPackId::Memory),
            "Artifacts Ruleset" => Some(PolicyPackId::Artifacts),
            "Secrets Ruleset" => Some(PolicyPackId::Secrets),
            "Build & Release Ruleset" => Some(PolicyPackId::BuildRelease),
            "Compliance Ruleset" => Some(PolicyPackId::Compliance),
            "Incident Ruleset" => Some(PolicyPackId::Incident),
            "LLM Output Ruleset" => Some(PolicyPackId::LlmOutput),
            "Adapter Lifecycle Ruleset" => Some(PolicyPackId::AdapterLifecycle),
            "Full Pack Example" => Some(PolicyPackId::FullPack),
            _ => None,
        }
    }

    /// Get policy pack description
    pub fn description(&self) -> &'static str {
        match self {
            PolicyPackId::Egress => "Zero data exfiltration during serving",
            PolicyPackId::Determinism => "Identical inputs produce identical outputs",
            PolicyPackId::Router => "Predictable, bounded adapter mixing",
            PolicyPackId::Evidence => "Answers cite sources or abstain",
            PolicyPackId::Refusal => "Safe no-answer behavior without hallucination",
            PolicyPackId::NumericUnits => "Prevent unit errors and fabricated numbers",
            PolicyPackId::RagIndex => "Strict per-tenant data boundaries",
            PolicyPackId::Isolation => "Process, file, and key isolation",
            PolicyPackId::Telemetry => "Observability for audit without disk melt",
            PolicyPackId::Retention => "Bounded storage and auditability",
            PolicyPackId::Performance => "Ensure serving stays snappy",
            PolicyPackId::Memory => "Avoid OOM, avoid thrash, keep quality",
            PolicyPackId::Artifacts => "Know exactly what you're running",
            PolicyPackId::Secrets => "Kill plaintext secrets and drift",
            PolicyPackId::BuildRelease => "No YOLO merges, no shadow kernels",
            PolicyPackId::Compliance => "Auditors get hashes, not hand-waving",
            PolicyPackId::Incident => "Predictable, documented reactions under stress",
            PolicyPackId::LlmOutput => "Outputs are parsable, attributable, and not loose",
            PolicyPackId::AdapterLifecycle => "Control sprawl and ensure adapters are useful",
            PolicyPackId::FullPack => "Complete policy pack example",
        }
    }
}

/// Policy pack configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyPackConfig {
    /// Policy pack ID
    pub id: PolicyPackId,

    /// Policy pack version (semver)
    pub version: String,

    /// Configuration data
    pub config: serde_json::Value,

    /// Whether the pack is enabled
    pub enabled: bool,

    /// Enforcement level
    pub enforcement_level: EnforcementLevel,

    /// Last updated timestamp
    pub last_updated: DateTime<Utc>,
}

impl PolicyPackConfig {
    /// Calculate BLAKE3 hash of the policy pack configuration
    ///
    /// Uses canonical JSON serialization for deterministic hashing.
    /// Per Determinism Ruleset #2: hash must be stable across runs.
    pub fn calculate_hash(&self) -> adapteros_core::B3Hash {
        // Serialize config to canonical JSON
        // Note: serde_json does not guarantee canonical ordering by default,
        // but for policy configs we control the structure so this is acceptable.
        // For production, consider using jcs (JSON Canonicalization Scheme).
        let json = serde_json::to_string(&self.config)
            .expect("Policy config must be serializable to JSON");

        adapteros_core::B3Hash::hash(json.as_bytes())
    }
}

/// Enforcement level
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EnforcementLevel {
    /// Informational only
    Info,

    /// Warning level
    Warning,

    /// Error level (blocks operation)
    Error,

    /// Critical level (system shutdown)
    Critical,
}

/// Policy pack validator trait
pub trait PolicyPackValidator {
    /// Validate a request against this policy pack
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult>;

    /// Get policy pack ID
    fn policy_pack_id(&self) -> PolicyPackId;

    /// Get policy pack name
    fn policy_pack_name(&self) -> &'static str;
}

/// Policy request for validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyRequest {
    /// Request identifier
    pub request_id: String,

    /// Request type
    pub request_type: RequestType,

    /// Tenant ID
    pub tenant_id: Option<String>,

    /// User ID
    pub user_id: Option<String>,

    /// Request context
    pub context: PolicyContext,

    /// Request metadata
    pub metadata: Option<serde_json::Value>,
}

/// Request types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestType {
    /// Inference request
    Inference,

    /// Adapter operation
    AdapterOperation,

    /// Memory operation
    MemoryOperation,

    /// Training operation
    TrainingOperation,

    /// Policy update
    PolicyUpdate,

    /// System operation
    SystemOperation,

    /// User operation
    UserOperation,

    /// Network operation
    NetworkOperation,

    /// File operation
    FileOperation,

    /// Database operation
    DatabaseOperation,
}

/// Policy context
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyContext {
    /// Component generating the request
    pub component: String,

    /// Operation being performed
    pub operation: String,

    /// Additional context data
    pub data: Option<serde_json::Value>,

    /// Request priority
    pub priority: Priority,
}

/// Request priority
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Priority {
    /// Low priority
    Low,

    /// Normal priority
    Normal,

    /// High priority
    High,

    /// Critical priority
    Critical,
}

/// Policy validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyValidationResult {
    /// Whether the request is valid
    pub valid: bool,

    /// Policy violations found
    pub violations: Vec<PolicyViolation>,

    /// Warnings
    pub warnings: Vec<PolicyWarning>,

    /// Validation timestamp
    pub timestamp: DateTime<Utc>,

    /// Validation duration
    pub duration_ms: u64,
}

/// Policy violation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyViolation {
    /// Violation identifier
    pub violation_id: String,

    /// Policy pack that was violated
    pub policy_pack: String,

    /// Violation severity
    pub severity: ViolationSeverity,

    /// Violation message
    pub message: String,

    /// Violation details
    pub details: Option<serde_json::Value>,

    /// Remediation steps
    pub remediation: Option<Vec<String>>,

    /// Violation timestamp
    pub timestamp: DateTime<Utc>,
}

/// Violation severity
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ViolationSeverity {
    /// Information
    Info,

    /// Warning
    Warning,

    /// Error
    Error,

    /// Critical
    Critical,

    /// Blocker
    Blocker,
}

/// Policy warning
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyWarning {
    /// Warning identifier
    pub warning_id: String,

    /// Policy pack
    pub policy_pack: String,

    /// Warning message
    pub message: String,

    /// Warning details
    pub details: Option<serde_json::Value>,

    /// Warning timestamp
    pub timestamp: DateTime<Utc>,
}

/// Policy pack manager
pub struct PolicyPackManager {
    /// Active policy packs
    packs: HashMap<PolicyPackId, Box<dyn PolicyPackValidator + Send + Sync>>,

    /// Policy pack configurations
    configs: HashMap<PolicyPackId, PolicyPackConfig>,
}

impl Default for PolicyPackManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyPackManager {
    /// Create a new policy pack manager
    pub fn new() -> Self {
        let mut manager = Self {
            packs: HashMap::new(),
            configs: HashMap::new(),
        };

        // Initialize all policy packs
        manager.initialize_policy_packs();

        manager
    }

    /// Initialize all policy packs
    fn initialize_policy_packs(&mut self) {
        info!("Initializing all 20 policy packs");

        // Register all policy pack validators
        self.register_pack(PolicyPackId::Egress, Box::new(EgressValidator::new()));
        self.register_pack(
            PolicyPackId::Determinism,
            Box::new(DeterminismValidator::new()),
        );
        self.register_pack(PolicyPackId::Router, Box::new(RouterValidator::new()));
        self.register_pack(PolicyPackId::Evidence, Box::new(EvidenceValidator::new()));
        self.register_pack(PolicyPackId::Refusal, Box::new(RefusalValidator::new()));
        self.register_pack(
            PolicyPackId::NumericUnits,
            Box::new(NumericUnitsValidator::new()),
        );
        self.register_pack(PolicyPackId::RagIndex, Box::new(RagIndexValidator::new()));
        self.register_pack(PolicyPackId::Isolation, Box::new(IsolationValidator::new()));
        self.register_pack(PolicyPackId::Telemetry, Box::new(TelemetryValidator::new()));
        self.register_pack(PolicyPackId::Retention, Box::new(RetentionValidator::new()));
        self.register_pack(
            PolicyPackId::Performance,
            Box::new(PerformanceValidator::new()),
        );
        self.register_pack(PolicyPackId::Memory, Box::new(MemoryValidator::new()));
        self.register_pack(PolicyPackId::Artifacts, Box::new(ArtifactsValidator::new()));
        self.register_pack(PolicyPackId::Secrets, Box::new(SecretsValidator::new()));
        self.register_pack(
            PolicyPackId::BuildRelease,
            Box::new(BuildReleaseValidator::new()),
        );
        self.register_pack(
            PolicyPackId::Compliance,
            Box::new(ComplianceValidator::new()),
        );
        self.register_pack(PolicyPackId::Incident, Box::new(IncidentValidator::new()));
        self.register_pack(PolicyPackId::LlmOutput, Box::new(LlmOutputValidator::new()));
        self.register_pack(
            PolicyPackId::AdapterLifecycle,
            Box::new(AdapterLifecycleValidator::new()),
        );
        self.register_pack(PolicyPackId::FullPack, Box::new(FullPackValidator::new()));

        // Set default configurations
        self.set_default_configurations();

        info!("All 20 policy packs initialized successfully");
    }

    /// Register a policy pack validator
    fn register_pack(
        &mut self,
        id: PolicyPackId,
        validator: Box<dyn PolicyPackValidator + Send + Sync>,
    ) {
        self.packs.insert(id, validator);
    }

    /// Set default configurations for all policy packs
    fn set_default_configurations(&mut self) {
        for id in PolicyPackId::all() {
            let config = PolicyPackConfig {
                id: id.clone(),
                version: "1.0.0".to_string(), // Default semver version
                config: self.get_default_config(&id),
                enabled: true,
                enforcement_level: EnforcementLevel::Error,
                last_updated: Utc::now(),
            };
            self.configs.insert(id, config);
        }
    }

    /// Get configuration for a policy pack
    pub fn get_config(&self, id: &PolicyPackId) -> Option<&PolicyPackConfig> {
        self.configs.get(id)
    }

    /// Get default configuration for a policy pack
    fn get_default_config(&self, id: &PolicyPackId) -> serde_json::Value {
        match id {
            PolicyPackId::Egress => serde_json::json!({
                "mode": "deny_all",
                "serve_requires_pf": true,
                "allow_tcp": false,
                "allow_udp": false,
                "uds_paths": ["/var/run/aos/<tenant>/*.sock"],
                "media_import": {"require_signature": true, "require_sbom": true}
            }),
            PolicyPackId::Determinism => serde_json::json!({
                "require_metallib_embed": true,
                "require_kernel_hash_match": true,
                "rng": "hkdf_seeded",
                "retrieval_tie_break": ["score_desc", "doc_id_asc"]
            }),
            PolicyPackId::Router => serde_json::json!({
                "k_sparse": 3,
                "gate_quant": "q15",
                "entropy_floor": 0.02,
                "sample_tokens_full": 128
            }),
            PolicyPackId::Evidence => serde_json::json!({
                "require_open_book": true,
                "min_spans": 1,
                "prefer_latest_revision": true,
                "warn_on_superseded": true
            }),
            PolicyPackId::Refusal => serde_json::json!({
                "abstain_threshold": 0.55,
                "missing_fields_templates": {
                    "torque_spec": ["aircraft_effectivity", "component_pn"]
                }
            }),
            PolicyPackId::NumericUnits => serde_json::json!({
                "canonical_units": {"torque": "in_lbf", "pressure": "psi"},
                "max_rounding_error": 0.5,
                "require_units_in_trace": true
            }),
            PolicyPackId::RagIndex => serde_json::json!({
                "index_scope": "per_tenant",
                "doc_tags_required": ["doc_id", "rev", "effectivity", "source_type"],
                "embedding_model_hash": "b3:...",
                "topk": 5,
                "order": ["score_desc", "doc_id_asc"]
            }),
            PolicyPackId::Isolation => serde_json::json!({
                "process_model": "per_tenant",
                "uds_root": "/var/run/aos/<tenant>",
                "forbid_shm": true,
                "keys": {"backend": "secure_enclave", "require_hardware": true}
            }),
            PolicyPackId::Telemetry => serde_json::json!({
                "schema_hash": "b3:...",
                "sampling": {"token": 0.05, "router": 1.0, "inference": 1.0},
                "router_full_tokens": 128,
                "bundle": {"max_events": 500000, "max_bytes": 268435456}
            }),
            PolicyPackId::Retention => serde_json::json!({
                "keep_bundles_per_cpid": 12,
                "keep_incident_bundles": true,
                "keep_promotion_bundles": true,
                "evict_strategy": "oldest_first_safe"
            }),
            PolicyPackId::Performance => serde_json::json!({
                "latency_p95_ms": 24,
                "router_overhead_pct_max": 8,
                "throughput_tokens_per_s_min": 40
            }),
            PolicyPackId::Memory => serde_json::json!({
                "min_headroom_pct": 15,
                "evict_order": ["ephemeral_ttl", "cold_lru", "warm_lru"],
                "k_reduce_before_evict": true
            }),
            PolicyPackId::Artifacts => serde_json::json!({
                "require_signature": true,
                "require_sbom": true,
                "cas_only": true
            }),
            PolicyPackId::Secrets => serde_json::json!({
                "env_allowed": [],
                "keystore": "secure_enclave",
                "rotate_on_promotion": true
            }),
            PolicyPackId::BuildRelease => serde_json::json!({
                "require_replay_zero_diff": true,
                "hallucination_thresholds": {"arr_min": 0.95, "ecs5_min": 0.75, "hlr_max": 0.03, "cr_max": 0.01},
                "require_signed_plan": true,
                "require_rollback_plan": true
            }),
            PolicyPackId::Compliance => serde_json::json!({
                "control_matrix_hash": "b3:...",
                "require_evidence_links": true,
                "require_itar_suite_green": true
            }),
            PolicyPackId::Incident => serde_json::json!({
                "memory": ["drop_ephemeral", "reduce_k", "evict_cold", "deny_new_sessions"],
                "router_skew": ["entropy_floor_on", "cap_activation", "recalibrate", "rebuild_plan"],
                "determinism": ["freeze_plan", "export_bundle", "diff_hashes", "rollback"],
                "violation": ["isolate", "export_bundle", "rotate_keys", "open_ticket"]
            }),
            PolicyPackId::LlmOutput => serde_json::json!({
                "format": "json",
                "require_trace": true,
                "forbidden_topics": ["tenant_crossing", "export_control_bypass"]
            }),
            PolicyPackId::AdapterLifecycle => serde_json::json!({
                "min_activation_pct": 2.0,
                "min_quality_delta": 0.5,
                "require_registry_admit": true
            }),
            PolicyPackId::FullPack => serde_json::json!({
                "schema": "adapteros.policy.v1",
                "packs": {
                    "egress": {"mode": "deny_all", "serve_requires_pf": true, "allow_tcp": false, "allow_udp": false, "uds_paths": ["/var/run/aos/<tenant>/*.sock"], "media_import": {"require_signature": true, "require_sbom": true}},
                    "determinism": {"require_metallib_embed": true, "require_kernel_hash_match": true, "rng": "hkdf_seeded", "retrieval_tie_break": ["score_desc", "doc_id_asc"]},
                    "router": {"k_sparse": 3, "gate_quant": "q15", "entropy_floor": 0.02, "sample_tokens_full": 128},
                    "evidence": {"require_open_book": true, "min_spans": 1, "prefer_latest_revision": true, "warn_on_superseded": true},
                    "refusal": {"abstain_threshold": 0.55, "missing_fields_templates": {"torque_spec": ["aircraft_effectivity", "component_pn"]}},
                    "numeric": {"canonical_units": {"torque": "in_lbf", "pressure": "psi"}, "max_rounding_error": 0.5, "require_units_in_trace": true},
                    "rag": {"index_scope": "per_tenant", "doc_tags_required": ["doc_id", "rev", "effectivity", "source_type"], "embedding_model_hash": "b3:...", "topk": 5, "order": ["score_desc", "doc_id_asc"]},
                    "isolation": {"process_model": "per_tenant", "uds_root": "/var/run/aos/<tenant>", "forbid_shm": true, "keys": {"backend": "secure_enclave", "require_hardware": true}},
                    "telemetry": {"schema_hash": "b3:...", "sampling": {"token": 0.05, "router": 1.0, "inference": 1.0}, "router_full_tokens": 128, "bundle": {"max_events": 500000, "max_bytes": 268435456}},
                    "retention": {"keep_bundles_per_cpid": 12, "keep_incident_bundles": true, "keep_promotion_bundles": true, "evict_strategy": "oldest_first_safe"},
                    "performance": {"latency_p95_ms": 24, "router_overhead_pct_max": 8, "throughput_tokens_per_s_min": 40},
                    "memory": {"min_headroom_pct": 15, "evict_order": ["ephemeral_ttl", "cold_lru", "warm_lru"], "k_reduce_before_evict": true},
                    "artifacts": {"require_signature": true, "require_sbom": true, "cas_only": true},
                    "secrets": {"env_allowed": [], "keystore": "secure_enclave", "rotate_on_promotion": true},
                    "build_release": {"require_replay_zero_diff": true, "hallucination_thresholds": {"arr_min": 0.95, "ecs5_min": 0.75, "hlr_max": 0.03, "cr_max": 0.01}, "require_signed_plan": true, "require_rollback_plan": true},
                    "compliance": {"control_matrix_hash": "b3:...", "require_evidence_links": true, "require_itar_suite_green": true},
                    "incident": {"memory": ["drop_ephemeral", "reduce_k", "evict_cold", "deny_new_sessions"], "router_skew": ["entropy_floor_on", "cap_activation", "recalibrate", "rebuild_plan"], "determinism": ["freeze_plan", "export_bundle", "diff_hashes", "rollback"], "violation": ["isolate", "export_bundle", "rotate_keys", "open_ticket"]},
                    "output": {"format": "json", "require_trace": true, "forbidden_topics": ["tenant_crossing", "export_control_bypass"]},
                    "adapters": {"min_activation_pct": 2.0, "min_quality_delta": 0.5, "require_registry_admit": true}
                }
            }),
        }
    }

    /// Validate a request against all active policy packs
    pub fn validate_request(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let start_time = std::time::Instant::now();
        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        debug!(
            request_id = %request.request_id,
            request_type = ?request.request_type,
            "Validating request against all policy packs"
        );

        // Validate against each active policy pack
        for (pack_id, validator) in &self.packs {
            if let Some(config) = self.configs.get(pack_id) {
                if !config.enabled {
                    continue;
                }

                match validator.validate(request) {
                    Ok(result) => {
                        violations.extend(result.violations);
                        warnings.extend(result.warnings);
                    }
                    Err(e) => {
                        error!(
                            policy_pack = %pack_id.name(),
                            error = %e,
                            "Policy pack validation failed"
                        );

                        violations.push(PolicyViolation {
                            violation_id: Uuid::new_v4().to_string(),
                            policy_pack: pack_id.name().to_string(),
                            severity: ViolationSeverity::Error,
                            message: format!("Policy pack validation failed: {}", e),
                            details: Some(serde_json::json!({"error": e.to_string()})),
                            remediation: Some(vec!["Check policy pack configuration".to_string()]),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        let duration = start_time.elapsed();

        // Determine validity based on enforcement levels
        let mut valid = true;
        for violation in &violations {
            if let Some(pack_id) = PolicyPackId::from_name(&violation.policy_pack) {
                if let Some(config) = self.configs.get(&pack_id) {
                    match config.enforcement_level {
                        EnforcementLevel::Info => {
                            // Info level violations don't block operations
                            continue;
                        }
                        EnforcementLevel::Warning => {
                            // Warning level violations don't block operations unless they're Error severity
                            if matches!(
                                violation.severity,
                                ViolationSeverity::Error
                                    | ViolationSeverity::Critical
                                    | ViolationSeverity::Blocker
                            ) {
                                valid = false;
                                break;
                            }
                        }
                        EnforcementLevel::Error => {
                            // Error level violations block operations only for Error, Critical, or Blocker severity
                            if matches!(
                                violation.severity,
                                ViolationSeverity::Error
                                    | ViolationSeverity::Critical
                                    | ViolationSeverity::Blocker
                            ) {
                                valid = false;
                                break;
                            }
                        }
                        EnforcementLevel::Critical => {
                            // Critical level violations always block operations
                            valid = false;
                            break;
                        }
                    }
                }
            }
        }

        let result = PolicyValidationResult {
            valid,
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: duration.as_millis() as u64,
        };

        if !result.valid {
            warn!(
                request_id = %request.request_id,
                violation_count = result.violations.len(),
                warning_count = result.warnings.len(),
                duration_ms = result.duration_ms,
                "Request validation failed"
            );
        } else {
            debug!(
                request_id = %request.request_id,
                warning_count = result.warnings.len(),
                duration_ms = result.duration_ms,
                "Request validation passed"
            );
        }

        Ok(result)
    }

    /// Get policy pack configuration
    pub fn get_pack_config(&self, pack_id: &PolicyPackId) -> Option<&PolicyPackConfig> {
        self.configs.get(pack_id)
    }

    /// Update policy pack configuration
    pub fn update_pack_config(
        &mut self,
        pack_id: PolicyPackId,
        config: PolicyPackConfig,
    ) -> Result<()> {
        info!(
            policy_pack = %pack_id.name(),
            "Updating policy pack configuration"
        );

        self.configs.insert(pack_id, config);
        Ok(())
    }

    /// Get all policy pack configurations
    pub fn get_all_configs(&self) -> &HashMap<PolicyPackId, PolicyPackConfig> {
        &self.configs
    }

    /// Get a policy pack validator by ID
    pub fn get_validator(&self, pack_id: &PolicyPackId) -> Option<&(dyn PolicyPackValidator + Send + Sync)> {
        self.packs.get(pack_id).map(|v| v.as_ref())
    }

    /// Enable or disable a policy pack
    pub fn set_pack_enabled(&mut self, pack_id: PolicyPackId, enabled: bool) -> Result<()> {
        if let Some(config) = self.configs.get_mut(&pack_id) {
            config.enabled = enabled;
            config.last_updated = Utc::now();

            info!(
                policy_pack = %pack_id.name(),
                enabled = enabled,
                "Policy pack enabled/disabled"
            );
        }

        Ok(())
    }
}

// Policy pack validator implementations

/// Egress policy pack validator
pub struct EgressValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for EgressValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl EgressValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "mode": "deny_all",
                "serve_requires_pf": true,
                "allow_tcp": false,
                "allow_udp": false,
                "uds_paths": ["/var/run/aos/<tenant>/*.sock"],
                "media_import": {"require_signature": true, "require_sbom": true}
            }),
        }
    }
}

impl PolicyPackValidator for EgressValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check for network operations and protocol violations
        if matches!(request.request_type, RequestType::NetworkOperation)
            || matches!(request.request_type, RequestType::Inference)
        {
            // Check for DNS resolution attempts
            if request.context.operation == "dns_resolution" {
                violations.push(PolicyViolation {
                    violation_id: Uuid::new_v4().to_string(),
                    policy_pack: "Egress Ruleset".to_string(),
                    severity: ViolationSeverity::Error,
                    message: "DNS resolution requests are not allowed".to_string(),
                    details: Some(serde_json::json!({"operation": "dns_resolution"})),
                    remediation: Some(vec!["DNS resolution is blocked for security".to_string()]),
                    timestamp: Utc::now(),
                });
            }

            if let Some(data) = &request.context.data {
                if let Some(protocol) = data.get("protocol") {
                    if protocol == "tcp" || protocol == "udp" {
                        violations.push(PolicyViolation {
                            violation_id: Uuid::new_v4().to_string(),
                            policy_pack: "Egress Ruleset".to_string(),
                            severity: ViolationSeverity::Blocker,
                            message: "TCP/UDP connections are not allowed".to_string(),
                            details: Some(serde_json::json!({"protocol": protocol})),
                            remediation: Some(vec!["Use Unix domain sockets only".to_string()]),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Egress
    }

    fn policy_pack_name(&self) -> &'static str {
        "Egress Ruleset"
    }
}

/// Determinism policy pack validator
pub struct DeterminismValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for DeterminismValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl DeterminismValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "require_metallib_embed": true,
                "require_kernel_hash_match": true,
                "rng": "hkdf_seeded",
                "retrieval_tie_break": ["score_desc", "doc_id_asc"]
            }),
        }
    }
}

impl PolicyPackValidator for DeterminismValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check for runtime kernel compilation
        if request.context.operation == "kernel_compile" {
            violations.push(PolicyViolation {
                violation_id: Uuid::new_v4().to_string(),
                policy_pack: "Determinism Ruleset".to_string(),
                severity: ViolationSeverity::Error,
                message: "Runtime kernel compilation is not allowed".to_string(),
                details: Some(serde_json::json!({"operation": "kernel_compile"})),
                remediation: Some(vec!["Use precompiled .metallib blobs".to_string()]),
                timestamp: Utc::now(),
            });
        }

        // Check for non-HKDF RNG usage
        if let Some(data) = &request.context.data {
            if let Some(rng_type) = data.get("rng_type") {
                if rng_type != "hkdf_seeded" {
                    violations.push(PolicyViolation {
                        violation_id: Uuid::new_v4().to_string(),
                        policy_pack: "Determinism Ruleset".to_string(),
                        severity: ViolationSeverity::Error,
                        message: "Non-HKDF RNG usage detected".to_string(),
                        details: Some(serde_json::json!({"rng_type": rng_type})),
                        remediation: Some(vec!["Use HKDF-seeded RNG only".to_string()]),
                        timestamp: Utc::now(),
                    });
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Determinism
    }

    fn policy_pack_name(&self) -> &'static str {
        "Determinism Ruleset"
    }
}

/// Router policy pack validator
pub struct RouterValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for RouterValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl RouterValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "k_sparse": 3,
                "gate_quant": "q15",
                "entropy_floor": 0.02,
                "sample_tokens_full": 128
            }),
        }
    }
}

impl PolicyPackValidator for RouterValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check K-sparse configuration
        if let Some(data) = &request.context.data {
            if let Some(k_value) = data.get("k_sparse") {
                if let Some(k) = k_value.as_u64() {
                    if k > 3 {
                        violations.push(PolicyViolation {
                            violation_id: Uuid::new_v4().to_string(),
                            policy_pack: "Router Ruleset".to_string(),
                            severity: ViolationSeverity::Error,
                            message: "K-sparse value exceeds maximum".to_string(),
                            details: Some(serde_json::json!({"k_sparse": k})),
                            remediation: Some(vec![
                                "Reduce K-sparse value to maximum of 3".to_string()
                            ]),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        // Check gate quantization
        if let Some(data) = &request.context.data {
            if let Some(quant_type) = data.get("gate_quant") {
                if quant_type != "q15" {
                    violations.push(PolicyViolation {
                        violation_id: Uuid::new_v4().to_string(),
                        policy_pack: "Router Ruleset".to_string(),
                        severity: ViolationSeverity::Error,
                        message: "Gate quantization must be Q15".to_string(),
                        details: Some(serde_json::json!({"gate_quant": quant_type})),
                        remediation: Some(vec!["Use Q15 quantization for gates".to_string()]),
                        timestamp: Utc::now(),
                    });
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Router
    }

    fn policy_pack_name(&self) -> &'static str {
        "Router Ruleset"
    }
}

/// Evidence policy pack validator
pub struct EvidenceValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for EvidenceValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl EvidenceValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "require_open_book": true,
                "min_spans": 1,
                "prefer_latest_revision": true,
                "warn_on_superseded": true
            }),
        }
    }
}

impl PolicyPackValidator for EvidenceValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check for evidence requirements
        if matches!(request.request_type, RequestType::Inference) {
            if let Some(data) = &request.context.data {
                if let Some(evidence_spans) = data.get("evidence_spans") {
                    if let Some(spans) = evidence_spans.as_array() {
                        if spans.is_empty() {
                            violations.push(PolicyViolation {
                                violation_id: Uuid::new_v4().to_string(),
                                policy_pack: "Evidence Ruleset".to_string(),
                                severity: ViolationSeverity::Error,
                                message: "Evidence spans are required for inference".to_string(),
                                details: Some(serde_json::json!({"evidence_spans": spans})),
                                remediation: Some(vec![
                                    "Include at least one evidence span".to_string()
                                ]),
                                timestamp: Utc::now(),
                            });
                        }
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Evidence
    }

    fn policy_pack_name(&self) -> &'static str {
        "Evidence Ruleset"
    }
}

/// Refusal policy pack validator
pub struct RefusalValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for RefusalValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl RefusalValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "abstain_threshold": 0.55,
                "missing_fields_templates": {
                    "torque_spec": ["aircraft_effectivity", "component_pn"]
                }
            }),
        }
    }
}

impl PolicyPackValidator for RefusalValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check confidence thresholds
        if let Some(data) = &request.context.data {
            if let Some(confidence) = data.get("confidence") {
                if let Some(conf) = confidence.as_f64() {
                    if conf < 0.55 {
                        violations.push(PolicyViolation {
                            violation_id: Uuid::new_v4().to_string(),
                            policy_pack: "Refusal Ruleset".to_string(),
                            severity: ViolationSeverity::Warning,
                            message: "Low confidence response should be refused".to_string(),
                            details: Some(serde_json::json!({"confidence": conf})),
                            remediation: Some(vec![
                                "Refuse response due to low confidence".to_string()
                            ]),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Refusal
    }

    fn policy_pack_name(&self) -> &'static str {
        "Refusal Ruleset"
    }
}

/// Numeric Units policy pack validator
pub struct NumericUnitsValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for NumericUnitsValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl NumericUnitsValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "canonical_units": {"torque": "in_lbf", "pressure": "psi"},
                "max_rounding_error": 0.5,
                "require_units_in_trace": true
            }),
        }
    }
}

impl PolicyPackValidator for NumericUnitsValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check for unit requirements
        if let Some(data) = &request.context.data {
            if let Some(numeric_values) = data.get("numeric_values") {
                if let Some(values) = numeric_values.as_array() {
                    for value in values {
                        if let Some(unit) = value.get("unit") {
                            if unit.is_null() {
                                violations.push(PolicyViolation {
                                    violation_id: Uuid::new_v4().to_string(),
                                    policy_pack: "Numeric & Units Ruleset".to_string(),
                                    severity: ViolationSeverity::Error,
                                    message: "Units are required for numeric values".to_string(),
                                    details: Some(serde_json::json!({"value": value})),
                                    remediation: Some(vec![
                                        "Include units for all numeric values".to_string()
                                    ]),
                                    timestamp: Utc::now(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::NumericUnits
    }

    fn policy_pack_name(&self) -> &'static str {
        "Numeric & Units Ruleset"
    }
}

/// RAG Index policy pack validator
pub struct RagIndexValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for RagIndexValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl RagIndexValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "index_scope": "per_tenant",
                "doc_tags_required": ["doc_id", "rev", "effectivity", "source_type"],
                "embedding_model_hash": "b3:...",
                "topk": 5,
                "order": ["score_desc", "doc_id_asc"]
            }),
        }
    }
}

impl PolicyPackValidator for RagIndexValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check tenant isolation
        if let Some(data) = &request.context.data {
            if let Some(tenant_id) = data.get("tenant_id") {
                if let Some(cross_tenant) = data.get("cross_tenant_access") {
                    if cross_tenant.as_bool().unwrap_or(false) {
                        violations.push(PolicyViolation {
                            violation_id: Uuid::new_v4().to_string(),
                            policy_pack: "RAG Index Ruleset".to_string(),
                            severity: ViolationSeverity::Blocker,
                            message: "Cross-tenant access detected".to_string(),
                            details: Some(serde_json::json!({"tenant_id": tenant_id})),
                            remediation: Some(vec!["Enforce per-tenant isolation".to_string()]),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::RagIndex
    }

    fn policy_pack_name(&self) -> &'static str {
        "RAG Index Ruleset"
    }
}

/// Isolation policy pack validator
pub struct IsolationValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for IsolationValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl IsolationValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "process_model": "per_tenant",
                "uds_root": "/var/run/aos/<tenant>",
                "forbid_shm": true,
                "keys": {"backend": "secure_enclave", "require_hardware": true}
            }),
        }
    }
}

impl PolicyPackValidator for IsolationValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check for shared memory usage
        if let Some(data) = &request.context.data {
            if let Some(use_shm) = data.get("use_shared_memory") {
                if use_shm.as_bool().unwrap_or(false) {
                    violations.push(PolicyViolation {
                        violation_id: Uuid::new_v4().to_string(),
                        policy_pack: "Isolation Ruleset".to_string(),
                        severity: ViolationSeverity::Error,
                        message: "Shared memory usage is forbidden".to_string(),
                        details: Some(serde_json::json!({"use_shared_memory": use_shm})),
                        remediation: Some(vec!["Disable shared memory usage".to_string()]),
                        timestamp: Utc::now(),
                    });
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Isolation
    }

    fn policy_pack_name(&self) -> &'static str {
        "Isolation Ruleset"
    }
}

/// Telemetry policy pack validator
pub struct TelemetryValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for TelemetryValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl TelemetryValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "schema_hash": "b3:...",
                "sampling": {"token": 0.05, "router": 1.0, "inference": 1.0},
                "router_full_tokens": 128,
                "bundle": {"max_events": 500000, "max_bytes": 268435456}
            }),
        }
    }
}

impl PolicyPackValidator for TelemetryValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check telemetry sampling rates
        if let Some(data) = &request.context.data {
            if let Some(sampling_rate) = data.get("sampling_rate") {
                if let Some(rate) = sampling_rate.as_f64() {
                    if rate > 1.0 {
                        violations.push(PolicyViolation {
                            violation_id: Uuid::new_v4().to_string(),
                            policy_pack: "Telemetry Ruleset".to_string(),
                            severity: ViolationSeverity::Warning,
                            message: "Sampling rate exceeds maximum".to_string(),
                            details: Some(serde_json::json!({"sampling_rate": rate})),
                            remediation: Some(vec![
                                "Reduce sampling rate to maximum of 1.0".to_string()
                            ]),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Telemetry
    }

    fn policy_pack_name(&self) -> &'static str {
        "Telemetry Ruleset"
    }
}

/// Retention policy pack validator
pub struct RetentionValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for RetentionValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl RetentionValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "keep_bundles_per_cpid": 12,
                "keep_incident_bundles": true,
                "keep_promotion_bundles": true,
                "evict_strategy": "oldest_first_safe"
            }),
        }
    }
}

impl PolicyPackValidator for RetentionValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let violations = Vec::new();
        let mut warnings = Vec::new();

        // Check bundle retention limits
        if let Some(data) = &request.context.data {
            if let Some(bundle_count) = data.get("bundle_count") {
                if let Some(count) = bundle_count.as_u64() {
                    if count > 12 {
                        warnings.push(PolicyWarning {
                            warning_id: Uuid::new_v4().to_string(),
                            policy_pack: "Retention Ruleset".to_string(),
                            message: "Bundle count exceeds retention limit".to_string(),
                            details: Some(serde_json::json!({"bundle_count": count})),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Retention
    }

    fn policy_pack_name(&self) -> &'static str {
        "Retention Ruleset"
    }
}

/// Performance policy pack validator
pub struct PerformanceValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for PerformanceValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "latency_p95_ms": 24,
                "router_overhead_pct_max": 8,
                "throughput_tokens_per_s_min": 40
            }),
        }
    }
}

impl PolicyPackValidator for PerformanceValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check latency requirements
        if let Some(data) = &request.context.data {
            if let Some(latency_p95) = data.get("latency_p95_ms") {
                if let Some(latency) = latency_p95.as_f64() {
                    if latency > 24.0 {
                        violations.push(PolicyViolation {
                            violation_id: Uuid::new_v4().to_string(),
                            policy_pack: "Performance Ruleset".to_string(),
                            severity: ViolationSeverity::Error,
                            message: "Latency exceeds p95 budget".to_string(),
                            details: Some(serde_json::json!({"latency_p95_ms": latency})),
                            remediation: Some(vec![
                                "Optimize performance to meet latency budget".to_string()
                            ]),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Performance
    }

    fn policy_pack_name(&self) -> &'static str {
        "Performance Ruleset"
    }
}

/// Memory policy pack validator
pub struct MemoryValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for MemoryValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "min_headroom_pct": 15,
                "evict_order": ["ephemeral_ttl", "cold_lru", "warm_lru"],
                "k_reduce_before_evict": true
            }),
        }
    }
}

impl PolicyPackValidator for MemoryValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check memory headroom
        if let Some(data) = &request.context.data {
            if let Some(headroom_pct) = data.get("headroom_pct") {
                if let Some(headroom) = headroom_pct.as_f64() {
                    if headroom < 15.0 {
                        violations.push(PolicyViolation {
                            violation_id: Uuid::new_v4().to_string(),
                            policy_pack: "Memory Ruleset".to_string(),
                            severity: ViolationSeverity::Error,
                            message: "Memory headroom below minimum threshold".to_string(),
                            details: Some(serde_json::json!({"headroom_pct": headroom})),
                            remediation: Some(vec![
                                "Increase memory headroom to at least 15%".to_string()
                            ]),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Memory
    }

    fn policy_pack_name(&self) -> &'static str {
        "Memory Ruleset"
    }
}

/// Artifacts policy pack validator
pub struct ArtifactsValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for ArtifactsValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl ArtifactsValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "require_signature": true,
                "require_sbom": true,
                "cas_only": true
            }),
        }
    }
}

impl PolicyPackValidator for ArtifactsValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check artifact signature requirements
        if let Some(data) = &request.context.data {
            if let Some(artifact) = data.get("artifact") {
                if let Some(signature) = artifact.get("signature") {
                    if signature.is_null() {
                        violations.push(PolicyViolation {
                            violation_id: Uuid::new_v4().to_string(),
                            policy_pack: "Artifacts Ruleset".to_string(),
                            severity: ViolationSeverity::Blocker,
                            message: "Artifact signature is required".to_string(),
                            details: Some(serde_json::json!({"artifact": artifact})),
                            remediation: Some(vec!["Sign artifact with Ed25519".to_string()]),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Artifacts
    }

    fn policy_pack_name(&self) -> &'static str {
        "Artifacts Ruleset"
    }
}

/// Secrets policy pack validator
pub struct SecretsValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for SecretsValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl SecretsValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "env_allowed": [],
                "keystore": "secure_enclave",
                "rotate_on_promotion": true
            }),
        }
    }
}

impl PolicyPackValidator for SecretsValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check for plaintext secrets
        if let Some(data) = &request.context.data {
            if let Some(secrets) = data.get("secrets") {
                if let Some(secret_list) = secrets.as_array() {
                    for secret in secret_list {
                        if let Some(plaintext) = secret.get("plaintext") {
                            if plaintext.as_bool().unwrap_or(false) {
                                violations.push(PolicyViolation {
                                    violation_id: Uuid::new_v4().to_string(),
                                    policy_pack: "Secrets Ruleset".to_string(),
                                    severity: ViolationSeverity::Blocker,
                                    message: "Plaintext secrets are not allowed".to_string(),
                                    details: Some(serde_json::json!({"secret": secret})),
                                    remediation: Some(vec![
                                        "Use Secure Enclave for secret storage".to_string(),
                                    ]),
                                    timestamp: Utc::now(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Secrets
    }

    fn policy_pack_name(&self) -> &'static str {
        "Secrets Ruleset"
    }
}

/// Build Release policy pack validator
pub struct BuildReleaseValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for BuildReleaseValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl BuildReleaseValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "require_replay_zero_diff": true,
                "hallucination_thresholds": {"arr_min": 0.95, "ecs5_min": 0.75, "hlr_max": 0.03, "cr_max": 0.01},
                "require_signed_plan": true,
                "require_rollback_plan": true
            }),
        }
    }
}

impl PolicyPackValidator for BuildReleaseValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check replay determinism
        if let Some(data) = &request.context.data {
            if let Some(replay_diff) = data.get("replay_diff") {
                if let Some(diff) = replay_diff.as_f64() {
                    if diff > 0.0 {
                        violations.push(PolicyViolation {
                            violation_id: Uuid::new_v4().to_string(),
                            policy_pack: "Build & Release Ruleset".to_string(),
                            severity: ViolationSeverity::Blocker,
                            message: "Replay shows non-zero diff".to_string(),
                            details: Some(serde_json::json!({"replay_diff": diff})),
                            remediation: Some(vec![
                                "Fix determinism issues before release".to_string()
                            ]),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::BuildRelease
    }

    fn policy_pack_name(&self) -> &'static str {
        "Build & Release Ruleset"
    }
}

/// Compliance policy pack validator
pub struct ComplianceValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for ComplianceValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl ComplianceValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "control_matrix_hash": "b3:...",
                "require_evidence_links": true,
                "require_itar_suite_green": true
            }),
        }
    }
}

impl PolicyPackValidator for ComplianceValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check compliance evidence
        if let Some(data) = &request.context.data {
            if let Some(compliance) = data.get("compliance") {
                if let Some(evidence_links) = compliance.get("evidence_links") {
                    if evidence_links.is_null() {
                        violations.push(PolicyViolation {
                            violation_id: Uuid::new_v4().to_string(),
                            policy_pack: "Compliance Ruleset".to_string(),
                            severity: ViolationSeverity::Error,
                            message: "Compliance evidence links are required".to_string(),
                            details: Some(serde_json::json!({"compliance": compliance})),
                            remediation: Some(vec![
                                "Provide evidence links for compliance".to_string()
                            ]),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Compliance
    }

    fn policy_pack_name(&self) -> &'static str {
        "Compliance Ruleset"
    }
}

/// Incident policy pack validator
pub struct IncidentValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for IncidentValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl IncidentValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "memory": ["drop_ephemeral", "reduce_k", "evict_cold", "deny_new_sessions"],
                "router_skew": ["entropy_floor_on", "cap_activation", "recalibrate", "rebuild_plan"],
                "determinism": ["freeze_plan", "export_bundle", "diff_hashes", "rollback"],
                "violation": ["isolate", "export_bundle", "rotate_keys", "open_ticket"]
            }),
        }
    }
}

impl PolicyPackValidator for IncidentValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Check incident response procedures
        if let Some(data) = &request.context.data {
            if let Some(incident_type) = data.get("incident_type") {
                if let Some(procedures) = data.get("procedures") {
                    if procedures.is_null() {
                        violations.push(PolicyViolation {
                            violation_id: Uuid::new_v4().to_string(),
                            policy_pack: "Incident Ruleset".to_string(),
                            severity: ViolationSeverity::Error,
                            message: "Incident response procedures are required".to_string(),
                            details: Some(serde_json::json!({"incident_type": incident_type})),
                            remediation: Some(vec![
                                "Follow documented incident response procedures".to_string(),
                            ]),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::Incident
    }

    fn policy_pack_name(&self) -> &'static str {
        "Incident Ruleset"
    }
}

/// LLM Output policy pack validator
pub struct LlmOutputValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for LlmOutputValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl LlmOutputValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "format": "json",
                "require_trace": true,
                "forbidden_topics": ["tenant_crossing", "export_control_bypass"]
            }),
        }
    }
}

impl PolicyPackValidator for LlmOutputValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let mut warnings = Vec::new();

        // Check output format requirements
        if let Some(data) = &request.context.data {
            if let Some(output_format) = data.get("output_format") {
                if output_format != "json" {
                    violations.push(PolicyViolation {
                        violation_id: Uuid::new_v4().to_string(),
                        policy_pack: "LLM Output Ruleset".to_string(),
                        severity: ViolationSeverity::Error,
                        message: "Output format must be JSON".to_string(),
                        details: Some(serde_json::json!({"output_format": output_format})),
                        remediation: Some(vec!["Use JSON format for all outputs".to_string()]),
                        timestamp: Utc::now(),
                    });
                }
            }
        }

        // If trace is required, warn when no evidence is present in response trace
        if let Some(data) = &request.context.data {
            let require_trace = self
                .config
                .get("require_trace")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);

            if require_trace {
                let evidence_len = data
                    .get("trace")
                    .and_then(|t| t.get("evidence"))
                    .and_then(|e| e.as_array())
                    .map(|arr| arr.len())
                    .unwrap_or(0);

                if evidence_len == 0 {
                    warnings.push(PolicyWarning {
                        warning_id: Uuid::new_v4().to_string(),
                        policy_pack: "LLM Output Ruleset".to_string(),
                        message: "No citations present in output trace".to_string(),
                        details: Some(serde_json::json!({
                            "hint": "Enable open-book or ensure RAG evidence is retrieved",
                        })),
                        timestamp: Utc::now(),
                    });
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::LlmOutput
    }

    fn policy_pack_name(&self) -> &'static str {
        "LLM Output Ruleset"
    }
}

/// Adapter Lifecycle policy pack validator
pub struct AdapterLifecycleValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for AdapterLifecycleValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl AdapterLifecycleValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "min_activation_pct": 2.0,
                "min_quality_delta": 0.5,
                "require_registry_admit": true
            }),
        }
    }
}

impl PolicyPackValidator for AdapterLifecycleValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let violations = Vec::new();
        let mut warnings = Vec::new();

        // Check adapter activation thresholds
        if let Some(data) = &request.context.data {
            if let Some(activation_pct) = data.get("activation_pct") {
                if let Some(activation) = activation_pct.as_f64() {
                    if activation < 2.0 {
                        warnings.push(PolicyWarning {
                            warning_id: Uuid::new_v4().to_string(),
                            policy_pack: "Adapter Lifecycle Ruleset".to_string(),
                            message: "Adapter activation below minimum threshold".to_string(),
                            details: Some(serde_json::json!({"activation_pct": activation})),
                            timestamp: Utc::now(),
                        });
                    }
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::AdapterLifecycle
    }

    fn policy_pack_name(&self) -> &'static str {
        "Adapter Lifecycle Ruleset"
    }
}

/// Full Pack policy pack validator
pub struct FullPackValidator {
    #[allow(dead_code)]
    config: serde_json::Value,
}

impl Default for FullPackValidator {
    fn default() -> Self {
        Self::new()
    }
}

impl FullPackValidator {
    pub fn new() -> Self {
        Self {
            #[allow(dead_code)]
            config: serde_json::json!({
                "schema": "adapteros.policy.v1",
                "packs": {
                    "egress": {"mode": "deny_all", "serve_requires_pf": true, "allow_tcp": false, "allow_udp": false, "uds_paths": ["/var/run/aos/<tenant>/*.sock"], "media_import": {"require_signature": true, "require_sbom": true}},
                    "determinism": {"require_metallib_embed": true, "require_kernel_hash_match": true, "rng": "hkdf_seeded", "retrieval_tie_break": ["score_desc", "doc_id_asc"]},
                    "router": {"k_sparse": 3, "gate_quant": "q15", "entropy_floor": 0.02, "sample_tokens_full": 128},
                    "evidence": {"require_open_book": true, "min_spans": 1, "prefer_latest_revision": true, "warn_on_superseded": true},
                    "refusal": {"abstain_threshold": 0.55, "missing_fields_templates": {"torque_spec": ["aircraft_effectivity", "component_pn"]}},
                    "numeric": {"canonical_units": {"torque": "in_lbf", "pressure": "psi"}, "max_rounding_error": 0.5, "require_units_in_trace": true},
                    "rag": {"index_scope": "per_tenant", "doc_tags_required": ["doc_id", "rev", "effectivity", "source_type"], "embedding_model_hash": "b3:...", "topk": 5, "order": ["score_desc", "doc_id_asc"]},
                    "isolation": {"process_model": "per_tenant", "uds_root": "/var/run/aos/<tenant>", "forbid_shm": true, "keys": {"backend": "secure_enclave", "require_hardware": true}},
                    "telemetry": {"schema_hash": "b3:...", "sampling": {"token": 0.05, "router": 1.0, "inference": 1.0}, "router_full_tokens": 128, "bundle": {"max_events": 500000, "max_bytes": 268435456}},
                    "retention": {"keep_bundles_per_cpid": 12, "keep_incident_bundles": true, "keep_promotion_bundles": true, "evict_strategy": "oldest_first_safe"},
                    "performance": {"latency_p95_ms": 24, "router_overhead_pct_max": 8, "throughput_tokens_per_s_min": 40},
                    "memory": {"min_headroom_pct": 15, "evict_order": ["ephemeral_ttl", "cold_lru", "warm_lru"], "k_reduce_before_evict": true},
                    "artifacts": {"require_signature": true, "require_sbom": true, "cas_only": true},
                    "secrets": {"env_allowed": [], "keystore": "secure_enclave", "rotate_on_promotion": true},
                    "build_release": {"require_replay_zero_diff": true, "hallucination_thresholds": {"arr_min": 0.95, "ecs5_min": 0.75, "hlr_max": 0.03, "cr_max": 0.01}, "require_signed_plan": true, "require_rollback_plan": true},
                    "compliance": {"control_matrix_hash": "b3:...", "require_evidence_links": true, "require_itar_suite_green": true},
                    "incident": {"memory": ["drop_ephemeral", "reduce_k", "evict_cold", "deny_new_sessions"], "router_skew": ["entropy_floor_on", "cap_activation", "recalibrate", "rebuild_plan"], "determinism": ["freeze_plan", "export_bundle", "diff_hashes", "rollback"], "violation": ["isolate", "export_bundle", "rotate_keys", "open_ticket"]},
                    "output": {"format": "json", "require_trace": true, "forbidden_topics": ["tenant_crossing", "export_control_bypass"]},
                    "adapters": {"min_activation_pct": 2.0, "min_quality_delta": 0.5, "require_registry_admit": true}
                }
            }),
        }
    }
}

impl PolicyPackValidator for FullPackValidator {
    fn validate(&self, request: &PolicyRequest) -> Result<PolicyValidationResult> {
        let mut violations = Vec::new();
        let warnings = Vec::new();

        // Full pack validation - check schema compliance
        if let Some(data) = &request.context.data {
            if let Some(schema) = data.get("schema") {
                if schema != "adapteros.policy.v1" {
                    violations.push(PolicyViolation {
                        violation_id: Uuid::new_v4().to_string(),
                        policy_pack: "Full Pack Example".to_string(),
                        severity: ViolationSeverity::Error,
                        message: "Invalid policy schema version".to_string(),
                        details: Some(serde_json::json!({"schema": schema})),
                        remediation: Some(vec![
                            "Use schema version 'adapteros.policy.v1'".to_string()
                        ]),
                        timestamp: Utc::now(),
                    });
                }
            }
        }

        Ok(PolicyValidationResult {
            valid: violations.is_empty(),
            violations,
            warnings,
            timestamp: Utc::now(),
            duration_ms: 0,
        })
    }

    fn policy_pack_id(&self) -> PolicyPackId {
        PolicyPackId::FullPack
    }

    fn policy_pack_name(&self) -> &'static str {
        "Full Pack Example"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_policy_pack_manager_creation() {
        let manager = PolicyPackManager::new();
        assert_eq!(manager.packs.len(), 20);
        assert_eq!(manager.get_all_configs().len(), 20);
    }

    #[test]
    fn test_policy_pack_validation() {
        let manager = PolicyPackManager::new();

        let request = PolicyRequest {
            request_id: "test-request".to_string(),
            request_type: RequestType::Inference,
            tenant_id: Some("test-tenant".to_string()),
            user_id: Some("test-user".to_string()),
            context: PolicyContext {
                component: "test-component".to_string(),
                operation: "test-operation".to_string(),
                data: Some(serde_json::json!({
                    "protocol": "tcp",
                    "confidence": 0.3
                })),
                priority: Priority::Normal,
            },
            metadata: None,
        };

        let result = manager.validate_request(&request).unwrap();
        assert!(!result.valid);
        assert!(!result.violations.is_empty());
    }

    #[test]
    fn test_policy_pack_configuration() {
        let mut manager = PolicyPackManager::new();

        let config = PolicyPackConfig {
            id: PolicyPackId::Egress,
            version: "1.0.0".to_string(),
            config: serde_json::json!({"mode": "deny_all"}),
            enabled: false,
            enforcement_level: EnforcementLevel::Warning,
            last_updated: Utc::now(),
        };

        manager
            .update_pack_config(PolicyPackId::Egress, config)
            .unwrap();

        let retrieved_config = manager.get_pack_config(&PolicyPackId::Egress).unwrap();
        assert!(!retrieved_config.enabled);
    }
}
