//! Collection management handlers
//!
//! Provides REST endpoints for managing document collections.
//! Collections group related documents together for organizational purposes.

use crate::auth::Claims;
use crate::audit_helper::{actions, log_success, resources};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use adapteros_db::collections::CreateCollectionParams;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    Extension,
};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use utoipa::ToSchema;
use uuid::Uuid;

/// Collection response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CollectionResponse {
    pub schema_version: String,
    pub collection_id: String,
    pub name: String,
    pub description: Option<String>,
    pub document_count: i32,
    pub tenant_id: String,
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// Collection detail response (includes documents)
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CollectionDetailResponse {
    pub schema_version: String,
    pub collection_id: String,
    pub name: String,
    pub description: Option<String>,
    pub document_count: i32,
    pub tenant_id: String,
    pub documents: Vec<CollectionDocumentInfo>,
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// Document info within a collection
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CollectionDocumentInfo {
    pub document_id: String,
    pub name: String,
    pub size_bytes: i64,
    pub status: String,
    pub added_at: String,
}

/// Create collection request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct CreateCollectionRequest {
    pub name: String,
    pub description: Option<String>,
}

/// Add document to collection request
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct AddDocumentRequest {
    pub document_id: String,
}

/// Create a new collection
#[utoipa::path(
    post,
    path = "/v1/collections",
    request_body = CreateCollectionRequest,
    responses(
        (status = 200, description = "Collection created successfully", body = CollectionResponse),
        (status = 400, description = "Invalid request"),
        (status = 500, description = "Internal server error")
    ),
    tag = "collections"
)]
pub async fn create_collection(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(req): Json<CreateCollectionRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload)
        .map_err(|e| (e.0, e.1.error.clone()))?;

    // Create database record
    let collection_id = state
        .db
        .create_collection(CreateCollectionParams {
            tenant_id: claims.tenant_id.clone(),
            name: req.name.clone(),
            description: req.description.clone(),
            metadata_json: None,
        })
        .await
        .map_err(|e| {
            error!("Failed to create collection record: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create collection record: {}", e),
            )
        })?;

    info!(
        "Created collection {} for tenant {}",
        collection_id, claims.tenant_id
    );

    // Audit log: collection created
    let _ = log_success(
        &state.db,
        &claims,
        actions::COLLECTION_CREATE,
        resources::COLLECTION,
        Some(&collection_id),
    )
    .await;

    Ok(Json(CollectionResponse {
        schema_version: "1.0".to_string(),
        collection_id,
        name: req.name,
        description: req.description,
        document_count: 0,
        tenant_id: claims.tenant_id,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: None,
    }))
}

/// List collections
#[utoipa::path(
    get,
    path = "/v1/collections",
    responses(
        (status = 200, description = "List of collections", body = Vec<CollectionResponse>),
        (status = 500, description = "Internal server error")
    ),
    tag = "collections"
)]
pub async fn list_collections(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)
        .map_err(|e| (e.0, e.1.error.clone()))?;

    let collections = state
        .db
        .list_collections(&claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to list collections: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list collections: {}", e),
            )
        })?;

    let mut responses = Vec::new();
    for c in collections {
        let document_count = state
            .db
            .count_collection_documents(&c.id)
            .await
            .unwrap_or(0);

        responses.push(CollectionResponse {
            schema_version: "1.0".to_string(),
            collection_id: c.id,
            name: c.name,
            description: c.description,
            document_count: document_count as i32,
            tenant_id: c.tenant_id,
            created_at: c.created_at.clone(),
            updated_at: Some(c.updated_at),
        });
    }

    Ok(Json(responses))
}

/// Get a specific collection with documents
#[utoipa::path(
    get,
    path = "/v1/collections/{id}",
    params(
        ("id" = String, Path, description = "Collection ID")
    ),
    responses(
        (status = 200, description = "Collection details", body = CollectionDetailResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Collection not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "collections"
)]
pub async fn get_collection(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)
        .map_err(|e| (e.0, e.1.error.clone()))?;

    let collection = state.db.get_collection(&id).await.map_err(|e| {
        error!("Failed to get collection: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get collection: {}", e),
        )
    })?;

    let collection = collection.ok_or((StatusCode::NOT_FOUND, "Collection not found".to_string()))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &collection.tenant_id)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    // Get documents in collection
    let documents = state
        .db
        .get_collection_documents(&id)
        .await
        .map_err(|e| {
            error!("Failed to list collection documents: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list collection documents: {}", e),
            )
        })?;

    let document_infos: Vec<CollectionDocumentInfo> = documents
        .into_iter()
        .map(|d| CollectionDocumentInfo {
            document_id: d.id,
            name: d.name,
            size_bytes: d.file_size,
            status: d.status,
            added_at: d.created_at,
        })
        .collect();

    let document_count = document_infos.len() as i32;

    Ok(Json(CollectionDetailResponse {
        schema_version: "1.0".to_string(),
        collection_id: collection.id,
        name: collection.name,
        description: collection.description,
        document_count,
        tenant_id: collection.tenant_id,
        documents: document_infos,
        created_at: collection.created_at,
        updated_at: Some(collection.updated_at),
    }))
}

