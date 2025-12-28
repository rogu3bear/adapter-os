use adapteros_db::{collections::CreateCollectionParams, documents::CreateDocumentParams};
use adapteros_server_api::create_app;
use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use tower::ServiceExt;
use uuid::Uuid;

mod common;

/// Ensure the orchestration config endpoint returns a deterministic stub.
#[tokio::test]
async fn orchestration_config_stub_works() {
    std::env::set_var("AOS_DEV_NO_AUTH", "1");

    let state = common::setup_state(None)
        .await
        .expect("failed to set up state");
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('system', 'System Tenant')",
    )
    .execute(state.db.pool())
    .await
    .expect("insert system tenant");

    let app = create_app(state);

    let response = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/v1/orchestration/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router should respond");

    assert_eq!(response.status(), StatusCode::OK);
}

/// Delete document/collection/evidence via API handlers.
#[tokio::test]
async fn delete_endpoints_succeed_and_404() {
    std::env::set_var("AOS_DEV_NO_AUTH", "1");

    let state = common::setup_state(None)
        .await
        .expect("failed to set up state");
    adapteros_db::sqlx::query(
        "INSERT OR IGNORE INTO tenants (id, name) VALUES ('system', 'System Tenant')",
    )
    .execute(state.db.pool())
    .await
    .expect("insert system tenant");

    // Create a document under the system tenant
    let doc_id = Uuid::now_v7().to_string();
    state
        .db
        .create_document(CreateDocumentParams {
            id: doc_id.clone(),
            tenant_id: "system".to_string(),
            name: "doc".to_string(),
            content_hash: "hash".to_string(),
            file_path: "var/test-documents/aos-test-doc".to_string(),
            file_size: 0,
            mime_type: "text/plain".to_string(),
            page_count: None,
        })
        .await
        .expect("create document");

    // Create a collection under the system tenant
    let coll_id = state
        .db
        .create_collection(CreateCollectionParams {
            tenant_id: "system".to_string(),
            name: "coll".to_string(),
            description: None,
            metadata_json: None,
        })
        .await
        .expect("create collection");

    // Create a dataset tied to system tenant for evidence
    let dataset_id = state
        .db
        .create_training_dataset(
            "dataset",
            Some("test"),
            "jsonl",
            "hash",
            "var/test-datasets/aos-test-dataset",
            None,
            None,
            Some("ready"),
            Some("hash"),
        )
        .await
        .expect("create dataset");
    adapteros_db::sqlx::query("UPDATE training_datasets SET tenant_id = ? WHERE id = ?")
        .bind("system")
        .bind(&dataset_id)
        .execute(state.db.pool())
        .await
        .expect("assign dataset tenant");

    let evidence_id = state
        .db
        .create_evidence_entry(
            Some(&dataset_id),
            None,
            "doc",
            "ref-1",
            None,
            "high",
            None,
            None,
        )
        .await
        .expect("create evidence");

    let app = create_app(state.clone());

    // Delete document
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/documents/{doc_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let deleted_doc = state
        .db
        .get_document("system", &doc_id)
        .await
        .expect("query doc");
    assert!(deleted_doc.is_none(), "document should be deleted");

    // Delete collection
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/collections/{coll_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let deleted_coll = state
        .db
        .get_collection("system", &coll_id)
        .await
        .expect("query collection");
    assert!(deleted_coll.is_none(), "collection should be deleted");

    // Delete evidence
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/v1/evidence/{evidence_id}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    let deleted_ev = state
        .db
        .get_evidence_entry(&evidence_id)
        .await
        .expect("query evidence");
    assert!(deleted_ev.is_none(), "evidence should be deleted");

    // 404 on missing evidence
    let resp = app
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/v1/evidence/nonexistent-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .expect("router responds");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}
