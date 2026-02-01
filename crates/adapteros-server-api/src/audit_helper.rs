//! Audit logging helper for handlers
//!
//! Provides convenient functions to log audit events from API handlers.
//! All sensitive operations should be logged for compliance and security review.
//!
//! # Security Note
//!
//! Audit logging failures are critical security events. When the primary audit chain
//! cannot be written (database unavailable, etc.), we MUST still record the event
//! via the tracing/logging system to maintain an evidence trail.

use crate::auth::Claims;
use adapteros_core::Result;
use adapteros_db::Db;
use tracing::{error, info, warn};

/// Log an audit action
///
/// Records the action to the database and logs it via tracing for observability.
///
/// # Arguments
/// * `db` - Database connection
/// * `claims` - JWT claims from authenticated user
/// * `action` - Action being performed (e.g., "adapter.register", "training.start")
/// * `resource_type` - Type of resource (e.g., "adapter", "policy", "tenant")
/// * `resource_id` - ID of the resource being acted upon
/// * `status` - "success" or "failure"
/// * `error_message` - Error details if status = "failure"
///
/// # Example
/// ```no_run
/// use adapteros_server_api::audit_helper::log_action;
/// use adapteros_db::Db;
/// use crate::auth::Claims;
///
/// # async fn example(db: &Db, claims: &Claims) -> adapteros_core::Result<()> {
/// log_action(
///     db,
///     claims,
///     "adapter.delete",
///     "adapter",
///     Some("adapter-xyz"),
///     "success",
///     None,
/// ).await?;
/// # Ok(())
/// # }
/// ```
pub async fn log_action(
    db: &Db,
    claims: &Claims,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    status: &str,
    error_message: Option<&str>,
) -> Result<()> {
    // Log to database
    db.log_audit(
        &claims.sub,
        &claims.role,
        &claims.tenant_id,
        action,
        resource_type,
        resource_id,
        status,
        error_message,
        None, // IP address (could be extracted from request headers if needed)
        None, // Additional metadata
    )
    .await?;

    // Log to tracing for real-time observability
    info!(
        event_type = "audit.action",
        user_id = %claims.sub,
        user_role = %claims.role,
        tenant_id = %claims.tenant_id,
        action = %action,
        resource_type = %resource_type,
        resource_id = ?resource_id,
        status = %status,
        error_message = ?error_message,
        "Audit log recorded"
    );

    Ok(())
}

/// Log a successful action
///
/// Convenience wrapper for log_action with status = "success"
pub async fn log_success(
    db: &Db,
    claims: &Claims,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
) -> Result<()> {
    log_action(
        db,
        claims,
        action,
        resource_type,
        resource_id,
        "success",
        None,
    )
    .await
}

/// Log a failed action
///
/// Convenience wrapper for log_action with status = "failure"
pub async fn log_failure(
    db: &Db,
    claims: &Claims,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    error: &str,
) -> Result<()> {
    log_action(
        db,
        claims,
        action,
        resource_type,
        resource_id,
        "failure",
        Some(error),
    )
    .await
}

/// Log a successful action, with explicit error handling on failure
///
/// Unlike `log_success`, this function NEVER silently discards errors.
/// If the audit chain cannot be written, it logs a CRITICAL error via tracing
/// to ensure there is always SOME record of the operation.
///
/// # Security
///
/// This function should be used instead of `let _ = log_success(...)` to prevent
/// silent audit trail gaps. Even when the primary audit fails, the tracing log
/// provides a secondary evidence trail.
pub async fn log_success_or_warn(
    db: &Db,
    claims: &Claims,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
) {
    if let Err(e) = log_success(db, claims, action, resource_type, resource_id).await {
        // CRITICAL: Primary audit chain failed - log via tracing as fallback evidence
        error!(
            event_type = "audit.chain_failure",
            user_id = %claims.sub,
            user_role = %claims.role,
            tenant_id = %claims.tenant_id,
            action = %action,
            resource_type = %resource_type,
            resource_id = ?resource_id,
            status = "success",
            audit_error = %e,
            "AUDIT CHAIN FAILURE: Operation succeeded but audit log could not be written. \
             This is a CRITICAL security event - the operation completed without audit trail."
        );
        warn!(
            action = %action,
            resource_id = ?resource_id,
            "Audit chain gap detected - manual reconciliation may be required"
        );
    }
}

