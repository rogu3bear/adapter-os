//! Audit logging helper for handlers
//!
//! Provides convenient functions to log audit events from API handlers.
//! All sensitive operations should be logged for compliance and security review.

use crate::auth::Claims;
use adapteros_core::Result;
use adapteros_db::Db;
use tracing::info;

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
    pub const COLLECTION_CREATE: &str = "collection.create";
    pub const COLLECTION_DELETE: &str = "collection.delete";
    pub const COLLECTION_ADD_DOCUMENT: &str = "collection.document.add";
    pub const COLLECTION_REMOVE_DOCUMENT: &str = "collection.document.remove";
    pub const INFERENCE_WITH_EVIDENCE: &str = "inference.execute_with_evidence";
}

/// Resource types as constants
pub mod resources {
    pub const ADAPTER: &str = "adapter";
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
