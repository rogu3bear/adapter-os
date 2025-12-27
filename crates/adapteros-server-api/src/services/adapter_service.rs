//! Adapter lifecycle service - extracts business logic from handlers
//!
//! This module provides a service layer for adapter lifecycle management,
//! separating business logic from HTTP handler concerns.
//!
//! Pattern: Service traits define operations, implementations contain business logic.
//! Handlers remain thin, focusing on HTTP concerns (auth, validation, response formatting).

use crate::state::AppState;
use adapteros_core::{error::AosError, identity::IdentityEnvelope};
use adapteros_db::{
    adapters::Adapter,
    query_performance::{QueryMetrics, ThresholdViolation, ViolationType},
};
use adapteros_lora_lifecycle::AdapterState;
use adapteros_manifest::AssuranceTier;
use adapteros_telemetry::unified_events::{EventType, LogLevel, TelemetryEventBuilder};
use async_trait::async_trait;
use serde_json::{json, Value};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;
use tracing::{error, info, warn};

pub type Result<T> = std::result::Result<T, AosError>;

/// Response type for lifecycle transitions
#[derive(Debug, Clone)]
pub struct LifecycleTransitionResult {
    pub adapter_id: String,
    pub old_state: String,
    pub new_state: String,
    pub reason: String,
    pub timestamp: String,
}

/// Response type for adapter health checks
#[derive(Debug, Clone)]
pub struct AdapterHealthResponse {
    pub adapter_id: String,
    pub current_state: String,
    pub is_loaded: bool,
    pub last_used: Option<String>,
    pub memory_usage: Option<u64>,
}

/// Adapter service trait for lifecycle management
///
/// This trait defines the core operations for managing adapter lifecycles,
/// including state transitions (promote/demote) and health checks.
///
/// Implementations should:
/// - Use LifecycleManager when available for state transitions
/// - Fall back to direct database updates when necessary
/// - Emit telemetry events for all state changes
/// - Maintain audit trails
#[async_trait]
pub trait AdapterService: Send + Sync {
    /// Promote an adapter to the next lifecycle state
    ///
    /// State progression: Unloaded → Cold → Warm → Hot → Resident
    ///
    /// # Arguments
    /// * `adapter_id` - Unique identifier for the adapter
    /// * `tenant_id` - Tenant ID (for isolation validation)
    /// * `reason` - Human-readable reason for the transition (for audit)
    /// * `actor` - User or system performing the action
    ///
    /// # Returns
    /// Result containing transition details or error
    ///
    /// # Errors
    /// - `AosError::NotFound` if adapter doesn't exist
    /// - `AosError::Validation` if already at maximum state
    /// - `AosError::Database` on database errors
    async fn promote_lifecycle(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        reason: &str,
        actor: &str,
    ) -> Result<LifecycleTransitionResult>;

    /// Demote an adapter to a lower lifecycle state
    ///
    /// State regression: Resident → Hot → Warm → Cold → Unloaded
    ///
    /// # Arguments
    /// * `adapter_id` - Unique identifier for the adapter
    /// * `tenant_id` - Tenant ID (for isolation validation)
    /// * `reason` - Human-readable reason for the transition (for audit)
    /// * `actor` - User or system performing the action
    ///
    /// # Returns
    /// Result containing transition details or error
    ///
    /// # Errors
    /// - `AosError::NotFound` if adapter doesn't exist
    /// - `AosError::Validation` if already at minimum state
    /// - `AosError::Database` on database errors
    async fn demote_lifecycle(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        reason: &str,
        actor: &str,
    ) -> Result<LifecycleTransitionResult>;

    /// Get adapter health status
    ///
    /// # Arguments
    /// * `adapter_id` - Unique identifier for the adapter
    /// * `tenant_id` - Tenant ID (for isolation validation)
    ///
    /// # Returns
    /// Result containing health information or error
    ///
    /// # Errors
    /// - `AosError::NotFound` if adapter doesn't exist
    /// - `AosError::Database` on database errors
    async fn get_health(&self, adapter_id: &str, tenant_id: &str) -> Result<AdapterHealthResponse>;