/// Delete a collection
#[utoipa::path(
    delete,
    path = "/v1/collections/{id}",
    params(
        ("id" = String, Path, description = "Collection ID")
    ),
    responses(
        (status = 204, description = "Collection deleted successfully"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Collection not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "collections"
)]
pub async fn delete_collection(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetDelete)
        .map_err(|e| (e.0, e.1.error.clone()))?;

    // Get collection to validate tenant
    let collection = state.db.get_collection(&id).await.map_err(|e| {
        error!("Failed to get collection: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get collection: {}", e),
        )
    })?;

    let collection = collection.ok_or((StatusCode::NOT_FOUND, "Collection not found".to_string()))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &collection.tenant_id)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    // Delete from database (cascades to collection_documents)
    state.db.delete_collection(&id).await.map_err(|e| {
        error!("Failed to delete collection: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete collection: {}", e),
        )
    })?;

    info!("Deleted collection {}", id);

    // Audit log: collection deleted
    let _ = log_success(
        &state.db,
        &claims,
        actions::COLLECTION_DELETE,
        resources::COLLECTION,
        Some(&id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// Add a document to a collection
#[utoipa::path(
    post,
    path = "/v1/collections/{id}/documents",
    params(
        ("id" = String, Path, description = "Collection ID")
    ),
    request_body = AddDocumentRequest,
    responses(
        (status = 200, description = "Document added successfully"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Collection or document not found"),
        (status = 409, description = "Document already in collection"),
        (status = 500, description = "Internal server error")
    ),
    tag = "collections"
)]
pub async fn add_document_to_collection(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
    Json(req): Json<AddDocumentRequest>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload)
        .map_err(|e| (e.0, e.1.error.clone()))?;

    // Verify collection exists and tenant isolation
    let collection = state.db.get_collection(&id).await.map_err(|e| {
        error!("Failed to get collection: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get collection: {}", e),
        )
    })?;

    let collection = collection.ok_or((StatusCode::NOT_FOUND, "Collection not found".to_string()))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &collection.tenant_id)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    // Verify document exists and belongs to same tenant
    let document = state
        .db
        .get_document(&req.document_id)
        .await
        .map_err(|e| {
            error!("Failed to get document: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to get document: {}", e),
            )
        })?;

    let document = document.ok_or((StatusCode::NOT_FOUND, "Document not found".to_string()))?;

    // CRITICAL: Ensure document belongs to same tenant
    validate_tenant_isolation(&claims, &document.tenant_id)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    // Add document to collection
    state
        .db
        .add_document_to_collection(&id, &req.document_id)
        .await
        .map_err(|e| {
            let error_str = e.to_string();
            if error_str.contains("UNIQUE constraint failed") {
                (
                    StatusCode::CONFLICT,
                    "Document already in collection".to_string(),
                )
            } else {
                error!("Failed to add document to collection: {}", e);
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Failed to add document to collection: {}", e),
                )
            }
        })?;

    info!("Added document {} to collection {}", req.document_id, id);

    // Audit log: document added to collection
    let _ = log_success(
        &state.db,
        &claims,
        actions::COLLECTION_ADD_DOCUMENT,
        resources::COLLECTION,
        Some(&id),
    )
    .await;

    Ok(StatusCode::OK)
}

/// Remove a document from a collection
#[utoipa::path(
    delete,
    path = "/v1/collections/{id}/documents/{doc_id}",
    params(
        ("id" = String, Path, description = "Collection ID"),
        ("doc_id" = String, Path, description = "Document ID")
    ),
    responses(
        (status = 204, description = "Document removed successfully"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Collection or document not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "collections"
)]
pub async fn remove_document_from_collection(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path((id, doc_id)): Path<(String, String)>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetDelete)
        .map_err(|e| (e.0, e.1.error.clone()))?;

    // Verify collection exists and tenant isolation
    let collection = state.db.get_collection(&id).await.map_err(|e| {
        error!("Failed to get collection: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get collection: {}", e),
        )
    })?;

    let collection = collection.ok_or((StatusCode::NOT_FOUND, "Collection not found".to_string()))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &collection.tenant_id)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    // Remove document from collection
    state
        .db
        .remove_document_from_collection(&id, &doc_id)
        .await
        .map_err(|e| {
            error!("Failed to remove document from collection: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to remove document from collection: {}", e),
            )
        })?;

    info!("Removed document {} from collection {}", doc_id, id);

    // Audit log: document removed from collection
    let _ = log_success(
        &state.db,
        &claims,
        actions::COLLECTION_REMOVE_DOCUMENT,
        resources::COLLECTION,
        Some(&id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}
