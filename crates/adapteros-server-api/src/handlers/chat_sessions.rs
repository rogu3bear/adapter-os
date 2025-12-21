//! Chat session API handlers
//!
//! Provides endpoints for managing persistent chat sessions with stack context
//! and trace linkage for the workspace experience.
//!
//! 【2025-01-25†prd-ux-01†chat_sessions_handlers】

use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::ErrorResponse;
use adapteros_api_types::{
    AdapterProvenance, BaseModelInfo, ChatProvenanceResponse, DatasetProvenance, ProvenanceEvent,
    ProvenanceEventType, SessionSummary, StackProvenance, TrainingJobProvenance,
};
use adapteros_db::chat_sessions::UpdateChatSessionParams;
use adapteros_db::contacts::ContactUpsertParams;
use adapteros_db::{
    AddMessageParams, ChatCategory, ChatMessage, ChatSearchResult, ChatSession,
    ChatSessionWithStatus, ChatTag, Contact, CreateChatSessionParams, InferenceEvidence,
    SessionShare,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};
use utoipa::{IntoParams, ToSchema};

/// Request to create a new chat session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateChatSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tenant_id: Option<String>,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
}

/// Response for chat session creation
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateChatSessionResponse {
    pub session_id: String,
    pub tenant_id: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub source_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ref_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags_json: Option<String>,
    pub created_at: String,
}

/// Request to update an existing chat session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateChatSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub stack_id: Option<Option<String>>,
    #[serde(default)]
    pub collection_id: Option<Option<String>>,
    #[serde(default)]
    pub document_id: Option<Option<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(default)]
    pub metadata_json: Option<Option<String>>,
    #[serde(default)]
    pub tags_json: Option<Option<String>>,
}

/// Request to add a message to a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddChatMessageRequest {
    pub role: String, // 'user', 'assistant', 'system', 'tool', 'owner_system'
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

/// API response wrapper for ChatMessage
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChatMessageResponse {
    pub id: String,
    pub session_id: String,
    pub tenant_id: String,
    pub role: String,
    pub content: String,
    pub timestamp: String,
    pub created_at: String,
    pub sequence: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata_json: Option<String>,
}

impl From<ChatMessage> for ChatMessageResponse {
    fn from(msg: ChatMessage) -> Self {
        let timestamp = msg
            .timestamp
            .clone()
            .unwrap_or_else(|| msg.created_at.clone());
        Self {
            id: msg.id,
            session_id: msg.session_id,
            tenant_id: msg.tenant_id,
            role: msg.role,
            content: msg.content,
            timestamp,
            created_at: msg.created_at,
            sequence: msg.sequence,
            metadata_json: msg.metadata_json,
        }
    }
}

/// Query parameters for listing sessions
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct ListSessionsQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_id: Option<String>,
}

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
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
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
            Json(ErrorResponse::new("Session name cannot be empty")
                .with_code("VALIDATION_ERROR")
                .with_string_details("Provide a non-empty name for the chat session")),
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
            Json(ErrorResponse::new("Invalid source_type")
                .with_code("VALIDATION_ERROR")
                .with_string_details(format!("source_type '{}' is not valid. Allowed values: {:?}", source_type, allowed_sources))),
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
                            .with_string_details(format!("Database error while validating collection '{}': {}", collection_id, e)),
                    ),
                )
            })?;
        if collection.is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("collection_id not found for tenant")
                        .with_code("VALIDATION_ERROR")
                        .with_string_details(format!("Collection '{}' does not exist for tenant '{}'", collection_id, target_tenant)),
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
                            .with_string_details(format!("Database error while validating document '{}': {}", document_id, e)),
                    ),
                )
            })?;
        if document.is_none() {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(
                    ErrorResponse::new("document_id not found for tenant")
                        .with_code("VALIDATION_ERROR")
                        .with_string_details(format!("Document '{}' does not exist for tenant '{}'", document_id, target_tenant)),
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
                            .with_string_details(format!("Document '{}' is not in collection '{}'", document_id, collection_id)),
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
                        .with_string_details(format!("Database error retrieving session '{}': {}", session_id, e)),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Session not found after creation")
                        .with_code("INTERNAL_ERROR")
                        .with_string_details(format!("Session '{}' was created but could not be retrieved", session_id)),
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
                        .with_string_details(format!("Database error retrieving session '{}': {}", session_id, e)),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Session not found")
                    .with_code("NOT_FOUND")
                    .with_string_details(format!("Chat session '{}' does not exist", session_id))),
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
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
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
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
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

/// Add a message to a chat session
///
/// POST /v1/chat/sessions/:session_id/messages
#[utoipa::path(
    post,
    path = "/v1/chat/sessions/{session_id}/messages",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    request_body = AddChatMessageRequest,
    responses(
        (status = 201, description = "Message added", body = ChatMessageResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn add_chat_message(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<AddChatMessageRequest>,
) -> Result<(StatusCode, Json<ChatMessageResponse>), (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
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

    // Generate message ID
    let message_id = format!("msg-{}", uuid::Uuid::new_v4());

    // Add message
    let params = AddMessageParams {
        id: message_id.clone(),
        session_id: session_id.clone(),
        tenant_id: Some(session.tenant_id.clone()),
        role: req.role,
        content: req.content,
        sequence: None,
        created_at: None,
        metadata_json: req.metadata_json,
    };

    state.db.add_chat_message(params).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to add message")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    // Retrieve added message
    let messages = state
        .db
        .get_chat_messages(&session_id, Some(1))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve message")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let message = messages.into_iter().last().ok_or_else(|| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Message not found after creation").with_code("INTERNAL_ERROR"),
            ),
        )
    })?;

    Ok((StatusCode::CREATED, Json(message.into())))
}

