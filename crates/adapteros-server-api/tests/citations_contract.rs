use adapteros_db::training_datasets::CreateTrainingDatasetRowParams;
use adapteros_server_api::citations::{
    collect_citations_for_adapters, collect_document_links_for_adapters,
};

mod common;

#[tokio::test]
async fn citations_use_existing_training_dataset_row_columns() {
    let state = common::setup_state(None).await.expect("state");

    let tenant_id = "tenant-1";
    let adapter_id = "adapter-citations-1";
    let dataset_id = "dst-citations-1";
    let dataset_version_id = "dsv-citations-1";

    state
        .db
        .create_training_dataset_with_id(
            dataset_id,
            "citations dataset",
            Some("deterministic fixture"),
            "jsonl",
            "hash-citations-1",
            "var/datasets/citations",
            Some("tenant-1-user"),
            None,
            Some("ready"),
            Some("hash-citations-1"),
            None,
        )
        .await
        .expect("dataset should be created");

    state
        .db
        .create_training_dataset_version_with_id(
            dataset_version_id,
            dataset_id,
            Some(tenant_id),
            Some("v1"),
            "var/datasets/citations/v1",
            "hash-citations-1",
            None,
            None,
            Some("tenant-1-user"),
        )
        .await
        .expect("dataset version should be created");

    let mut row = CreateTrainingDatasetRowParams::new(
        dataset_id,
        "Prompt preview from fixture",
        "Response preview from fixture",
    );
    row.dataset_version_id = Some(dataset_version_id.to_string());
    row.source_file = Some("docs/source_a.md".to_string());
    row.source_line = Some(42);
    row.tenant_id = Some(tenant_id.to_string());

    let row_id = state
        .db
        .insert_training_dataset_row(&row)
        .await
        .expect("dataset row should be inserted");

    state
        .db
        .record_adapter_training_lineage(
            adapter_id,
            dataset_id,
            Some(dataset_version_id),
            None,
            Some("hash-citations-1"),
            Some(tenant_id),
        )
        .await
        .expect("lineage should be recorded");

    let citations = collect_citations_for_adapters(
        &state,
        tenant_id,
        &[adapter_id.to_string()],
        "unused query",
        5,
    )
    .await;

    assert_eq!(citations.len(), 1, "one citation should be produced");
    let citation = &citations[0];
    assert_eq!(citation.adapter_id, adapter_id);
    assert_eq!(citation.file_path, "docs/source_a.md");
    assert_eq!(citation.chunk_id, format!("chunk_{}", row_id));
    assert_eq!(citation.offset_start, 42);
    assert_eq!(citation.offset_end, 43);
    assert_eq!(citation.preview, "Prompt preview from fixture");
    assert!(
        citation
            .citation_id
            .as_ref()
            .is_some_and(|id| !id.is_empty()),
        "citation id should be computed from content hash"
    );
}

#[tokio::test]
async fn document_links_use_dataset_metadata_document_ids() {
    let state = common::setup_state(None).await.expect("state");

    let tenant_id = "tenant-1";
    let adapter_id = "adapter-doclinks-1";
    let dataset_id = "dst-doclinks-1";
    let dataset_version_id = "dsv-doclinks-1";
    let document_id = "doc-doclinks-1";

    state
        .db
        .create_document(adapteros_db::documents::CreateDocumentParams {
            id: document_id.to_string(),
            tenant_id: tenant_id.to_string(),
            name: "Source Spec A".to_string(),
            content_hash: "hash-doclinks-1".to_string(),
            file_path: "var/datasets/doclinks/source-spec-a.txt".to_string(),
            file_size: 128,
            mime_type: "text/plain".to_string(),
            page_count: None,
        })
        .await
        .expect("document should be created");

    state
        .db
        .create_training_dataset_with_id(
            dataset_id,
            "doclinks dataset",
            Some("deterministic fixture"),
            "jsonl",
            "hash-doclinks-1",
            "var/datasets/doclinks",
            Some("tenant-1-user"),
            None,
            Some("ready"),
            Some("hash-doclinks-1"),
            None,
        )
        .await
        .expect("dataset should be created");

    state
        .db
        .create_training_dataset_version_with_id(
            dataset_version_id,
            dataset_id,
            Some(tenant_id),
            Some("v1"),
            "var/datasets/doclinks/v1",
            "hash-doclinks-1",
            None,
            None,
            Some("tenant-1-user"),
        )
        .await
        .expect("dataset version should be created");

    let mut row = CreateTrainingDatasetRowParams::new(
        dataset_id,
        "Prompt preview from fixture",
        "Response preview from fixture",
    );
    row.dataset_version_id = Some(dataset_version_id.to_string());
    row.source_file = Some("training.jsonl".to_string());
    row.source_line = Some(3);
    row.tenant_id = Some(tenant_id.to_string());
    row.metadata_json = Some(
        serde_json::json!({
            "source_document_id": document_id,
            "source_document_name": "Source Spec A",
            "source_chunk_index": 0
        })
        .to_string(),
    );

    state
        .db
        .insert_training_dataset_row(&row)
        .await
        .expect("dataset row should be inserted");

    state
        .db
        .record_adapter_training_lineage(
            adapter_id,
            dataset_id,
            Some(dataset_version_id),
            None,
            Some("hash-doclinks-1"),
            Some(tenant_id),
        )
        .await
        .expect("lineage should be recorded");

    let links =
        collect_document_links_for_adapters(&state, tenant_id, &[adapter_id.to_string()], 5).await;

    assert_eq!(links.len(), 1, "one document link should be produced");
    let link = &links[0];
    assert_eq!(link.document_id, document_id);
    assert_eq!(link.document_name, "Source Spec A");
    assert_eq!(
        link.download_url,
        format!("/v1/documents/{}/download", document_id)
    );
    assert_eq!(link.adapter_id.as_deref(), Some(adapter_id));
    assert_eq!(link.dataset_version_id.as_deref(), Some(dataset_version_id));
    assert_eq!(link.source_file.as_deref(), Some("training.jsonl"));
}
