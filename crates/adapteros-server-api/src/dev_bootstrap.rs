//! Dev no-auth bootstrap: ensures tenant/user/workspace exist when AOS_DEV_NO_AUTH is used.
//! Called from auth middleware so dataset upload and other operations have valid FK references.

#[cfg(debug_assertions)]
use adapteros_db::users::Role;
#[cfg(debug_assertions)]
use adapteros_db::workspaces::WorkspaceRole;
#[cfg(debug_assertions)]
use tracing::{info, warn};

use crate::state::AppState;

/// Ensure default tenant, dev-admin-user, and workspace exist for dev-no-auth mode.
/// Idempotent; safe to call on every dev-bypassed request.
#[cfg(debug_assertions)]
pub async fn ensure_dev_no_auth_bootstrap(state: &AppState) {
    let user_id = "dev-admin-user";
    let email = "dev-admin@adapteros.local";
    let tenant_id = "default";

    let Some(pool) = state.db.pool_opt() else {
        return;
    };

    match sqlx::query_scalar::<_, String>("SELECT id FROM tenants WHERE id = 'default'")
        .fetch_optional(pool)
        .await
    {
        Ok(None) => {
            if let Err(e) = sqlx::query(
                "INSERT INTO tenants (id, name, itar_flag, created_at) VALUES ('default', 'Default', 0, datetime('now'))",
            )
            .execute(pool)
            .await
            {
                warn!(error = %e, "Dev bootstrap: failed to create tenant");
            } else {
                info!("Dev bootstrap: created default tenant");
            }
        }
        Err(e) => warn!(error = %e, "Dev bootstrap: failed to check tenant"),
        Ok(Some(_)) => {}
    }

    if let Err(e) = state
        .db
        .ensure_user(
            user_id,
            email,
            "Developer Admin",
            "",
            Role::Admin,
            tenant_id,
        )
        .await
    {
        warn!(error = %e, "Dev bootstrap: failed to ensure user (non-fatal)");
    }

    if let Ok(None) = state.db.get_workspace_by_created_by(user_id).await {
        let ws_id = crate::id_generator::readable_id(adapteros_id::IdPrefix::Wsp, "dev");
        if let Err(e) = sqlx::query(
            "INSERT INTO workspaces (id, name, description, created_by, created_at, updated_at) VALUES (?, 'Default Workspace', 'Auto-created for dev no-auth', ?, datetime('now'), datetime('now'))",
        )
        .bind(&ws_id)
        .bind(user_id)
        .execute(pool)
        .await
        {
            warn!(error = %e, "Dev bootstrap: failed to create workspace");
        } else if let Err(e) = state
            .db
            .add_workspace_member(&ws_id, tenant_id, Some(user_id), WorkspaceRole::Owner, None, user_id)
            .await
        {
            warn!(error = %e, "Dev bootstrap: failed to add workspace member");
        } else {
            info!(workspace_id = %ws_id, "Dev bootstrap: created workspace for dev-admin-user");
        }
    }
}

#[cfg(not(debug_assertions))]
pub async fn ensure_dev_no_auth_bootstrap(_state: &AppState) {
    // No-op in release builds
}
