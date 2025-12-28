//! Handler for creating datasets from existing documents.

use super::types::CreateDatasetFromDocumentsRequest;
use crate::auth::Claims;
use crate::error_helpers::bad_request;
use crate::permissions::{require_permission, Permission};
use crate::services::{
    DatasetFromCollectionParams, DatasetFromDocumentIdsParams, DefaultTrainingDatasetService,
    TrainingDatasetService,
};
use crate::state::AppState;
use crate::types::{DatasetResponse, ErrorResponse};
use axum::{extract::State, http::StatusCode, response::IntoResponse, Extension, Json};
use std::sync::Arc;

/// Create a training dataset from existing documents or a document collection
///
/// Converts RAG documents into JSONL training format. Either `document_id` or
/// `collection_id` must be provided (mutually exclusive). The resulting dataset
/// is immediately marked as valid since the source documents are already indexed.
///
/// The JSONL format is: `{"text": "<chunk_text>"}` for each chunk, ordered
/// deterministically by (document_id ASC, chunk_index ASC) for reproducibility.
#[utoipa::path(
    post,
    path = "/v1/datasets/from-documents",
    request_body = CreateDatasetFromDocumentsRequest,
    responses(
        (status = 200, description = "Dataset created successfully", body = DatasetResponse),
        (status = 400, description = "Invalid request - must provide exactly one of document_id or collection_id"),
        (status = 403, description = "Tenant isolation violation"),
        (status = 404, description = "Document or collection not found"),
        (status = 500, description = "Internal server error")
    ),
    tag = "datasets"
)]
pub async fn create_dataset_from_documents(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<CreateDatasetFromDocumentsRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<ErrorResponse>)> {
    require_permission(&claims, Permission::DatasetUpload)?;

    // Validate exclusivity: only one source allowed
    let multiple_sources = request.collection_id.is_some()
        && (request.document_id.is_some() || request.document_ids.is_some());
    if multiple_sources {
        return Err(bad_request(
            "Cannot specify both document_id/document_ids and collection_id. Provide exactly one.",
        ));
    }

    let service = DefaultTrainingDatasetService::new(Arc::new(state.clone()));

    let dataset = match (
        request.document_ids,
        request.document_id,
        request.collection_id,
    ) {
        (Some(document_ids), None, None) => {
            service
                .create_from_document_ids(
                    &claims,
                    DatasetFromDocumentIdsParams {
                        document_ids,
                        name: request.name,
                        description: request.description,
                    },
                )
                .await?
        }
        (None, Some(document_id), None) => {
            service
                .create_from_document_ids(
                    &claims,
                    DatasetFromDocumentIdsParams {
                        document_ids: vec![document_id],
                        name: request.name,
                        description: request.description,
                    },
                )
                .await?
        }
        (None, None, Some(collection_id)) => {
            service
                .create_from_collection(
                    &claims,
                    DatasetFromCollectionParams {
                        collection_id,
                        name: request.name,
                        description: request.description,
                    },
                )
                .await?
        }
        (None, None, None) => {
            return Err(bad_request(
                "Must provide either document_id, document_ids, or collection_id",
            ));
        }
        _ => {
            return Err(bad_request(
                "Cannot specify both document_id/document_ids and collection_id. Provide exactly one.",
            ));
        }
    };

    Ok(Json(dataset))
}
