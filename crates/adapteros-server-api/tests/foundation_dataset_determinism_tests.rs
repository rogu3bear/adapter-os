mod common;

use adapteros_core::B3Hash;
use adapteros_db::{CreateChunkParams, CreateDocumentParams};
use adapteros_server_api::services::{
    DatasetFromDocumentIdsParams, DefaultTrainingDatasetService, TrainingDatasetService,
};
use anyhow::anyhow;
use std::sync::Arc;

use common::{test_admin_claims, TestkitEnvGuard};

#[tokio::test]
async fn same_indexed_document_input_produces_same_dataset_hash() -> anyhow::Result<()> {
    let _guard = TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None).await?;
    let claims = test_admin_claims();

    let doc_id = "doc-foundation-determinism";
    state
        .db
        .create_document(CreateDocumentParams {
            id: doc_id.to_string(),
            tenant_id: claims.tenant_id.clone(),
            name: "foundation-determinism".to_string(),
            content_hash: B3Hash::hash(b"foundation-document-body").to_hex(),
            file_path: format!("var/documents/{}/{}.md", claims.tenant_id, doc_id),
            file_size: 128,
            mime_type: "text/markdown".to_string(),
            page_count: Some(1),
        })
        .await?;

    // Insert out-of-order chunk indexes to verify deterministic sorting logic.
    state
        .db
        .create_document_chunk(CreateChunkParams {
            tenant_id: claims.tenant_id.clone(),
            document_id: doc_id.to_string(),
            chunk_index: 1,
            page_number: Some(1),
            start_offset: Some(10),
            end_offset: Some(20),
            chunk_hash: B3Hash::hash(b"chunk-1").to_hex(),
            text_preview: Some("second deterministic chunk".to_string()),
        })
        .await?;

    state
        .db
        .create_document_chunk(CreateChunkParams {
            tenant_id: claims.tenant_id.clone(),
            document_id: doc_id.to_string(),
            chunk_index: 0,
            page_number: Some(1),
            start_offset: Some(0),
            end_offset: Some(9),
            chunk_hash: B3Hash::hash(b"chunk-0").to_hex(),
            text_preview: Some("first deterministic chunk".to_string()),
        })
        .await?;

    state
        .db
        .mark_document_indexed(&claims.tenant_id, doc_id, Some(1))
        .await?;

    let service = DefaultTrainingDatasetService::new(Arc::new(state.clone()));
    let params = DatasetFromDocumentIdsParams {
        document_ids: vec![doc_id.to_string()],
        name: Some("foundation-determinism-dataset".to_string()),
        description: Some("Determinism check for document->dataset conversion".to_string()),
    };

    let first = service
        .create_from_document_ids(&claims, params.clone())
        .await
        .map_err(|(status, body)| {
            anyhow!(
                "first dataset generation failed: status={}, body={:?}",
                status,
                body.0
            )
        })?;

    let second = service
        .create_from_document_ids(&claims, params)
        .await
        .map_err(|(status, body)| {
            anyhow!(
                "second dataset generation failed: status={}, body={:?}",
                status,
                body.0
            )
        })?;

    assert_eq!(
        first.hash, second.hash,
        "dataset hash should be deterministic"
    );
    assert_eq!(
        first.dataset_hash_b3, second.dataset_hash_b3,
        "dataset_hash_b3 should be deterministic"
    );

    Ok(())
}

#[tokio::test]
async fn pending_document_with_chunks_is_accepted_for_dataset_conversion() -> anyhow::Result<()> {
    let _guard = TestkitEnvGuard::enabled(true).await;
    let state = common::setup_state(None).await?;
    let claims = test_admin_claims();

    let doc_id = "doc-foundation-pending-with-chunks";
    state
        .db
        .create_document(CreateDocumentParams {
            id: doc_id.to_string(),
            tenant_id: claims.tenant_id.clone(),
            name: "foundation-pending-with-chunks".to_string(),
            content_hash: B3Hash::hash(b"foundation-pending-doc").to_hex(),
            file_path: format!("var/documents/{}/{}.md", claims.tenant_id, doc_id),
            file_size: 96,
            mime_type: "text/markdown".to_string(),
            page_count: Some(1),
        })
        .await?;

    state
        .db
        .create_document_chunk(CreateChunkParams {
            tenant_id: claims.tenant_id.clone(),
            document_id: doc_id.to_string(),
            chunk_index: 0,
            page_number: Some(1),
            start_offset: Some(0),
            end_offset: Some(20),
            chunk_hash: B3Hash::hash(b"pending-chunk").to_hex(),
            text_preview: Some("pending chunk content".to_string()),
        })
        .await?;

    let service = DefaultTrainingDatasetService::new(Arc::new(state.clone()));
    let converted = service
        .create_from_document_ids(
            &claims,
            DatasetFromDocumentIdsParams {
                document_ids: vec![doc_id.to_string()],
                name: Some("foundation-pending-conversion".to_string()),
                description: Some("accept pending status when chunks already exist".to_string()),
            },
        )
        .await
        .map_err(|(status, body)| {
            anyhow!(
                "dataset conversion failed: status={}, body={:?}",
                status,
                body.0
            )
        })?;

    let dataset_version_id = converted.dataset_version_id.clone().unwrap_or_default();
    if converted.dataset_id.is_empty() || dataset_version_id.is_empty() {
        return Err(anyhow!(
            "dataset conversion returned empty ids: dataset_id='{}' dataset_version_id='{}'",
            converted.dataset_id,
            dataset_version_id
        ));
    }

    Ok(())
}