/// Get messages for a chat session
///
/// GET /v1/chat/sessions/:session_id/messages
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/{session_id}/messages",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID"),
        ("limit" = Option<i64>, Query, description = "Maximum messages to return")
    ),
    responses(
        (status = 200, description = "Messages retrieved", body = Vec<ChatMessageResponse>),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn get_chat_messages(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Query(query): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Vec<ChatMessageResponse>>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
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

    // Get limit from query
    let limit = query.get("limit").and_then(|s| s.parse::<i64>().ok());

    // Get messages
    let messages = state
        .db
        .get_chat_messages(&session_id, limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get messages")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Convert to API response type
    let response: Vec<ChatMessageResponse> = messages.into_iter().map(|m| m.into()).collect();

    Ok(Json(response))
}

/// Get session summary with trace counts
///
/// GET /v1/chat/sessions/:session_id/summary
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/{session_id}/summary",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 200, description = "Session summary", body = serde_json::Value),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn get_session_summary(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
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

    // Get summary
    let summary = state
        .db
        .get_session_summary(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get session summary")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(summary))
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
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
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

/// Get evidence for a chat message
///
/// GET /v1/chat/messages/:message_id/evidence
#[utoipa::path(
    get,
    path = "/v1/chat/messages/{message_id}/evidence",
    tag = "chat",
    params(
        ("message_id" = String, Path, description = "Message ID")
    ),
    responses(
        (status = 200, description = "Evidence retrieved", body = Vec<InferenceEvidence>),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn get_message_evidence(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(message_id): Path<String>,
) -> Result<Json<Vec<InferenceEvidence>>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Get evidence from database
    let evidence = state
        .db
        .get_evidence_by_message(&message_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get message evidence")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    debug!(
        message_id = %message_id,
        evidence_count = evidence.len(),
        "Retrieved message evidence"
    );

    Ok(Json(evidence))
}

/// Get provenance chain for a chat session
///
/// Returns the complete lineage: chat -> stack -> adapters -> training jobs -> datasets -> base model
///
/// GET /v1/chat/sessions/:session_id/provenance
#[utoipa::path(
    get,
    path = "/v1/chat/sessions/{session_id}/provenance",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    responses(
        (status = 200, description = "Provenance retrieved", body = ChatProvenanceResponse),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn get_chat_provenance(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<ChatProvenanceResponse>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
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

    // Verify tenant access
    // Tenant isolation check
    validate_tenant_isolation(&claims, &session.tenant_id)?;

    // Count messages for the session
    let messages = state
        .db
        .get_chat_messages(&session_id, None)
        .await
        .unwrap_or_default();
    let message_count = messages.len() as i64;

    // Load captured provenance entries if any
    let provenance_entries = state
        .db
        .list_chat_provenance_for_session(&session_id)
        .await
        .unwrap_or_default();
    let entries = if provenance_entries.is_empty() {
        None
    } else {
        Some(
            provenance_entries
                .into_iter()
                .map(|p| adapteros_api_types::ChatProvenanceEntry {
                    message_id: p.message_id,
                    inference_call_id: p.inference_call_id,
                    payload_snapshot: serde_json::from_str(&p.payload_snapshot)
                        .unwrap_or(serde_json::Value::String(p.payload_snapshot)),
                    created_at: p.created_at,
                })
                .collect(),
        )
    };

    // Build session summary
    let session_summary = SessionSummary {
        id: session.id.clone(),
        name: session.name.clone(),
        tenant_id: session.tenant_id.clone(),
        stack_id: session.stack_id.clone(),
        collection_id: session.collection_id.clone(),
        created_at: session.created_at.clone(),
        last_activity_at: session.last_activity_at.clone(),
        message_count,
    };

    // Load stack and adapters if session has a stack
    let mut stack_provenance = None;
    let mut adapter_provenances = Vec::new();
    let mut base_model_info = None;
    let mut timeline_events = Vec::new();

    if let Some(ref stack_id) = session.stack_id {
        // Get stack
        if let Ok(Some(stack)) = state.db.get_stack(&session.tenant_id, stack_id).await {
            let adapter_ids: Vec<String> =
                serde_json::from_str(&stack.adapter_ids_json).unwrap_or_default();

            stack_provenance = Some(StackProvenance {
                id: stack.id.clone(),
                name: stack.name.clone(),
                description: stack.description.clone(),
                workflow_type: stack.workflow_type.clone(),
                adapter_ids: adapter_ids.clone(),
                created_at: stack.created_at.clone(),
                created_by: stack.created_by.clone(),
            });

            // Add stack creation event
            timeline_events.push(ProvenanceEvent {
                event_type: ProvenanceEventType::StackCreated,
                entity_id: stack.id.clone(),
                entity_name: stack.name.clone(),
                timestamp: stack.created_at.clone(),
                description: format!("Stack '{}' created", stack.name),
            });

            // Load each adapter with extended provenance data
            for adapter_id in &adapter_ids {
                // Query adapter with training_job_id and base_model_id (direct SQL for fields not in struct)
                // SECURITY: Include tenant_id filter to prevent cross-tenant data leakage
                let adapter_row: Option<AdapterProvenanceRow> = sqlx::query_as(
                    "SELECT id, name, hash_b3, tier, training_job_id, base_model_id, created_at
                     FROM adapters WHERE id = ? AND tenant_id = ?",
                )
                .bind(adapter_id)
                .bind(&session.tenant_id)
                .fetch_optional(state.db.pool())
                .await
                .ok()
                .flatten();

                if let Some(adapter) = adapter_row {
                    let mut training_job_prov = None;

                    // Load training job if linked
                    if let Some(ref job_id) = adapter.training_job_id {
                        if let Ok(Some(job)) = state.db.get_training_job(job_id).await {
                            // SECURITY: Validate training job belongs to session tenant
                            // Skip if tenant mismatch to prevent cross-tenant data leakage
                            if job.tenant_id.as_ref() != Some(&session.tenant_id) {
                                continue;
                            }

                            let mut dataset_prov = None;

                            // Load dataset if linked
                            if let Some(ref dataset_id) = job.dataset_id {
                                if let Ok(Some(dataset)) =
                                    state.db.get_training_dataset(dataset_id).await
                                {
                                    // SECURITY: Validate dataset belongs to session tenant
                                    // Skip dataset provenance if tenant mismatch to prevent cross-tenant data leakage
                                    if dataset.tenant_id.as_ref() == Some(&session.tenant_id) {
                                        dataset_prov = Some(DatasetProvenance {
                                            id: dataset.id.clone(),
                                            name: dataset.name.clone(),
                                            description: dataset.description.clone(),
                                            format: dataset.format.clone(),
                                            file_count: dataset.file_count,
                                            total_size_bytes: dataset.total_size_bytes,
                                            hash_b3: dataset.hash_b3.clone(),
                                            validation_status: dataset.validation_status.clone(),
                                            created_at: dataset.created_at.clone(),
                                            created_by: dataset.created_by.clone(),
                                        });

                                        // Add dataset event
                                        timeline_events.push(ProvenanceEvent {
                                            event_type: ProvenanceEventType::DatasetCreated,
                                            entity_id: dataset.id.clone(),
                                            entity_name: dataset.name.clone(),
                                            timestamp: dataset.created_at.clone(),
                                            description: format!("Dataset '{}' created", dataset.name),
                                        });
                                    }
                                }
                            }

                            training_job_prov = Some(TrainingJobProvenance {
                                id: job.id.clone(),
                                status: job.status.clone(),
                                started_at: job.started_at.clone(),
                                completed_at: job.completed_at.clone(),
                                created_by: job.created_by.clone(),
                                dataset: dataset_prov,
                                base_model_id: job.base_model_id.clone(),
                                config_hash_b3: job.config_hash_b3.clone(),
                            });

                            // Add training job events
                            timeline_events.push(ProvenanceEvent {
                                event_type: ProvenanceEventType::TrainingJobStarted,
                                entity_id: job.id.clone(),
                                entity_name: job
                                    .adapter_name
                                    .clone()
                                    .unwrap_or_else(|| "training".to_string()),
                                timestamp: job.started_at.clone(),
                                description: "Training job started".to_string(),
                            });

                            if let Some(ref completed_at) = job.completed_at {
                                timeline_events.push(ProvenanceEvent {
                                    event_type: ProvenanceEventType::TrainingJobCompleted,
                                    entity_id: job.id.clone(),
                                    entity_name: job
                                        .adapter_name
                                        .clone()
                                        .unwrap_or_else(|| "training".to_string()),
                                    timestamp: completed_at.clone(),
                                    description: format!(
                                        "Training job completed with status: {}",
                                        job.status
                                    ),
                                });
                            }

                            // Use base_model_id from training job for overall base model
                            if base_model_info.is_none() {
                                if let Some(ref model_id) = job.base_model_id {
                                    // Query model info
                                    let model_row: Option<BaseModelRow> = sqlx::query_as(
                                        "SELECT id, name, hash_b3, created_at FROM models WHERE id = ?",
                                    )
                                    .bind(model_id)
                                    .fetch_optional(state.db.pool())
                                    .await
                                    .ok()
                                    .flatten();

                                    if let Some(model) = model_row {
                                        base_model_info = Some(BaseModelInfo {
                                            id: model.id,
                                            name: model.name,
                                            hash_b3: model.hash_b3,
                                            created_at: model.created_at,
                                        });
                                    }
                                }
                            }
                        }
                    }

                    adapter_provenances.push(AdapterProvenance {
                        id: adapter.id.clone(),
                        name: adapter.name.clone(),
                        hash_b3: adapter.hash_b3.clone(),
                        tier: adapter.tier.clone(),
                        externally_created: adapter.training_job_id.is_none(),
                        training_job: training_job_prov,
                        created_at: adapter.created_at.clone(),
                    });

                    // Add adapter registration event
                    timeline_events.push(ProvenanceEvent {
                        event_type: ProvenanceEventType::AdapterRegistered,
                        entity_id: adapter.id.clone(),
                        entity_name: adapter.name.clone(),
                        timestamp: adapter.created_at.clone(),
                        description: format!("Adapter '{}' registered", adapter.name),
                    });
                }
            }
        }
    }

    // Add chat started event
    timeline_events.push(ProvenanceEvent {
        event_type: ProvenanceEventType::ChatStarted,
        entity_id: session.id.clone(),
        entity_name: session.name.clone(),
        timestamp: session.created_at.clone(),
        description: format!("Chat session '{}' started", session.name),
    });

    // Sort timeline by timestamp
    timeline_events.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

    // Compute provenance hash (BLAKE3 of the provenance data for audit trail)
    let provenance_data = serde_json::json!({
        "session_id": session.id,
        "stack_id": session.stack_id,
        "adapter_ids": adapter_provenances.iter().map(|a| &a.id).collect::<Vec<_>>(),
        "message_count": message_count,
    });
    let provenance_hash = blake3::hash(provenance_data.to_string().as_bytes())
        .to_hex()
        .to_string();

    let computed_at = chrono::Utc::now().to_rfc3339();

    let response = ChatProvenanceResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        session: session_summary,
        stack: stack_provenance,
        adapters: adapter_provenances,
        base_model: base_model_info,
        timeline: Some(timeline_events),
        entries,
        provenance_hash,
        computed_at,
    };

    debug!(
        session_id = %session_id,
        adapters_count = response.adapters.len(),
        has_stack = response.stack.is_some(),
        "Retrieved chat provenance"
    );

    Ok(Json(response))
}

/// Helper struct for querying adapter provenance data
#[derive(Debug, sqlx::FromRow)]
#[allow(dead_code)] // base_model_id is queried for completeness but derived from training_job
struct AdapterProvenanceRow {
    id: String,
    name: String,
    hash_b3: String,
    tier: String,
    training_job_id: Option<String>,
    base_model_id: Option<String>,
    created_at: String,
}

/// Helper struct for querying base model data
#[derive(Debug, sqlx::FromRow)]
struct BaseModelRow {
    id: String,
    name: String,
    hash_b3: String,
    created_at: String,
}

/// Request to update collection binding for a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateCollectionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collection_id: Option<String>,
}

/// Update the collection binding for a chat session
///
/// PUT /v1/chat/sessions/:session_id/collection
#[utoipa::path(
    put,
    path = "/v1/chat/sessions/{session_id}/collection",
    tag = "chat",
    params(
        ("session_id" = String, Path, description = "Session ID")
    ),
    request_body = UpdateCollectionRequest,
    responses(
        (status = 200, description = "Collection updated", body = ChatSession),
        (status = 404, description = "Session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    )
)]
pub async fn update_session_collection(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(request): Json<UpdateCollectionRequest>,
) -> Result<Json<ChatSession>, (StatusCode, Json<ErrorResponse>)> {
    // Permission check
    require_permission(&claims, Permission::InferenceExecute).map_err(|e| {
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

    // Update collection binding
    state
        .db
        .update_session_collection(&session_id, request.collection_id.clone())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to update collection")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    // Retrieve updated session
    let updated_session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to retrieve updated session")
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

    info!(
        session_id = %session_id,
        collection_id = ?request.collection_id,
        tenant_id = %claims.tenant_id,
        "Chat session collection updated"
    );

    Ok(Json(updated_session))
}

// =============================================================================
// Tags API
// =============================================================================

/// Request to create a new tag
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateTagRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Request to update a tag
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateTagRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Request to assign tags to a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AssignTagsRequest {
    pub tag_ids: Vec<String>,
}

/// List all tags for the tenant
///
/// GET /v1/chat/tags
pub async fn list_chat_tags(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<ChatTag>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let tags = state
        .db
        .list_chat_tags(&claims.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list tags")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(tags))
}

/// Create a new tag
///
/// POST /v1/chat/tags
pub async fn create_chat_tag(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateTagRequest>,
) -> Result<(StatusCode, Json<ChatTag>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    if req.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Tag name cannot be empty").with_code("VALIDATION_ERROR")),
        ));
    }

    let tag = state
        .db
        .create_chat_tag(
            &claims.tenant_id,
            &req.name,
            req.color.as_deref(),
            req.description.as_deref(),
            Some(&claims.sub),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to create tag")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok((StatusCode::CREATED, Json(tag)))
}

/// Update a tag
///
/// PUT /v1/chat/tags/:tag_id
pub async fn update_chat_tag(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tag_id): Path<String>,
    Json(req): Json<UpdateTagRequest>,
) -> Result<Json<ChatTag>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Verify tag belongs to tenant
    let tag = state
        .db
        .get_chat_tag(&tag_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get tag")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Tag not found").with_code("NOT_FOUND")),
            )
        })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &tag.tenant_id)?;

    state
        .db
        .update_chat_tag(
            &tag_id,
            req.name.as_deref(),
            req.color.as_deref(),
            req.description.as_deref(),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to update tag")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let updated_tag = state
        .db
        .get_chat_tag(&tag_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get tag")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .unwrap();

    Ok(Json(updated_tag))
}

/// Delete a tag
///
/// DELETE /v1/chat/tags/:tag_id
pub async fn delete_chat_tag(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(tag_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Verify tag belongs to tenant
    let tag = state
        .db
        .get_chat_tag(&tag_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get tag")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Tag not found").with_code("NOT_FOUND")),
            )
        })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &tag.tenant_id)?;

    state.db.delete_chat_tag(&tag_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to delete tag")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Assign tags to a session
///
/// POST /v1/chat/sessions/:session_id/tags
pub async fn assign_tags_to_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<AssignTagsRequest>,
) -> Result<Json<Vec<ChatTag>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session belongs to tenant
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

    state
        .db
        .assign_tags_to_session(&session_id, &req.tag_ids, Some(&claims.sub))
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to assign tags")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let tags = state.db.get_session_tags(&session_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to get tags")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(tags))
}

