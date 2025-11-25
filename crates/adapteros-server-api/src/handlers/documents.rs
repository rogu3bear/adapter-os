//! Document management handlers
//!
//! Provides REST endpoints for PDF document upload, indexing, and management.
//! Documents are ingested, chunked, and stored with embeddings for RAG workflows.

use crate::auth::Claims;
use crate::audit_helper::{actions, log_success, resources};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::*;
use axum::{
    extract::{Multipart, Path, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    Extension,
};
use blake3::Hasher;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, error, info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

/// Default document storage root if not configured
const DEFAULT_DOCUMENT_STORAGE: &str = "var/documents";

/// Maximum document size (100MB)
const MAX_DOCUMENT_SIZE: usize = 100 * 1024 * 1024;

/// Document response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DocumentResponse {
    pub schema_version: String,
    pub document_id: String,
    pub name: String,
    pub hash_b3: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub storage_path: String,
    pub status: String, // "processing", "indexed", "failed"
    pub chunk_count: Option<i32>,
    pub tenant_id: String,
    pub created_at: String,
    pub updated_at: Option<String>,
}

/// Chunk response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ChunkResponse {
    pub schema_version: String,
    pub chunk_id: String,
    pub document_id: String,
    pub chunk_index: i32,
    pub text: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
}

/// Upload document request (multipart form)
/// Expected fields:
/// - file: PDF file (required)
/// - name: Document name (optional, defaults to filename)
#[utoipa::path(
    post,
    path = "/v1/documents/upload",
    responses(
        (status = 200, description = "Document uploaded successfully", body = DocumentResponse),
        (status = 400, description = "Invalid request"),
        (status = 413, description = "Document too large"),
        (status = 500, description = "Internal server error")
    ),
    tag = "documents"
)]
pub async fn upload_document(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload)
        .map_err(|e| (e.0, e.1.error.clone()))?;

    let document_id = Uuid::now_v7().to_string();
    let storage_root =
        std::env::var("DOCUMENT_STORAGE_PATH").unwrap_or_else(|_| DEFAULT_DOCUMENT_STORAGE.to_string());

    // Create tenant-specific document directory
    let tenant_path = PathBuf::from(&storage_root).join(&claims.tenant_id);
    fs::create_dir_all(&tenant_path).await.map_err(|e| {
        error!("Failed to create document directory: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create document directory: {}", e),
        )
    })?;

    let mut document_name = String::new();
    let mut file_data: Option<Vec<u8>> = None;
    let mut mime_type = "application/pdf".to_string();

    // Process multipart form
    while let Some(field) = multipart.next_field().await.map_err(|e| {
        (
            StatusCode::BAD_REQUEST,
            format!("Failed to read multipart field: {}", e),
        )
    })? {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "name" => {
                document_name = field.text().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Failed to read name field: {}", e),
                    )
                })?;
            }
            "file" => {
                let file_name = field
                    .file_name()
                    .ok_or((StatusCode::BAD_REQUEST, "File must have a name".to_string()))?
                    .to_string();

                if document_name.is_empty() {
                    document_name = file_name.clone();
                }

                if let Some(ct) = field.content_type() {
                    mime_type = ct.to_string();
                }

                let data = field.bytes().await.map_err(|e| {
                    (
                        StatusCode::BAD_REQUEST,
                        format!("Failed to read file data: {}", e),
                    )
                })?;

                if data.len() > MAX_DOCUMENT_SIZE {
                    return Err((
                        StatusCode::PAYLOAD_TOO_LARGE,
                        format!(
                            "Document exceeds maximum size of {}MB",
                            MAX_DOCUMENT_SIZE / 1024 / 1024
                        ),
                    ));
                }

                file_data = Some(data.to_vec());
            }
            _ => {
                debug!("Ignoring unknown field: {}", name);
            }
        }
    }

    let file_data = file_data
        .ok_or((StatusCode::BAD_REQUEST, "No file uploaded".to_string()))?;

    if document_name.is_empty() {
        document_name = format!("Document {}", &document_id[0..8]);
    }

    // Compute hash
    let mut hasher = Hasher::new();
    hasher.update(&file_data);
    let file_hash = hasher.finalize().to_hex().to_string();

    // Save file to disk
    let file_path = tenant_path.join(format!("{}.pdf", document_id));
    let mut file = fs::File::create(&file_path).await.map_err(|e| {
        error!("Failed to create file: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create file: {}", e),
        )
    })?;

    file.write_all(&file_data).await.map_err(|e| {
        error!("Failed to write file: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to write file: {}", e),
        )
    })?;

    file.flush().await.map_err(|e| {
        error!("Failed to flush file: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to flush file: {}", e),
        )
    })?;

    // Create database record
    use adapteros_db::documents::CreateDocumentParams;
    let document_id_result = state
        .db
        .create_document(CreateDocumentParams {
            tenant_id: claims.tenant_id.clone(),
            name: document_name.clone(),
            content_hash: file_hash.clone(),
            file_path: file_path.to_string_lossy().to_string(),
            file_size: file_data.len() as i64,
            mime_type: mime_type.clone(),
            page_count: None,
        })
        .await
        .map_err(|e| {
            error!("Failed to create document record: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to create document record: {}", e),
            )
        })?;

    info!(
        "Uploaded document {} ({} bytes) for tenant {}",
        document_id,
        file_data.len(),
        claims.tenant_id
    );

    // Audit log: document uploaded
    let _ = log_success(
        &state.db,
        &claims,
        actions::DOCUMENT_UPLOAD,
        resources::DOCUMENT,
        Some(&document_id),
    )
    .await;

    Ok(Json(DocumentResponse {
        schema_version: "1.0".to_string(),
        document_id: document_id.clone(),
        name: document_name,
        hash_b3: file_hash,
        size_bytes: file_data.len() as i64,
        mime_type,
        storage_path: file_path.to_string_lossy().to_string(),
        status: "processing".to_string(),
        chunk_count: None,
        tenant_id: claims.tenant_id,
        created_at: chrono::Utc::now().to_rfc3339(),
        updated_at: None,
    }))
}

