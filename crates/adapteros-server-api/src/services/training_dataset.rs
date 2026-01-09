//! Training dataset service
//!
//! Provides reusable helpers to build training datasets from existing
//! documents or collections so HTTP handlers can stay thin.

use crate::audit_helper::{actions, log_success_or_warn, resources};
use crate::citations::build_dataset_index;
#[cfg(feature = "embeddings")]
use crate::error_helpers::payload_too_large;
use crate::error_helpers::{bad_request, db_error, internal_error, not_found};
use crate::handlers::chunked_upload::FileValidator;
use crate::handlers::datasets::{
    bind_dataset_to_tenant, clean_dataset_dir, dataset_quota_limits, ensure_dirs, hash_file,
    map_validation_errors, map_validation_status, quota_error, resolve_dataset_root, DatasetPaths,
    STREAM_BUFFER_SIZE,
};
use crate::security::validate_tenant_isolation;
use crate::state::AppState;
use crate::storage_usage::compute_tenant_storage_usage;
use crate::types::{DatasetResponse, ErrorResponse};
#[cfg(feature = "embeddings")]
use adapteros_core::reject_forbidden_tmp_path;
use adapteros_secure_fs::path_policy::canonicalize_strict_in_allowed_roots;
use async_trait::async_trait;
use axum::body::Bytes;
use axum::http::StatusCode;
use axum::Json;
#[cfg(feature = "embeddings")]
use std::path::Path as StdPath;
use std::path::Path;
#[cfg(feature = "embeddings")]
use std::path::PathBuf;
use std::sync::Arc;
use tokio::fs;
#[cfg(feature = "embeddings")]
use tokio::io::AsyncWriteExt;
#[cfg(feature = "embeddings")]
use tokio::time::{sleep, Duration};
use tracing::{info, warn};
#[cfg(feature = "embeddings")]
use uuid::Uuid;

/// Maximum number of chunks allowed when composing JSONL datasets
const MAX_CHUNKS: usize = 50_000;
/// Maximum file size for generated JSONL (100MB)
const MAX_JSONL_SIZE: i64 = 100 * 1024 * 1024;
#[cfg(feature = "embeddings")]
const EMBEDDING_MAX_RETRIES: usize = 3;
#[cfg(feature = "embeddings")]
const EMBEDDING_BACKOFF_MS: u64 = 200;

/// Parameters for creating a dataset from explicit document IDs
#[derive(Debug, Clone)]
pub struct DatasetFromDocumentIdsParams {
    pub document_ids: Vec<String>,
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Parameters for creating a dataset from a collection
#[derive(Debug, Clone)]
pub struct DatasetFromCollectionParams {
    pub collection_id: String,
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Parameters for creating a dataset from a single already-uploaded document
#[derive(Debug, Clone)]
pub struct DatasetFromUploadedDocumentParams {
    pub document_id: String,
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Parameters for creating a dataset directly from a single uploaded file
#[derive(Debug, Clone)]
pub struct DatasetFromUploadParams {
    pub file_name: String,
    pub mime_type: Option<String>,
    pub data: Bytes,
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Maximum document size (100MB) to keep parity with document upload handler
#[cfg(feature = "embeddings")]
const MAX_DOCUMENT_SIZE: usize = 100 * 1024 * 1024;

#[async_trait]
pub trait TrainingDatasetService: Send + Sync {
    async fn create_from_document_ids(
        &self,
        claims: &crate::auth::Claims,
        params: DatasetFromDocumentIdsParams,
    ) -> Result<DatasetResponse, (StatusCode, Json<ErrorResponse>)>;

    async fn create_from_collection(
        &self,
        claims: &crate::auth::Claims,
        params: DatasetFromCollectionParams,
    ) -> Result<DatasetResponse, (StatusCode, Json<ErrorResponse>)>;

    async fn create_from_uploaded_document(
        &self,
        claims: &crate::auth::Claims,
        params: DatasetFromUploadedDocumentParams,
    ) -> Result<DatasetResponse, (StatusCode, Json<ErrorResponse>)>;

    #[cfg(feature = "embeddings")]
    async fn create_from_upload(
        &self,
        claims: &crate::auth::Claims,
        params: DatasetFromUploadParams,
    ) -> Result<DatasetResponse, (StatusCode, Json<ErrorResponse>)>;
}

pub struct DefaultTrainingDatasetService {
    state: Arc<AppState>,
}

impl DefaultTrainingDatasetService {
    pub fn new(state: Arc<AppState>) -> Self {
        Self { state }
    }

