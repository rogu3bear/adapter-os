//! Tests for training dataset creation from single upload
//!
//! These tests validate size limits, tenant isolation, and basic validation
//! for the /v1/training/datasets/from-upload flow.

#![cfg(feature = "embeddings")]

mod common;

use std::sync::Arc;

use adapteros_core::{B3Hash, Result};
use adapteros_retrieval::rag::EmbeddingModel;
use adapteros_server_api::services::{
    DatasetFromDocumentIdsParams, DatasetFromUploadParams, DefaultTrainingDatasetService,
};
use adapteros_server_api::types::ErrorResponse;
use axum::{http::StatusCode, response::Json};
use bytes::Bytes;

/// Simple embedding model stub for tests
struct MockEmbeddingModel {
    dim: usize,
}

impl EmbeddingModel for MockEmbeddingModel {
    fn encode_text(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.0; self.dim])
    }

    fn model_hash(&self) -> B3Hash {
        B3Hash::hash(b"mock_embedding")
    }

    fn dimension(&self) -> usize {
        self.dim
    }
}

fn mock_embedding() -> Arc<dyn EmbeddingModel + Send + Sync> {
    Arc::new(MockEmbeddingModel { dim: 4 })
}

#[tokio::test]
async fn dataset_upload_respects_size_limit() {
    let mut state = common::setup_state(None).await.unwrap();
    state = state.with_embedding_model(mock_embedding());
    let state = Arc::new(state);
    let service = DefaultTrainingDatasetService::new(state.clone());

    let claims = common::test_admin_claims();
    let too_big = Bytes::from(vec![0u8; 101 * 1024 * 1024]);

    let result = service
        .create_from_upload(
            &claims,
            DatasetFromUploadParams {
                file_name: "big.md".to_string(),
                mime_type: Some("text/markdown".to_string()),
                data: too_big,
                name: Some("big-dataset".to_string()),
                description: None,
            },
        )
        .await;

    match result {
        Err((code, Json(ErrorResponse { .. }))) => {
            assert_eq!(code, StatusCode::PAYLOAD_TOO_LARGE);
        }
        _ => panic!("Expected payload too large error"),
    }
}

#[tokio::test]
async fn dataset_upload_enforces_tenant_isolation() {
    let mut state = common::setup_state(None).await.unwrap();
    state = state.with_embedding_model(mock_embedding());
    let state = Arc::new(state);
    let service = DefaultTrainingDatasetService::new(state.clone());

    // Tenant 1 uploads and processes a document
    let claims_t1 = common::test_admin_claims();
    let _dataset = service
        .create_from_upload(
            &claims_t1,
            DatasetFromUploadParams {
                file_name: "readme.md".to_string(),
                mime_type: Some("text/markdown".to_string()),
                data: Bytes::from_static(b"# Hello\n\nThis is a test document."),
                name: Some("tenant1-dataset".to_string()),
                description: None,
            },
        )
        .await
        .expect("tenant1 upload should succeed");

    // Fetch document id for tenant 1
    let (docs, _) = state
        .db
        .list_documents_paginated(&claims_t1.tenant_id, 10, 0)
        .await
        .expect("list documents");
    let doc_id = docs.first().expect("doc exists").id.clone();

    // Tenant 2 should not be able to convert tenant 1's document
    let claims_t2 = common::test_viewer_claims();
    let err = service
        .create_from_document_ids(
            &claims_t2,
            DatasetFromDocumentIdsParams {
                document_ids: vec![doc_id],
                name: Some("cross-tenant".to_string()),
                description: None,
            },
        )
        .await
        .expect_err("cross-tenant dataset creation should fail");

    assert_eq!(err.0, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn dataset_upload_requires_file() {
    let mut state = common::setup_state(None).await.unwrap();
    state = state.with_embedding_model(mock_embedding());
    let state = Arc::new(state);
    let service = DefaultTrainingDatasetService::new(state.clone());

    let claims = common::test_admin_claims();

    let result = service
        .create_from_upload(
            &claims,
            DatasetFromUploadParams {
                file_name: "empty.md".to_string(),
                mime_type: Some("text/markdown".to_string()),
                data: Bytes::new(),
                name: Some("empty-dataset".to_string()),
                description: None,
            },
        )
        .await;

    match result {
        Err((code, _)) => assert_eq!(code, StatusCode::BAD_REQUEST),
        _ => panic!("Expected bad request for empty upload"),
    }
}
