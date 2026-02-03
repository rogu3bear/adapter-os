//! Contact handlers for chat sessions
//!
//! Provides list_contacts, create_contact, get_contact, delete_contact, get_contact_interactions
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_contacts】

use crate::auth::Claims;
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::contacts::ContactUpsertParams;
use adapteros_db::Contact;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};

use super::super::utils::aos_error_to_response;

/// List contacts
#[utoipa::path(
    get,
    path = "/v1/contacts",
    params(
        ("limit" = Option<i64>, Query, description = "Limit results"),
        ("offset" = Option<i64>, Query, description = "Offset results")
    ),
    responses(
        (status = 200, description = "List of contacts", body = Vec<Contact>),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn list_contacts(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<crate::types::PaginationParams>,
) -> impl IntoResponse {
    let limit = params.limit as i64;
    let offset = ((params.page.saturating_sub(1)) * params.limit) as i64;

    // Tenant isolation check
    if let Err(e) = validate_tenant_isolation(&claims, &claims.tenant_id) {
        return e.into_response();
    }

    match state
        .db
        .list_contacts(&claims.tenant_id, limit, offset)
        .await
    {
        Ok(contacts) => Json(contacts).into_response(),
        Err(e) => aos_error_to_response(e).into_response(),
    }
}

/// Create/Update contact
#[utoipa::path(
    post,
    path = "/v1/contacts",
    request_body = ContactUpsertParams,
    responses(
        (status = 200, description = "Contact ID", body = String),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn create_contact(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(payload): Json<ContactUpsertParams>,
) -> impl IntoResponse {
    // Tenant isolation check
    if let Err(e) = validate_tenant_isolation(&claims, &payload.tenant_id) {
        return e.into_response();
    }

    match state.db.upsert_contact(payload).await {
        Ok(id) => Json(id).into_response(),
        Err(e) => aos_error_to_response(e).into_response(),
    }
}

/// Get contact
#[utoipa::path(
    get,
    path = "/v1/contacts/{id}",
    params(
        ("id" = String, Path, description = "Contact ID")
    ),
    responses(
        (status = 200, description = "Contact details", body = Contact),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn get_contact(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let id = match crate::id_resolver::resolve_any_id(&state.db, &id).await {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };
    match state.db.get_contact(&id).await {
        Ok(Some(contact)) => {
            // Verify tenant ownership
            // Tenant isolation check
            if let Err(e) = validate_tenant_isolation(&claims, &contact.tenant_id) {
                return e.into_response();
            }
            Json(contact).into_response()
        }
        Ok(None) => aos_error_to_response(adapteros_core::AosError::NotFound(
            "Contact not found".into(),
        ))
        .into_response(),
        Err(e) => aos_error_to_response(e).into_response(),
    }
}

/// Delete contact
#[utoipa::path(
    delete,
    path = "/v1/contacts/{id}",
    params(
        ("id" = String, Path, description = "Contact ID")
    ),
    responses(
        (status = 200, description = "Contact deleted"),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn delete_contact(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let id = match crate::id_resolver::resolve_any_id(&state.db, &id).await {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };
    // Check existence and ownership first
    match state.db.get_contact(&id).await {
        Ok(Some(contact)) => {
            if contact.tenant_id != claims.tenant_id {
                return aos_error_to_response(adapteros_core::AosError::NotFound(
                    "Contact not found".into(),
                ))
                .into_response();
            }
        }
        Ok(None) => {
            return aos_error_to_response(adapteros_core::AosError::NotFound(
                "Contact not found".into(),
            ))
            .into_response()
        }
        Err(e) => return aos_error_to_response(e).into_response(),
    }

    match state.db.delete_contact(&id).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => aos_error_to_response(e).into_response(),
    }
}

/// Get contact interactions
#[utoipa::path(
    get,
    path = "/v1/contacts/{id}/interactions",
    params(
        ("id" = String, Path, description = "Contact ID"),
        ("limit" = Option<i64>, Query, description = "Limit results")
    ),
    responses(
        (status = 200, description = "List of interactions", body = Vec<adapteros_db::contacts::ContactInteraction>),
        (status = 404, description = "Contact not found", body = ErrorResponse),
        (status = 500, description = "Internal server error", body = ErrorResponse)
    )
)]
pub async fn get_contact_interactions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Query(params): Query<crate::types::PaginationParams>,
) -> impl IntoResponse {
    let id = match crate::id_resolver::resolve_any_id(&state.db, &id).await {
        Ok(id) => id,
        Err(e) => return e.into_response(),
    };
    let limit = params.limit as i64;

    // Verify ownership
    match state.db.get_contact(&id).await {
        Ok(Some(contact)) => {
            if contact.tenant_id != claims.tenant_id {
                return aos_error_to_response(adapteros_core::AosError::NotFound(
                    "Contact not found".into(),
                ))
                .into_response();
            }
        }
        Ok(None) => {
            return aos_error_to_response(adapteros_core::AosError::NotFound(
                "Contact not found".into(),
            ))
            .into_response()
        }
        Err(e) => return aos_error_to_response(e).into_response(),
    }

    match state.db.get_contact_interactions(&id, limit).await {
        Ok(interactions) => Json(interactions).into_response(),
        Err(e) => aos_error_to_response(e).into_response(),
    }
}
