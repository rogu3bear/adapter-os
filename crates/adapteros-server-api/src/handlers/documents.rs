//! Document management handlers
//!
//! Provides REST endpoints for PDF document upload, indexing, and management.
//! Documents are ingested, chunked, and stored with embeddings for RAG workflows.

use crate::api_error::ApiError;
use crate::audit_helper::{actions, log_success_or_warn, resources};
use crate::auth::Claims;
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::{ErrorResponse, PaginatedResponse};
use adapteros_core::reject_forbidden_tmp_path;
use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    Extension,
};
use serde::{Deserialize, Serialize};
use std::path::{Path as StdPath, PathBuf};
#[cfg(feature = "embeddings")]
use std::sync::Arc;
#[cfg(feature = "embeddings")]
use std::time::Duration;
use tokio::fs;
use tokio::io::AsyncWriteExt;
#[cfg(feature = "embeddings")]
use tokio::time::sleep;
use tracing::{debug, info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

/// Maximum document size (100MB)
const MAX_DOCUMENT_SIZE: usize = 100 * 1024 * 1024;

/// Query parameters for listing documents
#[derive(Debug, Clone, Default, Deserialize, utoipa::IntoParams)]
pub struct DocumentListParams {
    /// Filter by status (e.g., "indexed", "processing", "failed")
    pub status: Option<String>,
    /// Page number (1-indexed)
    #[serde(default = "default_page")]
    pub page: u32,
    /// Items per page
    #[serde(default = "default_limit")]
    pub limit: u32,
}

fn default_page() -> u32 {
    1
}

fn default_limit() -> u32 {
    20
}
#[cfg(feature = "embeddings")]
const EMBEDDING_MAX_RETRIES: usize = 3;
#[cfg(feature = "embeddings")]
const EMBEDDING_BACKOFF_MS: u64 = 200;

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
    /// True if this response points to a pre-existing document with identical content
    #[serde(default)]
    pub deduplicated: bool,
    // Error tracking and retry fields
    pub error_message: Option<String>,
    pub error_code: Option<String>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub processing_started_at: Option<String>,
    pub processing_completed_at: Option<String>,
}

