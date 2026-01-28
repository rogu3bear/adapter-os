//! Middleware helpers for admin handlers
//!
//! Provides role-checking functions for authorization.

use crate::auth::AdminClaims;
use crate::types::AdminErrorResponse;
use adapteros_db::users::Role;
use axum::{http::StatusCode, Json};
use std::str::FromStr;

/// Require a specific role
pub fn require_role(
    claims: &AdminClaims,
    required: Role,
) -> Result<(), (StatusCode, Json<AdminErrorResponse>)> {
    let user_role = Role::from_str(&claims.role).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(AdminErrorResponse::new("Invalid role").with_code("FORBIDDEN")),
        )
    })?;

    // Check if user's role has sufficient privileges
    let has_permission = match required {
        Role::Admin => matches!(user_role, Role::Admin),
        Role::Operator => matches!(user_role, Role::Admin | Role::Operator),
        Role::Viewer => matches!(user_role, Role::Admin | Role::Operator | Role::Viewer),
    };

    if !has_permission {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                AdminErrorResponse::new(format!("{:?} role required", required))
                    .with_code("FORBIDDEN"),
            ),
        ));
    }

    Ok(())
}

/// Require any of the specified roles
pub fn require_any_role(
    claims: &AdminClaims,
    roles: &[Role],
) -> Result<(), (StatusCode, Json<AdminErrorResponse>)> {
    let user_role = Role::from_str(&claims.role).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(AdminErrorResponse::new("Invalid role").with_code("FORBIDDEN")),
        )
    })?;

    // Check if user's role is in the allowed list
    let has_permission = roles.iter().any(|required| match required {
        Role::Admin => matches!(user_role, Role::Admin),
        Role::Operator => matches!(user_role, Role::Admin | Role::Operator),
        Role::Viewer => matches!(user_role, Role::Admin | Role::Operator | Role::Viewer),
    });

    if !has_permission {
        return Err((
            StatusCode::FORBIDDEN,
            Json(
                AdminErrorResponse::new("Insufficient permissions".to_string())
                    .with_code("FORBIDDEN"),
            ),
        ));
    }

    Ok(())
}
