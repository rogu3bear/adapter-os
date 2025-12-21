//! Permission matrix for role-based access control (RBAC)
//!
//! Defines fine-grained permissions and maps them to user roles.
//! Used by handlers to enforce access control before executing operations.

use crate::api_error::ApiError;
use crate::auth::Claims;
pub use adapteros_db::users::Role;
use std::{fmt, str::FromStr};
use tracing::{debug, warn};

/// Granular permissions for operations in AdapterOS
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Permission {
    // Adapter permissions
    AdapterList,
    AdapterView,
    AdapterRegister,
    AdapterDelete,
    AdapterLoad,
    AdapterUnload,

    // Training permissions
    TrainingStart,
    TrainingCancel,
    TrainingView,
    TrainingViewLogs,

    // Tenant permissions
    TenantManage,
    TenantView,
    TenantTokenRevoke, // Bulk revocation of all tenant tokens - PRD-03

    // Policy permissions
    PolicyView,
    PolicyValidate,
    PolicyApply,
    PolicySign,

    // Inference permissions
    InferenceExecute,

    // Metrics permissions
    MetricsView,

    // Node permissions
    NodeManage,
    NodeView,

    // Worker permissions
    WorkerSpawn,
    WorkerManage,
    WorkerView,

    // Git permissions
    GitView,
    GitManage,

    // Code intelligence permissions
    CodeView,
    CodeScan,

    // Audit permissions
    AuditView,

    // Adapter stack permissions
    AdapterStackView,
    AdapterStackManage,

    // Monitoring permissions
    MonitoringManage,

    // Replay permissions
    ReplayManage,

    // Federation permissions
    FederationView,
    FederationManage,

    // Plan permissions
    PlanView,
    PlanManage,

    // Promotion permissions
    PromotionManage,

    // Telemetry permissions
    TelemetryView,
    TelemetryManage,

    // Contact permissions
    ContactView,
    ContactManage,

    // Dataset permissions
    DatasetList,
    DatasetView,
    DatasetUpload,
    DatasetValidate,
    DatasetDelete,

    // Workspace permissions
    WorkspaceView,
    WorkspaceManage,
    WorkspaceMemberManage,
    WorkspaceResourceManage,

    // Notification permissions
    NotificationView,
    NotificationManage,

    // Dashboard permissions
    DashboardView,
    DashboardManage,

    // Activity permissions
    ActivityView,
    ActivityCreate,
}

const ALL_PERMISSIONS: [Permission; 57] = [
    Permission::AdapterList,
    Permission::AdapterView,
    Permission::AdapterRegister,
    Permission::AdapterDelete,
    Permission::AdapterLoad,
    Permission::AdapterUnload,
    Permission::TrainingStart,
    Permission::TrainingCancel,
    Permission::TrainingView,
    Permission::TrainingViewLogs,
    Permission::TenantManage,
    Permission::TenantView,
    Permission::TenantTokenRevoke,
    Permission::PolicyView,
    Permission::PolicyValidate,
    Permission::PolicyApply,
    Permission::PolicySign,
    Permission::InferenceExecute,
    Permission::MetricsView,
    Permission::NodeManage,
    Permission::NodeView,
    Permission::WorkerSpawn,
    Permission::WorkerManage,
    Permission::WorkerView,
    Permission::GitView,
    Permission::GitManage,
    Permission::CodeView,
    Permission::CodeScan,
    Permission::AuditView,
    Permission::AdapterStackView,
    Permission::AdapterStackManage,
    Permission::MonitoringManage,
    Permission::ReplayManage,
    Permission::FederationView,
    Permission::FederationManage,
    Permission::PlanView,
    Permission::PlanManage,
    Permission::PromotionManage,
    Permission::TelemetryView,
    Permission::TelemetryManage,
    Permission::ContactView,
    Permission::ContactManage,
    Permission::DatasetList,
    Permission::DatasetView,
    Permission::DatasetUpload,
    Permission::DatasetValidate,
    Permission::DatasetDelete,
    Permission::WorkspaceView,
    Permission::WorkspaceManage,
    Permission::WorkspaceMemberManage,
    Permission::WorkspaceResourceManage,
    Permission::NotificationView,
    Permission::NotificationManage,
    Permission::DashboardView,
    Permission::DashboardManage,
    Permission::ActivityView,
    Permission::ActivityCreate,
];