impl From<adapteros_db::documents::Document> for DocumentResponse {
    fn from(doc: adapteros_db::documents::Document) -> Self {
        Self {
            schema_version: "1.0".to_string(),
            document_id: doc.id,
            name: doc.name,
            hash_b3: doc.content_hash,
            size_bytes: doc.file_size,
            mime_type: doc.mime_type,
            storage_path: doc.file_path,
            status: doc.status,
            chunk_count: doc.page_count,
            tenant_id: doc.tenant_id,
            created_at: doc.created_at,
            updated_at: Some(doc.updated_at),
            deduplicated: false,
            error_message: doc.error_message,
            error_code: doc.error_code,
            retry_count: doc.retry_count,
            max_retries: doc.max_retries,
            processing_started_at: doc.processing_started_at,
            processing_completed_at: doc.processing_completed_at,
        }
    }
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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload)?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let document_id = Uuid::now_v7().to_string();
    let storage_root =
        std::env::var("AOS_DOCUMENTS_DIR")
            .ok()
            .unwrap_or_else(|| match state.config.read() {
                Ok(config) => config.paths.documents_root.clone(),
                Err(_) => {
                    tracing::error!("Config lock poisoned in upload_document");
                    "var/documents".to_string()
                }
            });

    let root = PathBuf::from(&storage_root);
    let root = if root.is_absolute() {
        root
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| StdPath::new("/").to_path_buf())
            .join(root)
    };
    reject_forbidden_tmp_path(&root, "documents-root")
        .map_err(|e| ApiError::internal(e.to_string()))?;

    // Create tenant-specific document directory
    let tenant_path = root.join(&claims.tenant_id);
    fs::create_dir_all(&tenant_path)
        .await
        .map_err(ApiError::db_error)?;

    let mut document_name = String::new();
    let mut file_data: Option<Vec<u8>> = None;
    let mut mime_type = "application/pdf".to_string();

    // Process multipart form
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| ApiError::bad_request(e.to_string()))?
    {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "name" => {
                document_name = field
                    .text()
                    .await
                    .map_err(|e| ApiError::bad_request(e.to_string()))?;
            }
            "file" => {
                let file_name = field
                    .file_name()
                    .ok_or_else(|| ApiError::bad_request("File must have a name"))?
                    .to_string();

                if document_name.is_empty() {
                    document_name = file_name.clone();
                }

                if let Some(ct) = field.content_type() {
                    mime_type = ct.to_string();
                }

                let data = field
                    .bytes()
                    .await
                    .map_err(|e| ApiError::bad_request(e.to_string()))?;

                if data.len() > MAX_DOCUMENT_SIZE {
                    return Err(ApiError::payload_too_large(&format!(
                        "Document exceeds maximum size of {}MB",
                        MAX_DOCUMENT_SIZE / 1024 / 1024
                    ))
                    .into());
                }

                file_data = Some(data.to_vec());
            }
            _ => {
                debug!("Ignoring unknown field: {}", name);
            }
        }
    }

    let file_data = file_data.ok_or_else(|| ApiError::bad_request("No file uploaded"))?;

    if document_name.is_empty() {
        document_name = format!("Document {}", &document_id[0..8]);
    }

    // Compute hash using B3Hash from adapteros-core
    use adapteros_core::B3Hash;
    let file_hash = B3Hash::hash(&file_data).to_hex();

    // Check for existing document with same content hash (deduplication)
    if let Some(existing_doc) = state
        .db
        .find_document_by_content_hash(&claims.tenant_id, &file_hash)
        .await
        .map_err(ApiError::db_error)?
    {
        info!(
            existing_id = %existing_doc.id,
            hash = %file_hash,
            "Deduplicated document upload - returning existing document"
        );

        // Audit log: document upload deduplicated
        log_success_or_warn(
            &state.db,
            &claims,
            actions::DOCUMENT_UPLOAD,
            resources::DOCUMENT,
            Some(&existing_doc.id),
        )
        .await;

        let mut response = DocumentResponse::from(existing_doc);
        response.deduplicated = true;
        return Ok(Json(response));
    }

    // Save file to disk (only for new documents)
    let file_path = tenant_path.join(format!("{}.pdf", document_id));
    let mut file = fs::File::create(&file_path)
        .await
        .map_err(ApiError::db_error)?;

    file.write_all(&file_data)
        .await
        .map_err(ApiError::db_error)?;

    file.flush().await.map_err(ApiError::db_error)?;

    // Create database record
    use adapteros_db::documents::CreateDocumentParams;
    let _document_id_result = state
        .db
        .create_document(CreateDocumentParams {
            id: document_id.clone(),
            tenant_id: claims.tenant_id.clone(),
            name: document_name.clone(),
            content_hash: file_hash.clone(),
            file_path: file_path.to_string_lossy().to_string(),
            file_size: file_data.len() as i64,
            mime_type: mime_type.clone(),
            page_count: None,
        })
        .await
        .map_err(ApiError::db_error)?;

    info!(
        "Uploaded document {} ({} bytes) for tenant {}",
        document_id,
        file_data.len(),
        claims.tenant_id
    );

    // Audit log: document uploaded
    log_success_or_warn(
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
        deduplicated: false,
        error_message: None,
        error_code: None,
        retry_count: 0,
        max_retries: 3,
        processing_started_at: None,
        processing_completed_at: None,
    }))
}