/// Get tags for a session
///
/// GET /v1/chat/sessions/:session_id/tags
pub async fn get_session_tags(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<ChatTag>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session belongs to tenant
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

    let tags = state.db.get_session_tags(&session_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to get tags")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    Ok(Json(tags))
}

/// Remove a tag from a session
///
/// DELETE /v1/chat/sessions/:session_id/tags/:tag_id
pub async fn remove_tag_from_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((session_id, tag_id)): Path<(String, String)>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session belongs to tenant
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

    state
        .db
        .remove_tag_from_session(&session_id, &tag_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to remove tag")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// Categories API
// =============================================================================

/// Request to create a category
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateCategoryRequest {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Request to update a category
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct UpdateCategoryRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
}

/// Request to set session category
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct SetCategoryRequest {
    pub category_id: Option<String>,
}

/// List all categories for the tenant
///
/// GET /v1/chat/categories
pub async fn list_chat_categories(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<ChatCategory>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let categories = state
        .db
        .list_chat_categories(&claims.tenant_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list categories")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(categories))
}

/// Create a new category
///
/// POST /v1/chat/categories
pub async fn create_chat_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateCategoryRequest>,
) -> Result<(StatusCode, Json<ChatCategory>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    if req.name.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(ErrorResponse::new("Category name cannot be empty").with_code("VALIDATION_ERROR")),
        ));
    }

    let category = state
        .db
        .create_chat_category(
            &claims.tenant_id,
            &req.name,
            req.parent_id.as_deref(),
            req.icon.as_deref(),
            req.color.as_deref(),
        )
        .await
        .map_err(|e| {
            let status = if e.to_string().contains("depth cannot exceed") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(
                    ErrorResponse::new("Failed to create category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok((StatusCode::CREATED, Json(category)))
}

/// Update a category
///
/// PUT /v1/chat/categories/:category_id
pub async fn update_chat_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(category_id): Path<String>,
    Json(req): Json<UpdateCategoryRequest>,
) -> Result<Json<ChatCategory>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Verify category belongs to tenant
    let category = state
        .db
        .get_chat_category(&category_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Category not found").with_code("NOT_FOUND")),
            )
        })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &category.tenant_id)?;

    state
        .db
        .update_chat_category(
            &category_id,
            req.name.as_deref(),
            req.icon.as_deref(),
            req.color.as_deref(),
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to update category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    let updated = state
        .db
        .get_chat_category(&category_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .unwrap();

    Ok(Json(updated))
}

/// Delete a category
///
/// DELETE /v1/chat/categories/:category_id
pub async fn delete_chat_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(category_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Verify category belongs to tenant
    let category = state
        .db
        .get_chat_category(&category_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Category not found").with_code("NOT_FOUND")),
            )
        })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &category.tenant_id)?;

    state
        .db
        .delete_chat_category(&category_id)
        .await
        .map_err(|e| {
            let status = if e.to_string().contains("Cannot delete category") {
                StatusCode::BAD_REQUEST
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (
                status,
                Json(
                    ErrorResponse::new("Failed to delete category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Set the category for a session
///
/// PUT /v1/chat/sessions/:session_id/category
pub async fn set_session_category(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<SetCategoryRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session belongs to tenant
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

    state
        .db
        .set_session_category(&session_id, req.category_id.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to set category")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

// =============================================================================
// Archive / Restore API
// =============================================================================

/// Request to archive a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ArchiveSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Archive a session
///
/// POST /v1/chat/sessions/:session_id/archive
pub async fn archive_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<ArchiveSessionRequest>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session belongs to tenant
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

    state
        .db
        .archive_session(&session_id, &claims.sub, req.reason.as_deref())
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to archive session")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Restore a deleted or archived session (admin-only)
///
/// POST /v1/chat/sessions/:session_id/restore
pub async fn restore_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Admin-only: requires WorkspaceManage
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - restore requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Verify session belongs to tenant
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

    state.db.restore_session(&session_id).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(
                ErrorResponse::new("Failed to restore session")
                    .with_code("DATABASE_ERROR")
                    .with_string_details(e.to_string()),
            ),
        )
    })?;

    info!(session_id = %session_id, user = %claims.sub, "Session restored");
    Ok(StatusCode::NO_CONTENT)
}

/// Permanently delete a session
///
/// DELETE /v1/chat/sessions/:session_id/permanent
pub async fn hard_delete_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    // Admin-only
    require_permission(&claims, Permission::WorkspaceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Verify session belongs to tenant
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

    state
        .db
        .hard_delete_session(&session_id)
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

    info!(session_id = %session_id, user = %claims.sub, "Session permanently deleted");
    Ok(StatusCode::NO_CONTENT)
}

/// Query parameters for listing archived sessions
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct ListArchivedQuery {
    pub limit: Option<i64>,
}

/// List archived sessions
///
/// GET /v1/chat/sessions/archived
pub async fn list_archived_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListArchivedQuery>,
) -> Result<Json<Vec<ChatSessionWithStatus>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let sessions = state
        .db
        .list_archived_sessions(&claims.tenant_id, Some(&claims.sub), query.limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list archived sessions")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(sessions))
}

/// List deleted sessions (trash)
///
/// GET /v1/chat/sessions/trash
pub async fn list_deleted_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListArchivedQuery>,
) -> Result<Json<Vec<ChatSessionWithStatus>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let sessions = state
        .db
        .list_deleted_sessions(&claims.tenant_id, Some(&claims.sub), query.limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to list deleted sessions")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(sessions))
}

// =============================================================================
// Search API
// =============================================================================

/// Query parameters for session search
#[derive(Debug, Clone, Deserialize, ToSchema, IntoParams)]
pub struct SearchSessionsQuery {
    pub q: String,
    #[serde(default = "default_scope")]
    pub scope: String,
    pub category_id: Option<String>,
    pub tags: Option<String>,
    #[serde(default)]
    pub include_archived: bool,
    #[serde(default = "default_limit")]
    pub limit: i64,
}

fn default_scope() -> String {
    "all".to_string()
}
fn default_limit() -> i64 {
    20
}

/// Search chat sessions and messages
///
/// GET /v1/chat/sessions/search
pub async fn search_chat_sessions(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<SearchSessionsQuery>,
) -> Result<Json<Vec<ChatSearchResult>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    if query.q.len() < 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            Json(
                ErrorResponse::new("Search query must be at least 2 characters")
                    .with_code("VALIDATION_ERROR"),
            ),
        ));
    }

    let tag_ids: Option<Vec<String>> = query
        .tags
        .map(|t| t.split(',').map(|s| s.trim().to_string()).collect());

    let results = state
        .db
        .search_chat_sessions(
            &claims.tenant_id,
            &query.q,
            &query.scope,
            query.category_id.as_deref(),
            tag_ids.as_deref(),
            query.include_archived,
            query.limit,
        )
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Search failed")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(results))
}