impl fmt::Display for Permission {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Check if a role has a specific permission
///
/// # Permission Matrix
///
/// **Admin** - Full access to all operations
/// **Operator** - Adapters, training, inference (not delete/tenant/policy)
/// **SRE** - Infrastructure, metrics, diagnostics (not manage nodes/adapters)
/// **Compliance** - Policies, audit trails (view only + validate)
/// **Viewer** - Strict read-only access
///
/// # Example
/// ```no_run
/// use adapteros_server_api::permissions::{Permission, has_permission};
/// use adapteros_db::users::Role;
///
/// assert!(has_permission(&Role::Admin, Permission::AdapterDelete));
/// assert!(!has_permission(&Role::Viewer, Permission::AdapterDelete));
/// assert!(has_permission(&Role::Compliance, Permission::PolicyValidate));
/// ```
pub fn has_permission(role: &Role, permission: Permission) -> bool {
    match (role, permission) {
        // Admin has all permissions
        (Role::Admin, _) => true,

        // Developer has all permissions (full access for development)
        (Role::Developer, _) => true,

        // ========== VIEWER ROLE ==========
        // Strict read-only access - no write operations
        (Role::Viewer, Permission::AdapterList) => true,
        (Role::Viewer, Permission::AdapterView) => true,
        (Role::Viewer, Permission::TrainingView) => true,
        (Role::Viewer, Permission::TrainingViewLogs) => true,
        (Role::Viewer, Permission::MetricsView) => true,
        (Role::Viewer, Permission::TenantView) => true,
        (Role::Viewer, Permission::PolicyView) => true,
        (Role::Viewer, Permission::NodeView) => true,
        (Role::Viewer, Permission::WorkerView) => true,
        (Role::Viewer, Permission::GitView) => true,
        (Role::Viewer, Permission::CodeView) => true,
        (Role::Viewer, Permission::AdapterStackView) => true,
        (Role::Viewer, Permission::PlanView) => true,
        (Role::Viewer, Permission::FederationView) => true,
        (Role::Viewer, Permission::TelemetryView) => true,
        (Role::Viewer, Permission::ContactView) => true,
        (Role::Viewer, Permission::DatasetList) => true,
        (Role::Viewer, Permission::DatasetView) => true,
        (Role::Viewer, Permission::WorkspaceView) => true,
        (Role::Viewer, Permission::NotificationView) => true,
        (Role::Viewer, Permission::DashboardView) => true,
        (Role::Viewer, Permission::ActivityView) => true,
        (Role::Viewer, _) => false, // All write operations blocked

        // ========== OPERATOR ROLE ==========
        // Runtime operations: adapters, training, inference
        (Role::Operator, Permission::AdapterList) => true,
        (Role::Operator, Permission::AdapterView) => true,
        (Role::Operator, Permission::AdapterRegister) => true,
        (Role::Operator, Permission::AdapterLoad) => true,
        (Role::Operator, Permission::AdapterUnload) => true,
        (Role::Operator, Permission::AdapterDelete) => false, // Cannot delete (Admin only)
        (Role::Operator, Permission::TrainingStart) => true,
        (Role::Operator, Permission::TrainingCancel) => true,
        (Role::Operator, Permission::TrainingView) => true,
        (Role::Operator, Permission::TrainingViewLogs) => true,
        (Role::Operator, Permission::InferenceExecute) => true,
        (Role::Operator, Permission::MetricsView) => true,
        (Role::Operator, Permission::WorkerView) => true,
        (Role::Operator, Permission::WorkerSpawn) => true,
        (Role::Operator, Permission::WorkerManage) => true,
        (Role::Operator, Permission::TenantView) => true,
        (Role::Operator, Permission::TenantManage) => false, // Cannot manage tenants
        (Role::Operator, Permission::TenantTokenRevoke) => false, // Cannot bulk-revoke tokens (Admin only) - PRD-03
        (Role::Operator, Permission::PolicyView) => true,
        (Role::Operator, Permission::PolicyApply) => false, // Cannot apply policies
        (Role::Operator, Permission::PolicySign) => false,  // Cannot sign policies
        (Role::Operator, Permission::GitView) => true,
        (Role::Operator, Permission::GitManage) => true,
        (Role::Operator, Permission::CodeView) => true,
        (Role::Operator, Permission::CodeScan) => true,
        (Role::Operator, Permission::AdapterStackView) => true,
        (Role::Operator, Permission::AdapterStackManage) => true,
        (Role::Operator, Permission::ContactView) => true,
        (Role::Operator, Permission::ContactManage) => true,
        (Role::Operator, Permission::PlanView) => true,
        (Role::Operator, Permission::TelemetryView) => true,
        (Role::Operator, Permission::DatasetList) => true,
        (Role::Operator, Permission::DatasetView) => true,
        (Role::Operator, Permission::DatasetUpload) => true,
        (Role::Operator, Permission::DatasetValidate) => true,
        (Role::Operator, Permission::DatasetDelete) => false, // Cannot delete (Admin only)
        (Role::Operator, Permission::WorkspaceView) => true,
        (Role::Operator, Permission::WorkspaceManage) => true,
        (Role::Operator, Permission::WorkspaceMemberManage) => true,
        (Role::Operator, Permission::WorkspaceResourceManage) => true,
        (Role::Operator, Permission::NotificationView) => true,
        (Role::Operator, Permission::NotificationManage) => true,
        (Role::Operator, Permission::DashboardView) => true,
        (Role::Operator, Permission::DashboardManage) => true,
        (Role::Operator, Permission::ActivityView) => true,
        (Role::Operator, Permission::ActivityCreate) => true,
        (Role::Operator, _) => false,

        // ========== SRE ROLE ==========
        // Infrastructure, monitoring, troubleshooting
        (Role::SRE, Permission::NodeView) => true,
        (Role::SRE, Permission::NodeManage) => false, // Cannot register/delete nodes (Admin only)
        (Role::SRE, Permission::MetricsView) => true,
        (Role::SRE, Permission::AdapterList) => true,
        (Role::SRE, Permission::AdapterView) => true,
        (Role::SRE, Permission::AdapterLoad) => true, // Can load for troubleshooting
        (Role::SRE, Permission::AdapterUnload) => true, // Can unload for troubleshooting
        (Role::SRE, Permission::AdapterRegister) => false, // Cannot register new adapters
        (Role::SRE, Permission::AdapterDelete) => false, // Cannot delete adapters
        (Role::SRE, Permission::InferenceExecute) => true, // Can test inference
        (Role::SRE, Permission::WorkerView) => true,
        (Role::SRE, Permission::WorkerManage) => false, // Cannot spawn/manage workers
        (Role::SRE, Permission::TrainingView) => true,
        (Role::SRE, Permission::TrainingViewLogs) => true,
        (Role::SRE, Permission::TrainingStart) => false,
        (Role::SRE, Permission::TrainingCancel) => false,
        (Role::SRE, Permission::PolicyView) => true,
        (Role::SRE, Permission::TenantView) => true,
        (Role::SRE, Permission::TenantManage) => false,
        (Role::SRE, Permission::TenantTokenRevoke) => false, // Cannot bulk-revoke tokens (Admin only) - PRD-03
        (Role::SRE, Permission::GitView) => true,
        (Role::SRE, Permission::CodeView) => true,
        (Role::SRE, Permission::AuditView) => true, // Can view audit logs for troubleshooting
        (Role::SRE, Permission::AdapterStackView) => true,
        (Role::SRE, Permission::MonitoringManage) => true, // Can manage monitoring rules and alerts
        (Role::SRE, Permission::PlanView) => true,
        (Role::SRE, Permission::TelemetryView) => true,
        (Role::SRE, Permission::ReplayManage) => true, // Can create/verify replay sessions for debugging
        (Role::SRE, Permission::FederationView) => true,
        (Role::SRE, Permission::DatasetList) => true,
        (Role::SRE, Permission::DatasetView) => true,
        (Role::SRE, Permission::WorkspaceView) => true,
        (Role::SRE, Permission::NotificationView) => true,
        (Role::SRE, Permission::NotificationManage) => true,
        (Role::SRE, Permission::DashboardView) => true,
        (Role::SRE, Permission::DashboardManage) => true,
        (Role::SRE, Permission::ActivityView) => true,
        (Role::SRE, _) => false,

        // ========== COMPLIANCE ROLE ==========
        // Policy oversight and audit trails
        (Role::Compliance, Permission::PolicyView) => true,
        (Role::Compliance, Permission::PolicyValidate) => true, // Can validate compliance
        (Role::Compliance, Permission::PolicyApply) => false,   // Cannot apply (Admin only)
        (Role::Compliance, Permission::PolicySign) => false,    // Cannot sign (Admin only)
        (Role::Compliance, Permission::MetricsView) => true,
        (Role::Compliance, Permission::AdapterList) => true,
        (Role::Compliance, Permission::AdapterView) => true,
        (Role::Compliance, Permission::AdapterRegister) => false,
        (Role::Compliance, Permission::AdapterDelete) => false,
        (Role::Compliance, Permission::AdapterLoad) => false,
        (Role::Compliance, Permission::AdapterUnload) => false,
        (Role::Compliance, Permission::TrainingView) => true,
        (Role::Compliance, Permission::TrainingViewLogs) => true,
        (Role::Compliance, Permission::TrainingStart) => false,
        (Role::Compliance, Permission::TrainingCancel) => false,
        (Role::Compliance, Permission::InferenceExecute) => false,
        (Role::Compliance, Permission::TenantView) => true,
        (Role::Compliance, Permission::NodeView) => true,
        (Role::Compliance, Permission::WorkerView) => true,
        (Role::Compliance, Permission::GitView) => true,
        (Role::Compliance, Permission::CodeView) => true,
        (Role::Compliance, Permission::AuditView) => true, // Primary use case
        (Role::Compliance, Permission::AdapterStackView) => true,
        (Role::Compliance, Permission::PlanView) => true,
        (Role::Compliance, Permission::FederationView) => true,
        (Role::Compliance, Permission::TelemetryView) => true,
        (Role::Compliance, Permission::ReplayManage) => true, // Can verify replay sessions for compliance
        (Role::Compliance, Permission::ContactView) => true,
        (Role::Compliance, Permission::DatasetList) => true,
        (Role::Compliance, Permission::DatasetView) => true,
        (Role::Compliance, Permission::DatasetValidate) => true, // Can validate datasets
        (Role::Compliance, Permission::WorkspaceView) => true,
        (Role::Compliance, Permission::NotificationView) => true,
        (Role::Compliance, Permission::NotificationManage) => true,
        (Role::Compliance, Permission::DashboardView) => true,
        (Role::Compliance, Permission::DashboardManage) => true,
        (Role::Compliance, Permission::ActivityView) => true,
        (Role::Compliance, _) => false,
    }
}

/// Require a specific permission from the authenticated user
///
/// Returns `Ok(())` if the user has permission, or `Err` with 403 Forbidden
///
/// # Example
/// ```no_run
/// use adapteros_server_api::permissions::{Permission, require_permission};
/// use crate::auth::Claims;
///
/// pub async fn my_handler(claims: Claims) -> ApiResult<Response> {
///     require_permission(&claims, Permission::AdapterRegister)?;
///     // ... proceed with operation
///     Ok(Json(response))
/// }
/// ```
pub fn require_permission(claims: &Claims, permission: Permission) -> Result<(), ApiError> {
    let role = Role::from_str(&claims.role).map_err(|_| {
        warn!(
            user_id = %claims.sub,
            role = %claims.role,
            "Invalid role in JWT claims - valid roles are: admin, developer, operator, sre, compliance, viewer"
        );
        ApiError::bad_request("invalid role in authentication token").with_details(format!(
            "role '{}' is not valid, expected one of: admin, developer, operator, sre, compliance, viewer",
            claims.role
        ))
    })?;

    debug!(
        user_id = %claims.sub,
        role = %claims.role,
        permission = ?permission,
        check_type = "permission",
        "Permission check performed"
    );

    if has_permission(&role, permission) {
        Ok(())
    } else {
        warn!(
            user_id = %claims.sub,
            role = %claims.role,
            required_permission = ?permission,
            "Permission denied"
        );
        Err(
            ApiError::forbidden("insufficient permissions").with_details(format!(
                "required permission: {:?}, user role: {}",
                permission, claims.role
            )),
        )
    }
}

/// Returns every granted permission name for a role
pub fn permissions_for_role(role: &Role) -> Vec<Permission> {
    ALL_PERMISSIONS
        .iter()
        .filter(|permission| has_permission(role, **permission))
        .copied()
        .collect()
}

/// Require any of the specified permissions
///
/// Returns `Ok(())` if the user has at least one of the permissions
pub fn require_any_permission(claims: &Claims, permissions: &[Permission]) -> Result<(), ApiError> {
    let role = Role::from_str(&claims.role).map_err(|_| {
        warn!(
            user_id = %claims.sub,
            role = %claims.role,
            "Invalid role in JWT claims - valid roles are: admin, developer, operator, sre, compliance, viewer"
        );
        ApiError::bad_request("invalid role in authentication token").with_details(format!(
            "role '{}' is not valid, expected one of: admin, developer, operator, sre, compliance, viewer",
            claims.role
        ))
    })?;

    for permission in permissions {
        if has_permission(&role, *permission) {
            return Ok(());
        }
    }

    Err(
        ApiError::forbidden("insufficient permissions").with_details(format!(
            "required one of: {:?}, user role: {}",
            permissions, claims.role
        )),
    )
}

/// Require any of the specified roles
///
/// Returns `Ok(())` if the user has any of the required roles (Admin always passes)
pub fn require_any_role(claims: &Claims, roles: &[Role]) -> Result<(), ApiError> {
    let user_role = Role::from_str(&claims.role).map_err(|_| {
        warn!(
            user_id = %claims.sub,
            role = %claims.role,
            "Invalid role in JWT claims - valid roles are: admin, developer, operator, sre, compliance, viewer"
        );
        ApiError::bad_request("invalid role in authentication token").with_details(format!(
            "role '{}' is not valid, expected one of: admin, developer, operator, sre, compliance, viewer",
            claims.role
        ))
    })?;

    // Admin and Developer bypass all role checks
    if user_role == Role::Admin || user_role == Role::Developer || roles.contains(&user_role) {
        return Ok(());
    }

    Err(
        ApiError::forbidden("insufficient permissions").with_details(format!(
            "required one of: {:?}, user role: {}",
            roles, claims.role
        )),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_admin_has_all_permissions() {
        assert!(has_permission(&Role::Admin, Permission::AdapterDelete));
        assert!(has_permission(&Role::Admin, Permission::TenantManage));
        assert!(has_permission(&Role::Admin, Permission::PolicySign));
    }

    #[test]
    fn test_viewer_read_only() {
        // Can view
        assert!(has_permission(&Role::Viewer, Permission::AdapterView));
        assert!(has_permission(&Role::Viewer, Permission::MetricsView));

        // Cannot write
        assert!(!has_permission(&Role::Viewer, Permission::AdapterRegister));
        assert!(!has_permission(&Role::Viewer, Permission::AdapterDelete));
        assert!(!has_permission(&Role::Viewer, Permission::TrainingStart));
        assert!(!has_permission(&Role::Viewer, Permission::InferenceExecute));
    }

    #[test]
    fn test_operator_permissions() {
        // Can manage adapters (except delete)
        assert!(has_permission(&Role::Operator, Permission::AdapterRegister));
        assert!(has_permission(&Role::Operator, Permission::AdapterLoad));
        assert!(!has_permission(&Role::Operator, Permission::AdapterDelete));

        // Can manage training
        assert!(has_permission(&Role::Operator, Permission::TrainingStart));
        assert!(has_permission(&Role::Operator, Permission::TrainingCancel));

        // Can execute inference
        assert!(has_permission(
            &Role::Operator,
            Permission::InferenceExecute
        ));

        // Cannot manage tenants or sign policies
        assert!(!has_permission(&Role::Operator, Permission::TenantManage));
        assert!(!has_permission(&Role::Operator, Permission::PolicySign));
    }

    #[test]
    fn test_sre_permissions() {
        // Can view and troubleshoot
        assert!(has_permission(&Role::SRE, Permission::MetricsView));
        assert!(has_permission(&Role::SRE, Permission::AdapterLoad));
        assert!(has_permission(&Role::SRE, Permission::AdapterUnload));
        assert!(has_permission(&Role::SRE, Permission::InferenceExecute));

        // Cannot register/delete
        assert!(!has_permission(&Role::SRE, Permission::AdapterRegister));
        assert!(!has_permission(&Role::SRE, Permission::AdapterDelete));
        assert!(!has_permission(&Role::SRE, Permission::NodeManage));
    }

    #[test]
    fn test_compliance_permissions() {
        // Can view and validate policies
        assert!(has_permission(&Role::Compliance, Permission::PolicyView));
        assert!(has_permission(
            &Role::Compliance,
            Permission::PolicyValidate
        ));
        assert!(has_permission(&Role::Compliance, Permission::AuditView));

        // Cannot apply or sign policies
        assert!(!has_permission(&Role::Compliance, Permission::PolicyApply));
        assert!(!has_permission(&Role::Compliance, Permission::PolicySign));

        // Cannot manage adapters or training
        assert!(!has_permission(
            &Role::Compliance,
            Permission::AdapterRegister
        ));
        assert!(!has_permission(
            &Role::Compliance,
            Permission::TrainingStart
        ));
        assert!(!has_permission(
            &Role::Compliance,
            Permission::InferenceExecute
        ));
    }
}