/// List documents with pagination and optional status filter
#[utoipa::path(
    get,
    path = "/v1/documents",
    params(
        ("status" = Option<String>, Query, description = "Filter by status (indexed, processing, failed)"),
        ("page" = Option<u32>, Query, description = "Page number (1-indexed)"),
        ("limit" = Option<u32>, Query, description = "Items per page")
    ),
    responses(
        (status = 200, description = "Paginated list of documents", body = PaginatedResponse<DocumentResponse>),
        (status = 500, description = "Internal server error")
    ),
    tag = "documents"
)]
pub async fn list_documents(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<DocumentListParams>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    // Fetch all documents for this tenant (pagination applied after filtering)
    let (all_documents, _) = state
        .db
        .list_documents_paginated(&claims.tenant_id, i64::MAX, 0)
        .await
        .map_err(ApiError::db_error)?;

    // Apply status filter if provided
    let filtered: Vec<_> = if let Some(ref status) = params.status {
        all_documents
            .into_iter()
            .filter(|d| d.status == *status)
            .collect()
    } else {
        all_documents
    };

    let total = filtered.len() as u64;

    // Apply pagination to filtered results
    let offset = (params.page.saturating_sub(1) * params.limit) as usize;
    let limit = params.limit as usize;
    let data: Vec<DocumentResponse> = filtered
        .into_iter()
        .skip(offset)
        .take(limit)
        .map(DocumentResponse::from)
        .collect();

    let pages = ((total as f64) / (params.limit as f64)).ceil() as u32;
    let response = adapteros_api_types::PaginatedResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        data,
        total,
        page: params.page,
        limit: params.limit,
        pages,
    };

    Ok(Json(response))
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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)?;

    // Tenant isolation enforced at DB layer - only returns document if tenant matches
    let document = state
        .db
        .get_document(&claims.tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?;

    let document = document.ok_or_else(|| ApiError::not_found("Document"))?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)?;

    Ok(Json(DocumentResponse::from(document)))
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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetDelete)?;

    // Get document to find storage path (tenant isolation enforced at DB layer)
    let document = state
        .db
        .get_document(&claims.tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?;

    let document = document.ok_or_else(|| ApiError::not_found("Document"))?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)?;

    // Delete from database (cascades to chunks)
    state
        .db
        .delete_document(&id)
        .await
        .map_err(ApiError::db_error)?;

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
    log_success_or_warn(
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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)?;

    // Verify document exists (tenant isolation enforced at DB layer)
    let document = state
        .db
        .get_document(&claims.tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?;

    let document = document.ok_or_else(|| ApiError::not_found("Document"))?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)?;

    let chunks = state
        .db
        .get_document_chunks(&claims.tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?;

    let responses: Vec<ChunkResponse> = chunks
        .into_iter()
        .map(|c| {
            // Parse embedding from JSON string if present
            let embedding = c
                .embedding_json
                .as_ref()
                .and_then(|json_str| serde_json::from_str::<Vec<f32>>(json_str).ok());

            // Construct metadata from chunk fields
            let metadata = serde_json::json!({
                "page_number": c.page_number,
                "start_offset": c.start_offset,
                "end_offset": c.end_offset,
                "chunk_hash": c.chunk_hash,
            });

            ChunkResponse {
                schema_version: "1.0".to_string(),
                chunk_id: c.id,
                document_id: c.document_id,
                chunk_index: c.chunk_index,
                text: c.text_preview.unwrap_or_default(),
                embedding,
                metadata: Some(metadata),
                created_at: chrono::Utc::now().to_rfc3339(),
            }
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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)?;

    // Get document to find storage path (tenant isolation enforced at DB layer)
    let document = state
        .db
        .get_document(&claims.tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?;

    let document = document.ok_or_else(|| ApiError::not_found("Document"))?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)?;

    // Read file
    let file_data = fs::read(&document.file_path)
        .await
        .map_err(ApiError::db_error)?;

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

/// Process document response
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct ProcessDocumentResponse {
    pub schema_version: String,
    pub document_id: String,
    pub status: String,
    pub chunk_count: i32,
    pub indexed_at: String,
}

/// Process a document: parse, chunk, generate embeddings, and index for RAG
///
/// This endpoint takes an uploaded document and:
/// 1. Parses the document content (PDF/Markdown)
/// 2. Chunks the content with configurable parameters
/// 3. Generates embeddings for each chunk
/// 4. Stores chunks in document_chunks table (for evidence tracking)
/// 5. Indexes chunks in rag_documents table (for vector search)
/// 6. Updates document status to "indexed"
///
/// **IMPORTANT**: This establishes the unified ID format where:
/// - `document_chunks.id` is a UUID (used for FK in inference_evidence)
/// - `rag_documents.doc_id` is `{document_id}__chunk_{index}` using the document's UUID
/// - This allows collection filtering to work correctly with UUID-based document_ids
#[cfg(feature = "embeddings")]
#[utoipa::path(
    post,
    path = "/v1/documents/{id}/process",
    params(
        ("id" = String, Path, description = "Document ID to process")
    ),
    responses(
        (status = 200, description = "Document processed successfully", body = ProcessDocumentResponse),
        (status = 400, description = "Document already processed or invalid"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Document not found"),
        (status = 500, description = "Processing failed")
    ),
    tag = "documents"
)]
pub async fn process_document(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetUpload)?;

    // Get document (tenant isolation enforced at DB layer)
    let document = state
        .db
        .get_document(&claims.tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?;
    let document = document.ok_or_else(|| ApiError::not_found("Document"))?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)?;

    // Check document state - validate current status
    match document.status.as_str() {
        "indexed" => {
            return Err(ApiError::bad_request("Document is already indexed").into());
        }
        "processing" => {
            return Err(ApiError::bad_request("Document is currently being processed").into());
        }
        "pending" | "failed" => {
            // Allowed to process - will acquire lock
        }
        _ => {
            return Err(
                ApiError::bad_request(format!("Unknown document status: {}", document.status))
                    .into(),
            );
        }
    }

    // Acquire processing lock atomically - prevents race conditions
    let acquired = state
        .db
        .try_acquire_processing_lock(&claims.tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?;

    if !acquired {
        return Err(
            ApiError::bad_request(
                "Failed to acquire processing lock (document may be processing by another request)",
            )
            .into(),
        );
    }

    // Process with error handling - on failure, mark as failed
    match process_document_inner(&state, &claims, &id, &document).await {
        Ok(response) => Ok(response),
        Err(e) => {
            // Mark document as failed with error details
            let error_msg = format!("{:?}", e);
            let _ = state
                .db
                .mark_document_failed(&claims.tenant_id, &id, &error_msg, "PROCESSING_ERROR")
                .await;
            Err(e)
        }
    }
}

/// Inner processing logic with transactional chunk creation
#[cfg(feature = "embeddings")]
async fn process_document_inner(
    state: &AppState,
    claims: &Claims,
    document_id: &str,
    document: &adapteros_db::documents::Document,
) -> Result<Json<ProcessDocumentResponse>, (StatusCode, Json<ErrorResponse>)> {
    use adapteros_core::B3Hash;
    use adapteros_ingest_docs::{default_ingest_options, DocumentIngestor};

    // Get embedding model from state
    let embedding_model = state.embedding_model.as_ref().ok_or_else(|| {
        ApiError::db_error("Embedding model not configured - enable embeddings feature")
    })?;

    // Read document file
    let file_data = fs::read(&document.file_path)
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to read document file: {}", e)))?;

    // Parse document into chunks - use resilient processing for PDFs
    let ingestor = DocumentIngestor::new(default_ingest_options(), None);
    let ingested_doc = if document.mime_type.contains("pdf") {
        // Use resilient PDF processing that continues on page errors
        let result = ingestor
            .ingest_pdf_bytes_resilient(&file_data, &document.name)
            .map_err(|e| ApiError::db_error(format!("Failed to parse PDF: {}", e)))?;

        // Log any page errors
        if result.successful_pages < result.total_pages {
            warn!(
                document_id = %document_id,
                total_pages = result.total_pages,
                successful_pages = result.successful_pages,
                "Document processed with some page failures"
            );
        }

        result.document
    } else if document.mime_type.contains("markdown") || document.name.ends_with(".md") {
        ingestor
            .ingest_markdown_bytes(&file_data, &document.name)
            .map_err(|e| ApiError::db_error(format!("Failed to parse markdown: {}", e)))?
    } else {
        return Err(
            ApiError::bad_request(format!("Unsupported document type: {}", document.mime_type))
                .into(),
        );
    };

    info!(
        document_id = %document_id,
        chunks = ingested_doc.chunks.len(),
        "Parsed document into chunks"
    );

    let model_hash = embedding_model.model_hash();
    let dimension = embedding_model.dimension();

    // Start transaction for atomic chunk creation
    let pool = state.db.pool();
    let mut tx = pool
        .begin()
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to start transaction: {}", e)))?;

    let mut chunk_count = 0;
    let mut failed_embeddings = 0;

    // Process each chunk within transaction
    for chunk in &ingested_doc.chunks {
        // Generate chunk UUID for document_chunks table
        let chunk_db_id = Uuid::now_v7().to_string();

        // Generate embedding with retry/backoff so one bad chunk does not abort the batch
        let embedding = embed_with_backoff(embedding_model, &chunk.text).await;

        // Compute chunk hash
        let chunk_hash = B3Hash::hash(chunk.text.as_bytes()).to_hex();

        // Create text preview (first 200 chars)
        let text_preview = if chunk.text.len() > 200 {
            format!("{}...", &chunk.text[..200])
        } else {
            chunk.text.clone()
        };

        // Store embedding as JSON (or failure marker)
        let (embedding_json, rag_embedding) = match embedding {
            Ok(vector) => {
                let serialized = serde_json::to_string(&vector).map_err(|e| {
                    ApiError::db_error(format!("Failed to serialize embedding: {}", e))
                })?;
                (Some(serialized), Some(vector))
            }
            Err(e) => {
                failed_embeddings += 1;
                warn!(
                    document_id = %document_id,
                    chunk_index = chunk.chunk_index,
                    error = %e,
                    "Embedding failed after retries; marking chunk as failed_embedding"
                );
                (Some("{\"status\":\"failed_embedding\"}".to_string()), None)
            }
        };

        // Insert into document_chunks table within transaction
        sqlx::query(
            r#"
            INSERT INTO document_chunks (
                id, tenant_id, document_id, chunk_index, page_number,
                start_offset, end_offset, chunk_hash, text_preview, embedding_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&chunk_db_id)
        .bind(&claims.tenant_id)
        .bind(document_id)
        .bind(chunk.chunk_index as i64)
        .bind(chunk.page_number.map(|p| p as i64))
        .bind(chunk.start_offset as i64)
        .bind(chunk.end_offset as i64)
        .bind(&chunk_hash)
        .bind(&text_preview)
        .bind(embedding_json.as_deref())
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            ApiError::db_error(format!(
                "Failed to insert chunk {}: {}",
                chunk.chunk_index, e
            ))
        })?;

        // Only insert into RAG if embedding succeeded
        if let Some(embedding_vec) = rag_embedding {
            // Generate RAG doc_id using UUID-based document_id
            // Format: {document_id}__chunk_{index}
            let rag_doc_id = format!("{}__chunk_{}", document_id, chunk.chunk_index);

            // Insert into RAG index within same transaction
            sqlx::query(
                r#"
                INSERT OR REPLACE INTO rag_documents (
                    doc_id, tenant_id, text, embedding_json, rev, effectivity, source_type
                ) VALUES (?, ?, ?, ?, ?, 'current', 'document')
                "#,
            )
            .bind(&rag_doc_id)
            .bind(&claims.tenant_id)
            .bind(&chunk.text)
            .bind(&serde_json::to_string(&embedding_vec).map_err(|e| {
                ApiError::db_error(format!(
                    "Failed to serialize embedding for rag_documents: {}",
                    e
                ))
            })?)
            .bind(Uuid::now_v7().to_string())
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                ApiError::db_error(format!(
                    "Failed to insert RAG chunk {}: {}",
                    chunk.chunk_index, e
                ))
            })?;
        }

        chunk_count += 1;

        debug!(
            document_id = %document_id,
            chunk_index = chunk.chunk_index,
            chunk_db_id = %chunk_db_id,
            "Indexed chunk"
        );
    }

    if failed_embeddings > 0 {
        warn!(
            document_id = %document_id,
            failed_chunks = failed_embeddings,
            "Some chunk embeddings failed; stored failed_embedding markers and continued"
        );
    }

    // Update document status to indexed within same transaction
    sqlx::query(
        r#"
        UPDATE documents
        SET status = 'indexed',
            page_count = ?,
            processing_completed_at = datetime('now'),
            updated_at = datetime('now')
        WHERE id = ? AND tenant_id = ?
        "#,
    )
    .bind(ingested_doc.page_count.map(|p| p as i64))
    .bind(document_id)
    .bind(&claims.tenant_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| ApiError::db_error(format!("Failed to update document status: {}", e)))?;

    // Commit transaction - all chunks and status update together
    tx.commit()
        .await
        .map_err(|e| ApiError::db_error(format!("Failed to commit transaction: {}", e)))?;

    info!(
        document_id = %document_id,
        chunk_count = chunk_count,
        "Document successfully indexed"
    );

    // Audit log: document processed
    log_success_or_warn(
        &state.db,
        claims,
        actions::DOCUMENT_UPLOAD,
        resources::DOCUMENT,
        Some(document_id),
    )
    .await;

    Ok(Json(ProcessDocumentResponse {
        schema_version: "1.0".to_string(),
        document_id: document_id.to_string(),
        status: "indexed".to_string(),
        chunk_count: chunk_count as i32,
        indexed_at: chrono::Utc::now().to_rfc3339(),
    }))
}

