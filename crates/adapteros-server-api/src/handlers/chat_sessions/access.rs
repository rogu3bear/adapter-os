use crate::api_error::ApiError;
use crate::auth::Claims;
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use adapteros_db::ChatSession;

pub fn is_admin(claims: &Claims) -> bool {
    claims.role.eq_ignore_ascii_case("admin")
        || claims
            .roles
            .iter()
            .any(|role| role.eq_ignore_ascii_case("admin"))
}

pub fn resolve_session_list_user_filter(
    claims: &Claims,
    requested_user_id: Option<String>,
) -> Result<Option<String>, ApiError> {
    if is_admin(claims) {
        return Ok(requested_user_id);
    }

    if let Some(requested) = requested_user_id {
        if requested != claims.sub {
            return Err(ApiError::forbidden(
                "Only admins can list sessions for other users",
            ));
        }
    }

    Ok(Some(claims.sub.clone()))
}

pub async fn ensure_session_read_access(
    state: &AppState,
    claims: &Claims,
    session: &ChatSession,
) -> Result<(), ApiError> {
    validate_tenant_isolation(claims, &session.tenant_id)?;

    if is_admin(claims) || is_session_owner(claims, session) {
        return Ok(());
    }

    if let Some(permission) = state
        .db
        .check_session_share_access(&session.id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
    {
        if can_read_from_share_permission(&permission) {
            return Ok(());
        }
    }

    Err(ApiError::forbidden("Session access denied"))
}

pub async fn ensure_session_write_access(
    state: &AppState,
    claims: &Claims,
    session: &ChatSession,
) -> Result<(), ApiError> {
    validate_tenant_isolation(claims, &session.tenant_id)?;

    if is_admin(claims) || is_session_owner(claims, session) {
        return Ok(());
    }

    if let Some(permission) = state
        .db
        .check_session_share_access(&session.id, &claims.sub, &claims.tenant_id)
        .await
        .map_err(|e| ApiError::db_error(&e).with_details(e.to_string()))?
    {
        if can_write_from_share_permission(&permission) {
            return Ok(());
        }
    }

    Err(ApiError::forbidden("Session write access denied"))
}

fn is_session_owner(claims: &Claims, session: &ChatSession) -> bool {
    session.user_id.as_deref() == Some(claims.sub.as_str())
        || session.created_by.as_deref() == Some(claims.sub.as_str())
}

fn can_read_from_share_permission(permission: &str) -> bool {
    permission.eq_ignore_ascii_case("view")
        || permission.eq_ignore_ascii_case("comment")
        || permission.eq_ignore_ascii_case("collaborate")
        || permission.eq_ignore_ascii_case("read")
        || permission.eq_ignore_ascii_case("write")
        || permission.eq_ignore_ascii_case("owner")
        || permission.eq_ignore_ascii_case("admin")
}

fn can_write_from_share_permission(permission: &str) -> bool {
    permission.eq_ignore_ascii_case("comment")
        || permission.eq_ignore_ascii_case("collaborate")
        || permission.eq_ignore_ascii_case("write")
        || permission.eq_ignore_ascii_case("owner")
        || permission.eq_ignore_ascii_case("admin")
}