    /// Get adapter by ID with tenant isolation
    ///
    /// # Arguments
    /// * `adapter_id` - Unique identifier for the adapter
    /// * `tenant_id` - Tenant ID for isolation validation
    ///
    /// # Returns
    /// Result containing adapter or None if not found
    ///
    /// # Errors
    /// - `AosError::Database` on database errors
    async fn get_adapter(&self, adapter_id: &str, tenant_id: &str) -> Result<Option<Adapter>>;
}

/// Default implementation of AdapterService using AppState
///
/// This implementation uses the lifecycle manager when available,
/// falling back to direct database updates when necessary.
pub struct DefaultAdapterService {
    state: Arc<AppState>,
}

impl DefaultAdapterService {
    /// Create a new adapter service
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    /// Determine the next state in the lifecycle progression
    fn next_state(current: &str) -> Result<&'static str> {
        match current {
            "unloaded" => Ok("cold"),
            "cold" => Ok("warm"),
            "warm" => Ok("hot"),
            "hot" => Ok("resident"),
            "resident" => Err(AosError::Validation(
                "Adapter already at maximum state (resident)".to_string(),
            )),
            _ => Err(AosError::Validation(format!(
                "Unknown adapter state '{}': requires manual repair/migration",
                current
            ))),
        }
    }

    /// Determine the previous state in the lifecycle regression
    fn previous_state(current: &str) -> Result<&'static str> {
        match current {
            "resident" => Ok("hot"),
            "hot" => Ok("warm"),
            "warm" => Ok("cold"),
            "cold" => Ok("unloaded"),
            "unloaded" => Err(AosError::Validation(
                "Adapter already at minimum state (unloaded)".to_string(),
            )),
            _ => Err(AosError::Validation(format!(
                "Unknown adapter state '{}': requires manual repair/migration",
                current
            ))),
        }
    }

    /// Map state string to AdapterState enum
    #[allow(dead_code)]
    fn state_to_enum(state: &str) -> AdapterState {
        match state {
            "unloaded" => AdapterState::Unloaded,
            "cold" => AdapterState::Cold,
            "warm" => AdapterState::Warm,
            "hot" => AdapterState::Hot,
            "resident" => AdapterState::Resident,
            _ => {
                warn!(state = %state, "Unknown state, defaulting to Unloaded");
                AdapterState::Unloaded
            }
        }
    }

    /// Execute state transition using lifecycle manager or direct DB update
    ///
    /// Uses Compare-And-Swap (CAS) to prevent TOCTOU race conditions.
    #[allow(clippy::too_many_arguments)]
    async fn execute_transition(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        old_state_str: &str,
        new_state_str: &str,
        reason: &str,
        lifecycle_manager: &Option<Arc<Mutex<adapteros_lora_lifecycle::LifecycleManager>>>,
        is_promotion: bool,
    ) -> Result<String> {
        let consistency = self.state.db.check_adapter_consistency(adapter_id).await?;
        if !consistency.is_ready() {
            let msg = consistency
                .message
                .unwrap_or_else(|| "KV consistency check failed".to_string());
            return Err(AosError::Validation(format!(
                "Adapter {} blocked: {}",
                adapter_id, msg
            )));
        }

        if let Some(ref lifecycle) = lifecycle_manager {
            let manager = lifecycle.lock().await;

            if let Some(adapter_idx) = manager.get_adapter_idx(adapter_id) {
                // Execute state transition via lifecycle manager
                // NOTE: promote_adapter/demote_adapter already follow DB-first pattern internally
                // They persist to DB first, then update in-memory state
                if is_promotion {
                    manager.promote_adapter(adapter_idx).await.map_err(|e| {
                        error!(error = %e, "Failed to promote adapter via lifecycle manager");
                        AosError::Lifecycle(format!("Failed to promote adapter: {}", e))
                    })?;
                } else {
                    manager.demote_adapter(adapter_idx).await.map_err(|e| {
                        error!(error = %e, "Failed to demote adapter via lifecycle manager");
                        AosError::Lifecycle(format!("Failed to demote adapter: {}", e))
                    })?;
                }

                // No need for additional DB sync - lifecycle manager already persisted changes
                Ok(new_state_str.to_string())
            } else {
                // Adapter not found in lifecycle manager, use CAS update directly
                let updated = self
                    .state
                    .lifecycle_db()
                    .update_adapter_state_cas_for_tenant(
                        tenant_id,
                        adapter_id,
                        old_state_str,
                        new_state_str,
                        reason,
                    )
                    .await
                    .map_err(|e| {
                        error!(error = %e, "Failed to update adapter state (CAS)");
                        AosError::Database(format!("Failed to update adapter state: {}", e))
                    })?;
                if !updated {
                    return Err(AosError::Validation(format!(
                        "State transition conflict: adapter {} is no longer in '{}' state",
                        adapter_id, old_state_str
                    )));
                }
                Ok(new_state_str.to_string())
            }
        } else {
            // No lifecycle manager: use CAS update directly
            let updated = self
                .state
                .lifecycle_db()
                .update_adapter_state_cas_for_tenant(
                    tenant_id,
                    adapter_id,
                    old_state_str,
                    new_state_str,
                    reason,
                )
                .await
                .map_err(|e| {
                    error!(error = %e, "Failed to update adapter state (CAS)");
                    AosError::Database(format!("Failed to update adapter state: {}", e))
                })?;
            if !updated {
                return Err(AosError::Validation(format!(
                    "State transition conflict: adapter {} is no longer in '{}' state",
                    adapter_id, old_state_str
                )));
            }
            Ok(new_state_str.to_string())
        }
    }
}

