//! Core CRUD handlers for chat sessions
//!
//! Provides create, read, update, delete operations for chat sessions.
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_core】

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_db::chat_sessions::UpdateChatSessionParams;
use adapteros_db::{ChatSession, CreateChatSessionParams};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Extension, Json,
};
use tracing::{debug, info};

use super::types::{
    CreateChatSessionRequest, CreateChatSessionResponse, ListSessionsQuery,
    UpdateChatSessionRequest,
};

/// Create a new chat session
///
/// POST /v1/chat/sessions
#[utoipa::path(
    post,
    path = "/v1/chat/sessions",
    tag = "chat",
    request_body = CreateChatSessionRequest,
    responses(
        (status = 201, description = "Session created", body = CreateChatSessionResponse),
        (status = 400, description = "Invalid request", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn create_chat_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateChatSessionRequest>,
) -> Result<(StatusCode, Json<CreateChatSessionResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Permission check: InferenceExecute allows chat sessions
    require_permission(&claims, Permission::InferenceExecute).map_err(|_e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    let target_tenant = claims.tenant_id.clone();

    // Tenant isolation check
    validate_tenant_isolation(&claims, &target_tenant)?;

    // Validate session name
    if req.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Session name cannot be empty")
                    .with_code("VALIDATION_ERROR")
                    .with_string_details("Provide a non-empty name for the chat session"),
            ),
        ));
    }

    let source_type = req.source_type.clone().unwrap_or_else(|| {
        if req.document_id.is_some() {
            "document".to_string()
        } else {
            "general".to_string()
        }
    });
    let allowed_sources = [
        "general",
        "document",
        "owner_system",
        "training_job",
        "cli",
        "cli_prompt",
    ];
    if !allowed_sources.contains(&source_type.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Invalid source_type")
                    .with_code("VALIDATION_ERROR")
                    .with_string_details(format!(
                        "source_type '{}' is not valid. Allowed values: {:?}",
                        source_type, allowed_sources
                    )),
            ),
        ));
    }

    if source_type == "document" && req.document_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("document_id is required for document chats")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    }

    if req.document_id.is_some() && source_type != "document" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("document_id is only allowed when source_type='document'")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    }

    if let Some(collection_id) = req.collection_id.as_ref() {
        let collection = state
            .db
            .get_collection(&target_tenant, collection_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to validate collection")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(format!(
                                "Database error while validating collection '{}': {}",
                                collection_id, e
                            )),
                    ),
                )
            })?;
        if collection.is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("collection_id not found for tenant")
                        .with_code("VALIDATION_ERROR")
                        .with_string_details(format!(
                            "Collection '{}' does not exist for tenant '{}'",
                            collection_id, target_tenant
                        )),
                ),
            ));
        }
    }

    if let Some(document_id) = req.document_id.as_ref() {
        let document = state
            .db
            .get_document(&target_tenant, document_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to validate document")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(format!(
                                "Database error while validating document '{}': {}",
                                document_id, e
                            )),
                    ),
                )
            })?;
        if document.is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("document_id not found for tenant")
                        .with_code("VALIDATION_ERROR")
                        .with_string_details(format!(
                            "Document '{}' does not exist for tenant '{}'",
                            document_id, target_tenant
                        )),
                ),
            ));
        }

        if let Some(collection_id) = req.collection_id.as_ref() {
            let in_collection = state
                .db
                .is_document_in_collection(collection_id, document_id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("Failed to verify document binding")
                                .with_code("DATABASE_ERROR")
                                .with_string_details(format!("Database error while checking if document '{}' is in collection '{}': {}", document_id, collection_id, e)),
                        ),
                    )
                })?;
            if !in_collection {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("document_id is not in the provided collection")
                            .with_code("VALIDATION_ERROR")
                            .with_string_details(format!(
                                "Document '{}' is not in collection '{}'",
                                document_id, collection_id
                            )),
                    ),
                ));
            }
        }
    }

    let tags_json = if let Some(tags) = req.tags.as_ref() {
        Some(serde_json::to_string(tags).map_err(|e| {
            (
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("Invalid tags payload")
                        .with_code("VALIDATION_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?)
    } else {
        None
    };

    // Generate session ID
    let session_id = format!("session-{}", uuid::Uuid::new_v4());

    // Create session parameters
    let params = CreateChatSessionParams {
        id: session_id.clone(),
        tenant_id: target_tenant.clone(),
        user_id: Some(claims.sub.clone()),
        created_by: Some(claims.sub.clone()),
        stack_id: req.stack_id,
        collection_id: req.collection_id,
        document_id: req.document_id,
        name: req.name,
        title: req.title.clone(),
        source_type: Some(source_type.clone()),
        source_ref_id: req.source_ref_id.clone(),
        metadata_json: req.metadata_json,
        tags_json,
        pinned_adapter_ids: None, // Inherits from tenant default
        codebase_adapter_id: None,
    };

    // Create session in database
    state.db.create_chat_session(params).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to create session")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Retrieve created session
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(format!(
                            "Database error retrieving session '{}': {}",
                            session_id, e
                        )),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Session not found after creation")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(format!(
                            "Session '{}' was created but could not be retrieved",
                            session_id
                        )),
                ),
            )
        })?;

    info!(
        session_id = %session_id,
        tenant_id = %target_tenant,
        user_id = %claims.sub,
        "Chat session created"
    );

    let session_name = session.name.clone();
    let session_id_value = session.id.clone();
    let session_tenant_id = session.tenant_id.clone();
    let session_source_type = session
        .source_type
        .clone()
        .unwrap_or_else(|| "general".to_string());

    Ok((
        StatusCode::CREATED,
        Json(CreateChatSessionResponse {
            session_id: session_id_value,
            tenant_id: session_tenant_id,
            name: session_name.clone(),
            title: session.title.clone().or_else(|| Some(session_name.clone())),
            source_type: session_source_type,
            source_ref_id: session.source_ref_id.clone(),
            stack_id: session.stack_id.clone(),
            collection_id: session.collection_id.clone(),
            document_id: session.document_id.clone(),
            tags_json: session.tags_json.clone(),
            created_at: session.created_at,
        }),
    ))
}