// =============================================================================
// Sharing API
// =============================================================================

/// Request to share a session
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ShareSessionRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    pub permission: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<String>,
}

/// Share a session
///
/// POST /v1/chat/sessions/:session_id/shares
pub async fn share_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<ShareSessionRequest>,
) -> Result<(StatusCode, Json<serde_json::Value>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceResourceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceResourceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Verify session belongs to tenant
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

    let mut share_ids = Vec::new();

    // Share with workspace
    if let Some(workspace_id) = &req.workspace_id {
        let id = state
            .db
            .share_session_with_workspace(
                &session_id,
                workspace_id,
                &req.permission,
                &claims.sub,
                req.expires_at.as_deref(),
            )
            .await
            .map_err(|e| {
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("Failed to share with workspace")
                            .with_code("DATABASE_ERROR")
                            .with_string_details(e.to_string()),
                    ),
                )
            })?;
        share_ids.push(serde_json::json!({"type": "workspace", "id": id}));
    }

    // Share with users
    if let Some(user_ids) = &req.user_ids {
        for user_id in user_ids {
            let id = state
                .db
                .share_session_with_user(
                    &session_id,
                    user_id,
                    &claims.tenant_id,
                    &req.permission,
                    &claims.sub,
                    req.expires_at.as_deref(),
                )
                .await
                .map_err(|e| {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        Json(
                            ErrorResponse::new("Failed to share with user")
                                .with_code("DATABASE_ERROR")
                                .with_string_details(e.to_string()),
                        ),
                    )
                })?;
            share_ids.push(serde_json::json!({"type": "user", "id": id, "user_id": user_id}));
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(serde_json::json!({"shares": share_ids})),
    ))
}