impl DefaultAdapterService {
    async fn promote_lifecycle_impl(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        reason: &str,
        actor: &str,
    ) -> Result<LifecycleTransitionResult> {
        let telemetry_start = Instant::now();

        // Get current adapter
        let adapter = self
            .state
            .db
            .get_adapter_for_tenant(tenant_id, adapter_id)
            .await
            .map_err(|e| {
                error!(adapter_id = %adapter_id, error = %e, "Failed to fetch adapter");
                AosError::Database(format!("Failed to fetch adapter: {}", e))
            })?
            .ok_or_else(|| {
                warn!(adapter_id = %adapter_id, "Adapter not found");
                AosError::NotFound(format!("Adapter not found: {}", adapter_id))
            })?;

        // Validate tenant isolation
        if adapter.tenant_id != tenant_id {
            return Err(AosError::Validation(format!(
                "Tenant isolation violation: adapter belongs to {}, requested by {}",
                adapter.tenant_id, tenant_id
            )));
        }

        if let Some(drift_meta) = adapter
            .metadata_json
            .as_deref()
            .and_then(parse_drift_gate_metadata)
        {
            let decision = evaluate_drift_gate(&drift_meta);
            match decision {
                DriftDecision::Block => {
                    warn!(
                        adapter_id = %adapter_id,
                        baseline = ?drift_meta.baseline_backend,
                        test_backend = ?drift_meta.test_backend,
                        "Promotion blocked: drift exceeds high-tier thresholds or missing metrics"
                    );
                    return Err(AosError::Validation(
                        "Adapter promotion blocked by drift gate".to_string(),
                    ));
                }
                DriftDecision::ReviewRequired => {
                    warn!(
                        adapter_id = %adapter_id,
                        weight_l_inf = ?drift_meta.weight_l_inf,
                        loss_l_inf = ?drift_meta.loss_l_inf,
                        tier = ?drift_meta.tier,
                        "Drift exceeds standard thresholds; promotion allowed but requires review"
                    );
                }
                DriftDecision::RecordOnly => {
                    // no-op
                }
            }
        }

        let old_state = adapter.current_state.clone();
        let new_state_str = Self::next_state(&old_state)?;

        // Record span fields for observability
        tracing::Span::current().record("transition.old_state", &old_state);
        tracing::Span::current().record("transition.new_state", new_state_str);

        // Execute state transition with CAS to prevent TOCTOU races
        let new_state = self
            .execute_transition(
                adapter_id,
                tenant_id,
                &old_state,
                new_state_str,
                reason,
                &self.state.lifecycle_manager,
                true,
            )
            .await?;

        let duration_secs = telemetry_start.elapsed().as_secs_f64();
        tracing::Span::current().record("transition.duration_ms", duration_secs * 1000.0);

        let timestamp = chrono::Utc::now().to_rfc3339();

        let telemetry_event = serde_json::json!({
            "event_type": "adapter.lifecycle.promoted",
            "component": "adapteros-server-api",
            "severity": "info",
            "message": format!("Adapter {} promoted: {} → {}", adapter_id, old_state, new_state),
            "metadata": {
                "adapter_id": adapter_id,
                "old_state": old_state,
                "new_state": new_state,
                "actor": actor,
                "reason": reason,
                "timestamp": timestamp.clone(),
                "duration_ms": duration_secs * 1000.0,
            }
        });

        info!(
            event = %telemetry_event,
            adapter_id = %adapter_id,
            old_state = %old_state,
            new_state = %new_state,
            actor = %actor,
            reason = %reason,
            duration_ms = %format!("{:.2}", duration_secs * 1000.0),
            "Adapter lifecycle promoted"
        );

        Ok(LifecycleTransitionResult {
            adapter_id: adapter_id.to_string(),
            old_state,
            new_state,
            reason: reason.to_string(),
            timestamp,
        })
    }

