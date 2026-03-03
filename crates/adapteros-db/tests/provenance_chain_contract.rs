use adapteros_db::{get_provenance_chain, Db};

#[tokio::test]
async fn provenance_chain_uses_migrated_json_token_columns() -> anyhow::Result<()> {
    let db = Db::new_in_memory().await?;

    sqlx::query("INSERT INTO tenants (id, name) VALUES (?, ?)")
        .bind("tenant-provenance")
        .bind("Provenance Tenant")
        .execute(db.pool_result()?)
        .await?;

    // Verify migrated token contract columns are present.
    let token_columns: Vec<String> =
        sqlx::query_scalar("SELECT name FROM pragma_table_info('inference_trace_tokens')")
            .fetch_all(db.pool_result()?)
            .await?;
    assert!(token_columns.iter().any(|c| c == "selected_adapter_ids"));
    assert!(token_columns.iter().any(|c| c == "gates_q15"));
    assert!(!token_columns.iter().any(|c| c == "adapter_ids_blob"));
    assert!(!token_columns.iter().any(|c| c == "gates_blob"));

    sqlx::query(
        "INSERT INTO training_datasets (id, name, format, hash_b3, storage_path, validation_status)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("dataset-1")
    .bind("Dataset 1")
    .bind("jsonl")
    .bind("dataset-hash")
    .bind("/tmp/dataset-1")
    .bind("valid")
    .execute(db.pool_result()?)
    .await?;

    sqlx::query(
        "INSERT INTO training_dataset_versions (id, dataset_id, tenant_id, version_number, storage_path, hash_b3)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("dataset-version-1")
    .bind("dataset-1")
    .bind("tenant-provenance")
    .bind(1_i64)
    .bind("/tmp/dataset-1/v1")
    .bind("dataset-version-hash")
    .execute(db.pool_result()?)
    .await?;

    sqlx::query(
        "INSERT INTO adapter_training_lineage
         (id, adapter_id, dataset_id, dataset_version_id, tenant_id)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("lineage-a")
    .bind("adapter-a")
    .bind("dataset-1")
    .bind("dataset-version-1")
    .bind("tenant-provenance")
    .execute(db.pool_result()?)
    .await?;

    sqlx::query(
        "INSERT INTO adapter_training_lineage
         (id, adapter_id, dataset_id, dataset_version_id, tenant_id)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("lineage-b")
    .bind("adapter-b")
    .bind("dataset-1")
    .bind("dataset-version-1")
    .bind("tenant-provenance")
    .execute(db.pool_result()?)
    .await?;

    sqlx::query(
        "INSERT INTO training_dataset_rows
         (id, dataset_id, dataset_version_id, prompt, response, content_hash_b3, source_file, tenant_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind("row-1")
    .bind("dataset-1")
    .bind("dataset-version-1")
    .bind("prompt")
    .bind("response")
    .bind("row-hash")
    .bind("src/main.rs")
    .bind("tenant-provenance")
    .execute(db.pool_result()?)
    .await?;

    sqlx::query(
        "INSERT INTO inference_traces
         (trace_id, tenant_id, request_id, context_digest, created_at, status)
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind("trace-1")
    .bind("tenant-provenance")
    .bind("req-1")
    .bind(vec![0xAA_u8; 32])
    .bind("2026-02-20T00:00:00Z")
    .bind("completed")
    .execute(db.pool_result()?)
    .await?;

    sqlx::query(
        "INSERT INTO inference_trace_tokens
         (trace_id, token_index, selected_adapter_ids, gates_q15, decision_hash)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("trace-1")
    .bind(0_i64)
    .bind(r#"["adapter-a","adapter-b"]"#)
    .bind("[100,300]")
    .bind(vec![0x11_u8; 32])
    .execute(db.pool_result()?)
    .await?;

    sqlx::query(
        "INSERT INTO inference_trace_tokens
         (trace_id, token_index, selected_adapter_ids, gates_q15, decision_hash)
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind("trace-1")
    .bind(1_i64)
    .bind(r#"["adapter-a"]"#)
    .bind("[200]")
    .bind(vec![0x22_u8; 32])
    .execute(db.pool_result()?)
    .await?;

    let chain = get_provenance_chain(&db, "trace-1").await?;

    assert_eq!(chain.trace_id, "trace-1");
    assert_eq!(chain.tenant_id, "tenant-provenance");
    assert_eq!(chain.request_id.as_deref(), Some("req-1"));
    assert!(chain.is_complete);
    assert!(chain.warnings.is_empty());
    assert_eq!(chain.source_documents.len(), 1);
    assert_eq!(chain.source_documents[0].source_file, "src/main.rs");

    assert_eq!(chain.adapters_used.len(), 2);
    let adapter_a = chain
        .adapters_used
        .iter()
        .find(|a| a.adapter_id == "adapter-a")
        .expect("adapter-a should be present");
    let adapter_b = chain
        .adapters_used
        .iter()
        .find(|a| a.adapter_id == "adapter-b")
        .expect("adapter-b should be present");

    // adapter-a appears twice with gates 100 and 200, so average should be 150.
    assert_eq!(adapter_a.gate_q15, 150);
    assert_eq!(adapter_b.gate_q15, 300);

    Ok(())
}