/// Update chat session metadata (title, bindings, metadata).
#[utoipa::path(
    put,
    path = "/v1/chat/sessions/{session_id}",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    request_body = UpdateChatSessionRequest,
    responses(
        (status = 200, description = "Session updated", body = ChatSession),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn update_chat_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<UpdateChatSessionRequest>,
) -> Result<Json<ChatSession>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session exists and tenant matches
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(format!(
                            "Database error retrieving session '{}': {}",
                            session_id, e
                        )),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(
                    ErrorResponse::new("Session not found")
                        .with_code("NOT_FOUND")
                        .with_string_details(format!(
                            "Chat session '{}' does not exist",
                            session_id
                        )),
                ),
            )
        })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    let current_source = session
        .source_type
        .clone()
        .unwrap_or_else(|| "general".to_string());
    let target_source_type = req
        .source_type
        .clone()
        .unwrap_or_else(|| current_source.clone());
    let allowed_sources = [
        "general",
        "document",
        "owner_system",
        "training_job",
        "cli",
        "cli_prompt",
    ];
    if !allowed_sources.contains(&target_source_type.as_str()) {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Invalid source_type").with_code("VALIDATION_ERROR")),
        ));
    }

    let target_document_id = match &req.document_id {
        Some(Some(id)) => Some(id.clone()),
        Some(None) => None,
        None => session.document_id.clone(),
    };
    let target_collection_id = match &req.collection_id {
        Some(Some(id)) => Some(id.clone()),
        Some(None) => None,
        None => session.collection_id.clone(),
    };

    if target_source_type == "document" && target_document_id.is_none() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("document_id is required for document chats")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    }

    if target_document_id.is_some() && target_source_type != "document" {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("document_id is only allowed when source_type='document'")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    }

    if let Some(collection_id) = target_collection_id.as_ref() {
        let collection = state
            .db
            .get_collection(&session.tenant_id, collection_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to validate collection")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
        if collection.is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("collection_id not found for tenant")
                        .with_code("VALIDATION_ERROR"),
                ),
            ));
        }
    }

    if let Some(document_id) = target_document_id.as_ref() {
        let document = state
            .db
            .get_document(&session.tenant_id, document_id)
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to validate document")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
        if document.is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("document_id not found for tenant")
                        .with_code("VALIDATION_ERROR"),
                ),
            ));
        }

        if let Some(collection_id) = target_collection_id.as_ref() {
            let in_collection = state
                .db
                .is_document_in_collection(collection_id, document_id)
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("Failed to verify document binding")
                                .with_code("DATABASE_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;
            if !in_collection {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(
                        ErrorResponse::new("document_id is not in the provided collection")
                            .with_code("VALIDATION_ERROR"),
                    ),
                ));
            }
        }
    }

    state
        .db
        .update_chat_session(
            &session_id,
            &session.tenant_id,
            UpdateChatSessionParams {
                name: req.name,
                title: req.title,
                stack_id: req.stack_id,
                collection_id: req.collection_id,
                document_id: req.document_id,
                source_type: req.source_type,
                metadata_json: req.metadata_json,
                tags_json: req.tags_json,
                codebase_adapter_id: None, // Preserve existing binding
            },
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to update session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let updated = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Session not found after update")
                        .with_code("INTERNAL_ERROR"),
                ),
            )
        })?;

    Ok(Json(updated))
}

/// List chat sessions for the current user/tenant
///
/// GET /v1/chat/sessions
#[utoipa::path(
    get,
    path = "/v1/chat/sessions",
    tag = "chat",
    params(ListSessionsQuery),
    responses(
        (status = 200, description = "Sessions retrieved", body = Vec<ChatSession>),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn list_chat_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListSessionsQuery>,
) -> Result<Json<Vec<ChatSession>>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|_e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    // List sessions - filter by user_id if provided, otherwise show all for tenant
    let user_filter = query.user_id.or(Some(claims.sub.clone()));
    let source_filter = query.source_type;
    let document_filter = query.document_id;
    let sessions = state
        .db
        .list_chat_sessions(
            &claims.tenant_id,
            user_filter.as_deref(),
            source_filter.as_deref(),
            document_filter.as_deref(),
            query.limit,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list sessions")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    debug!(
        tenant_id = %claims.tenant_id,
        count = sessions.len(),
        "Listed chat sessions"
    );

    Ok(Json(sessions))
}

/// Get a specific chat session
///
/// GET /v1/chat/sessions/:session_id
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/{session_id}",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 200, description = "Session retrieved", body = ChatSession),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn get_chat_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<ChatSession>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|_e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Get session
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Session not found").with_code("NOT_FOUND")),
            )
        })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    Ok(Json(session))
}

/// Soft delete a chat session (moves to trash)
///
/// DELETE /v1/chat/sessions/:session_id
#[utoipa::path(
    delete,
    path = "/v1/chat/sessions/{session_id}",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 204, description = "Session moved to trash"),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn delete_chat_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|_e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session exists and tenant has access
    let session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Session not found").with_code("NOT_FOUND")),
            )
        })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    // Soft delete session (moves to trash)
    state
        .db
        .soft_delete_session(&session_id, &claims.sub)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to delete session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    info!(
        session_id = %session_id,
        tenant_id = %claims.tenant_id,
        "Chat session soft deleted"
    );

    Ok(StatusCode::NO_CONTENT)
}