    async fn demote_lifecycle_impl(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        reason: &str,
        actor: &str,
    ) -> Result<LifecycleTransitionResult> {
        let telemetry_start = Instant::now();

        let adapter = self
            .state
            .db
            .get_adapter_for_tenant(tenant_id, adapter_id)
            .await
            .map_err(|e| {
                error!(adapter_id = %adapter_id, error = %e, "Failed to fetch adapter");
                AosError::Database(format!("Failed to fetch adapter: {}", e))
            })?
            .ok_or_else(|| {
                warn!(adapter_id = %adapter_id, "Adapter not found");
                AosError::NotFound(format!("Adapter not found: {}", adapter_id))
            })?;

        if adapter.tenant_id != tenant_id {
            return Err(AosError::Validation(format!(
                "Tenant isolation violation: adapter belongs to {}, requested by {}",
                adapter.tenant_id, tenant_id
            )));
        }

        let old_state = adapter.current_state.clone();
        let new_state_str = Self::previous_state(&old_state)?;

        tracing::Span::current().record("transition.old_state", &old_state);
        tracing::Span::current().record("transition.new_state", new_state_str);

        let new_state = self
            .execute_transition(
                adapter_id,
                tenant_id,
                &old_state,
                new_state_str,
                reason,
                &self.state.lifecycle_manager,
                false,
            )
            .await?;

        let duration_secs = telemetry_start.elapsed().as_secs_f64();
        tracing::Span::current().record("transition.duration_ms", duration_secs * 1000.0);

        let timestamp = chrono::Utc::now().to_rfc3339();

        let telemetry_event = serde_json::json!({
            "event_type": "adapter.lifecycle.demoted",
            "component": "adapteros-server-api",
            "severity": "info",
            "message": format!("Adapter {} demoted: {} → {}", adapter_id, old_state, new_state),
            "metadata": {
                "adapter_id": adapter_id,
                "old_state": old_state,
                "new_state": new_state,
                "actor": actor,
                "reason": reason,
                "timestamp": timestamp.clone(),
                "duration_ms": duration_secs * 1000.0,
            }
        });

        info!(
            event = %telemetry_event,
            adapter_id = %adapter_id,
            old_state = %old_state,
            new_state = %new_state,
            actor = %actor,
            reason = %reason,
            duration_ms = %format!("{:.2}", duration_secs * 1000.0),
            "Adapter lifecycle demoted"
        );

        Ok(LifecycleTransitionResult {
            adapter_id: adapter_id.to_string(),
            old_state,
            new_state,
            reason: reason.to_string(),
            timestamp,
        })
    }

    async fn get_health_inner(
        &self,
        adapter_id: &str,
        tenant_id: &str,
    ) -> Result<AdapterHealthResponse> {
        let adapter = self
            .state
            .db
            .get_adapter_for_tenant(tenant_id, adapter_id)
            .await
            .map_err(|e| {
                error!(adapter_id = %adapter_id, error = %e, "Failed to fetch adapter");
                AosError::Database(format!("Failed to fetch adapter: {}", e))
            })?
            .ok_or_else(|| {
                warn!(adapter_id = %adapter_id, "Adapter not found");
                AosError::NotFound(format!("Adapter not found: {}", adapter_id))
            })?;

        if adapter.tenant_id != tenant_id {
            return Err(AosError::Validation(format!(
                "Tenant isolation violation: adapter belongs to {}, requested by {}",
                adapter.tenant_id, tenant_id
            )));
        }

        let is_loaded = !matches!(adapter.current_state.as_str(), "unloaded");

        Ok(AdapterHealthResponse {
            adapter_id: adapter_id.to_string(),
            current_state: adapter.current_state.clone(),
            is_loaded,
            last_used: adapter.last_activated.clone(),
            memory_usage: None,
        })
    }

