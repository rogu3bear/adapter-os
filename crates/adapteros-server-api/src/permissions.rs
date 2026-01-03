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
/// # Permission Matrix (Simplified 3-Role Model)
///
/// **Admin** - Full access to all operations
/// **Operator** - Runtime operations: adapters, training, inference (not delete/tenant/policy management)
/// **Viewer** - Strict read-only access
///
/// # Example
/// ```no_run
/// use adapteros_server_api::permissions::{Permission, has_permission};
/// use adapteros_db::users::Role;
///
/// assert!(has_permission(&Role::Admin, Permission::AdapterDelete));
/// assert!(!has_permission(&Role::Viewer, Permission::AdapterDelete));
/// assert!(has_permission(&Role::Operator, Permission::TrainingStart));
/// ```
pub fn has_permission(role: &Role, permission: Permission) -> bool {
    match (role, permission) {
        // ========== ADMIN ROLE ==========
        // Full access to all operations
        (Role::Admin, _) => true,

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
        (Role::Viewer, Permission::AuditView) => true,
        (Role::Viewer, _) => false, // All write operations blocked

        // ========== OPERATOR ROLE ==========
        // Runtime operations: adapters, training, inference
        // Cannot: delete, manage tenants, manage policies, manage nodes
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
        (Role::Operator, Permission::TenantManage) => false, // Cannot manage tenants (Admin only)
        (Role::Operator, Permission::TenantTokenRevoke) => false, // Cannot bulk-revoke tokens (Admin only)
        (Role::Operator, Permission::PolicyView) => true,
        (Role::Operator, Permission::PolicyValidate) => true,
        (Role::Operator, Permission::PolicyApply) => false, // Cannot apply policies (Admin only)
        (Role::Operator, Permission::PolicySign) => false,  // Cannot sign policies (Admin only)
        (Role::Operator, Permission::NodeView) => true,
        (Role::Operator, Permission::NodeManage) => false, // Cannot manage nodes (Admin only)
        (Role::Operator, Permission::GitView) => true,
        (Role::Operator, Permission::GitManage) => true,
        (Role::Operator, Permission::CodeView) => true,
        (Role::Operator, Permission::CodeScan) => true,
        (Role::Operator, Permission::AuditView) => true,
        (Role::Operator, Permission::AdapterStackView) => true,
        (Role::Operator, Permission::AdapterStackManage) => true,
        (Role::Operator, Permission::MonitoringManage) => true,
        (Role::Operator, Permission::ReplayManage) => true,
        (Role::Operator, Permission::FederationView) => true,
        (Role::Operator, Permission::FederationManage) => false, // Cannot manage federation (Admin only)
        (Role::Operator, Permission::PlanView) => true,
        (Role::Operator, Permission::PlanManage) => true,
        (Role::Operator, Permission::PromotionManage) => true,
        (Role::Operator, Permission::TelemetryView) => true,
        (Role::Operator, Permission::TelemetryManage) => true,
        (Role::Operator, Permission::ContactView) => true,
        (Role::Operator, Permission::ContactManage) => true,
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
#[allow(clippy::result_large_err)]
pub fn require_permission(claims: &Claims, permission: Permission) -> Result<(), ApiError> {
    let role = Role::from_str(&claims.role).map_err(|_| {
        warn!(
            user_id = %claims.sub,
            role = %claims.role,
            "Invalid role in JWT claims - valid roles are: admin, operator, viewer"
        );
        ApiError::bad_request("invalid role in authentication token").with_details(format!(
            "role '{}' is not valid, expected one of: admin, operator, viewer",
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
#[allow(clippy::result_large_err)]
pub fn require_any_permission(claims: &Claims, permissions: &[Permission]) -> Result<(), ApiError> {
    let role = Role::from_str(&claims.role).map_err(|_| {
        warn!(
            user_id = %claims.sub,
            role = %claims.role,
            "Invalid role in JWT claims - valid roles are: admin, operator, viewer"
        );
        ApiError::bad_request("invalid role in authentication token").with_details(format!(
            "role '{}' is not valid, expected one of: admin, operator, viewer",
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
#[allow(clippy::result_large_err)]
pub fn require_any_role(claims: &Claims, roles: &[Role]) -> Result<(), ApiError> {
    let user_role = Role::from_str(&claims.role).map_err(|_| {
        warn!(
            user_id = %claims.sub,
            role = %claims.role,
            "Invalid role in JWT claims - valid roles are: admin, operator, viewer"
        );
        ApiError::bad_request("invalid role in authentication token").with_details(format!(
            "role '{}' is not valid, expected one of: admin, operator, viewer",
            claims.role
        ))
    })?;

    // Admin bypasses all role checks
    if user_role == Role::Admin || roles.contains(&user_role) {
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
        // Admin has full access to everything
        assert!(has_permission(&Role::Admin, Permission::AdapterDelete));
        assert!(has_permission(&Role::Admin, Permission::TenantManage));
        assert!(has_permission(&Role::Admin, Permission::PolicySign));
        assert!(has_permission(&Role::Admin, Permission::NodeManage));
        assert!(has_permission(&Role::Admin, Permission::FederationManage));
    }

    #[test]
    fn test_viewer_read_only() {
        // Can view
        assert!(has_permission(&Role::Viewer, Permission::AdapterView));
        assert!(has_permission(&Role::Viewer, Permission::AdapterList));
        assert!(has_permission(&Role::Viewer, Permission::MetricsView));
        assert!(has_permission(&Role::Viewer, Permission::AuditView));
        assert!(has_permission(&Role::Viewer, Permission::PolicyView));

        // Cannot write
        assert!(!has_permission(&Role::Viewer, Permission::AdapterRegister));
        assert!(!has_permission(&Role::Viewer, Permission::AdapterDelete));
        assert!(!has_permission(&Role::Viewer, Permission::TrainingStart));
        assert!(!has_permission(&Role::Viewer, Permission::InferenceExecute));
        assert!(!has_permission(&Role::Viewer, Permission::TenantManage));
        assert!(!has_permission(&Role::Viewer, Permission::PolicySign));
    }

    #[test]
    fn test_operator_permissions() {
        // Can manage adapters (except delete)
        assert!(has_permission(&Role::Operator, Permission::AdapterRegister));
        assert!(has_permission(&Role::Operator, Permission::AdapterLoad));
        assert!(has_permission(&Role::Operator, Permission::AdapterUnload));
        assert!(!has_permission(&Role::Operator, Permission::AdapterDelete));

        // Can manage training
        assert!(has_permission(&Role::Operator, Permission::TrainingStart));
        assert!(has_permission(&Role::Operator, Permission::TrainingCancel));

        // Can execute inference
        assert!(has_permission(
            &Role::Operator,
            Permission::InferenceExecute
        ));

        // Can view audit logs
        assert!(has_permission(&Role::Operator, Permission::AuditView));

        // Cannot manage tenants, nodes, or sign policies (Admin only)
        assert!(!has_permission(&Role::Operator, Permission::TenantManage));
        assert!(!has_permission(&Role::Operator, Permission::PolicySign));
        assert!(!has_permission(&Role::Operator, Permission::PolicyApply));
        assert!(!has_permission(&Role::Operator, Permission::NodeManage));
        assert!(!has_permission(
            &Role::Operator,
            Permission::FederationManage
        ));
        assert!(!has_permission(&Role::Operator, Permission::DatasetDelete));
    }

    #[test]
    fn test_role_hierarchy() {
        // Admin > Operator > Viewer
        // Admin can do everything Operator can
        assert!(has_permission(&Role::Admin, Permission::TrainingStart));
        assert!(has_permission(&Role::Operator, Permission::TrainingStart));
        assert!(!has_permission(&Role::Viewer, Permission::TrainingStart));

        // Only Admin can do admin-only things
        assert!(has_permission(&Role::Admin, Permission::TenantManage));
        assert!(!has_permission(&Role::Operator, Permission::TenantManage));
        assert!(!has_permission(&Role::Viewer, Permission::TenantManage));

        // Everyone can view
        assert!(has_permission(&Role::Admin, Permission::AdapterView));
        assert!(has_permission(&Role::Operator, Permission::AdapterView));
        assert!(has_permission(&Role::Viewer, Permission::AdapterView));
    }

    #[test]
    fn test_role_helper_methods() {
        assert!(Role::Admin.can_admin());
        assert!(Role::Admin.can_write());
        assert!(!Role::Admin.is_viewer());

        assert!(!Role::Operator.can_admin());
        assert!(Role::Operator.can_write());
        assert!(!Role::Operator.is_viewer());

        assert!(!Role::Viewer.can_admin());
        assert!(!Role::Viewer.can_write());
        assert!(Role::Viewer.is_viewer());
    }
}