/// Get shares for a session
///
/// GET /v1/chat/sessions/:session_id/shares
pub async fn get_session_shares(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
) -> Result<Json<Vec<SessionShare>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Verify session belongs to tenant
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

    let shares = state
        .db
        .get_session_shares(&session_id)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get shares")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(shares))
}

/// Revoke a session share
///
/// DELETE /v1/chat/sessions/:session_id/shares/:share_id
pub async fn revoke_session_share(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((session_id, share_id)): Path<(String, String)>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<StatusCode, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::WorkspaceResourceManage).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(
                ErrorResponse::new("Permission denied - requires WorkspaceResourceManage")
                    .with_code("FORBIDDEN"),
            ),
        )
    })?;

    // Verify session belongs to tenant
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

    let share_type = params.get("type").map(|s| s.as_str()).unwrap_or("user");

    state
        .db
        .revoke_session_share(&share_id, share_type)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to revoke share")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(StatusCode::NO_CONTENT)
}

/// Get sessions shared with the current user
///
/// GET /v1/chat/sessions/shared-with-me
pub async fn get_sessions_shared_with_me(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(query): Query<ListArchivedQuery>,
) -> Result<Json<Vec<ChatSessionWithStatus>>, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("FORBIDDEN")),
        )
    })?;

    // Tenant isolation check
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let sessions = state
        .db
        .get_sessions_shared_with_user(&claims.sub, &claims.tenant_id, query.limit)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    ErrorResponse::new("Failed to get shared sessions")
                        .with_code("DATABASE_ERROR")
                        .with_string_details(e.to_string()),
                ),
            )
        })?;

    Ok(Json(sessions))
}