/// List documents
#[utoipa::path(
    get,
    path = "/v1/documents",
    responses(
        (status = 200, description = "List of documents", body = Vec<DocumentResponse>),
        (status = 500, description = "Internal server error")
    ),
    tag = "documents"
)]
pub async fn list_documents(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)
        .map_err(|e| (e.0, e.1.error.clone()))?;

    let documents = state
        .db
        .list_documents(&claims.tenant_id)
        .await
        .map_err(|e| {
            error!("Failed to list documents: {}", e);
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to list documents: {}", e),
            )
        })?;

    let responses: Vec<DocumentResponse> = documents
        .into_iter()
        .map(|d| DocumentResponse {
            schema_version: "1.0".to_string(),
            document_id: d.id.clone(),
            name: d.name,
            hash_b3: d.content_hash,
            size_bytes: d.file_size,
            mime_type: d.mime_type,
            storage_path: d.file_path,
            status: d.status,
            chunk_count: d.page_count,
            tenant_id: d.tenant_id,
            created_at: d.created_at,
            updated_at: Some(d.updated_at),
        })
        .collect();

    Ok(Json(responses))
}

/// Get a specific document
#[utoipa::path(
    get,
    path = "/v1/documents/{id}",
    params(
        ("id" = String, Path, description = "Document ID")
    ),
    responses(
        (status = 200, description = "Document details", body = DocumentResponse),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Document not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "documents"
)]
pub async fn get_document(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)
        .map_err(|e| (e.0, e.1.error.clone()))?;

    let document = state.db.get_document(&id).await.map_err(|e| {
        error!("Failed to get document: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get document: {}", e),
        )
    })?;

    let document = document.ok_or((StatusCode::NOT_FOUND, "Document not found".to_string()))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    Ok(Json(DocumentResponse {
        schema_version: "1.0".to_string(),
        document_id: document.id,
        name: document.name,
        hash_b3: document.content_hash,
        size_bytes: document.file_size,
        mime_type: document.mime_type,
        storage_path: document.file_path,
        status: document.status,
        chunk_count: document.page_count,
        tenant_id: document.tenant_id,
        created_at: document.created_at,
        updated_at: Some(document.updated_at),
    }))
}