    async fn get_adapter_inner(
        &self,
        adapter_id: &str,
        tenant_id: &str,
    ) -> Result<Option<Adapter>> {
        self.state
            .db
            .get_adapter_for_tenant(tenant_id, adapter_id)
            .await
            .map_err(|e| {
                error!(adapter_id = %adapter_id, tenant_id = %tenant_id, error = %e, "Failed to fetch adapter");
                AosError::Database(format!("Failed to fetch adapter: {}", e))
            })
    }

    async fn instrument_tenant_operation(
        &self,
        query_name: &str,
        tenant_id: Option<&str>,
        duration: Duration,
        rows_returned: Option<i64>,
        used_index: bool,
        query_plan: Option<String>,
    ) {
        let metric = QueryMetrics {
            query_name: query_name.to_string(),
            execution_time_us: duration.as_micros() as u64,
            rows_returned,
            used_index,
            query_plan,
            timestamp: chrono::Utc::now().to_rfc3339(),
            tenant_id: tenant_id.map(|tid| tid.to_string()),
        };

        let alerts = self.capture_query_metrics(metric);

        if !alerts.is_empty() {
            if let Some(tenant) = tenant_id {
                for alert in alerts {
                    self.publish_regression_event(tenant, query_name, &alert)
                        .await;
                }
            } else {
                warn!(
                    query = %query_name,
                    "Performance regression detected for non-tenant operation"
                );
            }
        }

        let registry = self.state.metrics_registry.clone();
        let metric_name = format!("adapter_service.{}.duration_ms", query_name);
        let value_ms = duration.as_secs_f64() * 1000.0;
        tokio::spawn(async move {
            registry.record_metric(metric_name, value_ms).await;
        });
    }

    fn capture_query_metrics(&self, metric: QueryMetrics) -> Vec<ThresholdViolation> {
        if let Some(mut guard) = self.state.db.performance_monitor_mut() {
            if let Some(monitor) = guard.as_mut() {
                monitor.record(metric);
                return monitor.check_threshold_violations();
            }
        }
        Vec::new()
    }

    async fn publish_regression_event(
        &self,
        tenant_id: &str,
        query_name: &str,
        alert: &ThresholdViolation,
    ) {
        warn!(
            tenant_id = %tenant_id,
            query = %query_name,
            severity = ?alert.severity,
            violation = ?alert.violation_type,
            "Performance regression detected"
        );

        let (description, detail) = Self::describe_violation(&alert.violation_type);
        let identity = IdentityEnvelope::new(
            tenant_id.to_string(),
            "adapter_service".to_string(),
            "tenant_isolation_monitoring".to_string(),
            IdentityEnvelope::default_revision(),
        );

        let event = match TelemetryEventBuilder::new(
            EventType::Custom("adapter.performance.regression".to_string()),
            LogLevel::Warn,
            format!(
                "Tenant {} query {} triggered regression: {}",
                tenant_id, query_name, description
            ),
            identity,
        )
        .component("adapter_service.performance_monitor".to_string())
        .metadata(json!({
            "tenant_id": tenant_id,
            "query_name": query_name,
            "severity": format!("{:?}", alert.severity),
            "violation": detail,
            "timestamp": alert.timestamp,
        }))
        .build()
        {
            Ok(event) => event,
            Err(err) => {
                warn!(error = %err, "Failed to build telemetry regression event");
                return;
            }
        };

        if let Err(err) = self.state.telemetry_buffer.push(event.clone()).await {
            warn!(error = %err, tenant_id = %tenant_id, "Telemetry buffer rejected regression event");
        }
        if let Err(err) = self.state.telemetry_tx.send(event) {
            warn!(error = %err, tenant_id = %tenant_id, "Telemetry broadcast failed for regression event");
        }
    }