// Contact handlers

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
        Err(e) => super::utils::aos_error_to_response(e).into_response(),
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
    Json(payload): Json<adapteros_db::contacts::ContactUpsertParams>,
) -> impl IntoResponse {
    // Tenant isolation check
    if let Err(e) = validate_tenant_isolation(&claims, &payload.tenant_id) {
        return e.into_response();
    }

    match state.db.upsert_contact(payload).await {
        Ok(id) => Json(id).into_response(),
        Err(e) => super::utils::aos_error_to_response(e).into_response(),
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
    match state.db.get_contact(&id).await {
        Ok(Some(contact)) => {
            // Verify tenant ownership
            // Tenant isolation check
            if let Err(e) = validate_tenant_isolation(&claims, &contact.tenant_id) {
                return e.into_response();
            }
            Json(contact).into_response()
        }
        Ok(None) => super::utils::aos_error_to_response(adapteros_core::AosError::NotFound(
            "Contact not found".into(),
        ))
        .into_response(),
        Err(e) => super::utils::aos_error_to_response(e).into_response(),
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
    // Check existence and ownership first
    match state.db.get_contact(&id).await {
        Ok(Some(contact)) => {
            if contact.tenant_id != claims.tenant_id {
                return super::utils::aos_error_to_response(adapteros_core::AosError::NotFound(
                    "Contact not found".into(),
                ))
                .into_response();
            }
        }
        Ok(None) => {
            return super::utils::aos_error_to_response(adapteros_core::AosError::NotFound(
                "Contact not found".into(),
            ))
            .into_response()
        }
        Err(e) => return super::utils::aos_error_to_response(e).into_response(),
    }

    match state.db.delete_contact(&id).await {
        Ok(_) => StatusCode::OK.into_response(),
        Err(e) => super::utils::aos_error_to_response(e).into_response(),
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
    let limit = params.limit as i64;

    // Verify ownership
    match state.db.get_contact(&id).await {
        Ok(Some(contact)) => {
            if contact.tenant_id != claims.tenant_id {
                return super::utils::aos_error_to_response(adapteros_core::AosError::NotFound(
                    "Contact not found".into(),
                ))
                .into_response();
            }
        }
        Ok(None) => {
            return super::utils::aos_error_to_response(adapteros_core::AosError::NotFound(
                "Contact not found".into(),
            ))
            .into_response()
        }
        Err(e) => return super::utils::aos_error_to_response(e).into_response(),
    }

    match state.db.get_contact_interactions(&id, limit).await {
        Ok(interactions) => Json(interactions).into_response(),
        Err(e) => super::utils::aos_error_to_response(e).into_response(),
    }
}

// ============================================================================
// Chat Session Fork
// ============================================================================

/// Request to fork a chat session
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ForkChatSessionRequest {
    /// Optional name for the forked session (defaults to "{original_name} (forked)")
    #[serde(default)]
    pub name: Option<String>,

    /// Whether to copy messages from the source session (default: true)
    #[serde(default = "default_include_messages")]
    pub include_messages: bool,
}

fn default_include_messages() -> bool {
    true
}

/// Response from forking a chat session
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ForkChatSessionResponse {
    pub session_id: String,
    pub name: String,
    pub created_at: String,
    pub forked_from: ForkedFromInfo,
}

/// Information about the source session
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct ForkedFromInfo {
    pub session_id: String,
    pub name: String,
}

/// Fork an existing chat session
///
/// Creates a copy of a chat session with a new ID. Optionally copies
/// all messages from the source session.
#[utoipa::path(
    post,
    path = "/v1/chat/sessions/{session_id}/fork",
    request_body = ForkChatSessionRequest,
    params(
        ("session_id" = String, Path, description = "Session ID to fork")
    ),
    responses(
        (status = 201, description = "Session forked successfully", body = ForkChatSessionResponse),
        (status = 404, description = "Source session not found", body = ErrorResponse),
        (status = 403, description = "Forbidden", body = ErrorResponse)
    ),
    tag = "chat"
)]
pub async fn fork_chat_session(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(session_id): Path<String>,
    Json(req): Json<ForkChatSessionRequest>,
) -> Result<(StatusCode, Json<ForkChatSessionResponse>), (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::InferenceExecute).map_err(|_| {
        (
            StatusCode::FORBIDDEN,
            Json(ErrorResponse::new("Permission denied").with_code("PERMISSION_DENIED")),
        )
    })?;

    // First get the source session name for the response
    let source_session = state
        .db
        .get_chat_session(&session_id)
        .await
        .map_err(|e| {
            tracing::error!(error = %e, session_id = %session_id, "Failed to get source session");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorResponse::new("Database error").with_code("DATABASE_ERROR")),
            )
        })?
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ErrorResponse::new("Session not found").with_code("NOT_FOUND")),
            )
        })?;

    // Validate tenant isolation
    if source_session.tenant_id != claims.tenant_id {
        return Err((
            StatusCode::NOT_FOUND,
            Json(ErrorResponse::new("Session not found").with_code("NOT_FOUND")),
        ));
    }

    let source_name = source_session.name.clone();

    // Fork the session
    let new_session = state
        .db
        .fork_session(
            &claims.tenant_id,
            &session_id,
            req.name.as_deref(),
            req.include_messages,
        )
        .await
        .map_err(|e| {
            let error_str = e.to_string();
            if error_str.contains("not found") || error_str.contains("NotFound") {
                (
                    StatusCode::NOT_FOUND,
                    Json(ErrorResponse::new("Session not found").with_code("NOT_FOUND")),
                )
            } else {
                tracing::error!(error = %e, session_id = %session_id, "Failed to fork session");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new(format!("Failed to fork session: {}", e))
                            .with_code("DATABASE_ERROR"),
                    ),
                )
            }
        })?;

    info!(
        source_session_id = %session_id,
        new_session_id = %new_session.id,
        tenant_id = %claims.tenant_id,
        include_messages = req.include_messages,
        "Forked chat session"
    );

    Ok((
        StatusCode::CREATED,
        Json(ForkChatSessionResponse {
            session_id: new_session.id,
            name: new_session.name,
            created_at: new_session.created_at,
            forked_from: ForkedFromInfo {
                session_id,
                name: source_name,
            },
        }),
    ))
}
