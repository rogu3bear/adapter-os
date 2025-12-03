//! Document management handlers
//!
//! Provides REST endpoints for PDF document upload, indexing, and management.
//! Documents are ingested, chunked, and stored with embeddings for RAG workflows.

use crate::audit_helper::{actions, log_success, resources};
use crate::auth::Claims;
use crate::error_helpers::{bad_request, db_error, not_found, payload_too_large};
use crate::permissions::{require_permission, Permission};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::types::{ErrorResponse, PaginatedResponse};
use axum::{
    extract::{Multipart, Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Json},
    Extension,
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tokio::fs;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info, warn};
use utoipa::ToSchema;
use uuid::Uuid;

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
    /// True if this response points to a pre-existing document with identical content
    #[serde(default)]
    pub deduplicated: bool,
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

    let document_id = Uuid::now_v7().to_string();
    let storage_root = std::env::var("AOS_DOCUMENTS_DIR").ok().unwrap_or_else(|| {
        let config = state.config.read().expect("Config lock poisoned");
        config.paths.documents_root.clone()
    });

    // Create tenant-specific document directory
    let tenant_path = PathBuf::from(&storage_root).join(&claims.tenant_id);
    fs::create_dir_all(&tenant_path).await.map_err(db_error)?;

    let mut document_name = String::new();
    let mut file_data: Option<Vec<u8>> = None;
    let mut mime_type = "application/pdf".to_string();

    // Process multipart form
    while let Some(field) = multipart.next_field().await.map_err(bad_request)? {
        let name = field.name().unwrap_or("").to_string();

        match name.as_str() {
            "name" => {
                document_name = field.text().await.map_err(bad_request)?;
            }
            "file" => {
                let file_name = field
                    .file_name()
                    .ok_or_else(|| bad_request("File must have a name"))?
                    .to_string();

                if document_name.is_empty() {
                    document_name = file_name.clone();
                }

                if let Some(ct) = field.content_type() {
                    mime_type = ct.to_string();
                }

                let data = field.bytes().await.map_err(bad_request)?;

                if data.len() > MAX_DOCUMENT_SIZE {
                    return Err(payload_too_large(&format!(
                        "Document exceeds maximum size of {}MB",
                        MAX_DOCUMENT_SIZE / 1024 / 1024
                    )));
                }

                file_data = Some(data.to_vec());
            }
            _ => {
                debug!("Ignoring unknown field: {}", name);
            }
        }
    }

    let file_data = file_data.ok_or_else(|| bad_request("No file uploaded"))?;

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
        .map_err(db_error)?
    {
        info!(
            existing_id = %existing_doc.id,
            hash = %file_hash,
            "Deduplicated document upload - returning existing document"
        );

        // Audit log: document upload deduplicated
        let _ = log_success(
            &state.db,
            &claims,
            actions::DOCUMENT_UPLOAD,
            resources::DOCUMENT,
            Some(&existing_doc.id),
        )
        .await;

        return Ok(Json(DocumentResponse {
            schema_version: "1.0".to_string(),
            document_id: existing_doc.id,
            name: existing_doc.name,
            hash_b3: existing_doc.content_hash,
            size_bytes: existing_doc.file_size,
            mime_type: existing_doc.mime_type,
            storage_path: existing_doc.file_path,
            status: existing_doc.status,
            chunk_count: existing_doc.page_count,
            tenant_id: existing_doc.tenant_id,
            created_at: existing_doc.created_at,
            updated_at: Some(existing_doc.updated_at),
            deduplicated: true,
        }));
    }

    // Save file to disk (only for new documents)
    let file_path = tenant_path.join(format!("{}.pdf", document_id));
    let mut file = fs::File::create(&file_path).await.map_err(db_error)?;

    file.write_all(&file_data).await.map_err(db_error)?;

    file.flush().await.map_err(db_error)?;

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
        .map_err(db_error)?;

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
        deduplicated: false,
    }))
}