/// Log preflight result with bypass tracking
///
/// Records preflight checks and any bypasses used. Bypasses are security-sensitive
/// events that must be audited for compliance.
///
/// # Arguments
/// * `db` - Database connection
/// * `claims` - JWT claims from authenticated user
/// * `adapter_id` - The adapter being checked
/// * `preflight_result` - The result of preflight checks
pub async fn log_preflight_result(
    db: &Db,
    claims: &Claims,
    adapter_id: &str,
    preflight_result: &adapteros_core::preflight::PreflightResult,
) {
    use actions::*;

    // Log the main preflight result
    let main_action = if preflight_result.passed {
        PREFLIGHT_PASSED
    } else {
        PREFLIGHT_FAILED
    };

    // Store failure summary to extend its lifetime
    let failure_summary = preflight_result.failure_summary();

    if let Err(e) = log_action(
        db,
        claims,
        main_action,
        resources::ADAPTER,
        Some(adapter_id),
        if preflight_result.passed {
            "success"
        } else {
            "failure"
        },
        if preflight_result.passed {
            None
        } else {
            Some(&failure_summary)
        },
    )
    .await
    {
        warn!(
            adapter_id = %adapter_id,
            passed = preflight_result.passed,
            error = %e,
            "Failed to log preflight result to audit chain"
        );
    }

    // Log each bypass used - these are security-sensitive events
    for bypass in &preflight_result.bypasses_used {
        let bypass_action = match bypass.as_str() {
            "skip_maintenance_check" => PREFLIGHT_BYPASS_MAINTENANCE,
            "skip_conflict_check" => PREFLIGHT_BYPASS_CONFLICT,
            "force" => PREFLIGHT_BYPASS_FORCE,
            "allow_training_state" => PREFLIGHT_BYPASS_TRAINING_STATE,
            _ => "preflight.bypass.unknown",
        };

        if let Err(e) = log_action(
            db,
            claims,
            bypass_action,
            resources::ADAPTER,
            Some(adapter_id),
            "success",
            Some(&format!(
                "Bypass '{}' used - reason: {}",
                bypass,
                preflight_result
                    .audit_events
                    .iter()
                    .find(|e| e.bypass_used.as_deref() == Some(bypass.as_str()))
                    .and_then(|e| e.reason.as_deref())
                    .unwrap_or("unspecified")
            )),
        )
        .await
        {
            // CRITICAL: Bypass events MUST be logged
            error!(
                event_type = "audit.bypass_chain_failure",
                adapter_id = %adapter_id,
                bypass = %bypass,
                error = %e,
                "CRITICAL: Failed to log preflight bypass to audit chain. \
                 Security bypass event may not be in audit trail."
            );
        } else {
            info!(
                event_type = "preflight.bypass",
                adapter_id = %adapter_id,
                bypass = %bypass,
                actor = %claims.sub,
                "Preflight bypass used and logged"
            );
        }
    }
}

/// Log a failed action, with explicit error handling on audit failure
///
/// Unlike `log_failure`, this function NEVER silently discards errors.
/// If the audit chain cannot be written, it logs a CRITICAL error via tracing.
pub async fn log_failure_or_warn(
    db: &Db,
    claims: &Claims,
    action: &str,
    resource_type: &str,
    resource_id: Option<&str>,
    operation_error: &str,
) {
    if let Err(e) = log_failure(
        db,
        claims,
        action,
        resource_type,
        resource_id,
        operation_error,
    )
    .await
    {
        // CRITICAL: Primary audit chain failed - log via tracing as fallback evidence
        error!(
            event_type = "audit.chain_failure",
            user_id = %claims.sub,
            user_role = %claims.role,
            tenant_id = %claims.tenant_id,
            action = %action,
            resource_type = %resource_type,
            resource_id = ?resource_id,
            status = "failure",
            operation_error = %operation_error,
            audit_error = %e,
            "AUDIT CHAIN FAILURE: Operation failed and audit log could not be written. \
             This is a CRITICAL security event - the failure is not in the audit trail."
        );
        warn!(
            action = %action,
            resource_id = ?resource_id,
            "Audit chain gap detected - manual reconciliation may be required"
        );
    }
}

