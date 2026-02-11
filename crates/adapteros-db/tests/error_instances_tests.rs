use adapteros_db::errors::{ErrorInstanceRow, ListErrorsDbQuery};
use adapteros_db::Db;

#[tokio::test]
async fn inserts_and_queries_error_instances_and_buckets() -> Result<(), Box<dyn std::error::Error>>
{
    let db = Db::new_in_memory().await?;

    let tenant_id = "tnt-test";
    let created_at_unix_ms = 1_700_000_000_000i64;

    let row = ErrorInstanceRow {
        id: "err-test-00000000000000000000000000000000".to_string(),
        created_at_unix_ms,
        tenant_id: tenant_id.to_string(),
        source: "api".to_string(),
        error_code: "INTERNAL_ERROR".to_string(),
        kind: "server".to_string(),
        severity: "error".to_string(),
        message_user: "boom".to_string(),
        message_dev: Some("detail".to_string()),
        fingerprint: "fp-test".to_string(),
        tags_json: "{\"component\":\"api\"}".to_string(),
        session_id: Some("ses-test".to_string()),
        request_id: Some("req-test".to_string()),
        diag_trace_id: None,
        otel_trace_id: Some("0123456789abcdef0123456789abcdef".to_string()),
        http_method: Some("GET".to_string()),
        http_path: Some("/v1/test".to_string()),
        http_status: Some(500),
        run_id: None,
        receipt_hash: None,
        route_digest: None,
    };

    let id = db.insert_error_instance(&row).await?;
    assert_eq!(id, row.id);

    db.upsert_error_bucket(
        tenant_id,
        &row.fingerprint,
        &row.error_code,
        &row.kind,
        &row.severity,
        created_at_unix_ms,
        &row.id,
    )
    .await?;
    db.upsert_error_bucket(
        tenant_id,
        &row.fingerprint,
        &row.error_code,
        &row.kind,
        "fatal",
        created_at_unix_ms + 1,
        &row.id,
    )
    .await?;

    let got = db.get_error_instance(tenant_id, &row.id).await?;
    assert!(got.is_some());
    assert_eq!(got.unwrap().fingerprint, row.fingerprint);

    let list = db
        .list_error_instances(&ListErrorsDbQuery {
            tenant_id: tenant_id.to_string(),
            fingerprint: Some(row.fingerprint.clone()),
            limit: Some(10),
            ..Default::default()
        })
        .await?;
    assert_eq!(list.len(), 1);

    let buckets = db.list_error_buckets(tenant_id, 10, None).await?;
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].count, 2);
    assert_eq!(buckets[0].severity, "fatal");

    Ok(())
}