/// Delete a document
#[utoipa::path(
    delete,
    path = "/v1/documents/{id}",
    params(
        ("id" = String, Path, description = "Document ID")
    ),
    responses(
        (status = 204, description = "Document deleted successfully"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Document not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "documents"
)]
pub async fn delete_document(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetDelete)
        .map_err(|e| (e.0, e.1.error.clone()))?;

    // Get document to find storage path and validate tenant
    let document = state.db.get_document(&id).await.map_err(|e| {
        error!("Failed to get document: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get document: {}", e),
        )
    })?;

    let document = document.ok_or((StatusCode::NOT_FOUND, "Document not found".to_string()))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    // Delete from database (cascades to chunks)
    state.db.delete_document(&id).await.map_err(|e| {
        error!("Failed to delete document: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to delete document: {}", e),
        )
    })?;

    // Delete file from filesystem
    if tokio::fs::try_exists(&document.file_path)
        .await
        .unwrap_or(false)
    {
        tokio::fs::remove_file(&document.file_path)
            .await
            .map_err(|e| {
                warn!(
                    "Failed to delete document file at {}: {}",
                    document.file_path, e
                );
                e
            })
            .ok();
    }

    info!("Deleted document {} and its chunks", id);

    // Audit log: document deleted
    let _ = log_success(
        &state.db,
        &claims,
        actions::DOCUMENT_DELETE,
        resources::DOCUMENT,
        Some(&id),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}

/// List chunks for a document
#[utoipa::path(
    get,
    path = "/v1/documents/{id}/chunks",
    params(
        ("id" = String, Path, description = "Document ID")
    ),
    responses(
        (status = 200, description = "List of chunks", body = Vec<ChunkResponse>),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Document not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "documents"
)]
pub async fn list_document_chunks(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)
        .map_err(|e| (e.0, e.1.error.clone()))?;

    // Verify document exists and tenant isolation
    let document = state.db.get_document(&id).await.map_err(|e| {
        error!("Failed to get document: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get document: {}", e),
        )
    })?;

    let document = document.ok_or((StatusCode::NOT_FOUND, "Document not found".to_string()))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    let chunks = state.db.get_document_chunks(&id).await.map_err(|e| {
        error!("Failed to list chunks: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list chunks: {}", e),
        )
    })?;

    let responses: Vec<ChunkResponse> = chunks
        .into_iter()
        .map(|c| ChunkResponse {
            schema_version: "1.0".to_string(),
            chunk_id: c.id,
            document_id: c.document_id,
            chunk_index: c.chunk_index,
            text: c.text_preview.clone().unwrap_or_default(),
            embedding: None, // TODO: Add embedding field to DB schema
            metadata: None, // TODO: Add metadata field to DB schema
            created_at: chrono::Utc::now().to_rfc3339(),
        })
        .collect();

    Ok(Json(responses))
}

/// Download original document file
#[utoipa::path(
    get,
    path = "/v1/documents/{id}/download",
    params(
        ("id" = String, Path, description = "Document ID")
    ),
    responses(
        (status = 200, description = "Document file"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Document not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "documents"
)]
pub async fn download_document(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)
        .map_err(|e| (e.0, e.1.error.clone()))?;

    // Get document to find storage path and validate tenant
    let document = state.db.get_document(&id).await.map_err(|e| {
        error!("Failed to get document: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to get document: {}", e),
        )
    })?;

    let document = document.ok_or((StatusCode::NOT_FOUND, "Document not found".to_string()))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)
        .map_err(|(code, json_err)| (code, json_err.0.error))?;

    // Read file
    let file_data = fs::read(&document.file_path).await.map_err(|e| {
        error!("Failed to read document file: {}", e);
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read document file: {}", e),
        )
    })?;

    // Return file with appropriate headers
    use axum::http::header;
    use axum::response::IntoResponse;

    let mime_type = document.mime_type.clone();
    let filename = format!("attachment; filename=\"{}.pdf\"", document.name);

    let headers = [
        (header::CONTENT_TYPE, mime_type.as_str()),
        (header::CONTENT_DISPOSITION, filename.as_str()),
    ];

    Ok((headers, file_data).into_response())
}