    fn describe_violation(violation: &ViolationType) -> (String, serde_json::Value) {
        match violation {
            ViolationType::P95LatencyExceeded {
                actual_ms,
                threshold_ms,
            } => (
                format!("P95 latency {}ms exceeded {}ms", actual_ms, threshold_ms),
                json!({
                    "type": "p95_latency",
                    "actual_ms": actual_ms,
                    "threshold_ms": threshold_ms,
                }),
            ),
            ViolationType::P99LatencyExceeded {
                actual_ms,
                threshold_ms,
            } => (
                format!("P99 latency {}ms exceeded {}ms", actual_ms, threshold_ms),
                json!({
                    "type": "p99_latency",
                    "actual_ms": actual_ms,
                    "threshold_ms": threshold_ms,
                }),
            ),
            ViolationType::LowIndexUsage {
                actual_pct,
                threshold_pct,
            } => (
                format!(
                    "Index usage {:.1}% below threshold {:.1}%",
                    actual_pct, threshold_pct
                ),
                json!({
                    "type": "low_index_usage",
                    "actual_pct": actual_pct,
                    "threshold_pct": threshold_pct,
                }),
            ),
            ViolationType::HighVariance {
                coefficient,
                threshold,
            } => (
                format!(
                    "High variance coefficient {:.2} exceeds {:.2}",
                    coefficient, threshold
                ),
                json!({
                    "type": "high_variance",
                    "coefficient": coefficient,
                    "threshold": threshold,
                }),
            ),
        }
    }
}