#[cfg(feature = "embeddings")]
async fn embed_with_backoff(
    embedding_model: &Arc<dyn adapteros_ingest_docs::EmbeddingModel + Send + Sync>,
    text: &str,
) -> adapteros_core::Result<Vec<f32>> {
    let mut attempt = 0usize;
    let mut delay = Duration::from_millis(EMBEDDING_BACKOFF_MS);

    loop {
        attempt += 1;
        match embedding_model.encode_text(text) {
            Ok(v) => return Ok(v),
            Err(e) if attempt >= EMBEDDING_MAX_RETRIES => return Err(e),
            Err(e) => {
                warn!(
                    attempt = attempt,
                    max_attempts = EMBEDDING_MAX_RETRIES,
                    error = %e,
                    "Embedding generation failed, retrying with backoff"
                );
                sleep(delay).await;
                delay = delay.saturating_mul(2);
            }
        }
    }
}

/// Stub for process_document when embeddings feature is disabled
#[cfg(not(feature = "embeddings"))]
#[utoipa::path(
    post,
    path = "/v1/documents/{id}/process",
    params(
        ("id" = String, Path, description = "Document ID to process")
    ),
    responses(
        (status = 501, description = "Embeddings feature not enabled")
    ),
    tag = "documents"
)]
pub async fn process_document(
    State(_state): State<AppState>,
    Extension(_claims): Extension<Claims>,
    Path(_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    Err::<(), _>(
        ApiError::not_implemented(
            "Document processing requires the 'embeddings' feature to be enabled",
        )
        .into(),
    )
}

/// Retry a failed document processing.
/// Only works on documents in "failed" state that haven't exceeded max retries.
#[utoipa::path(
    post,
    path = "/v1/documents/{id}/retry",
    params(
        ("id" = String, Path, description = "Document ID to retry")
    ),
    responses(
        (status = 200, description = "Document queued for retry", body = DocumentResponse),
        (status = 400, description = "Document cannot be retried (not failed or max retries exceeded)"),
        (status = 404, description = "Document not found"),
    ),
    security(("bearer_auth" = [])),
    tag = "documents"
)]
pub async fn retry_document(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetUpload)?;

    // Get document with tenant isolation
    let document = state
        .db
        .get_document(&claims.tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Document"))?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)?;

    // Only failed documents can be retried
    if document.status != "failed" {
        return Err(ApiError::bad_request(format!(
            "Only failed documents can be retried. Current status: {}",
            document.status
        ))
        .into());
    }

    // Prepare document for retry (increments retry_count, resets to pending)
    let prepared = state
        .db
        .prepare_document_retry(&claims.tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?;

    if !prepared {
        return Err(ApiError::bad_request("Document has exceeded maximum retry attempts").into());
    }

    info!(
        document_id = %id,
        tenant_id = %claims.tenant_id,
        "Document queued for retry"
    );

    // Return updated document
    let updated = state
        .db
        .get_document(&claims.tenant_id, &id)
        .await
        .map_err(ApiError::db_error)?
        .ok_or_else(|| ApiError::not_found("Document"))?;

    // Audit log
    log_success_or_warn(
        &state.db,
        &claims,
        actions::DOCUMENT_RETRY,
        resources::DOCUMENT,
        Some(&id),
    )
    .await;

    Ok(Json(DocumentResponse::from(updated)))
}

/// Query parameters for listing failed documents
#[derive(Debug, Deserialize)]
pub struct ListFailedParams {
    pub limit: Option<i64>,
}

/// List failed documents that are eligible for retry.
#[utoipa::path(
    get,
    path = "/v1/documents/failed",
    params(
        ("limit" = Option<i64>, Query, description = "Maximum number of documents to return")
    ),
    responses(
        (status = 200, description = "List of retryable failed documents", body = Vec<DocumentResponse>),
    ),
    security(("bearer_auth" = [])),
    tag = "documents"
)]
pub async fn list_failed_documents(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Query(params): Query<ListFailedParams>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetView)?;

    // Validate tenant isolation
    validate_tenant_isolation(&claims, &claims.tenant_id)?;

    let limit = params.limit.unwrap_or(50);

    let documents = state
        .db
        .get_retryable_documents(&claims.tenant_id, limit)
        .await
        .map_err(ApiError::db_error)?;

    let response: Vec<DocumentResponse> =
        documents.into_iter().map(DocumentResponse::from).collect();

    Ok(Json(response))
}