    async fn build_dataset_from_chunks(
        &self,
        claims: &crate::auth::Claims,
        document_ids: &[String],
        dataset_name: String,
        description: Option<String>,
    ) -> Result<DatasetResponse, (StatusCode, Json<ErrorResponse>)> {
        // Retrieve chunks in deterministic order (DB layer enforces tenant_id)
        let chunks = self
            .state
            .db
            .get_chunks_for_documents(&claims.tenant_id, document_ids)
            .await
            .map_err(|e| db_error(format!("Failed to get document chunks: {}", e)))?;

        if chunks.is_empty() {
            return Err(bad_request(
                "No text chunks found in the selected documents",
            ));
        }

        if chunks.len() > MAX_CHUNKS {
            return Err(bad_request(format!(
                "Too many chunks ({}). Maximum allowed is {}. Try selecting fewer documents.",
                chunks.len(),
                MAX_CHUNKS
            )));
        }

        // Build JSONL lines
        let mut jsonl_lines: Vec<String> = Vec::with_capacity(chunks.len());
        for chunk in &chunks {
            if let Some(text) = &chunk.text_preview {
                if !text.trim().is_empty() {
                    let json_obj = serde_json::json!({ "text": text });
                    jsonl_lines.push(json_obj.to_string());
                }
            }
        }

        if jsonl_lines.is_empty() {
            return Err(bad_request(
                "No non-empty text chunks found in the selected documents",
            ));
        }

        let jsonl_content = jsonl_lines.join("\n");
        let content_bytes = jsonl_content.as_bytes();
        let file_size = content_bytes.len() as i64;

        if file_size > MAX_JSONL_SIZE {
            return Err(bad_request(format!(
                "Generated dataset too large ({} bytes). Maximum allowed is {} bytes.",
                file_size, MAX_JSONL_SIZE
            )));
        }

        let content_hash = hash_file(content_bytes);
        let dataset_root = resolve_dataset_root(&self.state).map_err(internal_error)?;
        let dataset_paths = DatasetPaths::new(dataset_root);
        let allowed_roots = [dataset_paths.root().to_path_buf()];
        ensure_dirs([
            dataset_paths.files.as_path(),
            dataset_paths.temp.as_path(),
            dataset_paths.chunked.as_path(),
            dataset_paths.logs.as_path(),
        ])
        .await?;

        // Create dataset record first to get canonical ID
        let dataset_id = self
            .state
            .db
            .create_training_dataset(
                &dataset_name,
                description.as_deref(),
                "jsonl",
                &content_hash,
                "",
                Some(&claims.sub),
                None,
                Some("ready"),
                Some(&content_hash),
                None,
            )
            .await
            .map_err(|e| db_error(format!("Failed to create dataset record: {}", e)))?;

        // Create directory for dataset
        let dataset_path = dataset_paths.dataset_dir(&claims.tenant_id, &dataset_id);
        if let Err(e) = ensure_dirs([dataset_path.as_path()]).await {
            self.cleanup_dataset(&dataset_id, &dataset_path).await;
            return Err(e);
        }
        let dataset_path = canonicalize_strict_in_allowed_roots(&dataset_path, &allowed_roots)
            .map_err(|e| internal_error(format!("Dataset path rejected: {}", e)))?;

        let file_name = "training.jsonl";
        let file_path = dataset_path.join(file_name);

        let (soft_quota, hard_quota) = dataset_quota_limits();
        let usage = compute_tenant_storage_usage(&self.state, &claims.tenant_id)
            .await
            .map_err(|e| internal_error(format!("Failed to compute storage usage: {}", e)))?;
        let predicted_usage = usage.total_bytes().saturating_add(file_size as u64);
        if predicted_usage > hard_quota {
            return Err(quota_error(format!(
                "Dataset storage quota exceeded: {} > {} bytes",
                predicted_usage, hard_quota
            )));
        }
        if predicted_usage > soft_quota {
            warn!(
                tenant_id = %claims.tenant_id,
                predicted_usage,
                soft_quota,
                "Dataset storage soft quota exceeded"
            );
        }

        // Write JSONL file
        if let Err(e) = fs::write(&file_path, content_bytes).await {
            self.cleanup_dataset(&dataset_id, &dataset_path).await;
            return Err(internal_error(format!(
                "Failed to write dataset file: {}",
                e
            )));
        }

        // Update storage path now that the file is written
        if let Err(e) = self
            .state
            .db
            .update_dataset_storage_path(&dataset_id, &dataset_path.to_string_lossy())
            .await
        {
            self.cleanup_dataset(&dataset_id, &dataset_path).await;
            return Err(db_error(format!("Failed to update storage path: {}", e)));
        }

        // Bind dataset to tenant for isolation
        if let Err(e) = bind_dataset_to_tenant(&self.state.db, &dataset_id, &claims.tenant_id).await
        {
            self.cleanup_dataset(&dataset_id, &dataset_path).await;
            return Err(e);
        }

        // Add file record
        if let Err(e) = self
            .state
            .db
            .add_dataset_file(
                &dataset_id,
                file_name,
                &file_path.to_string_lossy(),
                file_size,
                &content_hash,
                Some("application/jsonl"),
            )
            .await
        {
            self.cleanup_dataset(&dataset_id, &dataset_path).await;
            return Err(db_error(format!("Failed to add file record: {}", e)));
        }

        // Validate generated JSONL
        let validation_result =
            FileValidator::quick_validate(&file_path, "jsonl", STREAM_BUFFER_SIZE).await;
        let (validation_status, validation_errors) = match validation_result {
            Ok(()) => ("valid".to_string(), None),
            Err(e) => ("invalid".to_string(), Some(e.to_string())),
        };

        self.state
            .db
            .update_dataset_validation(
                &dataset_id,
                &validation_status,
                validation_errors.as_deref(),
            )
            .await
            .map_err(|e| db_error(format!("Failed to update validation status: {}", e)))?;

        // Create initial dataset version
        let dataset_version_id = self
            .state
            .db
            .create_training_dataset_version(
                &dataset_id,
                Some(&claims.tenant_id),
                None, // version_label
                &dataset_path.to_string_lossy(),
                &content_hash,
                None, // manifest_path
                None, // manifest_json
                Some(&claims.sub),
            )
            .await
            .map_err(|e| db_error(format!("Failed to create dataset version: {}", e)))?;

        // Derive trust_state from the newly created version for consistency with list/get endpoints
        let trust_state = self
            .state
            .db
            .get_latest_trusted_dataset_version_for_dataset(&dataset_id)
            .await
            .map_err(|e| db_error(format!("Failed to get trust state: {}", e)))?
            .map(|(_, trust)| trust);

        let response_validation_status = map_validation_status(&validation_status);
        let response_validation_errors = map_validation_errors(validation_errors);
        let now = chrono::Utc::now().to_rfc3339();

        // Audit log
        log_success_or_warn(
            &self.state.db,
            claims,
            actions::DATASET_CREATE,
            resources::DATASET,
            Some(&dataset_id),
        )
        .await;

        // Build citation index for training files (best-effort)
        if let Err(e) = build_dataset_index(&self.state, &dataset_id, &claims.tenant_id).await {
            warn!(
                dataset_id = %dataset_id,
                error = %e,
                "Failed to build dataset citation index (document-derived)"
            );
        }

        info!(
            dataset_id = %dataset_id,
            name = %dataset_name,
            chunks = chunks.len(),
            lines = jsonl_lines.len(),
            size_bytes = file_size,
            "Created dataset from documents"
        );

        Ok(DatasetResponse {
            schema_version: "1.0".to_string(),
            dataset_id,
            dataset_version_id: Some(dataset_version_id),
            name: dataset_name,
            description,
            file_count: 1,
            total_size_bytes: file_size,
            format: "jsonl".to_string(),
            hash: content_hash.clone(),
            dataset_hash_b3: Some(content_hash),
            storage_path: dataset_path.to_string_lossy().to_string(),
            status: "ready".to_string(),
            workspace_id: None,
            validation_status: response_validation_status,
            validation_errors: response_validation_errors,
            trust_state,
            created_by: claims.sub.clone(),
            created_at: now.clone(),
            updated_at: now,
        })
    }