/// Audit action types as constants for consistency
pub mod actions {
    // Adapter actions
    pub const ADAPTER_REGISTER: &str = "adapter.register";
    pub const ADAPTER_DELETE: &str = "adapter.delete";
    pub const ADAPTER_LOAD: &str = "adapter.load";
    pub const ADAPTER_UNLOAD: &str = "adapter.unload";
    pub const ADAPTER_LIFECYCLE_PROMOTE: &str = "adapter.lifecycle.promote";
    pub const ADAPTER_LIFECYCLE_DEMOTE: &str = "adapter.lifecycle.demote";
    pub const ADAPTER_PIN: &str = "adapter.pin";
    pub const ADAPTER_UNPIN: &str = "adapter.unpin";
    pub const ADAPTER_ARCHIVE: &str = "adapter.archive";
    pub const ADAPTER_UNARCHIVE: &str = "adapter.unarchive";

    // Training actions
    pub const TRAINING_START: &str = "training.start";
    pub const TRAINING_COMPLETE: &str = "training.complete";
    pub const TRAINING_CANCEL: &str = "training.cancel";

    // Inference actions
    pub const INFERENCE_EXECUTE: &str = "inference.execute";
    pub const TOKENIZE_EXECUTE: &str = "tokenize.execute";

    // Preflight actions (Gap 4 fix - bypass audit logging)
    pub const PREFLIGHT_PASSED: &str = "preflight.passed";
    pub const PREFLIGHT_FAILED: &str = "preflight.failed";
    pub const PREFLIGHT_BYPASS_MAINTENANCE: &str = "preflight.bypass.maintenance";
    pub const PREFLIGHT_BYPASS_CONFLICT: &str = "preflight.bypass.conflict";
    pub const PREFLIGHT_BYPASS_FORCE: &str = "preflight.bypass.force";
    pub const PREFLIGHT_BYPASS_TRAINING_STATE: &str = "preflight.bypass.training_state";

    // Tenant actions
    pub const TENANT_CREATE: &str = "tenant.create";
    pub const TENANT_UPDATE: &str = "tenant.update";
    pub const TENANT_PAUSE: &str = "tenant.pause";
    pub const TENANT_ARCHIVE: &str = "tenant.archive";

    // Node actions
    pub const NODE_REGISTER: &str = "node.register";
    pub const NODE_EVICT: &str = "node.evict";
    pub const NODE_OFFLINE: &str = "node.offline";

    // Policy actions
    pub const POLICY_APPLY: &str = "policy.apply";
    pub const POLICY_SIGN: &str = "policy.sign";
    pub const POLICY_VALIDATE: &str = "policy.validate";
    pub const POLICY_BINDING_ENABLE: &str = "policy.binding.enable";
    pub const POLICY_BINDING_DISABLE: &str = "policy.binding.disable";

    // Policy quarantine actions
    pub const POLICY_QUARANTINE_CLEAR: &str = "policy.quarantine.clear";
    pub const POLICY_QUARANTINE_CLEAR_PACK: &str = "policy.quarantine.clear_pack";
    pub const POLICY_QUARANTINE_ROLLBACK: &str = "policy.quarantine.rollback";

    // Stack policy actions (PRD-GOV-01)
    pub const STACK_POLICY_ASSIGN: &str = "stack.policy.assign";
    pub const STACK_POLICY_REVOKE: &str = "stack.policy.revoke";
    pub const STACK_POLICY_VIOLATION: &str = "stack.policy.violation";
    pub const STACK_POLICY_VIOLATION_RESOLVED: &str = "stack.policy.violation.resolved";

    // Worker actions
    pub const WORKER_SPAWN: &str = "worker.spawn";
    pub const WORKER_DEBUG_START: &str = "worker.debug.start";
    pub const WORKER_TROUBLESHOOT: &str = "worker.troubleshoot";