#[async_trait]
impl AdapterService for DefaultAdapterService {
    #[tracing::instrument(
        skip(self),
        fields(
            otel.kind = "server",
            adapter.id = %adapter_id,
            tenant.id = %tenant_id,
            transition.direction = "promote"
        )
    )]
    async fn promote_lifecycle(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        reason: &str,
        actor: &str,
    ) -> Result<LifecycleTransitionResult> {
        let start = Instant::now();
        let result = self
            .promote_lifecycle_impl(adapter_id, tenant_id, reason, actor)
            .await;
        let duration = start.elapsed();
        self.instrument_tenant_operation(
            "adapter_service.promote_lifecycle",
            Some(tenant_id),
            duration,
            Some(1),
            true,
            None,
        )
        .await;
        result
    }

    #[tracing::instrument(
        skip(self),
        fields(
            otel.kind = "server",
            adapter.id = %adapter_id,
            tenant.id = %tenant_id,
            transition.direction = "demote"
        )
    )]
    async fn demote_lifecycle(
        &self,
        adapter_id: &str,
        tenant_id: &str,
        reason: &str,
        actor: &str,
    ) -> Result<LifecycleTransitionResult> {
        let start = Instant::now();
        let result = self
            .demote_lifecycle_impl(adapter_id, tenant_id, reason, actor)
            .await;
        let duration = start.elapsed();
        self.instrument_tenant_operation(
            "adapter_service.demote_lifecycle",
            Some(tenant_id),
            duration,
            Some(1),
            true,
            None,
        )
        .await;
        result
    }

    async fn get_health(&self, adapter_id: &str, tenant_id: &str) -> Result<AdapterHealthResponse> {
        let start = Instant::now();
        let result = self.get_health_inner(adapter_id, tenant_id).await;
        let duration = start.elapsed();
        let rows_returned = match &result {
            Ok(_) => Some(1),
            Err(_) => None,
        };
        self.instrument_tenant_operation(
            "adapter_service.get_health",
            Some(tenant_id),
            duration,
            rows_returned,
            true,
            None,
        )
        .await;
        result
    }

    async fn get_adapter(&self, adapter_id: &str, tenant_id: &str) -> Result<Option<Adapter>> {
        let start = Instant::now();
        let result = self.get_adapter_inner(adapter_id, tenant_id).await;
        let duration = start.elapsed();
        let rows_returned = match &result {
            Ok(Some(_)) => Some(1),
            Ok(None) => Some(0),
            Err(_) => None,
        };
        self.instrument_tenant_operation(
            "adapter_service.get_adapter",
            Some(tenant_id),
            duration,
            rows_returned,
            true,
            None,
        )
        .await;
        result
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DriftDecision {
    RecordOnly,
    ReviewRequired,
    Block,
}

#[derive(Debug, Clone)]
struct DriftGateMetadata {
    tier: AssuranceTier,
    weight_l_inf: Option<f64>,
    loss_l_inf: Option<f64>,
    baseline_backend: Option<String>,
    test_backend: Option<String>,
}

fn parse_assurance_tier_str(value: Option<&str>) -> AssuranceTier {
    match value.map(|s| s.to_lowercase()) {
        Some(ref v) if v == "low" => AssuranceTier::Low,
        Some(ref v) if v == "high" => AssuranceTier::High,
        _ => AssuranceTier::Standard,
    }
}

fn parse_drift_gate_metadata(raw: &str) -> Option<DriftGateMetadata> {
    let value: Value = serde_json::from_str(raw).ok()?;

    let tier = value
        .get("assurance_tier")
        .or_else(|| value.get("drift_tier"))
        .and_then(|v| v.as_str());
    let tier = parse_assurance_tier_str(tier);

    let weight_l_inf = value
        .get("drift_metric")
        .or_else(|| value.get("drift_weight_metric"))
        .and_then(|v| v.as_f64());
    let loss_l_inf = value
        .get("drift_loss_metric")
        .or_else(|| value.get("drift_loss_l_inf"))
        .and_then(|v| v.as_f64());

    let baseline_backend = value
        .get("drift_baseline_backend")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let test_backend = value
        .get("drift_test_backend")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    Some(DriftGateMetadata {
        tier,
        weight_l_inf,
        loss_l_inf,
        baseline_backend,
        test_backend,
    })
}

fn evaluate_drift_gate(meta: &DriftGateMetadata) -> DriftDecision {
    const HIGH_WEIGHT_EPS: f64 = 1e-6;
    const HIGH_LOSS_EPS: f64 = 1e-4;
    const STANDARD_WEIGHT_EPS: f64 = 5e-5;
    const STANDARD_LOSS_EPS: f64 = 5e-4;

    match meta.tier {
        AssuranceTier::Low => DriftDecision::RecordOnly,
        AssuranceTier::Standard => {
            if exceeds(meta.weight_l_inf, STANDARD_WEIGHT_EPS)
                || exceeds(meta.loss_l_inf, STANDARD_LOSS_EPS)
            {
                DriftDecision::ReviewRequired
            } else {
                DriftDecision::RecordOnly
            }
        }
        AssuranceTier::High => {
            if meta.weight_l_inf.is_none() && meta.loss_l_inf.is_none() {
                return DriftDecision::Block;
            }
            if exceeds(meta.weight_l_inf, HIGH_WEIGHT_EPS)
                || exceeds(meta.loss_l_inf, HIGH_LOSS_EPS)
            {
                DriftDecision::Block
            } else {
                DriftDecision::RecordOnly
            }
        }
    }
}

fn exceeds(metric: Option<f64>, limit: f64) -> bool {
    metric.map(|m| m > limit).unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_next_state_transitions() {
        assert_eq!(
            DefaultAdapterService::next_state("unloaded").unwrap(),
            "cold"
        );
        assert_eq!(DefaultAdapterService::next_state("cold").unwrap(), "warm");
        assert_eq!(DefaultAdapterService::next_state("warm").unwrap(), "hot");
        assert_eq!(
            DefaultAdapterService::next_state("hot").unwrap(),
            "resident"
        );
        assert!(DefaultAdapterService::next_state("resident").is_err());
    }

    #[test]
    fn test_previous_state_transitions() {
        assert_eq!(
            DefaultAdapterService::previous_state("resident").unwrap(),
            "hot"
        );
        assert_eq!(
            DefaultAdapterService::previous_state("hot").unwrap(),
            "warm"
        );
        assert_eq!(
            DefaultAdapterService::previous_state("warm").unwrap(),
            "cold"
        );
        assert_eq!(
            DefaultAdapterService::previous_state("cold").unwrap(),
            "unloaded"
        );
        assert!(DefaultAdapterService::previous_state("unloaded").is_err());
    }

    #[test]
    fn test_unknown_state_errors() {
        assert!(DefaultAdapterService::next_state("mystery").is_err());
        assert!(DefaultAdapterService::previous_state("mystery").is_err());
    }

    #[test]
    fn test_state_to_enum() {
        assert!(matches!(
            DefaultAdapterService::state_to_enum("unloaded"),
            AdapterState::Unloaded
        ));
        assert!(matches!(
            DefaultAdapterService::state_to_enum("cold"),
            AdapterState::Cold
        ));
        assert!(matches!(
            DefaultAdapterService::state_to_enum("warm"),
            AdapterState::Warm
        ));
        assert!(matches!(
            DefaultAdapterService::state_to_enum("hot"),
            AdapterState::Hot
        ));
        assert!(matches!(
            DefaultAdapterService::state_to_enum("resident"),
            AdapterState::Resident
        ));
    }
}