    async fn cleanup_dataset(&self, dataset_id: &str, dataset_path: &Path) {
        clean_dataset_dir(dataset_path).await;
        if let Err(cleanup_err) = self.state.db.delete_training_dataset(dataset_id).await {
            warn!(
                dataset_id = %dataset_id,
                error = %cleanup_err,
                "Failed to cleanup orphaned dataset record"
            );
        }
    }

    #[cfg(feature = "embeddings")]
    async fn process_and_index_document(
        &self,
        claims: &crate::auth::Claims,
        document_id: &str,
        document_name: &str,
        mime_type: &str,
        file_data: &[u8],
    ) -> Result<(), (StatusCode, Json<ErrorResponse>)> {
        use adapteros_core::B3Hash;
        use adapteros_ingest_docs::{default_ingest_options, DocumentIngestor};

        // Get embedding model from state
        let embedding_model = self.state.embedding_model.as_ref().ok_or_else(|| {
            db_error("Embedding model not configured - enable embeddings feature")
        })?;

        let ingestor = DocumentIngestor::new(default_ingest_options(), None);
        let ingested_doc = if mime_type.contains("pdf") {
            ingestor.ingest_pdf_bytes(file_data, document_name)
        } else if mime_type.contains("markdown") || document_name.ends_with(".md") {
            ingestor.ingest_markdown_bytes(file_data, document_name)
        } else {
            return Err(bad_request(&format!(
                "Unsupported document type: {}",
                mime_type
            )));
        }
        .map_err(|e| db_error(format!("Failed to parse document: {}", e)))?;

        info!(
            document_id = %document_id,
            chunks = ingested_doc.chunks.len(),
            "Parsed document into chunks"
        );

        let model_hash = embedding_model.model_hash();
        let dimension = embedding_model.dimension();
        let mut failed_embeddings = 0usize;

        for chunk in &ingested_doc.chunks {
            let chunk_db_id = Uuid::now_v7().to_string();
            let embedding = embed_chunk_with_backoff(embedding_model, &chunk.text).await;
            let chunk_hash = B3Hash::hash(chunk.text.as_bytes()).to_hex();
            let text_preview = if chunk.text.len() > 200 {
                format!("{}...", &chunk.text[..200])
            } else {
                chunk.text.clone()
            };
            let (embedding_json, rag_embedding) = match embedding {
                Ok(vector) => {
                    let serialized = serde_json::to_string(&vector)
                        .map_err(|e| db_error(format!("Failed to serialize embedding: {}", e)))?;
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
            .execute(&*self.state.db.pool())
            .await
            .map_err(|e| db_error(format!("Failed to create document chunk: {}", e)))?;

            if let Some(rag_embedding) = rag_embedding {
                let rag_doc_id = format!("{}__chunk_{}", document_id, chunk.chunk_index);
                self.state
                    .db
                    .upsert_rag_document(adapteros_db::rag::RagDocumentWrite {
                        tenant_id: claims.tenant_id.clone(),
                        doc_id: rag_doc_id,
                        text: chunk.text.clone(),
                        embedding: rag_embedding,
                        rev: "v1".to_string(),
                        effectivity: "all".to_string(),
                        source_type: mime_type.to_string(),
                        superseded_by: None,
                        embedding_model_hash: model_hash,
                        embedding_dimension: dimension,
                    })
                    .await
                    .map_err(|e| db_error(format!("Failed to index chunk in RAG: {}", e)))?;
            }
        }

        if failed_embeddings > 0 {
            warn!(
                document_id = %document_id,
                failed_chunks = failed_embeddings,
                "Some chunk embeddings failed; stored failed_embedding markers and continued"
            );
        }

        self.state
            .db
            .update_document_status(document_id, "indexed")
            .await
            .map_err(db_error)?;

        if let Some(page_count) = ingested_doc.page_count {
            sqlx::query("UPDATE documents SET page_count = ? WHERE id = ?")
                .bind(page_count as i64)
                .bind(document_id)
                .execute(&*self.state.db.pool())
                .await
                .map_err(|e| db_error(format!("Failed to update page count: {}", e)))?;
        }

        info!(
            document_id = %document_id,
            chunk_count = ingested_doc.chunks.len(),
            "Document processed and indexed successfully"
        );

        log_success_or_warn(
            &self.state.db,
            claims,
            actions::DOCUMENT_UPLOAD,
            resources::DOCUMENT,
            Some(document_id),
        )
        .await;

        Ok(())
    }
}

#[async_trait]
impl TrainingDatasetService for DefaultTrainingDatasetService {
    async fn create_from_document_ids(
        &self,
        claims: &crate::auth::Claims,
        params: DatasetFromDocumentIdsParams,
    ) -> Result<DatasetResponse, (StatusCode, Json<ErrorResponse>)> {
        if params.document_ids.is_empty() {
            return Err(bad_request(
                "Must provide at least one document_id or document_ids entry",
            ));
        }

        let mut document_ids = params
            .document_ids
            .into_iter()
            .filter(|id| !id.trim().is_empty())
            .collect::<Vec<_>>();

        document_ids.sort();
        document_ids.dedup();

        if document_ids.is_empty() {
            return Err(bad_request(
                "Must provide at least one non-empty document_id",
            ));
        }

        // Validate documents and derive default name
        let mut doc_names = Vec::new();
        for doc_id in &document_ids {
            let doc = self
                .state
                .db
                .get_document(&claims.tenant_id, doc_id)
                .await
                .map_err(|e| db_error(format!("Failed to get document: {}", e)))?
                .ok_or_else(|| not_found("Document"))?;

            // Tenant isolation check
            validate_tenant_isolation(claims, &doc.tenant_id)?;

            // Ensure document is indexed
            if doc.status != "indexed" {
                return Err(bad_request(format!(
                    "Document must be indexed before conversion. Current status: {}",
                    doc.status
                )));
            }

            doc_names.push(doc.name);
        }

        let default_name = if doc_names.len() == 1 {
            format!("Training from doc: {}", doc_names[0])
        } else {
            format!("Training from {} documents", doc_names.len())
        };

        let dataset_name = params.name.unwrap_or(default_name);

        self.build_dataset_from_chunks(claims, &document_ids, dataset_name, params.description)
            .await
    }

    async fn create_from_collection(
        &self,
        claims: &crate::auth::Claims,
        params: DatasetFromCollectionParams,
    ) -> Result<DatasetResponse, (StatusCode, Json<ErrorResponse>)> {
        let collection = self
            .state
            .db
            .get_collection(&claims.tenant_id, &params.collection_id)
            .await
            .map_err(|e| db_error(format!("Failed to get collection: {}", e)))?
            .ok_or_else(|| not_found("Collection"))?;

        validate_tenant_isolation(claims, &collection.tenant_id)?;

        // Get documents in collection
        let docs = self
            .state
            .db
            .get_collection_documents(&claims.tenant_id, &params.collection_id)
            .await
            .map_err(|e| db_error(format!("Failed to get collection documents: {}", e)))?;

        if docs.is_empty() {
            return Err(bad_request("Collection is empty - no documents to convert"));
        }

        // Filter to indexed documents and sort deterministically
        let mut indexed_docs: Vec<_> = docs.into_iter().filter(|d| d.status == "indexed").collect();
        indexed_docs.sort_by(|a, b| a.id.cmp(&b.id));

        if indexed_docs.is_empty() {
            return Err(bad_request(
                "No indexed documents in collection. Documents must be indexed before conversion.",
            ));
        }

        let document_ids: Vec<String> = indexed_docs.iter().map(|d| d.id.clone()).collect();
        let dataset_name = params
            .name
            .unwrap_or_else(|| format!("Training from collection: {}", collection.name));

        self.build_dataset_from_chunks(claims, &document_ids, dataset_name, params.description)
            .await
    }

    async fn create_from_uploaded_document(
        &self,
        claims: &crate::auth::Claims,
        params: DatasetFromUploadedDocumentParams,
    ) -> Result<DatasetResponse, (StatusCode, Json<ErrorResponse>)> {
        // Ensure document exists and is indexed before creating dataset
        let doc = self
            .state
            .db
            .get_document(&claims.tenant_id, &params.document_id)
            .await
            .map_err(|e| db_error(format!("Failed to get document: {}", e)))?
            .ok_or_else(|| not_found("Document"))?;

        validate_tenant_isolation(claims, &doc.tenant_id)?;

        if doc.status != "indexed" {
            return Err(bad_request(format!(
                "Document must be indexed before conversion. Current status: {}",
                doc.status
            )));
        }

        self.create_from_document_ids(
            claims,
            DatasetFromDocumentIdsParams {
                document_ids: vec![params.document_id],
                name: params.name,
                description: params.description,
            },
        )
        .await
    }

    #[cfg(feature = "embeddings")]
    async fn create_from_upload(
        &self,
        claims: &crate::auth::Claims,
        params: DatasetFromUploadParams,
    ) -> Result<DatasetResponse, (StatusCode, Json<ErrorResponse>)> {
        if params.data.is_empty() {
            return Err(bad_request("No file uploaded"));
        }

        if params.data.len() > MAX_DOCUMENT_SIZE {
            return Err(payload_too_large(&format!(
                "Document exceeds maximum size of {}MB",
                MAX_DOCUMENT_SIZE / 1024 / 1024
            )));
        }

        let document_id = Uuid::now_v7().to_string();
        let storage_root = std::env::var("AOS_DOCUMENTS_DIR").ok().unwrap_or_else(|| {
            let config = self.state.config.read().map_err(|_| {
                tracing::error!("Config lock poisoned");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(
                        ErrorResponse::new("config lock poisoned").with_code("CONFIG_UNAVAILABLE"),
                    ),
                )
            })?;
            config.paths.documents_root.clone()
        });

        let root = PathBuf::from(&storage_root);
        let root = if root.is_absolute() {
            root
        } else {
            std::env::current_dir()
                .unwrap_or_else(|_| StdPath::new("/").to_path_buf())
                .join(root)
        };
        reject_forbidden_tmp_path(&root, "documents-root").map_err(internal_error)?;

        let tenant_path = root.join(&claims.tenant_id);
        fs::create_dir_all(&tenant_path).await.map_err(db_error)?;

        let mut document_name = params
            .name
            .clone()
            .or_else(|| Some(params.file_name.clone()))
            .unwrap_or_else(|| format!("Document {}", &document_id[0..8]));

        let mime_type = params
            .mime_type
            .clone()
            .unwrap_or_else(|| "application/octet-stream".to_string());

        use adapteros_core::B3Hash;
        let file_hash = B3Hash::hash(&params.data).to_hex();

        if let Some(existing_doc) = self
            .state
            .db
            .find_document_by_content_hash(&claims.tenant_id, &file_hash)
            .await
            .map_err(db_error)?
        {
            validate_tenant_isolation(claims, &existing_doc.tenant_id)?;

            // Process if not already indexed
            if existing_doc.status != "indexed" {
                let file_bytes = fs::read(&existing_doc.file_path)
                    .await
                    .map_err(|e| db_error(format!("Failed to read document file: {}", e)))?;

                self.process_and_index_document(
                    claims,
                    &existing_doc.id,
                    &existing_doc.name,
                    &existing_doc.mime_type,
                    &file_bytes,
                )
                .await?;
            }

            return self
                .create_from_document_ids(
                    claims,
                    DatasetFromDocumentIdsParams {
                        document_ids: vec![existing_doc.id.clone()],
                        name: params.name,
                        description: params.description,
                    },
                )
                .await;
        }

        // Choose file extension for storage (do not rely on user-provided path)
        let ext = if mime_type.contains("markdown") || params.file_name.ends_with(".md") {
            "md"
        } else if mime_type.contains("pdf") {
            "pdf"
        } else {
            "bin"
        };

        let file_path = tenant_path.join(format!("{}.{}", document_id, ext));

        let mut file = fs::File::create(&file_path).await.map_err(db_error)?;
        file.write_all(&params.data).await.map_err(db_error)?;
        file.flush().await.map_err(db_error)?;

        if document_name.is_empty() {
            document_name = format!("Document {}", &document_id[0..8]);
        }

        use adapteros_db::documents::CreateDocumentParams;
        self.state
            .db
            .create_document(CreateDocumentParams {
                id: document_id.clone(),
                tenant_id: claims.tenant_id.clone(),
                name: document_name.clone(),
                content_hash: file_hash.clone(),
                file_path: file_path.to_string_lossy().to_string(),
                file_size: params.data.len() as i64,
                mime_type: mime_type.clone(),
                page_count: None,
            })
            .await
            .map_err(db_error)?;

        info!(
            "Uploaded document {} ({} bytes) for tenant {}",
            document_id,
            params.data.len(),
            claims.tenant_id
        );

        log_success_or_warn(
            &self.state.db,
            claims,
            actions::DOCUMENT_UPLOAD,
            resources::DOCUMENT,
            Some(&document_id),
        )
        .await;

        self.process_and_index_document(
            claims,
            &document_id,
            &document_name,
            &mime_type,
            &params.data,
        )
        .await?;

        self.create_from_document_ids(
            claims,
            DatasetFromDocumentIdsParams {
                document_ids: vec![document_id],
                name: params.name,
                description: params.description,
            },
        )
        .await
    }
}

#[cfg(feature = "embeddings")]
async fn embed_chunk_with_backoff(
    embedding_model: &Arc<dyn adapteros_ingest_docs::EmbeddingModel>,
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