/// List documents with pagination
#[utoipa::path(
    get,
    path = "/v1/documents",
    params(
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
    Query(pagination): Query<adapteros_api_types::PaginationParams>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)?;

    let offset = (pagination.page.saturating_sub(1)) * pagination.limit;
    let (documents, total) = state
        .db
        .list_documents_paginated(&claims.tenant_id, pagination.limit as i64, offset as i64)
        .await
        .map_err(db_error)?;

    let data: Vec<DocumentResponse> = documents
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
            deduplicated: false,
        })
        .collect();

    let pages = ((total as f64) / (pagination.limit as f64)).ceil() as u32;
    let response = adapteros_api_types::PaginatedResponse {
        schema_version: adapteros_api_types::API_SCHEMA_VERSION.to_string(),
        data,
        total: total as u64,
        page: pagination.page,
        limit: pagination.limit,
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

    let document = state.db.get_document(&id).await.map_err(db_error)?;

    let document = document.ok_or_else(|| not_found("Document"))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)?;

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
        deduplicated: false,
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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetDelete)?;

    // Get document to find storage path and validate tenant
    let document = state.db.get_document(&id).await.map_err(db_error)?;

    let document = document.ok_or_else(|| not_found("Document"))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)?;

    // Delete from database (cascades to chunks)
    state.db.delete_document(&id).await.map_err(db_error)?;

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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)?;

    // Verify document exists and tenant isolation
    let document = state.db.get_document(&id).await.map_err(db_error)?;

    let document = document.ok_or_else(|| not_found("Document"))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)?;

    let chunks = state.db.get_document_chunks(&id).await.map_err(db_error)?;

    let responses: Vec<ChunkResponse> = chunks
        .into_iter()
        .map(|c| ChunkResponse {
            schema_version: "1.0".to_string(),
            chunk_id: c.id,
            document_id: c.document_id,
            chunk_index: c.chunk_index,
            text: c.text_preview.clone().unwrap_or_default(),
            embedding: None, // TODO: Add embedding field to DB schema
            metadata: None,  // TODO: Add metadata field to DB schema
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
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    // Check permission
    require_permission(&claims, Permission::DatasetView)?;

    // Get document to find storage path and validate tenant
    let document = state.db.get_document(&id).await.map_err(db_error)?;

    let document = document.ok_or_else(|| not_found("Document"))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)?;

    // Read file
    let file_data = fs::read(&document.file_path).await.map_err(db_error)?;

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
    use adapteros_core::B3Hash;
    use adapteros_ingest_docs::{default_ingest_options, DocumentIngestor};

    // Check permission
    require_permission(&claims, Permission::DatasetUpload)?;

    // Get document
    let document = state.db.get_document(&id).await.map_err(db_error)?;
    let document = document.ok_or_else(|| not_found("Document"))?;

    // CRITICAL: Validate tenant isolation
    validate_tenant_isolation(&claims, &document.tenant_id)?;

    // Check if already processed
    if document.status == "indexed" {
        return Err(bad_request("Document is already indexed"));
    }

    // Read document file
    let file_data = fs::read(&document.file_path)
        .await
        .map_err(|e| db_error(format!("Failed to read document file: {}", e)))?;

    // Get embedding model from state
    let embedding_model = state
        .embedding_model
        .as_ref()
        .ok_or_else(|| db_error("Embedding model not configured - enable embeddings feature"))?;

    // Parse document into chunks
    let ingestor = DocumentIngestor::new(default_ingest_options(), None);
    let ingested_doc = if document.mime_type.contains("pdf") {
        ingestor.ingest_pdf_bytes(&file_data, &document.name)
    } else if document.mime_type.contains("markdown") || document.name.ends_with(".md") {
        ingestor.ingest_markdown_bytes(&file_data, &document.name)
    } else {
        return Err(bad_request(&format!(
            "Unsupported document type: {}",
            document.mime_type
        )));
    }
    .map_err(|e| db_error(format!("Failed to parse document: {}", e)))?;

    info!(
        document_id = %id,
        chunks = ingested_doc.chunks.len(),
        "Parsed document into chunks"
    );

    // Create RAG index using the embedding model hash and dimension
    use adapteros_lora_rag::PgVectorIndex;
    let model_hash = embedding_model.model_hash();
    let dimension = embedding_model.dimension();
    let rag_index = PgVectorIndex::new_sqlite(state.db_pool.clone(), model_hash, dimension);

    let mut chunk_count = 0;

    // Process each chunk
    for chunk in &ingested_doc.chunks {
        // Generate chunk UUID for document_chunks table
        let chunk_db_id = Uuid::now_v7().to_string();

        // Generate embedding
        let embedding = embedding_model
            .encode_text(&chunk.text)
            .map_err(|e| db_error(format!("Failed to generate embedding: {}", e)))?;

        // Compute chunk hash
        let chunk_hash = B3Hash::hash(chunk.text.as_bytes()).to_hex();

        // Create text preview (first 200 chars)
        let text_preview = if chunk.text.len() > 200 {
            format!("{}...", &chunk.text[..200])
        } else {
            chunk.text.clone()
        };

        // Store embedding as JSON for document_chunks table
        let embedding_json = serde_json::to_string(&embedding)
            .map_err(|e| db_error(format!("Failed to serialize embedding: {}", e)))?;

        // Insert into document_chunks table (proper FK relationship)
        // Use direct SQL to include embedding_json which isn't in CreateChunkParams
        sqlx::query(
            r#"
            INSERT INTO document_chunks (
                id, document_id, chunk_index, page_number,
                start_offset, end_offset, chunk_hash, text_preview, embedding_json
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)
            "#,
        )
        .bind(&chunk_db_id)
        .bind(&id) // document UUID
        .bind(chunk.chunk_index as i64)
        .bind(chunk.page_number.map(|p| p as i64))
        .bind(chunk.start_offset as i64)
        .bind(chunk.end_offset as i64)
        .bind(&chunk_hash)
        .bind(&text_preview)
        .bind(&embedding_json)
        .execute(&*state.db.pool())
        .await
        .map_err(|e| db_error(format!("Failed to create document chunk: {}", e)))?;

        // Generate RAG doc_id using UUID-based document_id
        // Format: {document_id}__chunk_{index} where document_id is the UUID
        let rag_doc_id = format!("{}__chunk_{}", id, chunk.chunk_index);

        // Insert into rag_documents table for vector search
        rag_index
            .add_document(
                &claims.tenant_id,
                rag_doc_id,
                chunk.text.clone(),
                embedding,
                "v1".to_string(),
                "all".to_string(),
                document.mime_type.clone(),
                None,
            )
            .await
            .map_err(|e| db_error(format!("Failed to index chunk in RAG: {}", e)))?;

        chunk_count += 1;

        debug!(
            document_id = %id,
            chunk_index = chunk.chunk_index,
            chunk_db_id = %chunk_db_id,
            "Indexed chunk"
        );
    }

    // Update document status to indexed
    state
        .db
        .update_document_status(&id, "indexed")
        .await
        .map_err(db_error)?;

    // Update page count if available
    if let Some(page_count) = ingested_doc.page_count {
        sqlx::query("UPDATE documents SET page_count = ? WHERE id = ?")
            .bind(page_count as i64)
            .bind(&id)
            .execute(&*state.db.pool())
            .await
            .map_err(|e| db_error(format!("Failed to update page count: {}", e)))?;
    }

    info!(
        document_id = %id,
        chunk_count = chunk_count,
        "Document processed and indexed successfully"
    );

    // Audit log: document processed
    let _ = log_success(
        &state.db,
        &claims,
        actions::DOCUMENT_UPLOAD,
        resources::DOCUMENT,
        Some(&id),
    )
    .await;

    Ok(Json(ProcessDocumentResponse {
        schema_version: "1.0".to_string(),
        document_id: id,
        status: "indexed".to_string(),
        chunk_count,
        indexed_at: chrono::Utc::now().to_rfc3339(),
    }))
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
    use crate::error_helpers::not_implemented;
    Err::<(), _>(not_implemented(
        "Document processing requires the 'embeddings' feature to be enabled",
    ))
}