    // Adapter stack actions
    pub const STACK_CREATE: &str = "stack.create";
    pub const STACK_DELETE: &str = "stack.delete";
    pub const STACK_ACTIVATE: &str = "stack.activate";
    pub const STACK_DEACTIVATE: &str = "stack.deactivate";

    // Git actions
    pub const GIT_SESSION_START: &str = "git.session.start";
    pub const GIT_SESSION_END: &str = "git.session.end";

    // Code intelligence actions
    pub const CODE_REPO_REGISTER: &str = "code.repo.register";
    pub const CODE_SCAN_START: &str = "code.scan.start";
    pub const CODE_DELTA_CREATE: &str = "code.delta.create";

    // Domain adapter actions
    pub const DOMAIN_ADAPTER_CREATE: &str = "adapter.domain.create";
    pub const DOMAIN_ADAPTER_DELETE: &str = "adapter.domain.delete";

    // SSE authentication actions
    pub const SSE_AUTHENTICATION: &str = "sse.authenticate";
    pub const DOMAIN_ADAPTER_LOAD: &str = "adapter.domain.load";
    pub const DOMAIN_ADAPTER_UNLOAD: &str = "adapter.domain.unload";
    pub const DOMAIN_ADAPTER_EXECUTE: &str = "adapter.domain.execute";
    pub const DOMAIN_ADAPTER_TEST: &str = "adapter.domain.test";

    // Replay actions
    pub const REPLAY_CREATE: &str = "replay.create";
    pub const REPLAY_VERIFY: &str = "replay.verify";

    // Federation actions
    pub const FEDERATION_QUARANTINE_RELEASE: &str = "federation.quarantine.release";

    // Monitoring actions
    pub const MONITORING_RULE_CREATE: &str = "monitoring.rule.create";
    pub const MONITORING_ALERT_ACK: &str = "monitoring.alert.acknowledge";
    pub const MONITORING_ANOMALY_UPDATE: &str = "monitoring.anomaly.update";
    pub const MONITORING_DASHBOARD_CREATE: &str = "monitoring.dashboard.create";
    pub const MONITORING_REPORT_CREATE: &str = "monitoring.report.create";

    // Contact actions
    pub const CONTACT_CREATE: &str = "contact.create";
    pub const CONTACT_DELETE: &str = "contact.delete";

    // Plan actions
    pub const PLAN_BUILD: &str = "plan.build";
    pub const PLAN_REBUILD: &str = "plan.rebuild";
    pub const PLAN_COMPARE: &str = "plan.compare";

    // Promotion actions
    pub const PROMOTION_EXECUTE: &str = "promotion.execute";
    pub const PROMOTION_ROLLBACK: &str = "promotion.rollback";
    pub const PROMOTION_DRY_RUN: &str = "promotion.dry_run";

    // Telemetry actions
    pub const TELEMETRY_BUNDLE_EXPORT: &str = "telemetry.bundle.export";
    pub const TELEMETRY_BUNDLE_VERIFY: &str = "telemetry.bundle.verify";
    pub const TELEMETRY_BUNDLE_PURGE: &str = "telemetry.bundle.purge";

    // Dataset actions
    pub const DATASET_CREATE: &str = "dataset.create";
    pub const DATASET_UPLOAD: &str = "dataset.upload";
    pub const DATASET_VALIDATE: &str = "dataset.validate";
    pub const DATASET_DELETE: &str = "dataset.delete";
    pub const DATASET_CHUNKED_UPLOAD_INIT: &str = "dataset.chunked_upload.init";
    pub const DATASET_CHUNKED_UPLOAD_CHUNK: &str = "dataset.chunked_upload.chunk";
    pub const DATASET_CHUNKED_UPLOAD_COMPLETE: &str = "dataset.chunked_upload.complete";
    pub const DATASET_CHUNKED_UPLOAD_CANCEL: &str = "dataset.chunked_upload.cancel";
    pub const DATASET_CHUNKED_UPLOAD_CLEANUP: &str = "dataset.chunked_upload.cleanup";

    // Dataset safety actions
    pub const DATASET_SAFETY_UPDATE: &str = "dataset.safety.update";
    pub const DATASET_TRUST_OVERRIDE: &str = "dataset.trust.override";
    pub const DATASET_SAFETY_CHECK: &str = "dataset.safety.check";

    // Workspace actions
    pub const WORKSPACE_CREATE: &str = "workspace.create";
    pub const WORKSPACE_UPDATE: &str = "workspace.update";
    pub const WORKSPACE_DELETE: &str = "workspace.delete";
    pub const WORKSPACE_MEMBER_ADD: &str = "workspace.member.add";
    pub const WORKSPACE_MEMBER_UPDATE: &str = "workspace.member.update";
    pub const WORKSPACE_MEMBER_REMOVE: &str = "workspace.member.remove";
    pub const WORKSPACE_RESOURCE_SHARE: &str = "workspace.resource.share";
    pub const WORKSPACE_RESOURCE_UNSHARE: &str = "workspace.resource.unshare";

    // Notification actions
    pub const NOTIFICATION_READ: &str = "notification.read";
    pub const NOTIFICATION_READ_ALL: &str = "notification.read_all";

    // Dashboard actions
    pub const DASHBOARD_CONFIG_UPDATE: &str = "dashboard.config.update";
    pub const DASHBOARD_CONFIG_RESET: &str = "dashboard.config.reset";

    // Activity actions
    pub const ACTIVITY_EVENT_CREATE: &str = "activity.event.create";

    // Settings actions
    pub const SETTINGS_UPDATE: &str = "settings.update";

    // Document actions
    pub const DOCUMENT_UPLOAD: &str = "document.upload";
    pub const DOCUMENT_DELETE: &str = "document.delete";
    pub const DOCUMENT_RETRY: &str = "document.retry";
    pub const COLLECTION_CREATE: &str = "collection.create";
    pub const COLLECTION_DELETE: &str = "collection.delete";
    pub const COLLECTION_ADD_DOCUMENT: &str = "collection.document.add";
    pub const COLLECTION_REMOVE_DOCUMENT: &str = "collection.document.remove";
    pub const INFERENCE_WITH_EVIDENCE: &str = "inference.execute_with_evidence";
}

/// Resource types as constants
pub mod resources {
    pub const ADAPTER: &str = "adapter";
    pub const MODEL: &str = "model";
    pub const TRAINING_JOB: &str = "training_job";
    pub const TENANT: &str = "tenant";
    pub const NODE: &str = "node";
    pub const POLICY: &str = "policy";
    pub const POLICY_BINDING: &str = "policy_binding";
    pub const WORKER: &str = "worker";
    pub const ADAPTER_STACK: &str = "adapter_stack";
    pub const GIT_SESSION: &str = "git_session";
    pub const CODE_REPO: &str = "code_repo";
    pub const CODE_SCAN: &str = "code_scan";
    pub const DOMAIN_ADAPTER: &str = "domain_adapter";
    pub const REPLAY_SESSION: &str = "replay_session";
    pub const FEDERATION: &str = "federation";
    pub const MONITORING_RULE: &str = "monitoring_rule";
    pub const MONITORING_ALERT: &str = "monitoring_alert";
    pub const MONITORING_ANOMALY: &str = "monitoring_anomaly";
    pub const MONITORING_DASHBOARD: &str = "monitoring_dashboard";
    pub const MONITORING_REPORT: &str = "monitoring_report";
    pub const CONTACT: &str = "contact";
    pub const PLAN: &str = "plan";
    pub const PROMOTION: &str = "promotion";
    pub const TELEMETRY_BUNDLE: &str = "telemetry_bundle";
    pub const STREAM_ENDPOINT: &str = "stream_endpoint";
    pub const DATASET: &str = "dataset";
    pub const DATASET_VERSION: &str = "dataset_version";
    pub const WORKSPACE: &str = "workspace";
    pub const WORKSPACE_MEMBER: &str = "workspace_member";
    pub const WORKSPACE_RESOURCE: &str = "workspace_resource";
    pub const NOTIFICATION: &str = "notification";
    pub const DASHBOARD_CONFIG: &str = "dashboard_config";
    pub const ACTIVITY_EVENT: &str = "activity_event";
    pub const SETTINGS: &str = "settings";
    pub const DOCUMENT: &str = "document";
    pub const COLLECTION: &str = "collection";
}
